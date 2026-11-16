use std::collections::BTreeMap;

use crate::model::StatusBucket;

/// Display styling for a single status bucket (icon, zjstatus color, emphasis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BucketStyle<'a> {
    pub icon: &'a str,
    pub color: &'a str,
    pub bold: bool,
}

/// Anchor position for the compact pop-over mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
    Center,
}

impl Anchor {
    pub fn from_str(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "bottom-left" | "bottom_left" => Self::BottomLeft,
            "top-right" | "top_right" => Self::TopRight,
            "top-left" | "top_left" => Self::TopLeft,
            "center" => Self::Center,
            _ => Self::BottomRight,
        }
    }
}

/// Which attention sources count as valid signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalSource {
    /// Any signal tracked by Zellij (OSC 7501 program status, shell command finish/fail, bell, notification, watch).
    All,
    /// Only explicit OSC 7501 ProgramStatus reports from coding agents/programs.
    ProgramStatusOnly,
}

impl SignalSource {
    pub fn from_str(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "program_status" | "program" | "osc7501" => Self::ProgramStatusOnly,
            _ => Self::All,
        }
    }
}

/// All user-modifiable settings loaded from the KDL plugin block.
#[derive(Debug, Clone)]
pub struct PluginConfig {
    /// Floating pane frame title template (default: "Inbox ({total})").
    pub title: String,
    /// Name of the zjstatus pipe widget (default: "pipe_inbox" -> `{pipe_inbox}` in zjstatus.kdl).
    pub pipe_name: String,
    /// Which signal sources to track ("all" or "program_status").
    pub source: SignalSource,

    // Icons for each status bucket (modifiable in KDL)
    pub icon_needs_you: String,
    pub icon_failed: String,
    pub icon_done: String,
    pub icon_working: String,
    pub icon_idle: String,

    // Foreground hex colors for zjstatus dynamic format tags
    pub color_needs_you: String,
    pub color_failed: String,
    pub color_done: String,
    pub color_working: String,
    pub color_idle: String,

    /// Template for each status item in the bar (default: "{icon}{count}").
    pub count_format: String,
    /// Separator between status counts in the zjstatus bar.
    pub count_separator: String,
    /// Prefix/suffix around the entire status bar section when non-empty.
    pub bar_prefix: String,
    pub bar_suffix: String,
    /// Whether to show 0-count buckets in the status bar.
    pub show_zero: bool,

    // Pop-over layout & responsive thresholds
    pub anchor: Anchor,
    /// Status bar offset at the bottom (1 row for zjstatus at the bottom of the layout).
    pub bottom_bar_rows: usize,
    pub top_bar_rows: usize,
    pub popover_width: usize,
    pub popover_min_height: usize,
    pub popover_max_height: usize,
    /// If terminal viewport is smaller than these dimensions, LayoutEngine falls back to FullScreen.
    pub fullscreen_below_cols: usize,
    pub fullscreen_below_rows: usize,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            title: "Inbox ({total})".to_owned(),
            pipe_name: "pipe_inbox".to_owned(),
            source: SignalSource::All,

            // Informative UTF-8 / Nerd-Font-friendly defaults (all overridable in KDL)
            icon_needs_you: "● ".to_owned(),
            icon_failed: "✗ ".to_owned(),
            icon_done: "✓ ".to_owned(),
            icon_working: "◐ ".to_owned(),
            icon_idle: "○ ".to_owned(),

            // Matches the Catppuccin/Nightfox palette in zjstatus.kdl
            color_needs_you: "#fab387".to_owned(),
            color_failed: "#f38ba8".to_owned(),
            color_done: "#a6e3a1".to_owned(),
            color_working: "#f9e2af".to_owned(),
            color_idle: "#6c7086".to_owned(),

            count_format: "{icon}{count}".to_owned(),
            count_separator: "  ".to_owned(),
            bar_prefix: " ".to_owned(),
            bar_suffix: " ".to_owned(),
            show_zero: false,

            anchor: Anchor::BottomRight,
            bottom_bar_rows: 1,
            top_bar_rows: 0,
            popover_width: 52,
            popover_min_height: 5,
            popover_max_height: 16,
            fullscreen_below_cols: 64,
            fullscreen_below_rows: 12,
        }
    }
}

impl PluginConfig {
    /// Returns the icon, zjstatus hex color, and bold flag for a given status bucket.
    pub fn bucket_style(&self, bucket: StatusBucket) -> BucketStyle<'_> {
        match bucket {
            StatusBucket::NeedsYou => BucketStyle {
                icon: &self.icon_needs_you,
                color: &self.color_needs_you,
                bold: true,
            },
            StatusBucket::Failed => BucketStyle {
                icon: &self.icon_failed,
                color: &self.color_failed,
                bold: true,
            },
            StatusBucket::Done => BucketStyle {
                icon: &self.icon_done,
                color: &self.color_done,
                bold: false,
            },
            StatusBucket::Working => BucketStyle {
                icon: &self.icon_working,
                color: &self.color_working,
                bold: false,
            },
            StatusBucket::Idle => BucketStyle {
                icon: &self.icon_idle,
                color: &self.color_idle,
                bold: false,
            },
        }
    }

    pub fn from_kdl(map: &BTreeMap<String, String>) -> Self {
        let mut cfg = Self::default();

        macro_rules! set_str {
            ($($field:ident),+ $(,)?) => {
                $(
                    if let Some(v) = map.get(stringify!($field)) {
                        cfg.$field = v.clone();
                    }
                )+
            };
        }

        macro_rules! set_parsed {
            ($($field:ident),+ $(,)?) => {
                $(
                    if let Some(v) = map.get(stringify!($field)).and_then(|s| s.parse().ok()) {
                        cfg.$field = v;
                    }
                )+
            };
        }

        set_str!(
            title,
            pipe_name,
            icon_needs_you,
            icon_failed,
            icon_done,
            icon_working,
            icon_idle,
            color_needs_you,
            color_failed,
            color_done,
            color_working,
            color_idle,
            count_format,
            count_separator,
            bar_prefix,
            bar_suffix,
        );

        set_parsed!(
            bottom_bar_rows,
            top_bar_rows,
            popover_width,
            popover_min_height,
            popover_max_height,
            fullscreen_below_cols,
            fullscreen_below_rows,
        );

        if let Some(v) = map.get("source") {
            cfg.source = SignalSource::from_str(v);
        }
        if let Some(v) = map.get("show_zero") {
            cfg.show_zero = v == "true";
        }
        if let Some(v) = map.get("anchor") {
            cfg.anchor = Anchor::from_str(v);
        }

        cfg
    }
}
