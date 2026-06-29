use crate::output::DevlogOutput;
use crate::parser::ConversationEntry;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

/// SQLite stats index, derived from the raw devlog JSON files (which remain
/// the source of truth). Delete the .sqlite file and restart to rebuild.
///
/// Sessions are re-ingested on every resume/compact hook fire, and each push
/// stores a new JSON file containing the full transcript so far. Only the
/// newest ingest per (machine, project, session_id) is kept here, so tokens
/// are counted once instead of once per re-ingest.
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY,
    machine TEXT NOT NULL,
    project TEXT NOT NULL,
    session_id TEXT NOT NULL,
    ingested_at TEXT NOT NULL,
    last_activity TEXT NOT NULL,
    prompts INTEGER NOT NULL,
    tool_calls INTEGER NOT NULL,
    files_touched INTEGER NOT NULL,
    prompt_words INTEGER NOT NULL,
    response_words INTEGER NOT NULL,
    UNIQUE(machine, project, session_id)
);
CREATE TABLE IF NOT EXISTS message_usage (
    session_rowid INTEGER NOT NULL,
    ts TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cache_read_tokens INTEGER NOT NULL,
    cache_write_tokens INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_usage_session ON message_usage(session_rowid);
CREATE TABLE IF NOT EXISTS indexed_files (
    file_path TEXT PRIMARY KEY
);
"#;

pub fn open(storage_dir: &Path) -> Result<Connection> {
    let path = storage_dir.join("stats.sqlite");
    let conn = Connection::open(&path)
        .with_context(|| format!("Failed to open stats index: {}", path.display()))?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

/// Normalize an RFC3339 timestamp ("Z" or "+00:00" style) to a single UTC
/// format so string comparison orders correctly across producers.
pub fn normalize_ts(ts: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|dt| {
            dt.with_timezone(&chrono::Utc)
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string()
        })
        .unwrap_or_else(|_| ts.to_string())
}

struct SessionSummary {
    prompts: usize,
    tool_calls: usize,
    files_touched: usize,
    prompt_words: usize,
    response_words: usize,
    last_activity: String,
    /// (timestamp, model, input, output, cache_read, cache_write)
    usage_rows: Vec<(String, String, u64, u64, u64, u64)>,
}

fn summarize(devlog: &DevlogOutput) -> SessionSummary {
    use std::collections::HashSet;

    let ingest_ts = normalize_ts(&devlog.timestamp);
    let mut summary = SessionSummary {
        prompts: 0,
        tool_calls: 0,
        files_touched: 0,
        prompt_words: 0,
        response_words: 0,
        last_activity: String::new(),
        usage_rows: Vec::new(),
    };
    let mut files: HashSet<String> = HashSet::new();

    for entry in &devlog.conversation {
        match entry {
            ConversationEntry::User { content, timestamp } => {
                summary.prompts += 1;
                summary.prompt_words += count_words(content);
                if let Some(ts) = timestamp {
                    let ts = normalize_ts(ts);
                    if ts > summary.last_activity {
                        summary.last_activity = ts;
                    }
                }
            }
            ConversationEntry::Assistant {
                content,
                usage,
                model,
                timestamp,
            } => {
                summary.response_words += count_words(content);
                let ts = timestamp
                    .as_deref()
                    .map(normalize_ts)
                    .unwrap_or_else(|| ingest_ts.clone());
                if ts > summary.last_activity {
                    summary.last_activity = ts.clone();
                }
                if let Some(usage) = usage {
                    summary.usage_rows.push((
                        ts,
                        model.as_deref().unwrap_or("unknown").to_string(),
                        usage.input_tokens.unwrap_or(0),
                        usage.output_tokens.unwrap_or(0),
                        usage.cache_read_input_tokens.unwrap_or(0),
                        usage.cache_creation_input_tokens.unwrap_or(0),
                    ));
                }
            }
            ConversationEntry::ToolSummary { actions } => {
                summary.tool_calls += actions.len();
                for action in actions {
                    if let Some(file) = extract_file_from_action(action) {
                        files.insert(file);
                    }
                }
            }
        }
    }

    summary.files_touched = files.len();
    if summary.last_activity.is_empty() {
        summary.last_activity = ingest_ts;
    }
    summary
}

fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

fn extract_file_from_action(action: &str) -> Option<String> {
    // Actions look like: "edited src/main.rs", "read config.json", "created foo.txt"
    let prefixes = ["edited ", "read ", "created "];
    for prefix in prefixes {
        if action.starts_with(prefix) {
            return Some(action[prefix.len()..].to_string());
        }
    }
    None
}

/// Insert or replace one session in the index. Returns false if the index
/// already holds a newer (or equal) ingest of this session.
pub fn upsert_devlog(
    conn: &Connection,
    machine: &str,
    project: &str,
    devlog: &DevlogOutput,
) -> Result<bool> {
    let ingested_at = normalize_ts(&devlog.timestamp);

    let existing: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, ingested_at FROM sessions WHERE machine = ?1 AND project = ?2 AND session_id = ?3",
            params![machine, project, devlog.session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    if let Some((id, existing_ts)) = existing {
        if existing_ts >= ingested_at {
            return Ok(false);
        }
        conn.execute("DELETE FROM message_usage WHERE session_rowid = ?1", [id])?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
    }

    let summary = summarize(devlog);
    conn.execute(
        "INSERT INTO sessions (machine, project, session_id, ingested_at, last_activity,
                               prompts, tool_calls, files_touched, prompt_words, response_words)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            machine,
            project,
            devlog.session_id,
            ingested_at,
            summary.last_activity,
            summary.prompts as i64,
            summary.tool_calls as i64,
            summary.files_touched as i64,
            summary.prompt_words as i64,
            summary.response_words as i64,
        ],
    )?;
    let rowid = conn.last_insert_rowid();

    let mut stmt = conn.prepare_cached(
        "INSERT INTO message_usage (session_rowid, ts, model, input_tokens, output_tokens,
                                    cache_read_tokens, cache_write_tokens)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for (ts, model, input, output, cache_read, cache_write) in &summary.usage_rows {
        stmt.execute(params![
            rowid,
            ts,
            model,
            *input as i64,
            *output as i64,
            *cache_read as i64,
            *cache_write as i64
        ])?;
    }

    Ok(true)
}

/// Index any storage files not yet seen. Run at server startup; the first run
/// compiles the full history, later runs only pick up files written while the
/// server was down (live ingests are indexed directly).
pub fn backfill(conn: &mut Connection, storage_dir: &Path) -> Result<usize> {
    let mut indexed = 0usize;
    let tx = conn.transaction()?;

    for machine_entry in std::fs::read_dir(storage_dir)? {
        let machine_path = machine_entry?.path();
        if !machine_path.is_dir() {
            continue;
        }
        let machine = machine_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        for project_entry in std::fs::read_dir(&machine_path)? {
            let project_path = project_entry?.path();
            if !project_path.is_dir() {
                continue;
            }
            let project = project_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            for file_entry in std::fs::read_dir(&project_path)? {
                let file_path = file_entry?.path();
                if !file_path.extension().map(|e| e == "json").unwrap_or(false) {
                    continue;
                }
                let rel = format!(
                    "{}/{}/{}",
                    machine,
                    project,
                    file_path.file_name().unwrap_or_default().to_string_lossy()
                );
                let seen: bool = tx
                    .query_row(
                        "SELECT 1 FROM indexed_files WHERE file_path = ?1",
                        [&rel],
                        |_| Ok(true),
                    )
                    .optional()?
                    .unwrap_or(false);
                if seen {
                    continue;
                }

                match std::fs::read_to_string(&file_path)
                    .map_err(anyhow::Error::from)
                    .and_then(|c| serde_json::from_str::<DevlogOutput>(&c).map_err(Into::into))
                {
                    Ok(devlog) => {
                        upsert_devlog(&tx, &machine, &project, &devlog)?;
                        indexed += 1;
                    }
                    Err(e) => {
                        eprintln!("Skipping unparseable devlog {}: {}", rel, e);
                    }
                }
                // Record even unparseable files so they aren't retried every startup
                tx.execute(
                    "INSERT OR IGNORE INTO indexed_files (file_path) VALUES (?1)",
                    [&rel],
                )?;
            }
        }
    }

    tx.commit()?;
    Ok(indexed)
}

/// Record a live-ingested file as indexed (the session itself is upserted
/// separately) so the next startup backfill doesn't re-parse it.
pub fn mark_file_indexed(conn: &Connection, rel_path: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO indexed_files (file_path) VALUES (?1)",
        [rel_path],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::TokenUsage;

    fn devlog(session_id: &str, timestamp: &str, output_tokens: u64) -> DevlogOutput {
        DevlogOutput {
            schema_version: "1.0".to_string(),
            session_id: session_id.to_string(),
            timestamp: timestamp.to_string(),
            machine_id: "m1".to_string(),
            project_dir: "/p/proj".to_string(),
            git: None,
            conversation: vec![
                ConversationEntry::User {
                    timestamp: Some("2026-06-01T10:00:00Z".to_string()),
                    content: "hello there".to_string(),
                },
                ConversationEntry::Assistant {
                    timestamp: Some("2026-06-01T10:00:05Z".to_string()),
                    content: "hi".to_string(),
                    usage: Some(TokenUsage {
                        input_tokens: Some(10),
                        output_tokens: Some(output_tokens),
                        cache_creation_input_tokens: Some(5),
                        cache_read_input_tokens: Some(100),
                    }),
                    model: Some("claude-fable-5".to_string()),
                },
            ],
        }
    }

    #[test]
    fn reingest_replaces_older_keeps_newer() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();

        // First ingest
        assert!(upsert_devlog(&conn, "m1", "proj", &devlog("s1", "2026-06-01T10:01:00Z", 50)).unwrap());
        // Re-ingest of same session with later timestamp replaces it
        assert!(upsert_devlog(&conn, "m1", "proj", &devlog("s1", "2026-06-01T11:00:00Z", 75)).unwrap());
        // Older duplicate (e.g. backfill order) is ignored
        assert!(!upsert_devlog(&conn, "m1", "proj", &devlog("s1", "2026-06-01T09:00:00Z", 99)).unwrap());

        let (count, output): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), SUM(output_tokens) FROM message_usage",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1, "usage rows must not accumulate across re-ingests");
        assert_eq!(output, 75, "newest ingest wins");

        let sessions: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sessions, 1);
    }

    #[test]
    fn normalize_ts_orders_mixed_formats() {
        // Offset form converts to the same UTC instant as the Z form
        assert_eq!(
            normalize_ts("2026-06-12T10:41:34+01:00"),
            "2026-06-12T09:41:34.000Z"
        );
        assert!(normalize_ts("2026-06-12T09:00:00+00:00") < normalize_ts("2026-06-12T10:00:00Z"));
    }
}
