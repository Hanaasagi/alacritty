use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer, Serialize};

use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};

/// Kitty graphics protocol configuration.
#[derive(ConfigDeserialize, Serialize, Default, Clone, Debug, PartialEq)]
pub struct GraphicsConfig {
    /// Enable Kitty graphics protocol support.
    pub enabled: GraphicsEnabled,

    /// Maximum memory for stored images in MiB.
    pub max_memory: GraphicsMaxMemory,
}

impl GraphicsConfig {
    #[inline]
    pub fn enabled(&self) -> bool {
        matches!(self.enabled, GraphicsEnabled::True)
    }

    #[inline]
    pub fn max_memory_bytes(&self) -> usize {
        self.max_memory.0.saturating_mul(1024 * 1024) as usize
    }
}

#[derive(SerdeReplace, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsEnabled {
    /// Always enabled.
    True,
    /// Always disabled.
    False,
}

impl Default for GraphicsEnabled {
    fn default() -> Self {
        GraphicsEnabled::True
    }
}

impl<'de> Deserialize<'de> for GraphicsEnabled {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Bool(bool),
            Str(String),
        }

        match Repr::deserialize(deserializer)? {
            Repr::Bool(true) => Ok(Self::True),
            Repr::Bool(false) => Ok(Self::False),
            Repr::Str(value) => match value.to_lowercase().as_str() {
                "true" => Ok(Self::True),
                "false" => Ok(Self::False),
                _ => Err(SerdeError::custom(format!(
                    "unknown variant `{value}`, expected true or false"
                ))),
            },
        }
    }
}

/// Maximum graphics image storage in MiB.
#[derive(SerdeReplace, Serialize, Copy, Clone, Debug, PartialEq, Eq)]
pub struct GraphicsMaxMemory(pub u64);

impl Default for GraphicsMaxMemory {
    fn default() -> Self {
        Self(320)
    }
}

impl<'de> Deserialize<'de> for GraphicsMaxMemory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mib = u64::deserialize(deserializer)?;
        if mib == 0 {
            Err(SerdeError::custom("graphics max_memory must be at least 1 MiB"))
        } else {
            Ok(Self(mib))
        }
    }
}
