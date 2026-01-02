use crate::models::DevlogSession;
use anyhow::{Context, Result};
use duckdb::Connection;

pub fn init_database(db_path: &str) -> Result<Connection> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("Failed to open DuckDB database at {}", db_path))?;

    // Create sessions table
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY,
            session_id VARCHAR NOT NULL,
            machine_id VARCHAR NOT NULL,
            project_dir VARCHAR NOT NULL,
            timestamp TIMESTAMP NOT NULL,
            schema_version VARCHAR,
            git_remote VARCHAR,
            git_branch VARCHAR,
            git_commit VARCHAR,
            conversation JSON NOT NULL,
            received_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(machine_id, session_id)
        )
        "#,
        [],
    )
    .context("Failed to create sessions table")?;

    // Create indexes for common queries
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_machine_timestamp ON sessions(machine_id, timestamp)",
        [],
    )
    .context("Failed to create machine_timestamp index")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_project ON sessions(project_dir)",
        [],
    )
    .context("Failed to create project index")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_git_remote ON sessions(git_remote)",
        [],
    )
    .context("Failed to create git_remote index")?;

    Ok(conn)
}

pub fn insert_session(conn: &Connection, session: &DevlogSession) -> Result<()> {
    let conversation_json = serde_json::to_string(&session.conversation)
        .context("Failed to serialize conversation")?;

    let timestamp = chrono::DateTime::parse_from_rfc3339(&session.timestamp)
        .context("Failed to parse timestamp")?
        .naive_utc();

    conn.execute(
        r#"
        INSERT INTO sessions (
            session_id, machine_id, project_dir, timestamp,
            schema_version, git_remote, git_branch, git_commit,
            conversation
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT (machine_id, session_id) DO UPDATE SET
            timestamp = excluded.timestamp,
            project_dir = excluded.project_dir,
            schema_version = excluded.schema_version,
            git_remote = excluded.git_remote,
            git_branch = excluded.git_branch,
            git_commit = excluded.git_commit,
            conversation = excluded.conversation,
            received_at = CURRENT_TIMESTAMP
        "#,
        [
            &session.session_id as &dyn duckdb::ToSql,
            &session.machine_id,
            &session.project_dir,
            &timestamp,
            &session.schema_version,
            &session.git.as_ref().and_then(|g| g.remote.as_ref()),
            &session.git.as_ref().and_then(|g| g.branch.as_ref()),
            &session.git.as_ref().and_then(|g| g.commit.as_ref()),
            &conversation_json,
        ],
    )
    .context("Failed to insert session into database")?;

    Ok(())
}
