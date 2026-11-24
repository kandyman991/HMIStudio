# HMI Studio — curated engineering handoff

## Current milestone: RUST-00/01 (2026-11-24)
- Repository: this public HMIStudio repository. Main observed at 196d40fdb61a45c19883cf914c2c0f44af0343d9.
- [PR #1](https://github.com/kandyman991/HMIStudio/pull/1) remains draft. Its prior observed source head b11912d600080e3c8176d15cd7e06a20ea244863 passed both hosted Windows MSI and Ubuntu DEB build/test/upload jobs in [Desktop CI run 45374916620](https://github.com/kandyman991/HMIStudio/actions/runs/45374916620).
- The first shell has a React workspace, typed Tauri IPC and Rust in-memory insertion/selection. It is not a complete editor. No PLC connectivity or runtime generation is implemented.
- Original HTML prototype was not recovered; the current shell is a reconstruction. Do not claim exact visual fidelity.
- This PR adds repository continuity rules and a read-only snapshot workflow. Prior CI does not prove the new head passed.
- Read-only handoff workflow passed on predecessor 80a6b74ed29b58aa1068f549289ed3bcc6c3f431 ([run 45376915780](https://github.com/kandyman991/HMIStudio/actions/runs/45376915780)).
- Dependency lockfiles generated using npm and Cargo; local npm ci, TypeScript/Vite build, icon generation, Cargo locked metadata and rustfmt check passed. Native preflight unavailable because host lacks GTK/WebKit development libraries; no host provisioning performed. Both registries verified public. Locked native test and packaging CI must run on final head.
- Remaining: review PR independently, obtain exact-head CI, compare prototype, install/launch/close on Windows and Linux. Do not mark RUST-00/01 accepted until gates are met.
- Parallel assignment: RUST-03A project model may proceed in its own independent crate. Its branch/PR/test state is not verified here. Inspect live GitHub before integration; do not overwrite concurrent work.
- No private source has been imported. No license selected. No production or physical-device acceptance claimed.

## Independent review observation — 2026-11-24
- Reviewed predecessor `0f2dddb810f617341d774534170fffc0e8e41a5d`: [Desktop CI](https://github.com/kandyman991/HMIStudio/actions/runs/45377768471) and [handoff CI](https://github.com/kandyman991/HMIStudio/actions/runs/45377768467) succeeded. Both desktop jobs passed npm ci, frontend build, rustfmt, locked Rust tests, locked packaging, lockfile-diff check and artifact upload. Registry sources in both lockfiles are public; this is not a complete vulnerability or license audit.
- Review found inconsistent lockfile status in decision/policy records and a UI command overlap risk: the React busy flag alone is not a synchronous single-flight guard, and menu insertion could be offered before the initial snapshot. Corrected records, added an immediate single-flight guard and disabled menu insertion until snapshot availability. Native/local preflight not executed during review; require fresh CI and final-head review.
- No verified prototype comparison or approved deviation, nor Windows/Linux install/launch/close evidence. PR remains candidate and draft; no acceptance or merge claimed.

## Next exact action
Verify live PR #1 head and checks. Obtain exact-head locked CI and independent review, batch any required fixes. Recover the actual prototype or document owner-approved deviation, then perform desktop acceptance. Merge only after the agreed gates. Next milestone: RUST-02 workspace; coordinate RUST-03A integration separately.

## Maintenance
Append or supersede entries when milestone state changes. Include date, source SHA, PR/CI/test links, failures and next action. Do not paste private chat or secrets. Live GitHub evidence overrides stale observations; owner-approved documented decisions govern intent.
