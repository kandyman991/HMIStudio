# Milestone plan

| ID | Scope | Status |
| --- | --- | --- |
| RUST-00 | Repository rules, architecture, CI | Candidate |
| RUST-01 | Desktop shell, typed commands, Windows/Linux development installers | Candidate |
| RUST-02 | Menus, docking, tabs, shortcuts, workspace persistence | Planned |
| RUST-03 | Versioned project model, storage, command transactions, undo/redo | Planned |
| RUST-04 | Interactive semantic layout designer | Planned |
| RUST-05 | Existing project import and Compose rendering parity | Planned |
| RUST-06 | Drivers, tags, bindings, alarms, recipes, search, simulation | Planned |
| RUST-07 | Standalone export and project-only publishing to paired runtimes | Planned |
| RUST-08 | Migration acceptance and installers | Planned |
| RUST-09 | Local agent, advanced graphics, 3D | Planned |

## RUST-00/01 acceptance
- Review rules and architecture; select license before distributing migrated code.
- Recover original HTML and compare fidelity; current implementation is a reconstruction.
- Verify committed npm and Cargo lockfiles and locked CI installs/builds.
- Verify exact-head frontend build, Rust tests, Windows MSI and Linux DEB builds.
- Install/launch/close on both OSs, verify Rust command responses, inspect visual design.
- No production capabilities or private code transfer.
