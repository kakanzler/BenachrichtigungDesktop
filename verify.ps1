#requires -Version 7
# Full verification gate. Run before reporting any change as done.
$ErrorActionPreference = "Stop"

function Step($name, $argv) {
    Write-Host "`n=== $name ===" -ForegroundColor Cyan
    Push-Location (Join-Path $PSScriptRoot "src-tauri")
    try {
        & cargo @argv
        if ($LASTEXITCODE -ne 0) { throw "$name failed (exit $LASTEXITCODE)" }
    } finally { Pop-Location }
}

Step "cargo test" @("test")
Step "cargo clippy" @("clippy", "--all-targets", "--", "-D", "warnings")

Write-Host "`nAll checks passed." -ForegroundColor Green
