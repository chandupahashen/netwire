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

## Releasing (maintainers)

Releases are fully automatic: pushing a version tag builds the installers
and publishes a GitHub Release with updater metadata.

1. **Sync the version** in all three places: `package.json`,
   `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml`. Move the
   `CHANGELOG.md` `[Unreleased]` entries into a versioned section.
2. **One-time setup — signing secret.** The release signs updater artifacts
   with the repo's minisign keypair (public key lives in
   `tauri.conf.json` → `plugins.updater.pubkey`). Add the **private key
   content** as a repository secret named `TAURI_SIGNING_PRIVATE_KEY`
   (Settings → Secrets and variables → Actions → New repository secret).
   Never commit the private key. Without this secret the release job fails
   at the signing step — exactly like a local `tauri build` without the
   env var does.
3. **Tag and push:** `git tag v0.1.0; git push origin v0.1.0`. The tag must
   match the three package versions or the workflow fails fast with a
   clear error.
4. The `Release` workflow then: installs deps → checks formatting/backend →
   builds NSIS + MSI installers → signs them → publishes the GitHub Release
   with `latest.json` for the in-app updater (NSIS preferred).

To test the pipeline without shipping: push to a branch and open a PR —
the `CI` workflow runs the same frontend build, `cargo fmt --check`, and
`cargo check --locked` steps.

## Code of Conduct

By participating you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md).
