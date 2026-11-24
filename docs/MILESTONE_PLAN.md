# Detailed milestone plan

ROADMAP.md is the summary. This plan defines engineering merge gates and milestone acceptance, not claims of completion. Binding decision D-008 separates them: routine PRs require compilation, automated tests, exact-head CI, independent review, applicable dependency security/license checks and installer generation when packaging changes. Manual acceptance is required at designated milestone boundaries, not each intermediate PR. Record any deferred tests and their target milestone. See CHAT_HANDOFF.md for current evidence.

| Milestone | Dependencies | Acceptance |
| --- | --- | --- |
| RUST-00/01 foundation | None | Continuity rules, lockfiles, exact-head native builds/tests, installer generation and independent review; manual Windows/Linux install/launch/close and prototype comparison or approved deviation deferred to RUST-02 under D-008 |
| RUST-02 workspace | RUST-01 | Menus, dockable panels, shortcuts, tabs and persistent workspace state; manual Windows/Linux install/launch/close, command interaction and visual acceptance including deferred RUST-01 prototype comparison or approved deviation |
| RUST-03A project model | Independent | Framework-neutral versioned model, validation, deterministic serialization, schemas, fixtures and tests |
| RUST-03B engineering operations | RUST-03A | Transactional commands, project storage, undo/redo, recovery and persistence tests |
| RUST-04 layout editor | RUST-02, RUST-03B | Visual component operations, nested constraints, alignment and responsive preview; manual desktop designer acceptance |
| RUST-05 interoperability | RUST-04 | Authorized import fixtures and equivalent Compose rendering verified across target profiles; manual interoperability acceptance |
| RUST-06 engineering systems | RUST-03B | Tags, drivers, bindings, alarms, recipes, search and isolated simulation tests |
| RUST-07 deployment | RUST-05, relevant RUST-06 | Standalone bundles and project-only publishing, pairing and trust verification with platform-specific tests; manual deployment acceptance |
| RUST-08 migration acceptance | Prior milestones | Compatibility, installation and acceptance on Windows/Linux/Android runtime targets |
| RUST-09 advanced engineering | Stable core | Optional AI, vector/3D features; separate scoped validation |

RUST-00/01 merged as 7a07d7e (PR #1); manual acceptance deferred to RUST-02. RUST-03A merged as 0ccf044 (PR #2). RUST-03B merged as 101c8f3 (PR #3), exact-head core/desktop/handoff CI passed. RUST-02A/B merged (PRs #5 and #6); RUST-02C merged (PR #8, 4ca18a59); manual desktop acceptance remains open in #10. RUST-04A draft preparation is limited to nested flow insertion and sibling ordering; no dependency acceptance or visual parity claimed. See NATIVE_PERSISTENCE.md. See WORKSPACE_IMPLEMENTATION.md for slices and outstanding acceptance. Issue #4 remains open. No filesystem containment, runtime or production acceptance claimed.
