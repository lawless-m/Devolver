use crate::output::DevlogOutput;
use crate::parser::ConversationEntry;
use anyhow::Result;
use std::fs;
use std::path::Path;

/// A single search result with context
pub struct SearchResult {
    pub machine: String,
    pub project: String,
    pub session_id: String,
    pub session_file: String,
    pub timestamp: String,
    pub entry_type: String,
    pub snippet: String,
    pub query: String,
}

/// What to search through
#[derive(Clone, Copy, Default)]
pub enum SearchScope {
    PromptsOnly,
    #[default]
    Conversations,
    Everything,
}

impl SearchScope {
    pub fn from_str(s: &str) -> Self {
        match s {
            "prompts" => Self::PromptsOnly,
            "all" => Self::Everything,
            _ => Self::Conversations,
        }
    }
}

/// Search through devlog files for matching content
pub fn search_devlogs(
    storage_dir: &Path,
    query: &str,
    scope: SearchScope,
    days: Option<u32>,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let cutoff = days.map(|d| chrono::Utc::now() - chrono::Duration::days(d as i64));
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    if !storage_dir.exists() {
        return Ok(results);
    }

    // Walk storage directory: storage_dir/machine/project/*.json
    'outer: for machine_entry in fs::read_dir(storage_dir)? {
        let machine_entry = machine_entry?;
        let machine_path = machine_entry.path();
        if !machine_path.is_dir() {
            continue;
        }
        let machine = machine_entry.file_name().to_string_lossy().to_string();

        for project_entry in fs::read_dir(&machine_path)? {
            let project_entry = project_entry?;
            let project_path = project_entry.path();
            if !project_path.is_dir() {
                continue;
            }
            let project = project_entry.file_name().to_string_lossy().to_string();

            for file_entry in fs::read_dir(&project_path)? {
                let file_entry = file_entry?;
                let file_path = file_entry.path();

                if file_path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(devlog) = read_devlog(&file_path) {
                        // Check date filter
                        if let Some(ref cutoff) = cutoff {
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&devlog.timestamp)
                            {
                                if dt < *cutoff {
                                    continue;
                                }
                            }
                        }

                        let session_file =
                            file_path.file_name().unwrap_or_default().to_string_lossy().to_string();

                        // Search conversation entries
                        for entry in &devlog.conversation {
                            if let Some(result) = search_entry(
                                entry,
                                &query_lower,
                                query,
                                scope,
                                &machine,
                                &project,
                                &devlog.session_id,
                                &session_file,
                                &devlog.timestamp,
                            ) {
                                results.push(result);
                                if results.len() >= limit {
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Sort by timestamp descending (most recent first)
    results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(results)
}

fn read_devlog(path: &Path) -> Result<DevlogOutput> {
    let content = fs::read_to_string(path)?;
    let devlog: DevlogOutput = serde_json::from_str(&content)?;
    Ok(devlog)
}

fn search_entry(
    entry: &ConversationEntry,
    query_lower: &str,
    query_original: &str,
    scope: SearchScope,
    machine: &str,
    project: &str,
    session_id: &str,
    session_file: &str,
    timestamp: &str,
) -> Option<SearchResult> {
    let (entry_type, content) = match entry {
        ConversationEntry::User { content, .. } => ("user", content.as_str()),
        ConversationEntry::Assistant { content, .. } => {
            if matches!(scope, SearchScope::PromptsOnly) {
                return None;
            }
            ("assistant", content.as_str())
        }
        ConversationEntry::ToolSummary { actions } => {
            if !matches!(scope, SearchScope::Everything) {
                return None;
            }
            // Join actions for searching
            let joined = actions.join(" | ");
            if joined.to_lowercase().contains(query_lower) {
                return Some(SearchResult {
                    machine: machine.to_string(),
                    project: project.to_string(),
                    session_id: session_id.to_string(),
                    session_file: session_file.to_string(),
                    timestamp: timestamp.to_string(),
                    entry_type: "tool".to_string(),
                    snippet: create_snippet(&joined, query_lower),
                    query: query_original.to_string(),
                });
            }
            return None;
        }
    };

    let content_lower = content.to_lowercase();
    if content_lower.contains(query_lower) {
        Some(SearchResult {
            machine: machine.to_string(),
            project: project.to_string(),
            session_id: session_id.to_string(),
            session_file: session_file.to_string(),
            timestamp: timestamp.to_string(),
            entry_type: entry_type.to_string(),
            snippet: create_snippet(content, query_lower),
            query: query_original.to_string(),
        })
    } else {
        None
    }
}

/// Create a snippet with context around the match
fn create_snippet(content: &str, query_lower: &str) -> String {
    let content_lower = content.to_lowercase();
    let match_pos = match content_lower.find(query_lower) {
        Some(pos) => pos,
        None => return content.chars().take(200).collect(),
    };

    let context_chars = 80;

    // Find start position (try to start at word boundary)
    let start = if match_pos > context_chars {
        let candidate = match_pos - context_chars;
        // Find next space after candidate
        content[candidate..]
            .find(' ')
            .map(|i| candidate + i + 1)
            .unwrap_or(candidate)
    } else {
        0
    };

    // Find end position
    let end_candidate = match_pos + query_lower.len() + context_chars;
    let end = if end_candidate < content.len() {
        // Find previous space before end
        content[..end_candidate]
            .rfind(' ')
            .unwrap_or(end_candidate)
    } else {
        content.len()
    };

    let mut snippet = String::new();
    if start > 0 {
        snippet.push_str("...");
    }
    snippet.push_str(content[start..end].trim());
    if end < content.len() {
        snippet.push_str("...");
    }

    snippet
}
