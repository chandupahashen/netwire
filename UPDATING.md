# Updating NetWire

## In-app updates

Release builds include the Tauri updater. When a new version is published:

1. NetWire checks for updates on launch (and daily while running).
2. Accept the prompt to download and install — the app restarts itself.
3. Your history, quotas, rules, and settings are preserved: they live in
   the application-data database (`netwire.db`), which updates never touch.

If the updater reports an error, fall back to a manual install below.

## Manual install (Windows)

1. Download the latest `NetWire_<version>_x64-setup.exe` from the
   [releases page](../../releases).
2. Run the installer (per-machine install requires elevation).
3. Launch NetWire — existing data is picked up automatically.

Portable use is not officially supported yet; running the installed binary
with a custom data directory is untested.

## Data safety across updates

- **Preserved:** traffic history, alerts, device list, quotas, firewall
  rules (when they land), notification/remote/VirusTotal settings.
- **Reset on major schema changes:** if a future release migrates the
  database format, the migration runs automatically on first launch. If a
  migration ever fails, the app falls back to a fresh database rather than
  refusing to start — back up `%APPDATA%\com.netwire.app\netwire.db` first
  if your history matters to you.

## Downgrading

Downgrades are not tested. If you must roll back, uninstall, install the
older release, and be prepared to delete `netwire.db` if the app reports a
schema error on launch.

## For contributors

- Version bumps: `package.json`, `src-tauri/tauri.conf.json`, and
  `src-tauri/Cargo.toml` must stay in sync.
- Document user-facing changes under `[Unreleased]` in `CHANGELOG.md`;
  maintainers move them into a versioned section at release time.
