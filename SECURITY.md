# Security Policy

NetWire monitors network traffic and runs with elevated privileges for full
functionality, so we treat security reports seriously.

## Supported versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

Only the latest release receives security fixes. There is no LTS branch
while the project is pre-1.0.

## Reporting a vulnerability

**Do not open a public issue for security vulnerabilities.**

Instead, report privately via a
[GitHub Security Advisory](https://docs.github.com/en/code-security/security-advisories)
on this repository ("Report a vulnerability" button under the Security tab).
Include:

- Affected version and OS
- Description of the vulnerability and its impact
- Steps to reproduce (proof of concept welcome)
- Any suggested mitigation, if you have one

You can expect an initial response within 72 hours. We will keep you
informed as a fix is developed, and we credit reporters in the release
notes unless you prefer to remain anonymous.

## Scope notes

- The remote-monitoring WebSocket server binds localhost by default; LAN
  binding (`0.0.0.0`) is an explicit opt-in. Reports should note the
  binding configuration involved.
- The VirusTotal lookup sends file hashes (never file contents) to
  virustotal.com using the user's own API key.
- Out-of-scope: vulnerabilities in third-party crates/services themselves
  (report those upstream, though a heads-up is appreciated), and social
  engineering.

## Past advisories

None published yet. When they exist, they will be listed under the
repository's Security tab and referenced in `CHANGELOG.md`.
