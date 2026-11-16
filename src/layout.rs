use zellij_tile::prelude::*;

use crate::config::{Anchor, PluginConfig};

/// Terminal viewport dimensions for the current tab.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Viewport {
    pub cols: usize,
    pub rows: usize,
}

/// Resolved presentation mode for the plugin UI.
/// Designed so adding responsive breakpoints (e.g., FullScreen when terminal is small,
/// or a split preview when wide) never requires touching state or rendering logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewMode {
    /// Subtle floating pop-over anchored to a corner/edge.
    CompactPopover {
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    },
    /// Full-viewport overlay when the terminal is smaller than the configured thresholds.
    FullScreen {
        width: usize,
        height: usize,
    },
}

impl ViewMode {
    pub fn to_floating_coordinates(&self) -> FloatingPaneCoordinates {
        match *self {
            Self::CompactPopover {
                x,
                y,
                width,
                height,
            } => FloatingPaneCoordinates::default()
                .with_x_fixed(x)
                .with_y_fixed(y)
                .with_width_fixed(width)
                .with_height_fixed(height),
            Self::FullScreen { width, height } => FloatingPaneCoordinates::default()
                .with_x_fixed(0)
                .with_y_fixed(0)
                .with_width_fixed(width)
                .with_height_fixed(height),
        }
    }
}

pub struct LayoutEngine;

impl LayoutEngine {
    /// Computes `ViewMode` and floating coordinates given the current viewport,
    /// tracked item count, and an optional user-moved/resized floating `Rect`.
    pub fn resolve(
        viewport: Viewport,
        item_count: usize,
        user_rect: Option<Rect>,
        config: &PluginConfig,
    ) -> ViewMode {
        let mode = Self::resolve_auto(viewport, item_count, config);
        match (mode, user_rect) {
            (ViewMode::CompactPopover { .. }, Some(rect)) => {
                let width = rect.width.min(viewport.cols).max(20);
                let usable_rows = viewport
                    .rows
                    .saturating_sub(config.bottom_bar_rows + config.top_bar_rows);
                let height = rect.height.min(usable_rows).max(3);
                let max_x = viewport.cols.saturating_sub(width);
                let max_y = viewport
                    .rows
                    .saturating_sub(height + config.bottom_bar_rows);
                ViewMode::CompactPopover {
                    x: rect.x.min(max_x),
                    y: rect.y.clamp(config.top_bar_rows, max_y.max(config.top_bar_rows)),
                    width,
                    height,
                }
            }
            (other, _) => other,
        }
    }

    fn resolve_auto(viewport: Viewport, item_count: usize, config: &PluginConfig) -> ViewMode {
        let cols = viewport.cols.max(20);
        let rows = viewport.rows.max(6);

        // Fall back to FullScreen if the viewport is too cramped for a pop-over.
        if cols < config.fullscreen_below_cols || rows < config.fullscreen_below_rows {
            let usable_rows = rows.saturating_sub(config.bottom_bar_rows + config.top_bar_rows);
            return ViewMode::FullScreen {
                width: cols,
                height: usable_rows.max(4),
            };
        }

        // Dynamic height: header (1) + items (at least 1 row for "No active items") + footer help (1) + frame padding (2)
        let content_rows = item_count.max(1) + 2;
        let max_usable_rows = rows
            .saturating_sub(config.bottom_bar_rows + config.top_bar_rows + 1)
            .max(config.popover_min_height);
        let height = content_rows
            .clamp(config.popover_min_height, config.popover_max_height)
            .min(max_usable_rows);

        let width = config.popover_width.min(cols.saturating_sub(2)).max(28);

        let (x, y) = match config.anchor {
            Anchor::BottomRight => {
                let x = cols.saturating_sub(width);
                // Sit flush right above the bottom zjstatus bar
                let y = rows.saturating_sub(height + config.bottom_bar_rows);
                (x, y)
            }
            Anchor::BottomLeft => {
                let x = 0;
                let y = rows.saturating_sub(height + config.bottom_bar_rows);
                (x, y)
            }
            Anchor::TopRight => {
                let x = cols.saturating_sub(width);
                let y = config.top_bar_rows;
                (x, y)
            }
            Anchor::TopLeft => (0, config.top_bar_rows),
            Anchor::Center => {
                let x = cols.saturating_sub(width) / 2;
                let y = rows.saturating_sub(height) / 2;
                (x, y)
            }
        };

        ViewMode::CompactPopover {
            x,
            y,
            width,
            height,
        }
    }
}

/// Finds this plugin's visible floating pane rectangle from a `PaneManifest`, if present.
pub fn find_own_floating_rect(manifest: &PaneManifest, plugin_id: u32) -> Option<Rect> {
    manifest
        .panes
        .values()
        .flatten()
        .find(|p| p.is_plugin && p.id == plugin_id && p.is_floating && !p.is_suppressed)
        .map(|p| Rect::new(p.pane_x, p.pane_y, p.pane_columns, p.pane_rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_bottom_right_above_zjstatus_bar() {
        let cfg = PluginConfig::default(); // bottom_bar_rows = 1, popover_width = 52
        let vp = Viewport {
            cols: 120,
            rows: 40,
        };
        let mode = LayoutEngine::resolve(vp, 3, None, &cfg);
        assert_eq!(
            mode,
            ViewMode::CompactPopover {
                x: 120 - 52,
                y: 40 - 5 - 1,
                width: 52,
                height: 5,
            }
        );
    }

    #[test]
    fn switches_to_fullscreen_when_viewport_is_small() {
        let cfg = PluginConfig::default();
        let vp = Viewport { cols: 50, rows: 10 };
        let mode = LayoutEngine::resolve(vp, 3, None, &cfg);
        assert_eq!(
            mode,
            ViewMode::FullScreen {
                width: 50,
                height: 9,
            }
        );
    }

    #[test]
    fn clamps_user_rect_to_usable_viewport() {
        let cfg = PluginConfig::default(); // top_bar_rows = 0, bottom_bar_rows = 1
        let vp = Viewport {
            cols: 100,
            rows: 30,
        };
        let mode = LayoutEngine::resolve(vp, 2, Some(Rect::new(90, 28, 40, 10)), &cfg);
        assert_eq!(
            mode,
            ViewMode::CompactPopover {
                x: 60,
                y: 19,
                width: 40,
                height: 10,
            }
        );
    }
}
