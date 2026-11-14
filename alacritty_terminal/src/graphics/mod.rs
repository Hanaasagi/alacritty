//! Kitty graphics protocol support.

pub mod command;
pub mod exec;
pub mod image;
pub mod placement;
pub mod storage;
pub mod unicode_placeholder;

pub use command::{
    Action, AnimationControl, Command, Compression, CursorMovement, DeleteAction, DisplayCmd,
    Format, Medium, ParseError, Parser, Quiet, Response, Transmission, VirtualPlacement,
};
pub use exec::{ExecResult, execute};
pub use image::{Image, ImageError, LoadingImage};
pub use placement::{Placement, PlacementLocation};
pub use storage::{ImageStorage, StorageError};

/// Terminal-side Kitty graphics state.
pub struct GraphicsState {
    pub storage: ImageStorage,
    pub placements: Vec<Placement>,
    pub loading: Option<LoadingImage>,
    pub parser: command::Parser,
    /// Monotonic change counter. Bumped on every structural or content
    /// mutation so consumers can detect changes by comparing their cached
    /// snapshot against the current value. Never reset to zero.
    dirty: u64,
}

impl GraphicsState {
    pub fn new() -> Self {
        Self::with_storage_limit(320 * 1024 * 1024)
    }

    pub fn with_storage_limit(limit: usize) -> Self {
        Self {
            storage: ImageStorage::with_limit(limit),
            placements: Vec::new(),
            loading: None,
            parser: command::Parser::new(),
            dirty: 0,
        }
    }

    /// Bump the dirty counter for geometry-only changes (scroll, resize,
    /// history erasure) that move or remove placements without changing
    /// image content.
    #[inline]
    pub fn mark_dirty(&mut self) {
        self.dirty = self.dirty.wrapping_add(1);
    }

    /// Bump the dirty counter for content mutations (image add/remove,
    /// placement add/remove, pixel data change). Currently equivalent to
    /// `mark_dirty`; a separate `generation` counter for texture cache
    /// invalidation can be added later when animation support lands.
    #[inline]
    pub fn mark_mutated(&mut self) {
        self.mark_dirty();
    }

    /// Current dirty counter value. Consumers store this and compare with
    /// future values to detect changes — **never** reset it.
    #[inline]
    pub fn dirty(&self) -> u64 {
        self.dirty
    }
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self::new()
    }
}
