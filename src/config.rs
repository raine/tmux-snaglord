//! Configuration file handling

use anyhow::Result;
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;

/// Configuration loaded from file
#[derive(Debug, Deserialize, Default)]
pub struct Config {
    /// Custom prompt regex pattern
    pub prompt: Option<String>,
    /// Preset name to use
    pub preset: Option<String>,
}

impl Config {
    /// Load configuration from ~/.config/tmux-copy-tool/config.toml
    /// (or $XDG_CONFIG_HOME/tmux-copy-tool/config.toml if set)
    pub fn load() -> Result<Self> {
        if let Some(path) = Self::config_path()
            && path.exists()
        {
            let content = fs::read_to_string(&path)?;
            let config: Config = toml::from_str(&content)?;
            return Ok(config);
        }
        Ok(Config::default())
    }

    /// Get the path to the config file
    pub fn config_path() -> Option<PathBuf> {
        let config_dir = env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .ok()
            .or_else(|| {
                env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".config"))
            })?;

        Some(config_dir.join("tmux-copy-tool").join("config.toml"))
    }
}
