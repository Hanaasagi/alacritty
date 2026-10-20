use std::time::Instant;

use alacritty_terminal::index::Point;
use alacritty_terminal::vte::ansi::CursorShape;

use crate::display::SizeInfo;
use crate::display::color::Rgb;

/// Pixel-level snapshot of cursor position, size, and color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorSnapshot {
    /// Top-left X coordinate in pixels from the window's left edge.
    pub x: f32,
    /// Top-left Y coordinate in pixels (OpenGL Y-up, from the window bottom).
    pub y: f32,
    /// Width in pixels.
    pub width: f32,
    /// Height in pixels.
    pub height: f32,
    /// Cursor color (RGBA, 0.0–1.0).
    pub color: [f32; 4],
    /// Cursor style index matching cursor-shaders conventions.
    pub style: i32,
}

impl Default for CursorSnapshot {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, width: 0.0, height: 0.0, color: [1.0, 1.0, 1.0, 1.0], style: 0 }
    }
}

/// Tracks cursor position changes for shader animation uniforms.
pub struct CursorTracker {
    current: CursorSnapshot,
    previous: CursorSnapshot,
    change_time: Instant,
    initialized: bool,
}

impl CursorTracker {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            current: CursorSnapshot::default(),
            previous: CursorSnapshot::default(),
            change_time: now,
            initialized: false,
        }
    }

    /// Update cursor state. Returns `true` if position or shape changed.
    pub fn update(
        &mut self,
        point: Point<usize>,
        shape: CursorShape,
        color: Rgb,
        width: u32,
        size_info: &SizeInfo,
        start_time: Instant,
    ) -> bool {
        let new_snapshot = Self::make_snapshot(point, shape, color, width, size_info);

        if !self.initialized {
            self.current = new_snapshot;
            self.previous = new_snapshot;
            self.change_time = start_time;
            self.initialized = true;
            return false;
        }

        let changed = (self.current.x - new_snapshot.x).abs() > 0.5
            || (self.current.y - new_snapshot.y).abs() > 0.5
            || (self.current.width - new_snapshot.width).abs() > 0.5
            || self.current.style != new_snapshot.style;

        if changed {
            self.previous = self.current;
            self.change_time = Instant::now();
        }

        self.current = new_snapshot;
        changed
    }

    fn make_snapshot(
        point: Point<usize>,
        shape: CursorShape,
        color: Rgb,
        width: u32,
        size_info: &SizeInfo,
    ) -> CursorSnapshot {
        let cell_width = size_info.cell_width();
        let cell_height = size_info.cell_height();
        let padding_x = size_info.padding_x();
        let padding_y = size_info.padding_y();
        let window_height = size_info.height();

        let terminal_x = point.column.0 as f32 * cell_width + padding_x;
        let terminal_y = point.line as f32 * cell_height + padding_y;

        // GL Y-up: convert top edge from screen-top-origin to screen-bottom-origin.
        // Ghostty convention: iCurrentCursor.y = top edge of cursor cell in GL coords.
        let gl_y = window_height - terminal_y;

        let cursor_width = cell_width * width as f32;
        let cursor_height = cell_height;

        let style = match shape {
            CursorShape::Block => 0,
            CursorShape::HollowBlock => 1,
            CursorShape::Beam => 2,
            CursorShape::Underline => 3,
            CursorShape::Hidden => 0,
        };

        CursorSnapshot {
            x: terminal_x,
            y: gl_y,
            width: cursor_width,
            height: cursor_height,
            color: [
                f32::from(color.r) / 255.0,
                f32::from(color.g) / 255.0,
                f32::from(color.b) / 255.0,
                1.0,
            ],
            style,
        }
    }

    /// Reset the change timestamp, e.g. when the shader clock restarts.
    pub fn reset(&mut self, now: Instant) {
        self.change_time = now;
    }

    pub fn current(&self) -> &CursorSnapshot {
        &self.current
    }

    pub fn previous(&self) -> &CursorSnapshot {
        &self.previous
    }

    pub fn change_time(&self) -> Instant {
        self.change_time
    }
}
