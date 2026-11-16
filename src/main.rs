mod config;
mod format;
mod layout;
mod model;
mod render;

use std::collections::BTreeMap;
use zellij_tile::prelude::*;

use config::PluginConfig;
use format::{format_pane_title, format_zjstatus_pipe};
use layout::{find_own_floating_rect, LayoutEngine, ViewMode, Viewport};
use model::{InboxSnapshot, SignalTracker, StatusBucket};
use render::Renderer;

#[derive(Default)]
struct State {
    config: PluginConfig,
    tracker: SignalTracker,
    snapshot: InboxSnapshot,
    viewport: Viewport,
    active_tab_pos: usize,
    selected_index: usize,
    is_visible: bool,
    own_plugin_id: Option<u32>,
    last_piped_output: Option<String>,
    last_pane_title: Option<String>,
    last_tab_count: usize,
    user_rect: Option<Rect>,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.config = PluginConfig::from_kdl(&configuration);
        self.own_plugin_id = Some(get_plugin_ids().plugin_id);

        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::MessageAndLaunchOtherPlugins,
        ]);

        subscribe(&[
            EventType::PermissionRequestResult,
            EventType::GlobalAttentionUpdate,
            EventType::SessionUpdate,
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::Visible,
            EventType::Key,
        ]);
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        let action = pipe_message
            .payload
            .as_deref()
            .unwrap_or(pipe_message.name.as_str());

        if action == "toggle"
            || action == "inbox_toggle"
            || pipe_message.name == "zjstatus_inbox_toggle"
            || pipe_message.name == "inbox_toggle"
        {
            if self.is_visible {
                self.hide_popover();
                return false;
            } else {
                self.show_popover();
                return true;
            }
        }
        false
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                // Zellij forces the plugin pane visible when showing the one-time
                // permission prompt, so always hide here once granted and discard
                // the permission dialog's centered coordinates.
                self.user_rect = None;
                self.hide_popover();
                self.refresh_and_pipe(true);
                false
            }
            Event::GlobalAttentionUpdate(global) => {
                self.tracker.update_global(global);
                self.refresh_and_pipe(false);
                self.is_visible
            }
            Event::SessionUpdate(sessions, _) => {
                self.tracker.update_sessions(&sessions);
                self.refresh_and_pipe(false);
                self.is_visible
            }
            Event::TabUpdate(tabs) => self.handle_tab_update(&tabs),
            Event::PaneUpdate(manifest) => {
                self.handle_pane_update(&manifest);
                false
            }
            Event::Visible(visible) => {
                self.is_visible = visible;
                visible
            }
            Event::Key(key) => self.handle_key(key),
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        Renderer::render(
            &self.snapshot,
            self.selected_index,
            rows,
            cols,
            now_ms(),
            &self.config,
        );
    }
}

impl State {
    fn current_view_mode(&self) -> ViewMode {
        LayoutEngine::resolve(
            self.viewport,
            self.snapshot.items.len(),
            self.user_rect,
            &self.config,
        )
    }

    fn handle_tab_update(&mut self, tabs: &[TabInfo]) -> bool {
        let mut viewport_changed = false;
        if let Some(active) = tabs.iter().find(|t| t.active) {
            self.active_tab_pos = active.position;
            let cols = active.viewport_columns.max(active.display_area_columns);
            let rows = active.viewport_rows.max(active.display_area_rows);
            if cols > 0 && rows > 0 {
                let new_vp = Viewport { cols, rows };
                if new_vp != self.viewport {
                    self.viewport = new_vp;
                    viewport_changed = true;
                }
            }
        }
        // If a new tab was opened, force re-broadcast so the new tab's zjstatus instance gets the state immediately.
        let tab_count_changed = tabs.len() != self.last_tab_count;
        self.last_tab_count = tabs.len();
        if tab_count_changed {
            self.refresh_and_pipe(true);
        }
        if self.is_visible && viewport_changed {
            self.apply_layout();
        }
        self.is_visible
    }

    fn handle_pane_update(&mut self, manifest: &PaneManifest) {
        if !self.is_visible {
            return;
        }
        if let Some(plugin_id) = self.own_plugin_id {
            if let Some(rect) = find_own_floating_rect(manifest, plugin_id) {
                self.user_rect = Some(rect);
            }
        }
    }

    fn show_popover(&mut self) {
        let target_tab = self.active_tab_pos;
        self.is_visible = true;
        self.selected_index = 0;
        // 1. Place at our target coordinates first while the floating layer is still
        // hidden (`change_floating_panes_coordinates` unsuppresses into `floating_panes`
        // without calling `show_floating_panes`, preventing a centered-box flash).
        self.apply_layout();
        // 2. Mark selectable while still hidden.
        set_selectable(true);
        // 3. Move the positioned floating pane to `target_tab` (blocks until complete).
        if let Some(plugin_id) = self.own_plugin_id {
            break_panes_to_tab_with_index(&[PaneId::Plugin(plugin_id)], target_tab, false);
        }
        // 4. Reveal and focus the floating layer once, already in place on `target_tab`.
        show_self(true);
    }

    fn hide_popover(&mut self) {
        self.is_visible = false;
        // Mark unselectable while still in `floating_panes` so Zellij cleanly
        // returns client focus to the underlying tiled pane before suppressing.
        set_selectable(false);
        hide_self();
    }

    fn apply_layout(&mut self) {
        if self.viewport.cols == 0 || self.viewport.rows == 0 {
            return;
        }
        let mode = self.current_view_mode();
        if let Some(plugin_id) = self.own_plugin_id {
            let coords = mode.to_floating_coordinates();
            change_floating_panes_coordinates(vec![(PaneId::Plugin(plugin_id), coords)]);
        }
    }

    fn refresh_and_pipe(&mut self, force_pipe: bool) {
        self.snapshot = self.tracker.snapshot(&self.config);
        if !self.snapshot.items.is_empty() {
            self.selected_index = self.selected_index.min(self.snapshot.items.len() - 1);
        } else {
            self.selected_index = 0;
        }

        if let Some(plugin_id) = self.own_plugin_id {
            let title = format_pane_title(&self.snapshot, &self.config);
            if force_pipe || self.last_pane_title.as_ref() != Some(&title) {
                rename_plugin_pane(plugin_id, &title);
                self.last_pane_title = Some(title);
            }
        }

        let formatted = format_zjstatus_pipe(&self.snapshot.counts, &self.config);
        if force_pipe || self.last_piped_output.as_ref() != Some(&formatted) {
            self.last_piped_output = Some(formatted.clone());
            let payload = format!("zjstatus::pipe::{}::{}", self.config.pipe_name, formatted);
            pipe_message_to_plugin(MessageToPlugin::new("zjstatus").with_payload(payload));
        }
    }

    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        let plain = key.has_no_modifiers();
        let ctrl = key.has_only_modifiers(&[KeyModifier::Ctrl]);
        let alt = key.has_only_modifiers(&[KeyModifier::Alt]);

        match key.bare_key {
            BareKey::Esc | BareKey::Char('q') if plain => {
                self.hide_popover();
                false
            }
            BareKey::Char('i') if ctrl || alt => {
                self.hide_popover();
                false
            }
            BareKey::Down | BareKey::Char('j') if plain => {
                if !self.snapshot.items.is_empty() {
                    self.selected_index =
                        (self.selected_index + 1).min(self.snapshot.items.len() - 1);
                }
                true
            }
            BareKey::Up | BareKey::Char('k') if plain => {
                self.selected_index = self.selected_index.saturating_sub(1);
                true
            }
            BareKey::Enter if plain => {
                self.jump_to_selected();
                false
            }
            BareKey::Char('c') | BareKey::Char('d') | BareKey::Backspace if plain => {
                self.dismiss_selected();
                true
            }
            _ => false,
        }
    }

    fn jump_to_selected(&mut self) {
        let Some(item) = self.snapshot.items.get(self.selected_index).cloned() else {
            self.hide_popover();
            return;
        };

        self.hide_popover();

        if item.is_current_session {
            match item.key.pane_id {
                PaneId::Terminal(id) => focus_terminal_pane(id, true, false),
                PaneId::Plugin(id) => focus_plugin_pane(id, true, false),
            }
        } else {
            let pane = match item.key.pane_id {
                PaneId::Terminal(id) => (id, false),
                PaneId::Plugin(id) => (id, true),
            };
            switch_session_with_focus(&item.key.session, Some(item.tab_id), Some(pane));
        }
    }

    fn dismiss_selected(&mut self) {
        let Some(item) = self.snapshot.items.get(self.selected_index).cloned() else {
            return;
        };
        if item.bucket == StatusBucket::Idle {
            // Already idle: remove from tracked set until it emits a new signal
            self.tracker.forget_pane(&item.key);
        } else {
            mark_session_pane_seen(&item.key.session, item.key.pane_id);
        }
        self.refresh_and_pipe(false);
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
