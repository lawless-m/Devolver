$ErrorActionPreference = 'Stop'

cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$dest = Join-Path $HOME 'bin'
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item target\release\devlog.exe (Join-Path $dest 'devlog.exe')
Write-Host "Installed devlog.exe to $dest"
