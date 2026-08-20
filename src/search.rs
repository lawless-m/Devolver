use crate::output::DevlogOutput;
use crate::parser::ConversationEntry;
use anyhow::Result;
use std::fs;
use std::path::Path;

/// A single search result with context
pub struct SearchResult {
    pub machine: String,
    pub project: String,
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

                        // Search conversation entries
                        let timestamps = entry_timestamps(&devlog);
                        for (entry, ts) in devlog.conversation.iter().zip(timestamps) {
                            // The file-level check above only rules out files whose
                            // whole history predates the cutoff; a recently ingested
                            // file can still hold old conversation, so filter again
                            // on when each entry was actually written.
                            if let Some(ref cutoff) = cutoff {
                                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
                                    if dt < *cutoff {
                                        continue;
                                    }
                                }
                            }
                            if let Some(result) = search_entry(
                                entry,
                                &query_lower,
                                query,
                                scope,
                                &machine,
                                &project,
                                ts,
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

/// When each entry was written. Tool summaries carry no timestamp of their own,
/// so they inherit the turn they follow; the file's ingest time is used only when
/// nothing in the conversation is timestamped. Re-ingesting an old session must
/// not re-date its contents.
fn entry_timestamps<'a>(devlog: &'a DevlogOutput) -> Vec<&'a str> {
    let first = devlog
        .conversation
        .iter()
        .find_map(|e| match e {
            ConversationEntry::User { timestamp, .. }
            | ConversationEntry::Assistant { timestamp, .. } => timestamp.as_deref(),
            ConversationEntry::ToolSummary { .. } => None,
        })
        .unwrap_or(&devlog.timestamp);

    let mut last = first;
    devlog
        .conversation
        .iter()
        .map(|entry| {
            if let ConversationEntry::User { timestamp, .. }
            | ConversationEntry::Assistant { timestamp, .. } = entry
            {
                if let Some(ts) = timestamp.as_deref() {
                    last = ts;
                }
            }
            last
        })
        .collect()
}

fn search_entry(
    entry: &ConversationEntry,
    query_lower: &str,
    query_original: &str,
    scope: SearchScope,
    machine: &str,
    project: &str,
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
            timestamp: timestamp.to_string(),
            entry_type: entry_type.to_string(),
            snippet: create_snippet(content, query_lower),
            query: query_original.to_string(),
        })
    } else {
        None
    }
}

/// Create a snippet with context around the match (char-safe)
fn create_snippet(content: &str, query_lower: &str) -> String {
    let content_lower = content.to_lowercase();

    // Find match position in chars (not bytes)
    let match_byte_pos = match content_lower.find(query_lower) {
        Some(pos) => pos,
        None => return content.chars().take(200).collect(),
    };

    // Convert byte position to char position
    let match_char_pos = content_lower[..match_byte_pos].chars().count();
    let query_char_len = query_lower.chars().count();
    let total_chars = content.chars().count();

    let context_chars = 80;

    // Calculate start/end in char positions
    let start_char = match_char_pos.saturating_sub(context_chars);
    let end_char = (match_char_pos + query_char_len + context_chars).min(total_chars);

    // Extract snippet using chars
    let snippet_chars: String = content
        .chars()
        .skip(start_char)
        .take(end_char - start_char)
        .collect();

    let mut snippet = String::new();
    if start_char > 0 {
        snippet.push_str("...");
    }
    snippet.push_str(snippet_chars.trim());
    if end_char < total_chars {
        snippet.push_str("...");
    }

    snippet
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A June conversation, re-ingested in July: entries must keep their own
    /// dates, and the tool summary must inherit the turn it follows rather than
    /// the ingest time.
    fn re_ingested_devlog() -> DevlogOutput {
        DevlogOutput {
            schema_version: "1.0".to_string(),
            session_id: "s1".to_string(),
            timestamp: "2026-07-13T15:23:12Z".to_string(),
            machine_id: "m1".to_string(),
            project_dir: "/p/proj".to_string(),
            git: None,
            conversation: vec![
                ConversationEntry::User {
                    timestamp: Some("2026-06-22T15:57:54Z".to_string()),
                    content: "fetch the T0 pdf".to_string(),
                },
                ConversationEntry::ToolSummary {
                    actions: vec!["read T0.pdf".to_string()],
                },
                ConversationEntry::Assistant {
                    timestamp: Some("2026-06-22T15:58:23Z".to_string()),
                    content: "done".to_string(),
                    usage: None,
                    model: None,
                },
            ],
        }
    }

    #[test]
    fn entries_keep_their_own_dates_when_re_ingested() {
        let devlog = re_ingested_devlog();
        let stamps = entry_timestamps(&devlog);
        assert_eq!(
            stamps,
            vec![
                "2026-06-22T15:57:54Z",
                "2026-06-22T15:57:54Z", // tool summary follows the prompt
                "2026-06-22T15:58:23Z",
            ]
        );
        assert!(!stamps.contains(&devlog.timestamp.as_str()));
    }

    #[test]
    fn file_timestamp_is_used_only_when_nothing_is_dated() {
        let mut devlog = re_ingested_devlog();
        devlog.conversation = vec![ConversationEntry::ToolSummary {
            actions: vec!["read T0.pdf".to_string()],
        }];
        assert_eq!(entry_timestamps(&devlog), vec!["2026-07-13T15:23:12Z"]);
    }
}
