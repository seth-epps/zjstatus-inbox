use std::collections::{BTreeMap, BTreeSet};
use zellij_tile::prelude::*;

use crate::config::{PluginConfig, SignalSource};

/// Normalized status buckets for display in the status bar and pop-over list.
/// Ordered from highest priority (NeedsYou) to lowest (Idle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StatusBucket {
    NeedsYou,
    Failed,
    Done,
    Working,
    Idle,
}

impl StatusBucket {
    pub const ALL: [Self; 5] = [
        Self::NeedsYou,
        Self::Failed,
        Self::Done,
        Self::Working,
        Self::Idle,
    ];

    pub fn from_attention(status: AttentionStatus) -> Self {
        match status {
            AttentionStatus::NeedsYou => Self::NeedsYou,
            AttentionStatus::Failed => Self::Failed,
            AttentionStatus::Done => Self::Done,
            AttentionStatus::Working => Self::Working,
            // Quiet / NewOutput / Idle with remembered status:
            // The pane has emitted signal previously and is now effectively idle/resting.
            AttentionStatus::Quiet | AttentionStatus::NewOutput | AttentionStatus::Idle => {
                Self::Idle
            }
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::NeedsYou => "needs you",
            Self::Failed => "failed",
            Self::Done => "done",
            Self::Working => "working",
            Self::Idle => "idle",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusCounts {
    pub needs_you: usize,
    pub failed: usize,
    pub done: usize,
    pub working: usize,
    pub idle: usize,
}

impl StatusCounts {
    pub fn total_reported(&self) -> usize {
        self.needs_you + self.failed + self.done + self.working + self.idle
    }

    pub fn total_active(&self) -> usize {
        self.needs_you + self.failed + self.done + self.working
    }

    pub fn get(&self, bucket: StatusBucket) -> usize {
        match bucket {
            StatusBucket::NeedsYou => self.needs_you,
            StatusBucket::Failed => self.failed,
            StatusBucket::Done => self.done,
            StatusBucket::Working => self.working,
            StatusBucket::Idle => self.idle,
        }
    }

    pub fn add(&mut self, bucket: StatusBucket) {
        match bucket {
            StatusBucket::NeedsYou => self.needs_you += 1,
            StatusBucket::Failed => self.failed += 1,
            StatusBucket::Done => self.done += 1,
            StatusBucket::Working => self.working += 1,
            StatusBucket::Idle => self.idle += 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PaneKey {
    pub session: String,
    pub pane_id: PaneId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedMeta {
    pub tab_id: usize,
    pub tab_name: String,
    pub pane_title: String,
    pub last_text: String,
    pub since_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedItem {
    pub key: PaneKey,
    pub is_current_session: bool,
    pub tab_id: usize,
    pub tab_name: String,
    pub pane_title: String,
    pub bucket: StatusBucket,
    pub detail_text: String,
    pub since_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InboxSnapshot {
    pub current_session: String,
    pub counts: StatusCounts,
    pub items: Vec<TrackedItem>,
}

/// Tracks panes that have explicitly emitted status signals so we can distinguish
/// a pane that reported status and is now `Idle` from a silent shell pane that
/// has never emitted any signal (absence of signal).
#[derive(Debug, Default)]
pub struct SignalTracker {
    /// Panes known to have emitted signal during their lifetime.
    known_signaled: BTreeMap<PaneKey, TrackedMeta>,
    /// Open terminal panes per session (populated when SessionUpdate arrives).
    open_panes_by_session: BTreeMap<String, BTreeSet<PaneId>>,
    /// Latest GlobalAttention snapshot from Zellij.
    last_global: Option<GlobalAttention>,
}

impl SignalTracker {
    pub fn update_sessions(&mut self, sessions: &[SessionInfo]) {
        self.open_panes_by_session.clear();
        for session in sessions {
            let mut panes = BTreeSet::new();
            for pane_list in session.panes.panes.values() {
                for pane in pane_list {
                    if !pane.is_plugin {
                        panes.insert(PaneId::Terminal(pane.id));
                    }
                }
            }
            self.open_panes_by_session.insert(session.name.clone(), panes);
        }
        self.prune_closed_panes();
    }

    pub fn update_global(&mut self, global: GlobalAttention) {
        self.last_global = Some(global);
        self.prune_closed_panes();
    }

    /// Removes any tracked pane whose session exited or whose terminal pane was closed.
    fn prune_closed_panes(&mut self) {
        let running_sessions: Option<BTreeSet<&str>> = self
            .last_global
            .as_ref()
            .map(|g| g.sessions.iter().map(|s| s.name.as_str()).collect());

        self.known_signaled.retain(|key, _| {
            if let Some(ref running) = running_sessions {
                if !running.contains(key.session.as_str()) {
                    return false;
                }
            }
            if let Some(open_panes) = self.open_panes_by_session.get(&key.session) {
                if !open_panes.contains(&key.pane_id) {
                    return false;
                }
            }
            true
        });
    }

    /// Remove a specific pane from tracking (e.g., if user explicitly forgets/clears it when already idle).
    pub fn forget_pane(&mut self, key: &PaneKey) {
        self.known_signaled.remove(key);
    }

    pub fn snapshot(&mut self, config: &PluginConfig) -> InboxSnapshot {
        let Some(ref global) = self.last_global else {
            return InboxSnapshot::default();
        };
        if global.attention_disabled {
            return InboxSnapshot::default();
        }

        let current_session = global.current_session.clone();
        let mut active_items_by_key: BTreeMap<PaneKey, TrackedItem> = BTreeMap::new();

        for global_item in &global.items {
            let Some(item) = build_active_item(global_item, config.source) else {
                continue;
            };
            self.known_signaled.insert(
                item.key.clone(),
                TrackedMeta {
                    tab_id: item.tab_id,
                    tab_name: item.tab_name.clone(),
                    pane_title: item.pane_title.clone(),
                    last_text: item.detail_text.clone(),
                    since_ms: item.since_ms,
                },
            );
            active_items_by_key.insert(item.key.clone(), item);
        }

        self.append_idle_items(&current_session, &mut active_items_by_key);

        let mut counts = StatusCounts::default();
        let mut items: Vec<TrackedItem> = active_items_by_key
            .into_values()
            .inspect(|item| counts.add(item.bucket))
            .collect();
        sort_items(&mut items);

        InboxSnapshot {
            current_session,
            counts,
            items,
        }
    }

    /// Inserts `Idle` entries for any previously-signaled pane that is still open
    /// but no longer present in the current active attention list.
    fn append_idle_items(
        &self,
        current_session: &str,
        active_items_by_key: &mut BTreeMap<PaneKey, TrackedItem>,
    ) {
        for (key, meta) in &self.known_signaled {
            if active_items_by_key.contains_key(key) {
                continue;
            }
            active_items_by_key.insert(
                key.clone(),
                TrackedItem {
                    key: key.clone(),
                    is_current_session: key.session == current_session,
                    tab_id: meta.tab_id,
                    tab_name: meta.tab_name.clone(),
                    pane_title: meta.pane_title.clone(),
                    bucket: StatusBucket::Idle,
                    detail_text: "idle".to_owned(),
                    since_ms: meta.since_ms,
                },
            );
        }
    }
}

fn build_active_item(
    global_item: &GlobalAttentionItem,
    source_filter: SignalSource,
) -> Option<TrackedItem> {
    let attention = &global_item.item.attention;
    if attention.is_muted() || !matches_source(attention, source_filter) {
        return None;
    }

    let bucket = StatusBucket::from_attention(attention.effective_status());
    let detail_text = if !attention.text.is_empty() {
        attention.text.clone()
    } else if let Some(ref rem) = attention.remembered {
        rem.text.clone()
    } else {
        bucket.label().to_owned()
    };

    let since_ms = if attention.since_ms > 0 {
        attention.since_ms
    } else if let Some(ref rem) = attention.remembered {
        rem.since_ms
    } else {
        0
    };

    Some(TrackedItem {
        key: PaneKey {
            session: global_item.session.clone(),
            pane_id: global_item.item.pane_id,
        },
        is_current_session: global_item.is_current,
        tab_id: global_item.item.tab_id,
        tab_name: global_item.item.tab_name.clone(),
        pane_title: global_item.item.pane_title.clone(),
        bucket,
        detail_text,
        since_ms,
    })
}

fn sort_items(items: &mut [TrackedItem]) {
    // Sort by urgency bucket first, then most recent, then current session first.
    items.sort_by(|a, b| {
        a.bucket
            .cmp(&b.bucket)
            .then(b.since_ms.cmp(&a.since_ms))
            .then(b.is_current_session.cmp(&a.is_current_session))
            .then(a.key.session.cmp(&b.key.session))
    });
}

fn matches_source(attention: &PaneAttention, source_filter: SignalSource) -> bool {
    match source_filter {
        SignalSource::All => {
            attention.source.is_some()
                || attention.remembered.is_some()
                || !attention.status.is_idle()
        }
        SignalSource::ProgramStatusOnly => {
            attention.source == Some(AttentionSource::ProgramStatus)
                || attention.remembered.is_some()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(
        session: &str,
        pane_id: u32,
        status: AttentionStatus,
        source: Option<AttentionSource>,
        remembered: Option<RememberedStatus>,
    ) -> (String, SessionAttention) {
        let attention = PaneAttention {
            status,
            source,
            remembered,
            since_ms: 1000,
            ..Default::default()
        };
        let mut counts = AttentionCounts::default();
        counts.add(&attention);
        let item = AttentionItem {
            pane_id: PaneId::Terminal(pane_id),
            tab_id: 0,
            tab_name: "main".to_owned(),
            pane_title: format!("agent-{}", pane_id),
            attention,
        };
        (
            session.to_owned(),
            SessionAttention {
                status: counts.highest_counted(),
                counts,
                items: vec![item],
                ..Default::default()
            },
        )
    }

    #[test]
    fn silent_sessions_produce_empty_snapshot_and_empty_bar() {
        let mut tracker = SignalTracker::default();
        let cfg = PluginConfig::default();

        // Two running sessions with zero attention items (silent shell panes)
        let global = GlobalAttention::build(
            "dev",
            vec![
                ("dev".to_owned(), SessionAttention::default()),
                ("ops".to_owned(), SessionAttention::default()),
            ],
        );
        tracker.update_global(global);
        let snap = tracker.snapshot(&cfg);

        assert_eq!(snap.counts.total_reported(), 0);
    }

    #[test]
    fn tracks_active_and_transitioned_to_idle_panes() {
        let mut tracker = SignalTracker::default();
        let cfg = PluginConfig::default();

        // Step 1: Session "api" has pane 1 Working, Session "web" has pane 2 NeedsYou
        let global1 = GlobalAttention::build(
            "api",
            vec![
                make_item(
                    "api",
                    1,
                    AttentionStatus::Working,
                    Some(AttentionSource::ProgramStatus),
                    None,
                ),
                make_item(
                    "web",
                    2,
                    AttentionStatus::NeedsYou,
                    Some(AttentionSource::ProgramStatus),
                    None,
                ),
            ],
        );
        tracker.update_global(global1);
        let snap1 = tracker.snapshot(&cfg);
        assert_eq!(snap1.counts.needs_you, 1);
        assert_eq!(snap1.counts.working, 1);
        assert_eq!(snap1.counts.idle, 0);

        // Step 2: Pane 1 in "api" transitions to Idle (no longer in active items, but session still runs)
        let global2 = GlobalAttention::build(
            "api",
            vec![
                ("api".to_owned(), SessionAttention::default()),
                make_item(
                    "web",
                    2,
                    AttentionStatus::NeedsYou,
                    Some(AttentionSource::ProgramStatus),
                    None,
                ),
            ],
        );
        tracker.update_global(global2);
        let snap2 = tracker.snapshot(&cfg);
        assert_eq!(snap2.counts.needs_you, 1);
        assert_eq!(snap2.counts.working, 0);
        assert_eq!(snap2.counts.idle, 1);
    }
}
