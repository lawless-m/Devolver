# devlog

![devlog](devlog.png)

A Rust CLI tool that captures Claude Code conversations for later reference and search.

## Overview

When working with Claude Code, the development conversation is lost after context compression. This tool preserves the "how did we build this" narrative alongside your code by ingesting session JSONL files and extracting human-readable conversations.

## Features

- Ingests Claude Code session JSONL files
- Filters to user prompts, assistant text responses, and tool summaries
- Enriches with git metadata (remote, branch, commit)
- Outputs one JSON file per session to `.devlog/`

## Installation

```bash
cargo build --release
```

## Usage

```bash
devlog ingest <path-to-session.jsonl>
```

### Rejoin a past session

```bash
devlog resume
```

Shows recent sessions (last 30 days) from the central server, grouped by host
with the local host last (nearest the prompt), titled by each session's first
prompt. Picking one runs
`claude --resume <session-id> --permission-mode bypassPermissions` in the
session's project directory. Sessions from another machine can't be launched
locally — picking one prints the host so you can ssh in and run `devlog resume`
there. Resume replays Claude Code's own transcript, so it only works while
that file still exists on the owning host (~30 days by default).

### Via Claude Code hooks

Configure in `.claude/settings.json` to trigger on `PreCompact` and `SessionEnd`.

## Output

Output files are written to `.devlog/` with the format:
```
YYYY-MM-DD-HHMMSS-<session_id_short>.json
```

## Documentation

- [SPEC.md](SPEC.md) - Full specification
- [SCHEMA.md](SCHEMA.md) - Output JSON schema
- [JSONL_FORMAT.md](JSONL_FORMAT.md) - Input format details
- [HOOKS.md](HOOKS.md) - Claude Code hooks integration

