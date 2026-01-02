use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub push: Option<PushConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PushConfig {
    pub endpoint: String,
    pub enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            push: Some(PushConfig {
                endpoint: "http://localhost:8080/ingest".to_string(),
                enabled: false,
            }),
        }
    }
}

impl Config {
    /// Load config from ~/.devlog/config.toml
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            // Create default config
            let default_config = Config::default();
            default_config.save()?;
            return Ok(default_config);
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config from {}", config_path.display()))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {}", config_path.display()))?;

        Ok(config)
    }

    /// Save config to ~/.devlog/config.toml
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        // Ensure directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
        }

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;

        fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config to {}", config_path.display()))?;

        Ok(())
    }

    fn config_path() -> Result<PathBuf> {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .context("Neither USERPROFILE nor HOME environment variable is set")?;

        Ok(PathBuf::from(home).join(".devlog").join("config.toml"))
    }
}
