# Contributing to NetWire

Thanks for considering a contribution. This guide covers setup, workflow,
and what makes a pull request easy to merge.

## Prerequisites

- **Node.js 22+** with **pnpm 9+** (`corepack enable` then `corepack prepare pnpm@latest --activate`)
- **Rust stable** (via [rustup](https://rustup.rs/)) with the MSVC toolchain on Windows
- **Tauri v2 system dependencies** for your OS — see the
  [Tauri prerequisites guide](https://tauri.app/start/prerequisites/)
- Windows 10/11 recommended for the full feature set (per-flow counters need
  elevation; see below)

## Setup

```sh
pnpm install
pnpm dev        # runs the Tauri app with hot reload (frontend + Rust watcher)
```

Useful commands:

```sh
pnpm build                        # typecheck + production frontend build
cargo test --manifest-path src-tauri/Cargo.toml   # Rust unit tests
cargo check --manifest-path src-tauri/Cargo.toml  # fast backend check
```

## Running elevated

Per-connection byte counters (EStats) require administrator rights on
Windows. Without elevation the app runs in estimate mode and shows an
admin hint — that is expected behavior, not a bug. To exercise the measured
path, run your terminal as administrator before `pnpm dev`.

## Project layout

- `src/` — React + TypeScript frontend (`App.tsx`, `components/`, `types.ts`)
- `src-tauri/src/` — Rust backend:
  - `monitor.rs` — live capture (socket table + interface counters)
  - `estats.rs` — Windows per-flow byte accounting
  - `store.rs` — SQLite persistence (history, alerts, quotas, settings)
  - `notify.rs` — desktop toast center (policy + queue)
  - `enrich.rs` — rDNS, GeoIP, LAN discovery
  - `remote.rs` / `vt.rs` — remote monitoring server, VirusTotal lookup
  - `lib.rs` — commands, monitor loop, tray, app wiring

## Pull request guidelines

1. **One concern per PR.** Keep diffs focused; split unrelated changes.
2. **Verify before pushing:** `cargo test` (backend), `pnpm build`
   (typecheck + bundle). If you touched the monitor loop or IPC, boot with
   `pnpm dev` and smoke-test the affected tab.
3. **Match the existing design language.** Frontend changes should reuse the
   tokens in `src/App.css` (`--surface`, `--accent-*`, `.panel`, `.btn`,
   `.tbl`) and work in both dark and light themes.
4. **No new npm/Rust dependencies without justification** in the PR
   description — every dependency is a supply-chain and maintenance cost.
5. **Never commit secrets.** API keys (e.g. VirusTotal) live in local app
   settings only.
6. Describe user-facing changes the way a user would notice them; update
   `CHANGELOG.md` under `[Unreleased]` when behavior changes.

## Reporting bugs

Open an issue with: NetWire version, OS + version, whether running elevated,
steps to reproduce, and relevant log lines (dev mode prints `[netwire] ...`
messages to the terminal). Screenshots of the affected tab help.

## Code of Conduct

By participating you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md).
