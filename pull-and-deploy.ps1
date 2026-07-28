# Pull the latest master and reinstall the devlog binary.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

git pull
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& "$PSScriptRoot\install.ps1"
