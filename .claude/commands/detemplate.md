# /detemplate - Remove Git Template Artifacts

Remove files from the initial Git template commit, typically used when starting a new project from the claude-skills template.

## Instructions

1. **Identify template files**: Check the initial commit (usually the first commit in the repository) to see what files were part of the template

2. **Remove template artifacts**:
   - Remove the `.claude/` directory (contains template skills and commands)
   - Remove template documentation files like `spellbook.png`, `WINDOWS_SYNC_SETUP.md`, etc.
   - Remove any other template-specific files

3. **Verify before removal**:
   - Use `git log --oneline` to see commit history
   - Use `git show <commit-hash> --name-only` to list files from the initial commit
   - Check that no important files were added after the template commit

4. **Execute cleanup**:
   ```bash
   rm -rf .claude/
   rm -f spellbook.png WINDOWS_SYNC_SETUP.md
   # Add any other template files to remove
   ```

5. **Create project-specific README**: Replace generic template README with project-specific documentation

6. **Commit the changes**:
   ```bash
   git add -A
   git commit -m "chore: Remove template artifacts and add project documentation"
   ```

## Safety Notes

- Always verify commit history before removing files
- Ensure you're not deleting files that were added after the template
- The `.claude/` directory may contain project-specific settings - check before removing
- This command removes itself when `.claude/commands/` is deleted (by design)
