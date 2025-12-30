use crate::git::GitInfo;
use crate::parser::ConversationEntry;
use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct DevlogOutput {
    pub schema_version: String,
    pub session_id: String,
    pub timestamp: String,
    pub project_dir: String,
    pub git: Option<GitInfo>,
    pub conversation: Vec<ConversationEntry>,
}

/// Write the devlog output to the .devlog directory
pub fn write_output(output: &DevlogOutput) -> Result<()> {
    // Determine output directory
    let output_dir = get_output_dir(&output.project_dir)?;

    // Ensure directory exists
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("Failed to create output directory: {}", output_dir.display()))?;

    // Generate filename: YYYY-MM-DD-HHMMSS-<session_id_short>.json
    let filename = generate_filename(&output.session_id);
    let output_path = output_dir.join(&filename);

    // Serialize to JSON
    let json = serde_json::to_string_pretty(output).context("Failed to serialize output")?;

    // Write to file
    fs::write(&output_path, json)
        .with_context(|| format!("Failed to write output file: {}", output_path.display()))?;

    eprintln!("Wrote devlog to: {}", output_path.display());
    Ok(())
}

fn get_output_dir(project_dir: &str) -> Result<PathBuf> {
    let mut path = PathBuf::from(project_dir);
    path.push(".devlog");
    Ok(path)
}

fn generate_filename(session_id: &str) -> String {
    let now = Utc::now();
    let date_part = now.format("%Y-%m-%d-%H%M%S");

    // Shorten session_id for filename
    let short_id: String = session_id.chars().take(8).collect();

    format!("{}-{}.json", date_part, short_id)
}
