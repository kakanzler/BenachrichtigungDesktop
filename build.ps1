#!/usr/bin/env pwsh
# Release build and NSIS installer, written to src-tauri\target\release\bundle\nsis.
& "$PSScriptRoot\frontend\node_modules\.bin\tauri.cmd" build @args
