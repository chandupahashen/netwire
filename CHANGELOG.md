# Changelog

NetWire changes are recorded here. This project follows the spirit of [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and uses semantic versioning for releases.

## [Unreleased]

### Added

### Changed

### Fixed

## [0.2.0] - 2026-09-28

First public release.

### Added

- Live traffic monitor: per-second throughput graph, per-app and per-host
  breakdowns, exact interface totals via OS counters.
- Byte-accurate per-connection accounting on Windows (EStats, elevation
  required) with a Live Connections view; proportional estimates elsewhere.
- 30-day SQLite history with time-machine ranges (5m–30d) and CSV export
  for history and connections.
- Custom desktop toast notifications: severity routing, per-type toggles,
  quiet hours, burst coalescing, click-through View deep-links.
- Alert feed: first-seen apps, flagged-IP contacts, global and per-app
  daily quotas, LAN device joins, with OS toast fallback.
- LAN Things tab: ARP-table device discovery with vendor guesses.
- VirusTotal file lookup from the Usage tab (user API key).
- Remote monitoring: token-authed WebSocket server plus read-only remote
  view from Settings.
- System tray with live sparkline, tooltip rates, and menu; launch at
  startup (on by default after install, toggleable).
- Themes: dark/light modes with teal, cyan, and violet accents.
- Auto-updater wired to GitHub Releases; GitHub Actions CI and
  tag-triggered release pipeline.
