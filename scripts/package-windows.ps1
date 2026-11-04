$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$manifest = Get-Content Cargo.toml -Raw
$version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if (-not $version) { throw "Version missing from Cargo.toml" }
if ($args.Count -eq 0 -or $args[0] -ne '--skip-build') { cargo build --release; if ($LASTEXITCODE -ne 0) { throw "Cargo build failed" } }
$stage = Join-Path $env:TEMP "rustbridge-win-$PID"
New-Item -ItemType Directory -Force -Path $stage, "dist" | Out-Null
Copy-Item target/release/rustbridge.exe $stage
Copy-Item README.md,LICENSE $stage
$zip = Join-Path (Get-Location) "dist/rustbridge_$version-windows-x64.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip -CompressionLevel Optimal
Remove-Item $stage -Recurse -Force
Write-Host "Portable Windows release: $zip"
