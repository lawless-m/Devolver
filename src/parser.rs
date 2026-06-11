use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

/// Raw entry from Claude Code JSONL file
#[derive(Debug, Deserialize)]
pub struct RawEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub message: Option<MessageContent>,
    pub tool: Option<String>,
    pub input: Option<serde_json::Value>,
    pub timestamp: Option<String>,
    // Additional fields we might encounter
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Object(MessageObject),
    String(String),
}

#[derive(Debug, Deserialize)]
pub struct MessageObject {
    pub content: Option<ContentType>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ContentType {
    String(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
    pub name: Option<String>,
    pub input: Option<serde_json::Value>,
}

/// Token usage information from Claude API
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
}

/// Output conversation entry
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ConversationEntry {
    #[serde(rename = "user")]
    User {
        timestamp: Option<String>,
        content: String,
    },
    #[serde(rename = "assistant")]
    Assistant {
        timestamp: Option<String>,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
    },
    #[serde(rename = "tool_summary")]
    ToolSummary { actions: Vec<String> },
}

/// Open a file, retrying briefly on Windows sharing/lock violations.
///
/// Claude Code keeps the session JSONL open while writing it, which can yield
/// ERROR_SHARING_VIOLATION (os error 32) / ERROR_LOCK_VIOLATION (os error 33)
/// for a brief window when a hook (resume/compact/Stop) reads it mid-write.
/// Retry a few times so the transient lock clears instead of failing the ingest.
fn open_with_retry(path: &Path) -> std::io::Result<File> {
    const MAX_ATTEMPTS: u32 = 10;
    let mut attempt = 0;
    loop {
        match File::open(path) {
            Ok(file) => return Ok(file),
            Err(e) => {
                attempt += 1;
                let transient = matches!(e.raw_os_error(), Some(32) | Some(33));
                if transient && attempt < MAX_ATTEMPTS {
                    sleep(Duration::from_millis(50));
                    continue;
                }
                return Err(e);
            }
        }
    }
}

/// Parse a JSONL session file into raw entries
pub fn parse_session_file(path: &Path) -> Result<Vec<RawEntry>> {
    let file = open_with_retry(path).context("Failed to open session file")?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("Failed to read line {}", line_num + 1))?;
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<RawEntry>(&line) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                eprintln!(
                    "Warning: Failed to parse line {}: {} (skipping)",
                    line_num + 1,
                    e
                );
            }
        }
    }

    Ok(entries)
}

/// Filter and transform raw entries into conversation entries
pub fn filter_to_conversation(entries: Vec<RawEntry>) -> Vec<ConversationEntry> {
    let mut conversation = Vec::new();
    let mut pending_tools: Vec<String> = Vec::new();

    for entry in entries {
        match entry.entry_type.as_str() {
            "human" | "user" => {
                // Flush pending tools
                flush_tool_summary(&mut conversation, &mut pending_tools);

                let content = extract_content(&entry);
                if !content.is_empty() {
                    conversation.push(ConversationEntry::User {
                        timestamp: entry.timestamp,
                        content,
                    });
                }
            }
            "assistant" => {
                // Flush pending tools before new assistant message
                flush_tool_summary(&mut conversation, &mut pending_tools);

                let content = extract_content(&entry);
                if !content.is_empty() {
                    let usage = extract_usage(&entry);
                    conversation.push(ConversationEntry::Assistant {
                        timestamp: entry.timestamp,
                        content,
                        usage,
                    });
                }

                // Check for tool_use blocks in message content
                if let Some(MessageContent::Object(ref msg)) = entry.message {
                    if let Some(ref content) = msg.content {
                        if let ContentType::Blocks(blocks) = content {
                            for block in blocks {
                                if block.block_type == "tool_use" {
                                    if let Some(action) = summarize_tool_use_from_block(block) {
                                        pending_tools.push(action);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "tool_use" => {
                let action = summarize_tool_use(&entry);
                if let Some(action) = action {
                    pending_tools.push(action);
                }
            }
            "tool_result" => {
                // Skip tool results - we only capture summaries
            }
            _ => {
                // Skip other types (system, etc.)
            }
        }
    }

    // Flush any remaining tools
    flush_tool_summary(&mut conversation, &mut pending_tools);

    conversation
}

fn flush_tool_summary(conversation: &mut Vec<ConversationEntry>, pending_tools: &mut Vec<String>) {
    if !pending_tools.is_empty() {
        conversation.push(ConversationEntry::ToolSummary {
            actions: pending_tools.drain(..).collect(),
        });
    }
}

fn extract_content(entry: &RawEntry) -> String {
    if let Some(ref msg) = entry.message {
        match msg {
            MessageContent::String(s) => return s.clone(),
            MessageContent::Object(obj) => {
                if let Some(ref content) = obj.content {
                    match content {
                        ContentType::String(s) => return s.clone(),
                        ContentType::Blocks(blocks) => {
                            let texts: Vec<String> = blocks
                                .iter()
                                .filter(|b| b.block_type == "text")
                                .filter_map(|b| b.text.clone())
                                .collect();
                            return texts.join("\n");
                        }
                    }
                }
            }
        }
    }

    // Try to extract from extra fields
    if let Some(content) = entry.extra.get("content") {
        if let Some(s) = content.as_str() {
            return s.to_string();
        }
    }

    String::new()
}

fn extract_usage(entry: &RawEntry) -> Option<TokenUsage> {
    // Look for usage in message.usage or extra.message.usage
    if let Some(MessageContent::Object(ref msg)) = entry.message {
        if let Some(usage_val) = msg.extra.get("usage") {
            // Try to deserialize the usage object
            if let Ok(usage) = serde_json::from_value::<TokenUsage>(usage_val.clone()) {
                return Some(usage);
            }
        }
    }

    None
}

fn summarize_tool_use(entry: &RawEntry) -> Option<String> {
    let tool_name = entry.tool.as_ref()?;
    let input = entry.input.as_ref();

    Some(format_tool_action(tool_name, input))
}

fn summarize_tool_use_from_block(block: &ContentBlock) -> Option<String> {
    let tool_name = block.name.as_ref()?;
    Some(format_tool_action(tool_name, block.input.as_ref()))
}

fn format_tool_action(tool_name: &str, input: Option<&serde_json::Value>) -> String {
    match tool_name {
        "Edit" | "MultiEdit" => {
            let path = extract_path(input);
            format!("edited {}", path)
        }
        "Write" => {
            let path = extract_path(input);
            format!("created {}", path)
        }
        "Read" => {
            let path = extract_path(input);
            format!("read {}", path)
        }
        "Bash" => {
            let cmd = extract_command(input);
            format!("ran {}", truncate(&cmd, 50))
        }
        "Glob" | "Grep" => {
            let pattern = extract_pattern(input);
            format!("searched for {}", truncate(&pattern, 40))
        }
        "Task" => "used subagent".to_string(),
        "WebSearch" => "used WebSearch".to_string(),
        "WebFetch" => {
            let url = extract_url(input);
            format!("fetched {}", truncate(&url, 40))
        }
        "TodoWrite" => "updated todo list".to_string(),
        _ => format!("used {}", tool_name),
    }
}

fn extract_path(input: Option<&serde_json::Value>) -> String {
    input
        .and_then(|v| {
            v.get("file_path")
                .or_else(|| v.get("path"))
                .and_then(|p| p.as_str())
        })
        .unwrap_or("<unknown>")
        .to_string()
}

fn extract_command(input: Option<&serde_json::Value>) -> String {
    input
        .and_then(|v| v.get("command").and_then(|c| c.as_str()))
        .unwrap_or("<command>")
        .to_string()
}

fn extract_pattern(input: Option<&serde_json::Value>) -> String {
    input
        .and_then(|v| v.get("pattern").and_then(|p| p.as_str()))
        .unwrap_or("<pattern>")
        .to_string()
}

fn extract_url(input: Option<&serde_json::Value>) -> String {
    input
        .and_then(|v| v.get("url").and_then(|u| u.as_str()))
        .unwrap_or("<url>")
        .to_string()
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
