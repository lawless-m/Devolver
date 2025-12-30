@echo off
REM Git Repository Sync Script for Windows Scheduled Tasks
REM This script commits local changes, pulls from remote, and pushes back

setlocal enabledelayedexpansion

REM Set repository path using environment variables
set REPO_PATH=%USERPROFILE%\.claude
set GIT_BASH="%USERPROFILE%\AppData\Local\Programs\Git\git-bash.exe"

REM Change to repository directory
cd /d "%REPO_PATH%"
if errorlevel 1 (
    echo Error: Could not change to repository directory
    exit /b 1
)

REM Get current timestamp
for /f "tokens=2 delims==" %%I in ('wmic os get localdatetime /value') do set datetime=%%I
set TIMESTAMP=%datetime:~0,4%-%datetime:~4,2%-%datetime:~6,2% %datetime:~8,2%:%datetime:~10,2%:%datetime:~12,2%

REM Get hostname
set HOSTNAME=%COMPUTERNAME%

REM Run git commands using git-bash
%GIT_BASH% -c "cd '%REPO_PATH%' && git add -A && git diff-index --quiet HEAD || git commit -m 'Auto-sync: Local changes from %HOSTNAME% at %TIMESTAMP%' && git pull --rebase && git push"

if errorlevel 1 (
    echo Sync completed with warnings or no changes to commit
    exit /b 0
)

echo Sync completed successfully at %TIMESTAMP%
exit /b 0
