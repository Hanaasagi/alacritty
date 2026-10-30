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
    /// Whether graphics state needs to be synced to the renderer.
    pub dirty: bool,
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
            dirty: false,
        }
    }
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self::new()
    }
}
