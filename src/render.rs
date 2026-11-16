use zellij_tile::prelude::*;

use crate::config::PluginConfig;
use crate::format::format_age;
use crate::model::{InboxSnapshot, StatusBucket, TrackedItem};

pub struct Renderer;

impl Renderer {
    pub fn render(
        snapshot: &InboxSnapshot,
        selected_index: usize,
        rows: usize,
        cols: usize,
        now_ms: u64,
        config: &PluginConfig,
    ) {
        if rows == 0 || cols == 0 {
            return;
        }
        Self::render_list(snapshot, selected_index, rows, cols, now_ms, config);
    }

    fn render_list(
        snapshot: &InboxSnapshot,
        selected_index: usize,
        rows: usize,
        cols: usize,
        now_ms: u64,
        config: &PluginConfig,
    ) {
        let list_avail_rows = rows.saturating_sub(1).max(1);

        if snapshot.items.is_empty() {
            print_text_with_coordinates(
                Text::from(" No panes have reported status yet"),
                0,
                0,
                Some(cols),
                None,
            );
        } else {
            // Scroll window around `selected_index`
            let total = snapshot.items.len();
            let start = if total <= list_avail_rows {
                0
            } else {
                let half = list_avail_rows / 2;
                selected_index
                    .saturating_sub(half)
                    .min(total.saturating_sub(list_avail_rows))
            };

            for (row_offset, idx) in (start..(start + list_avail_rows).min(total)).enumerate() {
                let item = &snapshot.items[idx];
                let is_selected = idx == selected_index;
                let text = Self::format_item_row(item, cols, now_ms, config, is_selected);
                print_text_with_coordinates(text, 0, row_offset, Some(cols), None);
            }
        }

        if rows >= 2 {
            let footer = " <↑↓/jk> select  <Enter> go  <c> dismiss  <Esc> close";
            let clipped: String = footer.chars().take(cols).collect();
            print_text_with_coordinates(
                Text::from(clipped.as_str()).color_range(3, ..),
                0,
                rows - 1,
                Some(cols),
                None,
            );
        }
    }

    fn format_item_row(
        item: &TrackedItem,
        cols: usize,
        now_ms: u64,
        config: &PluginConfig,
        is_selected: bool,
    ) -> Text {
        let icon = config.bucket_style(item.bucket).icon.trim();

        let age = format_age(item.since_ms, now_ms);
        let age_col = if age.is_empty() {
            String::new()
        } else {
            format!(" {}", age)
        };

        let label = if item.pane_title.is_empty() {
            &item.tab_name
        } else {
            &item.pane_title
        };
        let prefix = format!(" {} {} › {} ", icon, item.key.session, label);
        let prefix_len = prefix.chars().count();
        let age_len = age_col.chars().count();

        let detail_budget = cols.saturating_sub(prefix_len + age_len + 1);
        let detail_trimmed: String = item.detail_text.chars().take(detail_budget).collect();
        let pad_len = cols.saturating_sub(prefix_len + detail_trimmed.chars().count() + age_len);
        let padding = " ".repeat(pad_len);

        let full_line = format!("{}{}{}{}", prefix, detail_trimmed, padding, age_col);
        let icon_end = 1 + icon.chars().count();

        let mut text = apply_bucket_color(Text::from(full_line.as_str()), item.bucket, icon_end);
        if is_selected {
            text = text.selected();
        }
        text
    }
}

fn apply_bucket_color(text: Text, bucket: StatusBucket, icon_end: usize) -> Text {
    match bucket {
        StatusBucket::NeedsYou => text.color_range(0, 1..icon_end),
        StatusBucket::Failed => text.error_color_range(1..icon_end),
        StatusBucket::Done => text.success_color_range(1..icon_end),
        StatusBucket::Working => text.color_range(1, 1..icon_end),
        StatusBucket::Idle => text,
    }
}
