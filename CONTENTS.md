# devlog Specification Package

## Contents

| File | Description |
|------|-------------|
| `SPEC.md` | **Start here.** Main project specification - scope, goals, usage |
| `SCHEMA.md` | Output JSON schema with field descriptions and examples |
| `JSONL_FORMAT.md` | Notes on Claude Code's input format (needs verification) |
| `HOOKS.md` | Claude Code hook configuration for automatic triggering |

## Quick summary

**devlog** is a Rust CLI tool that captures Claude Code conversations as JSON files for later reference.

**MVP scope:**
- Parse Claude Code session JSONL
- Filter to prompts + responses + tool summaries
- Add git metadata (remote, branch, commit)
- Output to `.devlog/` in project directory

**Not in MVP:**
- Search/indexing
- Sync between machines  
- Database storage
- Auto-commit (mentioned but deferred)

## Getting started

1. Read `SPEC.md` for the full picture
2. Generate a sample Claude Code transcript to examine the actual JSONL format
3. Implement the parser based on observed format
4. Test with the hook configuration from `HOOKS.md`

## Key decisions made

- **Rust** for the implementation (your preference, no runtime deps)
- **JSON output** (not Markdown) - easier to parse later for search
- **Per-project storage** in `.devlog/` for now
- **Git metadata captured** but routing logic (gogs vs github) deferred
- **Schema versioning** included for future compatibility

## Open questions for implementation

1. What exactly does Claude Code provide on stdin to hooks?
2. Where is the session JSONL file located / how to find it?
3. Exact field names in Claude Code's JSONL (verify with real output)

## Future phases (not in this spec)

- Phase 2: Search CLI (`devlog search "query"`)
- Phase 3: Storage routing (gogs projects keep local, github projects go to central repo)
- Phase 4: Sync and indexing across machines
