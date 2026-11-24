# Milestone plan

| ID | Scope | Status |
| --- | --- | --- |
| RUST-00 | Repository rules, architecture, CI | Merged |
| RUST-01 | Desktop shell, typed commands, Windows/Linux development installers | Merged; manual acceptance deferred to RUST-02 |
| RUST-02 | Menus, docking, tabs, shortcuts, workspace persistence | RUST-02A/B/C merged; desktop acceptance open (#10) |
| RUST-03A | Versioned project model, validation, serialization and schema | Merged |
| RUST-03B | Storage, command transactions, undo/redo | Merged |
| RUST-04 | Interactive semantic layout designer | RUST-04A draft preparation; acceptance dependency open |
| RUST-05 | Existing project import and Compose rendering parity | Planned |
| RUST-06 | Drivers, tags, bindings, alarms, recipes, search, simulation | Planned |
| RUST-07 | Standalone export and project-only publishing to paired runtimes | Planned |
| RUST-08 | Migration acceptance and installers | Planned |
| RUST-09 | Local agent, advanced graphics, 3D | Planned |

## RUST-00/01 acceptance
- Review rules and architecture; select license before distributing migrated code.
- Current implementation is a reconstruction; original HTML fidelity comparison or approved deviation deferred to RUST-02 under D-008.
- Verify committed npm and Cargo lockfiles and locked CI installs/builds.
- Verify exact-head frontend build, Rust tests, Windows MSI and Linux DEB builds.
- Independent review and applicable security/license checks required before engineering merge.
- Manual install/launch/close on both OSs, command interaction and visual acceptance are deferred to RUST-02, not claimed passed. Routine PRs do not require manual owner acceptance; see D-008.
- No production capabilities or private code transfer.

## CI-02 — Change-aware CI
Merged in PR #14 (`39eba35`): applicable tests, stable engineering gate, pinned Rust dependency/compilation cache and on-demand Windows/Linux installers with three-day retention. Documentation-only routing demonstrated on PR #9 (zero artifacts); see docs/CI_RUNNER_POLICY.md.

## HMI Forge reuse track (REUSE-01 merged; REUSE-02A in progress)
REUSE-01 merged as `238fd18` (PR #9); scoped planning baseline, not transfer clearance. REUSE-02A evidence and open gates: docs/reuse/REUSE_02A_PREFLIGHT.md. REUSE-02A pinned-source rights/provenance, advisory and sensitive-content preflight before any transfer; REUSE-02 bridge proposed under RUST-06; REUSE-03/04 map to RUST-05 import/runtime interoperability; REUSE-05 to RUST-07 deployment; REUSE-06 to RUST-08 acceptance. These mappings are planning proposals, not completed work. See docs/reuse/AUDIT_SUMMARY.md.
