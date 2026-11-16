use crate::config::PluginConfig;
use crate::model::{InboxSnapshot, StatusBucket, StatusCounts};

/// Formats non-empty status buckets according to `config.count_format` and `config.show_zero`.
/// When `styled` is true, prefixes each token with zjstatus `#[fg=...,bold]` tags.
fn format_bucket_counts(counts: &StatusCounts, config: &PluginConfig, styled: bool) -> Vec<String> {
    let mut parts = Vec::new();
    for bucket in StatusBucket::ALL {
        let count = counts.get(bucket);
        if count == 0 && !config.show_zero {
            continue;
        }
        let style = config.bucket_style(bucket);
        if style.icon.is_empty() {
            continue;
        }
        let rendered = config
            .count_format
            .replace("{icon}", style.icon)
            .replace("{count}", &count.to_string());

        if styled {
            let tag = if style.bold {
                format!("#[fg={},bold]", style.color)
            } else {
                format!("#[fg={}]", style.color)
            };
            parts.push(format!("{}{}", tag, rendered));
        } else {
            parts.push(rendered);
        }
    }
    parts
}

/// Formats `StatusCounts` into a zjstatus dynamic format string (`#[fg=...,bold]icon count`).
/// Returns an empty string when no panes have reported any signal (unless `show_zero` is true).
pub fn format_zjstatus_pipe(counts: &StatusCounts, config: &PluginConfig) -> String {
    let parts = format_bucket_counts(counts, config, true);
    if parts.is_empty() {
        return String::new();
    }

    format!(
        "{}{}{}",
        config.bar_prefix,
        parts.join(&config.count_separator),
        config.bar_suffix
    )
}

/// Formats the floating pane border title from `config.title`, supporting template variables:
/// `{total}`, `{active}`, `{needs_you}`, `{failed}`, `{done}`, `{working}`, `{idle}`, `{summary}`, `{session}`.
pub fn format_pane_title(snapshot: &InboxSnapshot, config: &PluginConfig) -> String {
    let counts = &snapshot.counts;
    let summary = format_bucket_counts(counts, config, false).join(&config.count_separator);

    config
        .title
        .replace("{total}", &counts.total_reported().to_string())
        .replace("{active}", &counts.total_active().to_string())
        .replace("{needs_you}", &counts.needs_you.to_string())
        .replace("{failed}", &counts.failed.to_string())
        .replace("{done}", &counts.done.to_string())
        .replace("{working}", &counts.working.to_string())
        .replace("{idle}", &counts.idle.to_string())
        .replace("{summary}", &summary)
        .replace("{session}", &snapshot.current_session)
}

/// Formats an elapsed duration since `since_ms` as a compact relative age string.
pub fn format_age(since_ms: u64, now_ms: u64) -> String {
    if since_ms == 0 || now_ms <= since_ms {
        return String::new();
    }
    let secs = (now_ms - since_ms) / 1000;
    if secs < 10 {
        "now".to_owned()
    } else if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_custom_count_format() {
        let mut cfg = PluginConfig::default();
        cfg.count_format = "{count}:{icon}".to_owned();
        cfg.icon_needs_you = "!".to_owned();
        cfg.icon_working = "W".to_owned();

        let counts = StatusCounts {
            needs_you: 2,
            working: 3,
            ..Default::default()
        };

        assert_eq!(
            format_zjstatus_pipe(&counts, &cfg),
            " #[fg=#fab387,bold]2:!  #[fg=#f9e2af]3:W "
        );
    }
}
