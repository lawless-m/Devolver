---
name: auto-testing
description: Pattern for automated testing with GitHub issue creation and Claude Code auto-fixing. Creates Test → Fail → Issue → Fix → Repeat cycle until tests pass.
tags: [testing, automation, github, ci]
version: 1.0
---

# Automated Test-Fix-Test Pattern

This skill documents the pattern for building automated test-fix loops with Claude Code, based on the iSCSI project.

## Core Concept

```
1. Check for open issues FIRST
2. If issues exist → Attempt fix
3. If no issues → Run tests
4. If tests fail → Create issue
5. Repeat until all tests pass
```

## Key Principles

### 1. Shell-Based Gating (Not LLM Judgment)

**Problem**: LLMs rationalize failures and close issues prematurely

**Solution**: Use shell exit codes to enforce conditions

```bash
# BAD - Relying on LLM judgment
claude "Run tests and close issue if they pass"

# GOOD - Shell enforces the rule
./run-tests.sh full
TEST_EXIT_CODE=$?

if [ $TEST_EXIT_CODE -eq 0 ]; then
    gh issue close $ISSUE_NUM --comment "Tests passed ✅"
else
    echo "Tests failed. Leaving issue open."
fi
```

**Key insight**: `tests && close` logic should be in shell, not in prompts.

### 2. Issue-First Workflow

**Check for existing issues BEFORE running tests** to avoid:
- Creating duplicate issues
- Wasting test runs when you know what needs fixing
- Issue spam

```bash
# Check for open issues FIRST
OPEN_ISSUES=$(gh issue list --state open --label test-failure)

if [ -n "$OPEN_ISSUES" ]; then
    # Fix existing issue
    ./fix-issue.sh $ISSUE_NUM
else
    # No issues - run tests to verify
    ./run-tests.sh
fi
```

### 3. Separate Scripts for Different Tasks

| Script | Purpose | Closes Issues? |
|--------|---------|----------------|
| `implement-issue.sh` | Implement new features/tests | No - shell does it based on test results |
| `fix-issue.sh` | Fix bugs from failing tests | No - shell does it based on test results |
| `auto-fix-loop.sh` | Orchestrates test-fix cycles | No - delegates to fix-issue.sh |

**Separation of concerns**:
- Claude: Writes code
- Shell: Runs tests, checks exit codes, manages issues
- GitHub: Tracks what needs doing

### 4. Test Validation with Reference Implementations

Before assuming tests are correct, validate against known-good implementations:

```bash
# Run your tests against reference implementation
./iscsi-test-suite /tmp/tgtd-config.toml

# If tests pass there but fail on your code → Your code has bugs
# If tests fail on both → Your tests might be wrong
```

### 5. Exit Code Semantics

Establish clear exit code meanings:

```bash
0   = Success (all tests passed)
1   = Test failures
124 = Timeout (likely hang/infinite loop)
```

Use these in shell gating:

```bash
if [ $EXIT_CODE -eq 0 ]; then
    # Close issue
elif [ $EXIT_CODE -eq 124 ]; then
    # Comment about timeout, leave open
else
    # Comment about failure, leave open
fi
```

### 6. Docker Isolation

Run tests and fixes in containers to:
- Ensure clean environment
- Avoid state pollution between runs
- Easy setup on different machines

```bash
docker run --rm \
    -v $PWD/repo:/repo \
    -v ~/.config/gh:/home/user/.config/gh:ro \
    -v ~/.claude/.credentials.json:/home/user/.claude/.credentials.json:ro \
    your-image \
    /bin/bash -c 'cd /repo && ./your-script.sh'
```

### 7. Iteration Tracking

Track failed attempts using WIP branches:

```bash
WIP_BRANCH="auto-fix-wip/issue-${ISSUE_NUM}"

# Before each iteration, commit previous failed attempt
if [ $iteration -gt 1 ]; then
    git checkout -b "$WIP_BRANCH" || git checkout "$WIP_BRANCH"
    git add -A
    git commit -m "WIP: Attempted fix iteration $((iteration - 1))"
    git push origin "$WIP_BRANCH"
    git checkout master
fi
```

Benefits:
- Preserves context for next iteration
- Shows what was tried before
- Helps avoid repeating failed approaches

## Example Implementation

### implement-issue.sh Structure

```bash
#!/bin/bash
set -euo pipefail

ISSUE_NUM=$1
MODEL=${2:-sonnet}

# Get issue details
ISSUE_BODY=$(gh issue view $ISSUE_NUM --json body --jq '.body')

# Build prompt
PROMPT="Implement the feature described in issue #$ISSUE_NUM:

$ISSUE_BODY

IMPORTANT: DO NOT run tests. DO NOT close the issue.
Your job:
1. Implement the feature
2. Commit changes
3. Push to GitHub
4. Add comment documenting what you did

The wrapper script will run tests and close the issue if they pass."

# Run Claude
claude --model "$MODEL" "$PROMPT"
CLAUDE_EXIT=$?

# Shell-based test gating (NOT Claude's responsibility)
if [ $CLAUDE_EXIT -eq 0 ]; then
    echo "Running tests to verify implementation..."
    ./run-tests.sh full
    TEST_EXIT=$?

    if [ $TEST_EXIT -eq 0 ]; then
        gh issue close $ISSUE_NUM --comment "✅ Implementation complete and tests pass"
    elif [ $TEST_EXIT -eq 124 ]; then
        gh issue comment $ISSUE_NUM --body "⚠️ Timeout. Leaving open for debugging."
    else
        gh issue comment $ISSUE_NUM --body "⚠️ Tests failed (exit $TEST_EXIT). Leaving open."
    fi
fi
```

### auto-fix-loop.sh Structure

```bash
#!/bin/bash
set -euo pipefail

MAX_ITERATIONS=${1:-10}
MODEL=${2:-sonnet}

iteration=0
while [ $iteration -lt $MAX_ITERATIONS ]; do
    iteration=$((iteration + 1))

    # 1. Check for open issues FIRST
    OPEN_ISSUES=$(gh issue list --state open --label test-failure --json number --jq '.[].number')

    if [ -n "$OPEN_ISSUES" ]; then
        # Found open issue - attempt fix
        ISSUE_NUM=$(echo "$OPEN_ISSUES" | head -1)
        echo "Found issue #$ISSUE_NUM, attempting fix..."
        ./fix-issue.sh --model "$MODEL" --iteration "$iteration" "$ISSUE_NUM"
    else
        # No open issues - run tests to verify
        echo "No open issues. Running tests..."
        if ./run-tests.sh full; then
            echo "✅ All tests passed!"
            exit 0
        else
            echo "Tests failed. New issue may be created."
        fi
    fi

    sleep 2
done

echo "Max iterations reached"
exit 1
```

## Model Selection Strategy

| Model | Use Case | Why |
|-------|----------|-----|
| Haiku | Quick iterations, simple fixes | Fast, cost-efficient |
| Sonnet | Standard fixes, implementations | Balance of speed and correctness |
| Opus | Complex bugs, architectural changes | Deep reasoning for hard problems |

## Prompting Best Practices

### DO:
- ✅ Be explicit about what Claude should NOT do (don't run tests, don't close issues)
- ✅ Separate concerns (Claude writes code, shell manages process)
- ✅ Provide context from previous failed attempts
- ✅ Include debugging tools available in environment
- ✅ Show example of correct behavior (reference implementation results)

### DON'T:
- ❌ Ask Claude to judge if tests passed (use exit codes)
- ❌ Let Claude decide when to close issues (use shell gating)
- ❌ Give Claude multiple responsibilities (implement AND test AND close)
- ❌ Trust Claude's interpretation of timeout errors

## GitHub Integration

### Issue Creation

```bash
# Create issue from test failure
TEST_OUTPUT=$(./run-tests.sh full 2>&1)
TEST_EXIT=$?

if [ $TEST_EXIT -ne 0 ]; then
    gh issue create --title "Test Failure: $TEST_NAME" \
        --body "Tests failed with exit code $TEST_EXIT

\`\`\`
$TEST_OUTPUT
\`\`\`

Exit codes:
- 0: Pass
- 1: Failure
- 124: Timeout
" \
        --label test-failure
fi
```

### Issue Management

```bash
# Check for duplicates before creating
EXISTING=$(gh issue list --state open --search "$TITLE" --json number --jq '.[0].number')

if [ -n "$EXISTING" ]; then
    echo "Issue #$EXISTING already exists"
    gh issue comment $EXISTING --body "Test still failing in commit $(git rev-parse --short HEAD)"
else
    gh issue create --title "$TITLE" --body "$BODY"
fi
```

## Testing Strategy

### 1. Simple Tests First
Run quick smoke tests before full suite:

```bash
./run-tests.sh simple   # Fast subset (30s)
./run-tests.sh full     # Complete suite (2-3min)
```

### 2. Target Restart Between Runs
Ensure fresh state:

```bash
# Kill old target
pkill -f target_process
sleep 0.5

# Start fresh target
cargo run --example target &
TARGET_PID=$!
```

### 3. Timeout Handling
Set reasonable timeouts:

```bash
timeout 10 ./test-suite || {
    EXIT=$?
    if [ $EXIT -eq 124 ]; then
        echo "TEST TIMED OUT after 10s"
    fi
    exit $EXIT
}
```

## Common Pitfalls

### 1. Claude Rationalizes Failures
**Problem**: "The timeout is due to infrastructure issues, not my code"

**Solution**: Don't let Claude see test results. Shell handles success/failure logic.

### 2. Creating Duplicate Issues
**Problem**: Every test run creates a new issue

**Solution**: Check for existing issues BEFORE running tests.

### 3. Closing Issues Prematurely
**Problem**: Claude closes issue despite test failures

**Solution**: Remove issue-closing from Claude's responsibilities entirely.

### 4. Stale Test Environment
**Problem**: Testing old code after new changes

**Solution**: Always restart target/server before running tests:
```bash
pkill -f target_process
cargo run --example target &
```

### 5. No Context Between Iterations
**Problem**: Claude repeats failed approaches

**Solution**:
- Commit failed attempts to WIP branch
- Include previous commit messages in prompt
- Show what was tried before

## Metrics and Monitoring

Track effectiveness:

```bash
# Count iterations to success
echo "Fixed in iteration $iteration"

# Track time per fix
START_TIME=$(date +%s)
# ... fix process ...
END_TIME=$(date +%s)
echo "Time taken: $((END_TIME - START_TIME))s"

# Success rate
TOTAL_ATTEMPTS=10
SUCCESSFUL_FIXES=7
echo "Success rate: $((SUCCESSFUL_FIXES * 100 / TOTAL_ATTEMPTS))%"
```

## Extending the Pattern

This pattern applies to:
- Any project with automated tests
- CI/CD pipelines
- Code review automation
- Regression testing
- Performance testing
- Security scanning

Just replace the test runner and adapt issue labeling.

## Real-World Results

From iSCSI project:
- 25 tests passing (0 failed)
- Fixed multi-PDU transfer bug (6-line fix)
- Validated tests against reference implementation
- Automated test → issue → fix → close cycle
- Shell gating prevented premature issue closing

## Related Files

Example implementation from iSCSI project:
- `implement-issue.sh` - Feature implementation with test gating
- `fix-issue.sh` - Bug fixing with context preservation
- `auto-fix-loop.sh` - Orchestration with issue-first logic
- `run-tests.sh` - Test runner with target restart

---

**Key Takeaway**: Treat the LLM as a code writer, not a process manager. Use shell scripts to enforce rules and gate decisions.
