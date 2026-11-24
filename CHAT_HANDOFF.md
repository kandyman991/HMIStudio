# HMI Studio — curated engineering handoff

## Current milestone: RUST-00/01 (2026-11-24)
- Repository: this public HMIStudio repository. Main observed at 196d40fdb61a45c19883cf914c2c0f44af0343d9.
- [PR #1](https://github.com/kandyman991/HMIStudio/pull/1) remains draft. Its prior observed source head b11912d600080e3c8176d15cd7e06a20ea244863 passed both hosted Windows MSI and Ubuntu DEB build/test/upload jobs in [Desktop CI run 45374916620](https://github.com/kandyman991/HMIStudio/actions/runs/45374916620).
- The first shell has a React workspace, typed Tauri IPC and Rust in-memory insertion/selection. It is not a complete editor. No PLC connectivity or runtime generation is implemented.
- Original HTML prototype was not recovered; the current shell is a reconstruction. Do not claim exact visual fidelity.
- This PR adds repository continuity rules and a read-only snapshot workflow. Prior CI does not prove the new head passed.
- Remaining: generate and commit npm/Cargo lockfiles, enforce locked builds, review PR independently, obtain exact-head CI, compare prototype, install/launch/close on Windows and Linux. Do not mark RUST-00/01 accepted until gates are met.
- Parallel assignment: RUST-03A project model may proceed in its own independent crate. Its branch/PR/test state is not verified here. Inspect live GitHub before integration; do not overwrite concurrent work.
- No private source has been imported. No license selected. No production or physical-device acceptance claimed.

## Next exact action
Verify live PR #1 head and checks. Generate both dependency lockfiles in the development checkout, switch CI to locked dependency commands, run preflight, batch fixes, and obtain exact-head CI and review. Recover the actual prototype or document owner-approved deviation, then perform desktop acceptance. Merge only after the agreed gates. Next milestone: RUST-02 workspace; coordinate RUST-03A integration separately.

## Maintenance
Append or supersede entries when milestone state changes. Include date, source SHA, PR/CI/test links, failures and next action. Do not paste private chat or secrets. Live GitHub evidence overrides stale observations; owner-approved documented decisions govern intent.
