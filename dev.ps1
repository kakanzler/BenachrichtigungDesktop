#!/usr/bin/env pwsh
# Runs the debug build. If the installed app is running, single-instance sends
# this launch to it instead, so quit the installed one from the tray first.
& "$PSScriptRoot\frontend\node_modules\.bin\tauri.cmd" dev @args
