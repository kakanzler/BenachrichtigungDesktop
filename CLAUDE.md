# BenachrichtigungDesktop

Always-on-top card at the bottom right of the desktop showing today's ActiveRecall
counts for English (`../601---English`) and Forscher (`../602---Forscher`). Clicking
a row brings that app forward. Tauri 2, with plain HTML/CSS/JS and no bundler.

## Commands

- `./verify.ps1`: cargo test and clippy `-D warnings`. Run before reporting work as done.
- `./dev.ps1`: debug run. Quit the installed app from the tray first, or single-instance hands the launch to it.
- `./build.ps1`: release build and NSIS installer. Silent install: `setup.exe /S /D=D:\Program Files\Benachrichtigung` (`/D=` last and unquoted).
- `/benachrichtigung-release`: verify, commit, push, merge a PR, build from main, install and restart. See `.claude/skills/benachrichtigung-release/SKILL.md`.
- `cargo test real_data_counts -- --ignored --nocapture` in `src-tauri/`: prints counts from the real databases, read-only.

## Layout

- `src-tauri/src/counts/`: read-only due counts, one file per app
- `src-tauri/src/model.rs`: what the card shows (zero rows, ×, busy). Pure.
- `src-tauri/src/activate.rs`: focus an existing window or launch the exe
- `src-tauri/src/placement.rs`: positioning and showing without taking focus
- `src-tauri/src/lib.rs`: wiring (poller, commands, tray, autostart)
- `frontend/src/`: the card. `frontend/package.json` only holds the Tauri CLI, which is why `frontendDist` is `frontend/src`.

## Rules that are easy to break

- **Never write to either app's database, and never call their HTTP APIs.** English's `GET` endpoints run `settle()`, which can charge penalties and update `ar_last_seen`. Open databases with `SQLITE_OPEN_READ_ONLY` only, and never use `immutable=1`: Forscher keeps most of its data in the WAL.
- **Do not launch the real English or Forscher to test this app.** Opening English settles real data. Debug builds honour the `BENACHRICHTIGUNG_CONFIG` environment variable, a path to a JSON file with `englishDb`, `englishExe`, `forscherDb`, `forscherExe` and `zukunftExe`, so fixtures and stand-in exes can be used. The same keys work in `config.json` in the app data folder.
- **Show the window through `placement.rs`, not Tauri's `show()`,** which can activate the window and steal keyboard focus when the card reappears.
- **Match target windows by exe path, not title.** Neither app guards against a second instance, so a missed match opens a broken second copy.
- **A failed read must not look like zero.** Zero hides the row, so failures show `?` and busy keeps the previous value.

## Dependencies on the other apps' code

These are copies of logic with no shared contract. Re-check them when the source changes:

| Here | Source |
|---|---|
| `counts/forscher.rs` query | `602---Forscher/src-tauri/src/cmd/dashboard.rs` (`due_today`) |
| `counts/english.rs` `COUNT_SQL` | `601---English/AI-Coach/backend/app/engines/active_recall.py` (`due_count`, `_target_for`) |
| `counts/english.rs` `pending_shift` | same file (`settle_absence`, `vacation_days`, `_absence_threshold`) |

A renamed table shows `?`. **A changed shift rule or a new `target_type` is not detected** and silently skews the count.
