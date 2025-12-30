# Claude Code Session JSONL Format

## Overview

Claude Code stores session transcripts as JSONL (JSON Lines) files. Each line is a self-contained JSON object representing a message or event in the session.

## File location

Session transcripts are typically found at:
- `~/.claude/` directory
- Can be explicitly output via `claude --output-transcript <filename>`

## Known message types

Based on Claude Code's transcript format, the ingester needs to handle these entry types:

### User message
```json
{
  "type": "human",
  "message": {
    "content": "User's prompt text here"
  },
  "timestamp": "2024-12-30T14:30:52.123Z"
}
```

### Assistant message
```json
{
  "type": "assistant", 
  "message": {
    "content": "Assistant's response text",
    "tool_use": [...]
  },
  "timestamp": "2024-12-30T14:31:15.456Z"
}
```

### Tool use
```json
{
  "type": "tool_use",
  "tool": "Edit",
  "input": {
    "path": "src/main.rs",
    ...
  },
  "timestamp": "2024-12-30T14:31:20.789Z"
}
```

### Tool result
```json
{
  "type": "tool_result",
  "tool": "Edit",
  "output": "...",
  "timestamp": "2024-12-30T14:31:21.012Z"
}
```

## Important notes

1. **Format may vary**: The exact field names and structure may differ between Claude Code versions. The ingester should be defensive and handle missing fields gracefully.

2. **Discovery needed**: When implementing, examine actual JSONL output from Claude Code to confirm field names. Use:
   ```bash
   claude --output-transcript sample.jsonl
   # Then inspect the file
   ```

3. **Tool types to recognise**:
   - `Edit` / `Write` - file modifications
   - `Read` - file reads
   - `Bash` - shell commands
   - `Task` - subagent tasks
   - Others as discovered

4. **Content extraction**: Assistant messages may have content as a string or as an array of content blocks. Handle both:
   ```json
   // Simple string
   {"content": "Hello"}
   
   // Content blocks
   {"content": [{"type": "text", "text": "Hello"}, ...]}
   ```

## Recommended approach

1. Start by generating a sample transcript:
   ```bash
   claude --output-transcript test-session.jsonl
   ```

2. Inspect the structure:
   ```bash
   head -20 test-session.jsonl | jq .
   ```

3. Build parsing based on actual observed format

4. Test with multiple sessions to ensure robustness
