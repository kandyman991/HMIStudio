# RUST-02 — Professional desktop workspace

## RUST-02A: authoritative in-memory session bridge (candidate)

The desktop host now owns a Mutex<ProjectSession> initialized from the public
minimal project fixture. The webview sends a restricted typed WorkspaceRequest
(snapshot, validate, execute, undo, redo); each reply contains the core response
and authoritative snapshot from the same locked session. No arbitrary path,
save, reload, close, recovery, filesystem or shell requests cross this boundary.
A separate explicit discard-and-exit command is available for this in-memory
prototype; the native close request is prevented while dirty. No persistence
claim is made. Native close behavior and confirmation need manual validation.

The UI renders actual project screens and nested layers, a structural preview,
selection, basic text and screen-name editing, insertion/deletion, undo/redo,
revision status, diagnostics, screen tabs and configurable visibility of the
explorer, inspector and diagnostics. Ctrl/Cmd-Z, Ctrl/Cmd-Shift-Z and Ctrl/Cmd-Y
are supported outside text inputs. Only panel visibility is persisted in
webview localStorage. This is not a full docking system, runtime renderer,
or semantic layout-parity implementation. Unimplemented menus are disabled.

### Local evidence, 2026-11-24

- Baseline main 101c8f311a5fb8110ccfe127c0f4edde39c100f0, merged PR #3.
- npm ci (ignore scripts), TypeScript/Vite production build and Rust formatting passed.
- Isolated non-Tauri harness compiled the exact session_host.rs source and
  ran three tests (rename/undo/redo/stale revision, allocated gauge ID, and
  denied persistence/path requests); all passed. Same tests are compiled by
  desktop CI. Harness uses independent lock resolution; native desktop
  compilation against the desktop lock remains required.
- Locked desktop metadata adds three external crates already used by the
  project model: schemars_derive 1.2.3 (MIT), serde_derive_internals 0.30.0
  and serde_path_to_error 0.1.20 (MIT OR Apache-2.0). No removals/upgrades.
- OSV scan of desktop lock after core integration returned existing foundation
  advisories only (glib, chrono, proc-macro-error), tracked in #4. Not a
  clean desktop audit; do not broaden native integration without reassessment.
- Local host lacks GTK3/WebKitGTK development libraries; no native desktop
  compilation, installer generation or visual test run locally. Hosted
  Windows/Linux CI and independent review required. No project license selected.

## Remaining slices (proposed sequencing)
- RUST-02B: dockable/resizable panels, layout presets, tab lifecycle, command
  registry, keyboard navigation and versioned workspace preference restoration.
- RUST-02C: host-owned native dialog, single-writer lock, trusted project and
  sidecar path validation, symlink containment, autosave and explicit recovery,
  save conflict handling, close/save/discard lifecycle. Review filesystem
  and dependency risks before enabling; no webview filesystem permissions.
- RUST-02 acceptance: manual Windows/Linux installer, launch, interaction,
  close, dock/preferences persistence, file lifecycle and original prototype
  comparison or approved deviation under D-008. RUST-04 owns designer layout
  operations and renderer fidelity; RUST-05 owns Compose parity.
