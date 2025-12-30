# Claude Code Hook Configuration

## Overview

The devlog ingester is triggered automatically by Claude Code hooks. This document describes how to set up the hooks.

## Hook events

The tool should be triggered on:

1. **PreCompact** - Before context compression (captures full conversation before it's summarised)
2. **SessionEnd** - When a session ends normally (captures final state)

## Configuration

Add to your Claude Code settings file (`.claude/settings.json` or `~/.claude/settings.json`):

```json
{
  "hooks": {
    "PreCompact": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "devlog ingest"
          }
        ]
      }
    ],
    "SessionEnd": [
      {
        "hooks": [
          {
            "type": "command", 
            "command": "devlog ingest"
          }
        ]
      }
    ]
  }
}
```

## Environment variables available

The hook command has access to:

- `CLAUDE_PROJECT_DIR` - Absolute path to the project root
- `CLAUDE_CODE_REMOTE` - "true" if running in web interface, empty for CLI

## Input via stdin

Hooks receive JSON on stdin with event details. The format varies by hook type.

For `PreCompact`, the stdin should include the session transcript path or content. **This needs verification** - examine what Claude Code actually provides.

## Fallback: explicit path

If stdin doesn't provide the transcript, the tool can:

1. Look for the most recent session file in `~/.claude/`
2. Accept an explicit path argument: `devlog ingest /path/to/session.jsonl`

## Post-MVP: auto-commit

Once the routing logic (gogs vs github) is implemented, the hook configuration could be extended:

```json
{
  "hooks": {
    "PreCompact": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "devlog ingest && devlog commit"
          }
        ]
      }
    ]
  }
}
```

## Testing hooks

1. Configure the hook in settings
2. Start a Claude Code session
3. Run `/hooks` to verify configuration is loaded
4. Either wait for auto-compact or run `/compact` manually
5. Check `.devlog/` directory for output

## Notes

- Hooks run with a 60-second timeout by default
- PreCompact cannot block compaction (exit code 2 only shows stderr to user)
- Multiple hooks run in parallel if configured
