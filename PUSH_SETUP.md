# Devlog Push Setup

This document describes how to set up centralized devlog collection across multiple machines.

## Architecture

- **Client** (`devlog`): Runs on each development machine, ingests Claude Code sessions and pushes them to a central server
- **Receiver** (`devlog-receiver`): Runs on a permanently-online Linux machine, receives sessions via HTTP and stores them in DuckDB

## Client Setup (Windows/Linux/Mac)

### 1. Build and Install

```bash
cd /path/to/Devolver
cargo build --release
cp target/release/devlog /path/to/bin/
```

### 2. Configure Push Endpoint

The config file is created automatically at `~/.devlog/config.toml` on first run. Edit it to enable push:

```toml
[push]
endpoint = "http://your-central-server:8080/ingest"
enabled = true
```

### 3. Update Claude Code Hooks

Edit `~/.claude/settings.json` to call `devlog ingest` on context compression:

```json
{
  "hooks": {
    "PreCompact": [
      {
        "matcher": "manual",
        "hooks": [{"type": "command", "command": "/path/to/devlog ingest"}]
      },
      {
        "matcher": "auto",
        "hooks": [{"type": "command", "command": "/path/to/devlog ingest"}]
      }
    ]
  }
}
```

Now sessions will be:
1. Ingested to local `.devlog/` folder
2. Automatically pushed to central server (if push is enabled)

## Receiver Setup (Linux Server)

### 1. Build

```bash
cd /path/to/Devolver/devlog-receiver
cargo build --release
```

### 2. Configure Environment

```bash
# Optional: Set custom database path (defaults to devlog.duckdb)
export DEVLOG_DB_PATH=/data/devlog/sessions.duckdb

# Optional: Set bind address (defaults to 0.0.0.0:8080)
export DEVLOG_BIND_ADDR=0.0.0.0:8080
```

### 3. Run the Receiver

```bash
./target/release/devlog-receiver
```

Or run as a systemd service (recommended):

```ini
# /etc/systemd/system/devlog-receiver.service
[Unit]
Description=Devlog Session Receiver
After=network.target

[Service]
Type=simple
User=devlog
WorkingDirectory=/opt/devlog
Environment="DEVLOG_DB_PATH=/data/devlog/sessions.duckdb"
Environment="DEVLOG_BIND_ADDR=0.0.0.0:8080"
Environment="RUST_LOG=info"
ExecStart=/opt/devlog/devlog-receiver
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable devlog-receiver
sudo systemctl start devlog-receiver
sudo systemctl status devlog-receiver
```

## Database Schema

The receiver stores sessions in DuckDB with the following schema:

```sql
CREATE TABLE sessions (
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
```

Indexes:
- `idx_machine_timestamp ON (machine_id, timestamp)`
- `idx_project ON (project_dir)`
- `idx_git_remote ON (git_remote)`

## Manual Push

You can manually push the most recent session from any project:

```bash
cd /path/to/project
devlog push
```

Or push a specific devlog file:

```bash
devlog push /path/to/.devlog/2026-01-02-120000-abc123.json
```

## Querying the Database

Connect to DuckDB and query your sessions:

```bash
duckdb /data/devlog/sessions.duckdb
```

Example queries:

```sql
-- View all machines
SELECT DISTINCT machine_id, COUNT(*) as session_count
FROM sessions
GROUP BY machine_id;

-- Sessions from last 7 days
SELECT machine_id, project_dir, git_branch, timestamp
FROM sessions
WHERE timestamp > NOW() - INTERVAL '7 days'
ORDER BY timestamp DESC;

-- Find sessions related to a specific repo
SELECT session_id, machine_id, git_branch, timestamp
FROM sessions
WHERE git_remote LIKE '%yourrepo%'
ORDER BY timestamp DESC;

-- Extract conversation from a session
SELECT json_extract_string(conversation, '$')
FROM sessions
WHERE session_id = 'abc123';
```

## Troubleshooting

### Client Issues

1. **Push fails but ingest succeeds**: Check that the endpoint is reachable:
   ```bash
   curl http://your-server:8080/health
   ```

2. **Config not found**: Run any `devlog` command once to create default config at `~/.devlog/config.toml`

3. **Sessions not pushing**: Check push is enabled in config and that endpoint URL is correct

### Receiver Issues

1. **Port already in use**: Change `DEVLOG_BIND_ADDR` to use a different port

2. **Database errors**: Check file permissions on the DuckDB file and its directory

3. **Out of disk space**: Monitor the size of the DuckDB file:
   ```bash
   du -h /data/devlog/sessions.duckdb
   ```

## Next Steps

Now that you have centralized collection working, you can:

1. **Build analytics**: Query DuckDB to analyze development patterns
2. **Create dashboards**: Export data and visualize trends
3. **Search history**: Full-text search across all conversations
4. **Generate summaries**: Feed sessions back into Claude for weekly summaries
