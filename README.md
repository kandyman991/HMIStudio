# HMI Studio

Rust-powered industrial HMI engineering desktop application (Windows and Linux), with a Tauri 2 web frontend. The future runtime target remains Kotlin/Compose on Windows, Linux and Android.

## Development

Install Rust stable, Node.js 22 (22.12+), and Tauri platform prerequisites: https://v2.tauri.app/start/prerequisites/. On Linux install WebKitGTK 4.1, appindicator, librsvg and development tools. On Windows install MSVC build tools and WebView2.

```sh
npm ci
npm run tauri -- icon assets/icon.svg
npm run tauri -- dev
```

`npm run check` checks TypeScript; `npm run build` builds frontend assets; `cargo test --locked --manifest-path src-tauri/Cargo.toml` tests the Rust command core after generating icons and building frontend assets. `npm run tauri -- build -- --locked` packages a local unsigned development build.

## Status

RUST-00/01 merged (PR #1): a reconstructed workspace shell, not the original HTML prototype (which was not available during recovery). RUST-03A/03B model and engineering core are merged. RUST-02A candidate replaces the demo engine with the authoritative in-memory project session; unavailable actions remain explicitly disabled. No PLC connectivity or runtime export yet. Dependency lockfiles are committed; exact-head native CI and review passed for PR #1; dependency residual risks and redistribution obligations tracked in issue #4. Manual installer launch/close and visual fidelity acceptance are deferred to RUST-02 under D-008, not passed. No private source has been migrated. No open-source license is declared pending owner selection.

See [architecture](docs/ARCHITECTURE.md), [roadmap](ROADMAP.md) and [engineering rules](AGENTS.md).
