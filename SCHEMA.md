# devlog Output Schema

## Root object

```json
{
  "schema_version": "1.0",
  "session_id": "string",
  "timestamp": "ISO 8601 datetime",
  "project_dir": "string (absolute path)",
  "git": {
    "remote": "string (origin URL)",
    "branch": "string",
    "commit": "string (full SHA)"
  },
  "conversation": [
    { "type": "user", ... },
    { "type": "assistant", ... },
    { "type": "tool_summary", ... }
  ]
}
```

## Field descriptions

### Top-level fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `schema_version` | string | yes | Schema version for future compatibility. Currently "1.0" |
| `session_id` | string | yes | Claude Code session identifier |
| `timestamp` | string | yes | ISO 8601 datetime when ingestion occurred |
| `project_dir` | string | yes | Absolute path to project directory (`CLAUDE_PROJECT_DIR`) |
| `git` | object \| null | yes | Git metadata, or null if not in a git repo |
| `conversation` | array | yes | Ordered list of conversation entries |

### Git object

| Field | Type | Description |
|-------|------|-------------|
| `remote` | string \| null | Origin remote URL. Null if no remote configured |
| `branch` | string | Current branch name |
| `commit` | string | Full SHA of HEAD commit |

### Conversation entries

Entries appear in chronological order. Three types:

#### User entry
```json
{
  "type": "user",
  "timestamp": "2024-12-30T14:30:52Z",
  "content": "Can you add JWT authentication to the API?"
}
```

#### Assistant entry
```json
{
  "type": "assistant",
  "timestamp": "2024-12-30T14:31:15Z",
  "content": "I'll add JWT authentication. The approach will be..."
}
```

#### Tool summary entry
```json
{
  "type": "tool_summary",
  "actions": [
    "edited src/auth.rs",
    "edited src/main.rs",
    "ran cargo build"
  ]
}
```

Tool summaries are inserted after the assistant message that triggered them, and aggregate consecutive tool uses into a single entry.

## Example complete output

```json
{
  "schema_version": "1.0",
  "session_id": "sess_abc123def456",
  "timestamp": "2024-12-30T14:45:00Z",
  "project_dir": "/home/user/projects/myapi",
  "git": {
    "remote": "git@gogs.example.com:user/myapi.git",
    "branch": "feature/auth",
    "commit": "a1b2c3d4e5f6789012345678901234567890abcd"
  },
  "conversation": [
    {
      "type": "user",
      "timestamp": "2024-12-30T14:30:52Z",
      "content": "Can you add JWT authentication to the API?"
    },
    {
      "type": "assistant",
      "timestamp": "2024-12-30T14:31:15Z",
      "content": "I'll add JWT authentication using the `jsonwebtoken` crate. This will involve:\n\n1. Adding the dependency\n2. Creating an auth module with token generation and validation\n3. Adding middleware to protect routes\n\nLet me start by updating the Cargo.toml and creating the auth module."
    },
    {
      "type": "tool_summary",
      "actions": [
        "edited Cargo.toml",
        "created src/auth.rs",
        "edited src/main.rs"
      ]
    },
    {
      "type": "user",
      "timestamp": "2024-12-30T14:35:22Z",
      "content": "Make the token expiry configurable via environment variable"
    },
    {
      "type": "assistant",
      "timestamp": "2024-12-30T14:35:45Z",
      "content": "Good idea. I'll add a `JWT_EXPIRY_HOURS` environment variable with a sensible default of 24 hours."
    },
    {
      "type": "tool_summary",
      "actions": [
        "edited src/config.rs",
        "edited src/auth.rs"
      ]
    }
  ]
}
```

## Tool action format

Tool uses are summarised as simple action strings:

| Tool type | Format | Example |
|-----------|--------|---------|
| File write/edit | `edited <path>` | `edited src/main.rs` |
| File create | `created <path>` | `created src/auth.rs` |
| File read | `read <path>` | `read src/config.rs` |
| Bash command | `ran <command summary>` | `ran cargo build` |
| Other tools | `used <tool_name>` | `used WebSearch` |

For bash commands, only the command itself is shown (truncated if very long), not its output.
