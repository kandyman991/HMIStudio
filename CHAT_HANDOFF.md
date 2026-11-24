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

## Owner-directed acceptance change — 2026-11-24 (D-008)
- Supersedes the earlier manual merge gates above. PR #1 predecessor 3767484d2afd590273ee7347f4674f7e655d8289 passed [Desktop CI](https://github.com/kandyman991/HMIStudio/actions/runs/45379956997) and [handoff CI](https://github.com/kandyman991/HMIStudio/actions/runs/45379957113).
- Manual Windows/Linux install/launch/close, command interaction and original prototype comparison or approved deviation are explicitly deferred to RUST-02 workspace acceptance. No manual test or visual fidelity result claimed. Do not block routine development or request owner installation before then.
- PR #2 RUST-03A is open at observed head ca60316e163eb0441e7d2eb9ccb3be2900d0dcfd; verify its own CI and review. Do not overwrite concurrent work.

## Integration observation — 2026-11-24
- PR #1 merged as 7a07d7e74cc156ed76238e89068ff320b84a5978; reviewed source 1ec641d6b668e2343eb1ad187eb161c86b191d82 passed desktop CI 45395978746 and handoff CI 45395978594. Dependency concerns and third-party distribution obligations tracked in #4. RUST-01 manual acceptance remains deferred to RUST-02.
- PR #2 predecessor ca60316e163eb0441e7d2eb9ccb3be2900d0dcfd passed project model CI 45385961404. Reconciled with main without conflicts; corrected portable paths and action read permissions, 76 local tests passed, crate advisory scan clean. See docs/PROJECT_MODEL_VALIDATION.md. Final corrected-head CI still required.
- PR #3 observed head 33db7fcdf51a5e43a4da703af75e5f3e3919f76d is stacked on ca60316; preserve its ancestry and integrate updated main before review.
## RUST-03B integration review — 2026-11-24
- PR #2 merged as 0ccf044d2a65cc86400e053647e95520fcfe98c1; source 36943f5 passed project model CI 45405558411, desktop CI 45405558172, handoff 45405558243. 76 local model tests passed.
- PR #3 predecessor 33db7fcdf51a5e43a4da703af75e5f3e3919f76d passed engineering core CI 45399624280 and project model CI 45399623931. Reconciled with main without conflicts. Persistence and ID retirement findings corrected; 58 core tests passed locally, no lockfile advisory findings. See docs/ENGINEERING_CORE_VALIDATION.md. Corrected-head hosted CI pending.
- No Tauri integration, production functionality, real power-loss or Kotlin parity acceptance yet. Issue #4 remains. Manual desktop acceptance at RUST-02.
## RUST-02A start — 2026-11-24
- PR #3 merged as 101c8f311a5fb8110ccfe127c0f4edde39c100f0. Corrected source 897693263da6e1d3f5644944fe3bc566288a5cdf passed core CI 45408061199, desktop CI 45408060862, handoff CI 45408061233. Both platform jobs in both matrices succeeded.
- Branch feat/rust-02a-session-workspace starts from that main. Replaces demo Engine with restricted ProjectSession bridge; tabs, layer selection, basic edits, undo/redo, diagnostics, panel visibility. Local frontend build and three isolated host tests passed. Native CI pending. No disk operations exposed. See docs/WORKSPACE_IMPLEMENTATION.md.
- Retain issue #4 and D-008 manual acceptance. No private source migration.
## Next exact action
Review RUST-02A candidate and exact-head hosted desktop CI, then implement remaining workspace docking and trusted native persistence slices. Require RUST-02 manual Windows/Linux acceptance at milestone boundary.

## Maintenance
Append or supersede entries when milestone state changes. Include date, source SHA, PR/CI/test links, failures and next action. Do not paste private chat or secrets. Live GitHub evidence overrides stale observations; owner-approved documented decisions govern intent.
