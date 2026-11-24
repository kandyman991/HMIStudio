# Architecture and migration

Established requirements: Rust engineering desktop; customizable HTML-based workspace; preserve Kotlin/Compose runtime targets Windows/Linux/Android; standalone runtime + project bundle AND project-only publishing. Development/simulation independent from production acceptance.

Proposed implementation: React/TypeScript UI calls a small typed Tauri IPC command dispatcher. Rust owns state and engineering mutations. No generic eval, arbitrary shell, filesystem or network bridge. UI owns transient presentation state only. The versioned project model will become the shared interchange contract; semantic containers, sizes, constraints, tags, events and assets map to Compose, not arbitrary HTML translation.

Current shell is an independently reconstructed design, not a verified port of the original prototype. The demo state is in memory; no persistence or actual runtime generation. Commands for adding/selecting components demonstrate IPC only. Next: command IDs and transaction model, versioned schema, undo/redo, import compatibility fixtures, layout parity tests.

Migration: keep legacy private repository intact. Audit each candidate module for licensing and sensitive content before any public transfer. Preserve validated runtime pairing, deployment listener and publishing behavior; do not assume protocol compatibility until tested. Deployment signing and actual machinery acceptance remain separate. Build both export modes using the same interchange contract.

Risks: WebView differences, editor/runtime layout parity, dependency licensing, public CI trust, artifact signing, IPC validation. Validate via cross-platform builds, install/close tests, deterministic fixtures and real-device runtime acceptance. No production use claimed.
