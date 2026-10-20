use std::path::PathBuf;

use alacritty_config_derive::ConfigDeserialize;
use serde::Serialize;

/// Custom shader configuration.
#[derive(ConfigDeserialize, Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ShaderConfig {
    /// Custom shader entries.
    ///
    /// Each entry may be a built-in shader name (e.g. `cursor_sweep`), a file path, or a mix of
    /// both. Paths may be absolute, relative to the configuration file directory, or use `~` for
    /// the user's home directory.
    #[serde(default)]
    pub custom_shader: Vec<PathBuf>,

    /// Shader animation redraw strategy.
    #[serde(default)]
    pub animation: ShaderAnimation,
}

/// Shader animation redraw strategy.
#[derive(ConfigDeserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShaderAnimation {
    /// Do not redraw unless the terminal updates.
    #[default]
    Off,
    /// Redraw while a cursor shader animation is likely in progress.
    WhenNeeded,
    /// Redraw continuously while the window is focused.
    Focused,
    /// Redraw continuously, even when unfocused.
    Always,
}
