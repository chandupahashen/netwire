# NetWire

NetWire is an open-source Windows desktop app for watching network activity. It is built with Tauri, Rust, React, and TypeScript.

## Features

- Live download and upload traffic graph
- Per-application usage and remote host details
- Live connection rates, marked as measured or estimated
- Local traffic history with preset and custom date ranges, plus CSV export
- Alerts and LAN device discovery
- Optional remote monitoring, per-app quotas, and VirusTotal lookups
- Dark and light themes with blue and violet accents

Some Windows per-connection counters depend on operating-system permissions. When exact counters are unavailable, NetWire marks rates as estimated.

## Download

Download the MSI or NSIS installer from this repository's **Releases** page. NetWire currently targets Windows.

## Build from source

### Requirements

- Windows with the WebView2 Runtime
- Node.js and pnpm 12.3.4 (the version is pinned in `package.json`)
- Rust stable and the Windows build tools required by Tauri

See the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for platform setup.

### Package Windows installers

Run `.\package.ps1` from PowerShell. If this checkout has no GitHub `origin`,
pass `-GitHubRepository owner/repository` so the packaged app can check that
repository for updates. The script uses the private updater key at
`.tauri/netwire-updater.key`; see [UPDATING.md](UPDATING.md) for setup.

For signing and release setup, see the [update publishing guide](UPDATING.md).

### Commands

```powershell
pnpm install --frozen-lockfile
pnpm dev
```

Build the frontend or package the desktop app:

```powershell
pnpm build
pnpm tauri build
```

The Windows installers are written under `src-tauri/target/release/bundle/`.

## Data and privacy

Traffic history is stored locally by the app. Optional VirusTotal lookups and remote monitoring use network connections. Review the settings and only enable optional services you intend to use.

## Contributing

Bug reports, feature requests, and pull requests are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Please follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Please report security issues privately. See [SECURITY.md](SECURITY.md) for reporting guidance.

## License

NetWire is licensed under the [MIT License](LICENSE).
