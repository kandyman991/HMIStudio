# RUST-02 — Professional desktop workspace

## RUST-02A: authoritative in-memory session bridge (merged)

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

## RUST-02B: workspace presentation (merged)
- Explorer and inspector can exchange side docks; diagnostics is anchored at the bottom. All three docks can hide/show and resize using pointer handles or focused arrow/Home/End keys. Six bounded sizes avoid uncontrolled workspace geometry and new CSP exceptions.
- Standard, Design and Diagnostics presets; reset and v2 preference restoration with validation, bounds and v1 visibility migration. Only presentation state enters localStorage.
- Open screens from explorer; close individual tabs without deleting the screen; cycle tabs using Ctrl/Cmd-Tab and Ctrl/Cmd-Shift-Tab; Ctrl/Cmd-W closes the active tab. Reconcile tabs with authoritative screen IDs after each response, including undo/redo. In-memory project means tab IDs are intentionally session-only.
- Shared registry controls menu/toolbar availability, check marks and shortcuts. Ignore global edit shortcuts inside text inputs. Planned commands remain disabled.
- npm test runs five Node test cases on pure workspace state (corruption, migration, bounded sizes, presets, tab lifecycle and invalid IDs). npm build runs these tests before TypeScript and Vite, using the existing Node 22 CI configuration. No local test execution claimed; exact-head CI and independent review pending.
- Bounded docking only: arbitrary floating/nested docking and custom named preset management are not implemented. Manual Windows/Linux keyboard, layout, resize and restart checks remain pending at RUST-02 acceptance.

## RUST-02C candidate
See [Native project persistence](NATIVE_PERSISTENCE.md). Native dialogs, trusted local storage, autosave, recovery and dirty close lifecycle implemented; native CI and independent review pending.

## Remaining slices (proposed sequencing)
- RUST-02C: host-owned native dialog, single-writer lock, trusted project and
  sidecar path validation, symlink containment, autosave and explicit recovery,
  save conflict handling, close/save/discard lifecycle. Review filesystem
  and dependency risks before enabling; no webview filesystem permissions.
- RUST-02 acceptance: manual Windows/Linux installer, launch, interaction,
  close, dock/preferences persistence, file lifecycle and original prototype
  comparison or approved deviation under D-008. RUST-04 owns designer layout
  operations and renderer fidelity; RUST-05 owns Compose parity.
