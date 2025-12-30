# Windows Sync Setup

This guide explains how to set up automatic Git repository syncing on Windows using Task Scheduler.

## Files

- `sync-repo.bat` - Batch script version (simpler, uses git-bash.exe)
- `sync-repo.ps1` - PowerShell script version (better error handling, recommended)

## Prerequisites

- Git for Windows installed at: `%USERPROFILE%\AppData\Local\Programs\Git\`
- This repository (Claude-Skills) cloned to: `%USERPROFILE%\.claude`

The scripts use environment variables and will work for any user.

**Note:** On Windows, we clone the repo directly to `%USERPROFILE%\.claude` to avoid symlink issues. On Linux, we can symlink `~/.claude` to the repo location.

## Setup Instructions

### Option 1: Using PowerShell Script (Recommended)

1. Open Task Scheduler (`taskschd.msc`)

2. Click "Create Task" (not "Create Basic Task")

3. **General Tab:**
   - Name: `Git Sync - Claude Skills`
   - Description: `Automatically sync Claude Skills repository`
   - Security options: Run whether user is logged on or not (optional)
   - Configure for: Windows 10/11

4. **Triggers Tab:**
   - Click "New..."
   - Begin the task: On a schedule
   - Settings: Daily or choose your preferred interval
   - Repeat task every: 15 minutes (or your preference)
   - For a duration of: Indefinitely
   - Enabled: Checked

5. **Actions Tab:**
   - Click "New..."
   - Action: Start a program
   - Program/script: `powershell.exe`
   - Add arguments: `-ExecutionPolicy Bypass -File "%USERPROFILE%\.claude\sync-repo.ps1"`
   - Start in: `%USERPROFILE%\.claude`

6. **Conditions Tab:**
   - Uncheck "Start the task only if the computer is on AC power" (if laptop)

7. **Settings Tab:**
   - Allow task to be run on demand: Checked
   - Run task as soon as possible after a scheduled start is missed: Checked
   - If the task fails, restart every: 1 minute
   - Attempt to restart up to: 3 times

### Option 2: Using Batch Script

Follow the same steps as above, but in step 5:
- Program/script: `%USERPROFILE%\.claude\sync-repo.bat`
- Add arguments: (leave blank)
- Start in: `%USERPROFILE%\.claude`

## Testing

After setup, right-click the task in Task Scheduler and select "Run" to test it immediately.

Check the "Last Run Result" column:
- `0x0` = Success
- Other values = Error (check the History tab for details)

## Logs

The PowerShell script outputs to the Task Scheduler history. To view:
1. Open Task Scheduler
2. Find your task
3. Click the "History" tab

## Troubleshooting

If the sync fails:

1. **Check paths** - Make sure Git and repo paths match your system
2. **Test manually** - Run the script from PowerShell/Command Prompt to see errors
3. **Execution policy** - For PowerShell, ensure execution policy allows scripts:
   ```powershell
   Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
   ```
4. **Git credentials** - Ensure Git doesn't prompt for passwords (use SSH keys or credential manager)

## Customization

Edit the scripts to change:
- Repository path: Update `$RepoPath` variable
- Git path: Update `$GitPath` variable
- Commit message format: Modify the commit message string
