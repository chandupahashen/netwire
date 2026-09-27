# NetWire

NetWire is a desktop network monitor built with Tauri, React, and TypeScript.

## Development

Install the pinned pnpm version with Corepack, then install the dependencies:

```sh
corepack pnpm install
```

Start the desktop app and its Vite development server:

```sh
pnpm dev
```

`pnpm build` builds the frontend. `pnpm tauri build` creates a desktop release build.

Development requires Node.js, Rust, and the platform dependencies listed in the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).
