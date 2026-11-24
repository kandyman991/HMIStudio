# HMI Studio

Rust-powered industrial HMI engineering desktop application (Windows and Linux), with a Tauri 2 web frontend. The future runtime target remains Kotlin/Compose on Windows, Linux and Android.

## Development

Install Rust stable, Node.js 22 (22.12+), and Tauri platform prerequisites: https://v2.tauri.app/start/prerequisites/. On Linux install WebKitGTK 4.1, appindicator, librsvg and development tools. On Windows install MSVC build tools and WebView2.

```sh
npm install
npm run tauri -- icon assets/icon.svg
npm run tauri -- dev
```

`npm run check` checks TypeScript; `npm run build` builds frontend assets; `cargo test --manifest-path src-tauri/Cargo.toml` tests the Rust command core after generating icons and building frontend assets. `npm run tauri -- build` packages a local unsigned development build.

## Status

RUST-00/01 candidate: a reconstructed workspace shell, not the original HTML prototype (which was not available during recovery). Only the implemented Rust commands are functional; unavailable engineering actions are explicitly marked. No PLC connectivity or runtime export yet. Dependency lockfiles, native CI evidence, installer launch/close and visual fidelity acceptance are still required. No private source has been migrated. No open-source license is declared pending owner selection.

See [architecture](docs/ARCHITECTURE.md), [roadmap](ROADMAP.md) and [engineering rules](AGENTS.md).
