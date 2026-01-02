mod parser;
mod git;
mod output;
mod config;
mod push;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "devlog")]
#[command(about = "Claude Code session ingester - captures conversations for later reference")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ingest a Claude Code session JSONL file
    Ingest {
        /// Path to the session JSONL file (optional - will try stdin or find most recent)
        path: Option<PathBuf>,
    },
    /// Push the most recent session to the central endpoint
    Push {
        /// Path to the devlog JSON file to push (optional - will find most recent)
        path: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Ingest { path } => {
            ingest_session(path)?;
        }
        Commands::Push { path } => {
            push_session(path)?;
        }
    }

    Ok(())
}

fn ingest_session(path: Option<PathBuf>) -> Result<()> {
    // Determine the session file path
    let session_path = match path {
        Some(p) => p,
        None => find_session_from_stdin_or_recent()?,
    };

    eprintln!("Ingesting session from: {}", session_path.display());

    // Parse the JSONL file
    let entries = parser::parse_session_file(&session_path)
        .with_context(|| format!("Failed to parse session file: {}", session_path.display()))?;

    // Filter and transform to conversation
    let conversation = parser::filter_to_conversation(entries);

    // Get git metadata
    let git_info = git::get_git_metadata();

    // Get project directory
    let project_dir = std::env::var("CLAUDE_PROJECT_DIR")
        .unwrap_or_else(|_| std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string()));

    // Extract session ID from filename or generate one
    let session_id = extract_session_id(&session_path);

    // Build output
    let output = output::DevlogOutput {
        schema_version: "1.0".to_string(),
        session_id,
        timestamp: chrono::Utc::now().to_rfc3339(),
        machine_id: output::get_machine_id(),
        project_dir,
        git: git_info,
        conversation,
    };

    // Write output
    let _output_path = output::write_output(&output)?;

    eprintln!("Session ingested successfully");

    // Auto-push if enabled
    if let Err(e) = push::push_session(&output) {
        eprintln!("Warning: Failed to push session: {}", e);
        // Don't fail the whole ingest if push fails
    }

    Ok(())
}

fn find_session_from_stdin_or_recent() -> Result<PathBuf> {
    // First, try to read from stdin (hook input)
    use std::io::{self, BufRead};

    let stdin = io::stdin();
    let mut stdin_content = String::new();

    // Try non-blocking read from stdin
    if atty::is(atty::Stream::Stdin) {
        // No stdin piped, look for recent session
    } else {
        for line in stdin.lock().lines() {
            if let Ok(line) = line {
                stdin_content.push_str(&line);
            }
        }
    }

    // Try to parse stdin as JSON with transcript path
    if !stdin_content.is_empty() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdin_content) {
            if let Some(path) = json.get("transcript_path").and_then(|v| v.as_str()) {
                return Ok(PathBuf::from(path));
            }
            if let Some(path) = json.get("session_file").and_then(|v| v.as_str()) {
                return Ok(PathBuf::from(path));
            }
        }
    }

    // Fallback: find most recent session in ~/.claude/
    find_most_recent_session()
}

fn find_most_recent_session() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let claude_dir = PathBuf::from(home).join(".claude").join("projects");

    if !claude_dir.exists() {
        anyhow::bail!("No Claude directory found at {}", claude_dir.display());
    }

    let mut most_recent: Option<(PathBuf, std::time::SystemTime)> = None;

    fn find_jsonl_files(dir: &PathBuf, most_recent: &mut Option<(PathBuf, std::time::SystemTime)>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    find_jsonl_files(&path, most_recent);
                } else if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                    if let Ok(meta) = path.metadata() {
                        if let Ok(modified) = meta.modified() {
                            match most_recent {
                                Some((_, ref time)) if modified > *time => {
                                    *most_recent = Some((path, modified));
                                }
                                None => {
                                    *most_recent = Some((path, modified));
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    find_jsonl_files(&claude_dir, &mut most_recent);

    most_recent
        .map(|(path, _)| path)
        .context("No session files found")
}

fn extract_session_id(path: &PathBuf) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("session_{}", chrono::Utc::now().timestamp()))
}

fn push_session(path: Option<PathBuf>) -> Result<()> {
    // Find the devlog file to push
    let devlog_path = match path {
        Some(p) => p,
        None => find_most_recent_devlog()?,
    };

    eprintln!("Pushing devlog from: {}", devlog_path.display());

    // Read the devlog file
    let content = std::fs::read_to_string(&devlog_path)
        .with_context(|| format!("Failed to read devlog file: {}", devlog_path.display()))?;

    let output: output::DevlogOutput = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse devlog file: {}", devlog_path.display()))?;

    // Push it
    push::push_session(&output)?;

    Ok(())
}

fn find_most_recent_devlog() -> Result<PathBuf> {
    // Look in current directory's .devlog folder
    let current_dir = std::env::current_dir().context("Failed to get current directory")?;
    let devlog_dir = current_dir.join(".devlog");

    if !devlog_dir.exists() {
        anyhow::bail!("No .devlog directory found in current directory");
    }

    let mut most_recent: Option<(PathBuf, std::time::SystemTime)> = None;

    for entry in std::fs::read_dir(&devlog_dir)
        .with_context(|| format!("Failed to read directory: {}", devlog_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(meta) = path.metadata() {
                if let Ok(modified) = meta.modified() {
                    match most_recent {
                        Some((_, ref time)) if modified > *time => {
                            most_recent = Some((path, modified));
                        }
                        None => {
                            most_recent = Some((path, modified));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    most_recent
        .map(|(path, _)| path)
        .context("No devlog JSON files found in .devlog directory")
}
