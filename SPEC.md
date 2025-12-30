# devlog - Claude Code Session Ingester

## Overview

A Rust CLI tool that captures Claude Code conversations for later reference and search. Triggered by Claude Code hooks, it extracts the human-readable conversation (prompts and responses) while filtering out operational noise (file diffs, tool internals).

## Problem Statement

When working with Claude Code across multiple projects and machines, the development conversation is lost after context compression. This tool preserves the "how did we build this" narrative alongside the code.

## MVP Scope

**What it does:**
- Ingests Claude Code session JSONL files
- Filters to: user prompts, assistant text responses, tool summaries (filenames only)
- Enriches with git metadata (remote, branch, commit)
- Outputs one JSON file per session

**What it doesn't do (yet):**
- Search/indexing
- Sync between machines
- Database storage
- Web UI

## Installation

The tool will be a single Rust binary called `devlog`.

## Usage

### Manual invocation
```bash
devlog ingest <path-to-session.jsonl>
```

### Via Claude Code hooks
Configured in `.claude/settings.json` to trigger on `PreCompact` and `SessionEnd`.

## Output

### Location
`.devlog/` directory in the current project root.

### Filename format
```
YYYY-MM-DD-HHMMSS-<session_id_short>.json
```

Example: `2024-12-30-143052-abc123.json`

### JSON structure
See `SCHEMA.md` for the complete output schema.

## Git metadata capture

The tool captures the current git state at ingestion time:
- **remote**: Origin URL (used later to determine gogs vs github)
- **branch**: Current branch name
- **commit**: Current HEAD commit hash

If not in a git repository, the `git` field is `null`.

## Conversation filtering rules

### Included
- User messages (role: "user") - full content
- Assistant messages (role: "assistant") - text content only

### Summarised
- Tool use: collapsed to action + target
  - File edits → `"edited src/main.rs"`
  - File reads → `"read src/config.rs"`  
  - Bash commands → `"ran cargo build"`
  - Other tools → `"used <tool_name>"`

### Excluded
- Tool results (full output/diffs)
- System messages
- Internal Claude Code metadata

## Error handling

- If JSONL is malformed: log error, skip entry, continue
- If git commands fail: set `git` field to `null`, continue
- If output directory doesn't exist: create it
- If write fails: exit with error code 1

## Future considerations (not in MVP)

- `--output` flag to specify custom output location
- `--format markdown` for human-readable output
- Search subcommand once storage strategy is decided
- Auto-commit to git after writing
- Remote detection (gogs vs github) for routing
