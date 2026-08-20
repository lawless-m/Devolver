use crate::git::GitInfo;
use crate::parser::ConversationEntry;
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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

/// Write the devlog output. Preferred location is the project's own `.devlog/`
/// so the narrative lives alongside the code. If that directory can't be written
/// — e.g. the session ran in a root-owned path like `/etc/...` — fall back to
/// `~/.devlog/sessions/<project>/` so the session is never lost.
pub fn write_output(output: &DevlogOutput) -> Result<PathBuf> {
    let filename = generate_filename(&output.session_id);
    let json = serde_json::to_string_pretty(output).context("Failed to serialize output")?;

    // Preferred: alongside the code, in the project's own .devlog/.
    let primary = get_output_dir(&output.project_dir)?;
    match try_write(&primary, &filename, &json) {
        Ok(path) => {
            // Self-register .devlog in the project's .gitignore so the captured
            // logs never show up as untracked. In-repo only, and best-effort.
            if output.git.is_some() {
                if let Err(e) = ensure_gitignored(&output.project_dir) {
                    eprintln!("Warning: could not update .gitignore: {}", e);
                }
            }
            eprintln!("Wrote devlog to: {}", path.display());
            Ok(path)
        }
        Err(e) => {
            // Project dir not writable (permission denied, read-only fs, …).
            // Never lose the session: write to a home-based fallback instead.
            let fallback = fallback_output_dir(&output.project_dir)?;
            eprintln!(
                "Warning: could not write to {} ({}); falling back to {}",
                primary.display(),
                e,
                fallback.display()
            );
            let path = try_write(&fallback, &filename, &json)?;
            eprintln!("Wrote devlog to: {}", path.display());
            Ok(path)
        }
    }
}

/// Create `dir` (if needed) and write `filename` into it. Returns the full path.
fn try_write(dir: &Path, filename: &str, json: &str) -> Result<PathBuf> {
    fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create output directory: {}", dir.display()))?;
    let path = dir.join(filename);
    fs::write(&path, json)
        .with_context(|| format!("Failed to write output file: {}", path.display()))?;
    Ok(path)
}

fn get_output_dir(project_dir: &str) -> Result<PathBuf> {
    let mut path = PathBuf::from(project_dir);
    path.push(".devlog");
    Ok(path)
}

/// Guaranteed-writable fallback: `~/.devlog/sessions/<munged-project-dir>/`.
/// The project path is flattened to a single directory name (like Claude Code
/// munges cwd), so sessions from different projects don't collide.
fn fallback_output_dir(project_dir: &str) -> Result<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .context("Neither USERPROFILE nor HOME environment variable is set")?;
    let munged: String = project_dir
        .trim()
        .chars()
        // The drive colon has to go too: "C:-Windows" is not a legal directory
        // name, and Path::join treats it as drive-relative and drops the home
        // prefix. Same flattening Claude Code applies to its own project dirs.
        .map(|c| if c == '/' || c == '\\' || c == ':' { '-' } else { c })
        .collect();
    let munged = munged.trim_matches('-');
    let munged = if munged.is_empty() { "root" } else { munged };
    Ok(PathBuf::from(home)
        .join(".devlog")
        .join("sessions")
        .join(munged))
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
