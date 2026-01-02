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
                endpoint: "http://localhost:8090/ingest".to_string(),
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
            // Try to copy template from ~/.claude/devlog-config.toml
            if let Ok(template_path) = Self::template_path() {
                if template_path.exists() {
                    return Self::create_from_template(&template_path, &config_path);
                }
            }

            // No template found, create default config
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

    fn create_from_template(template_path: &PathBuf, config_path: &PathBuf) -> Result<Self> {
        eprintln!("Copying config template from: {}", template_path.display());

        // Ensure directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
        }

        // Copy template to config location
        fs::copy(template_path, config_path)
            .with_context(|| format!("Failed to copy template from {}", template_path.display()))?;

        eprintln!("Created config file at: {}", config_path.display());

        // Load and return the config
        let content = fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config from {}", config_path.display()))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {}", config_path.display()))?;

        Ok(config)
    }

    fn template_path() -> Result<PathBuf> {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .context("Neither USERPROFILE nor HOME environment variable is set")?;

        Ok(PathBuf::from(home).join(".claude").join("devlog-config.toml"))
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

        // Add helpful header comment for first-time users
        let content_with_header = format!(
            "# Devlog Configuration\n\
             # To enable automatic push to central server:\n\
             # 1. Set enabled = true\n\
             # 2. Update endpoint to your server URL (e.g., http://YOUR_SERVER:8090/ingest)\n\
             \n{}",
            content
        );

        fs::write(&config_path, content_with_header)
            .with_context(|| format!("Failed to write config to {}", config_path.display()))?;

        eprintln!("Created config file at: {}", config_path.display());
        eprintln!("Edit this file to enable push to central server");

        Ok(())
    }

    fn config_path() -> Result<PathBuf> {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .context("Neither USERPROFILE nor HOME environment variable is set")?;

        Ok(PathBuf::from(home).join(".devlog").join("config.toml"))
    }
}
