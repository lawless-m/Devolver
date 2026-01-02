use crate::config::Config;
use crate::output::DevlogOutput;
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use std::time::Duration;

/// Push a devlog session to the central endpoint
pub fn push_session(output: &DevlogOutput) -> Result<()> {
    let config = Config::load()?;

    let push_config = match config.push {
        Some(ref pc) if pc.enabled => pc,
        Some(_) => {
            eprintln!("Push is disabled in config");
            return Ok(());
        }
        None => {
            eprintln!("No push config found");
            return Ok(());
        }
    };

    eprintln!("Pushing session to: {}", push_config.endpoint);

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("Failed to create HTTP client")?;

    let response = client
        .post(&push_config.endpoint)
        .json(output)
        .send()
        .with_context(|| format!("Failed to push to {}", push_config.endpoint))?;

    if response.status().is_success() {
        eprintln!("Session pushed successfully");
        Ok(())
    } else {
        anyhow::bail!(
            "Push failed with status {}: {}",
            response.status(),
            response.text().unwrap_or_else(|_| "unknown error".to_string())
        )
    }
}
