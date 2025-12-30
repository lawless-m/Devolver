# Git Repository Sync Script for Windows Scheduled Tasks
# This script commits local changes, pulls from remote, and pushes back

param(
    [string]$RepoPath = "$env:USERPROFILE\.claude",
    [string]$GitPath = "$env:USERPROFILE\AppData\Local\Programs\Git\bin\git.exe"
)

# Change to repository directory
try {
    Set-Location $RepoPath -ErrorAction Stop
} catch {
    Write-Error "Failed to change to repository directory: $RepoPath"
    exit 1
}

# Get current timestamp and hostname
$timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
$hostname = $env:COMPUTERNAME

Write-Host "Starting sync at $timestamp on $hostname"

# Function to run git commands
function Invoke-Git {
    param([string]$Command)
    Write-Host "Running: git $Command"
    $process = Start-Process -FilePath $GitPath -ArgumentList $Command.Split(' ') -NoNewWindow -Wait -PassThru
    return $process.ExitCode
}

# Check for changes
Invoke-Git "add -A" | Out-Null

# Check if there are changes to commit
$status = & "$GitPath" status --porcelain
if ($status) {
    Write-Host "Changes detected, committing..."
    Invoke-Git "commit -m `"Auto-sync: Local changes from $hostname at $timestamp`"" | Out-Null
} else {
    Write-Host "No changes to commit"
}

# Pull with rebase
Write-Host "Pulling changes..."
$pullResult = Invoke-Git "pull --rebase"
if ($pullResult -ne 0) {
    Write-Warning "Pull failed or had conflicts"
    exit 1
}

# Push changes
Write-Host "Pushing changes..."
$pushResult = Invoke-Git "push"
if ($pushResult -ne 0) {
    Write-Warning "Push failed"
    exit 1
}

Write-Host "Sync completed successfully at $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
exit 0
