//! Configuration file handling

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

/// Configuration loaded from file
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Config {
    /// Custom prompt regex pattern
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Preset name to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
}

impl Config {
    /// Load configuration from ~/.config/tmux-snag/config.toml
    /// (or $XDG_CONFIG_HOME/tmux-snag/config.toml if set)
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

    /// Save configuration to disk
    pub fn save(&self) -> Result<PathBuf> {
        let path = Self::config_path()
            .ok_or_else(|| anyhow::anyhow!("Could not determine config path"))?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("Failed to create config directory")?;
        }

        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(&path, content).context("Failed to write config file")?;

        Ok(path)
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

        Some(config_dir.join("tmux-snag").join("config.toml"))
    }
}
