# Detailed milestone plan

ROADMAP.md is the summary. This plan defines acceptance, not claims of completion. See CHAT_HANDOFF.md for current evidence.

| Milestone | Dependencies | Acceptance |
| --- | --- | --- |
| RUST-00/01 foundation | None | Continuity rules, lockfiles, exact-head native builds/tests, original prototype comparison or approved deviation, Windows/Linux install/launch/close, independent review |
| RUST-02 workspace | RUST-01 | Menus, dockable panels, shortcuts, tabs and persistent workspace state; tested on desktop targets |
| RUST-03A project model | Independent | Framework-neutral versioned model, validation, deterministic serialization, schemas, fixtures and tests |
| RUST-03B engineering operations | RUST-03A | Transactional commands, project storage, undo/redo, recovery and persistence tests |
| RUST-04 layout editor | RUST-02, RUST-03B | Visual component operations, nested constraints, alignment and responsive preview |
| RUST-05 interoperability | RUST-04 | Authorized import fixtures and equivalent Compose rendering verified across target profiles |
| RUST-06 engineering systems | RUST-03B | Tags, drivers, bindings, alarms, recipes, search and isolated simulation tests |
| RUST-07 deployment | RUST-05, relevant RUST-06 | Standalone bundles and project-only publishing, pairing and trust verification with platform-specific tests |
| RUST-08 migration acceptance | Prior milestones | Compatibility, installation and acceptance on Windows/Linux/Android runtime targets |
| RUST-09 advanced engineering | Stable core | Optional AI, vector/3D features; separate scoped validation |

Current RUST-00/01 candidate in PR #1, earlier head CI passed. RUST-03A parallel work assigned; live branch/PR status must be checked. All other milestones planned. Update this document and ROADMAP.md when statuses actually change.
