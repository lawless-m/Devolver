use crate::git::GitInfo;
use crate::parser::ConversationEntry;
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct DevlogOutput {
    pub schema_version: String,
    pub session_id: String,
    pub timestamp: String,
    pub machine_id: String,
    pub project_dir: String,
    pub git: Option<GitInfo>,
    pub conversation: Vec<ConversationEntry>,
}

/// Write the devlog output to the .devlog directory
pub fn write_output(output: &DevlogOutput) -> Result<PathBuf> {
    // Determine output directory
    let output_dir = get_output_dir(&output.project_dir)?;

    // Ensure directory exists
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("Failed to create output directory: {}", output_dir.display()))?;

    // Self-register .devlog in the project's .gitignore so the captured logs
    // never show up as untracked. Only inside a git repo, and best-effort — a
    // .gitignore failure must not abort the ingest.
    if output.git.is_some() {
        if let Err(e) = ensure_gitignored(&output.project_dir) {
            eprintln!("Warning: could not update .gitignore: {}", e);
        }
    }

    // Generate filename: YYYY-MM-DD-HHMMSS-<session_id_short>.json
    let filename = generate_filename(&output.session_id);
    let output_path = output_dir.join(&filename);

    // Serialize to JSON
    let json = serde_json::to_string_pretty(output).context("Failed to serialize output")?;

    // Write to file
    fs::write(&output_path, json)
        .with_context(|| format!("Failed to write output file: {}", output_path.display()))?;

    eprintln!("Wrote devlog to: {}", output_path.display());
    Ok(output_path)
}

fn get_output_dir(project_dir: &str) -> Result<PathBuf> {
    let mut path = PathBuf::from(project_dir);
    path.push(".devlog");
    Ok(path)
}

/// Ensure the project's `.gitignore` ignores the `.devlog` directory.
/// Idempotent: a no-op if an equivalent entry is already present (`.devlog`,
/// `/.devlog`, `.devlog/`). Creates the file if it doesn't exist.
fn ensure_gitignored(project_dir: &str) -> Result<()> {
    let gitignore = PathBuf::from(project_dir).join(".gitignore");
    let existing = fs::read_to_string(&gitignore).unwrap_or_default();

    let already = existing.lines().any(|line| {
        line.trim().trim_start_matches('/').trim_end_matches('/') == ".devlog"
    });
    if already {
        return Ok(());
    }

    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(".devlog/\n");
    fs::write(&gitignore, content)
        .with_context(|| format!("Failed to write {}", gitignore.display()))?;
    eprintln!("Added .devlog/ to {}", gitignore.display());
    Ok(())
}

fn generate_filename(session_id: &str) -> String {
    let now = Utc::now();
    let date_part = now.format("%Y-%m-%d-%H%M%S");

    // Shorten session_id for filename
    let short_id: String = session_id.chars().take(8).collect();

    format!("{}-{}.json", date_part, short_id)
}

/// Get a stable machine identifier (hostname)
pub fn get_machine_id() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::ensure_gitignored;
    use std::fs;

    fn tmp_project(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("devlog_gi_{}_{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_gitignore_when_absent() {
        let dir = tmp_project("absent");
        ensure_gitignored(dir.to_str().unwrap()).unwrap();
        assert_eq!(fs::read_to_string(dir.join(".gitignore")).unwrap(), ".devlog/\n");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn appends_with_newline_when_missing_trailing() {
        let dir = tmp_project("append");
        fs::write(dir.join(".gitignore"), "/target").unwrap();
        ensure_gitignored(dir.to_str().unwrap()).unwrap();
        assert_eq!(fs::read_to_string(dir.join(".gitignore")).unwrap(), "/target\n.devlog/\n");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn idempotent_for_each_equivalent_form() {
        for form in [".devlog", "/.devlog", ".devlog/"] {
            let dir = tmp_project("idem");
            fs::write(dir.join(".gitignore"), format!("{form}\n")).unwrap();
            ensure_gitignored(dir.to_str().unwrap()).unwrap();
            assert_eq!(
                fs::read_to_string(dir.join(".gitignore")).unwrap(),
                format!("{form}\n"),
                "should not re-add for form {form}"
            );
            fs::remove_dir_all(&dir).unwrap();
        }
    }
}
