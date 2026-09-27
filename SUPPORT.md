# Support

## Where to ask

- **Bug reports / feature requests:** open a
  [GitHub issue](../../issues) — search existing issues first, yours may
  already be tracked.
- **Questions and troubleshooting:** open a
  [GitHub Discussion](../../discussions) (Q&A category).
- **Security issues:** see [SECURITY.md](SECURITY.md) — do not post them
  publicly.

Response times are best-effort; this is a community project maintained in
spare time.

## What to include

Help us help you — include all of the following:

1. **NetWire version** (title bar / Settings, or `package.json` version)
2. **OS and version** (e.g. Windows 11 23H2)
3. **Elevated or not** — many "wrong numbers" reports come down to running
   unelevated (estimate mode). The header shows `LIVE · MEASURED` when
   per-flow counters are active.
4. **What you expected vs. what happened**, with steps to reproduce
5. **Logs** — run with `pnpm dev` and paste lines starting with `[netwire]`
6. **Screenshots** of the relevant tab, if UI-related

## Common answers

- **Per-app bytes look estimated / "run as administrator" notice:**
  Windows requires elevation for per-flow counters. Restart elevated.
- **Country column is blank:** GeoIP needs a `GeoLite2-Country.mmdb` file in
  the app-data directory (a MaxMind license is required to download it).
- **No LAN devices found:** click Rescan on the Things tab; VPNs and some
  virtual adapters can hide the local ARP table.
- **Remote monitoring won't connect:** check the server is enabled in
  Settings, the port is reachable (firewall), and the token matches —
  the token is sent as the first WebSocket message.
