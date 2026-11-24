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
## RUST-02A independent review — 2026-11-24
- Reviewed PR #5 predecessor 9874868f1570bf7b7597d6869842ff6e68b38ef2 against main 101c8f311a5fb8110ccfe127c0f4edde39c100f0 (zero commits behind). Source review found inspector blur-triggered IPC could silently swallow the next toolbar action, rejected values could remain displayed, and selection could change during a pending operation.
- Corrective commit c258c10953f63df31ee573e0b153d73f2813a860 uses explicit Apply/Enter and Escape reset for inspector fields, guards command overlap and navigation, and clears stale selection on screen creation. No new dependencies. Static source inspection only; frontend build and native tests have not yet run on corrected source. Original desktop CI 45410119932 was in progress at review; handoff CI 45410119886 passed on the predecessor only.
- Review of Cargo lock delta found three added registry packages already used by the model; issue #4 residual advisories remain. No fresh independent advisory scan performed. No disk access or new native integration authorized. Explicit manual RUST-02 acceptance remains deferred under D-008.
## RUST-02A merge and RUST-02B candidate — 2026-11-24
- PR #5 merged as d17f299c4173f55e67bb30f0c9492f6c72039bf7 after independent source review and exact-head ee7995608a9a254ce32a8bb634f288c5569b0cfa Desktop CI 45410721032 and handoff CI 45410721077. Both desktop jobs passed locked tests, build, installer and lockfile checks. Manual acceptance remains deferred under D-008. Issue #4 remains open; no fresh independent advisory scan claimed.
- RUST-02B branch feat/rust-02b-workspace starts from that merged main. Adds bounded side dock swapping, pointer/keyboard resizing, three predefined layouts, versioned preferences with v1 migration, session-only screen tab lifecycle, and a shared command registry. Adds five state regression tests to npm build. No new dependencies or native permissions. No local test or visual acceptance claimed by this implementation session.
- Docking is limited to left/right swapping and a bottom diagnostics dock; arbitrary floating/nested docking and user-named presets are not implemented. Tab state deliberately does not persist across restarts of the in-memory project. RUST-02C still owns native persistence.
## RUST-02B merge and RUST-02C candidate — 2026-11-24
- PR #6 merged as 264ace7e47d3c16c21c8415875fb64db1bf7c1d0. Source 77d020fcf1876ff94a4d06315509f8c59df89d2a was independently reviewed against d17f299c (zero behind); Desktop CI 45413654836 and handoff CI 45413654872 passed. No blocking findings, new dependencies or permissions. Manual keyboard/dock checks deferred to RUST-02.
- RUST-02C branch feat/rust-02c-native-persistence starts from that merge. Native project dialogs, OS advisory locks, guarded storage, 30-second Rust autosave, explicit recovery and close/save/discard flow. See docs/NATIVE_PERSISTENCE.md for decisions, local evidence and limitations.
- Local official Node 22 frontend build and 5 isolated Rust storage-host tests passed; native Tauri compilation unavailable locally (missing WebKitGTK development libraries). No local installation or visual acceptance. Exact-head hosted desktop CI and independent review required. Issue #4 remains.
## RUST-02C independent review — 2026-11-24
- PR #8 predecessor de884bbf087b2d87b0b26df83361e0a2eb83b128 is zero commits behind main 264ace7e. Desktop CI 45416711659 and handoff CI 45416711993 passed; Windows MSI and Ubuntu DEB jobs completed locked builds/tests and installer upload.
- Review found stale recovery after undo to the saved state. Corrected session-owned autosave cleanup and redo bookkeeping; added host regression test. Static inspection only; no local compilation or test execution on the correction. No new dependencies or permissions in correction; predecessor reports targeted advisory and license checks on four new registry packages, not a full audit. Issue #4 remains.
## RUST-02C merged; partial desktop evidence — 2026-11-24
- PR #8 merged at 4ca18a5973e0b624d06318c4767812989802c29b; exact source 620c09cc347c859b9e9ad8431890fdcfcf55872c passed Desktop CI 45418769428, engineering core CI 45418769365 and handoff CI 45418769375. Native logs show 9 Windows / 10 Linux tests passed, including the new regression.
- Linux CI DEB extracted into isolated temporary storage; app launched with isolated XDG directories and X11, mapped a window and exited normally after WM_DELETE_WINDOW. No package-manager install, interactive/visual acceptance or native Wayland test. Windows desktop unavailable. Acceptance issue #10 tracks the outstanding D-008 gates.
- RUST-04A preparatory draft from the merged source: nested row/column insertion and sibling order, typed engine transactions, pure selection-rule tests. No new dependencies, schema changes or native permissions. No local build/test claim. Existing #7/#9 are independent.
## Next exact action
Complete issue #10 before marking RUST-02 accepted; review and test RUST-04A draft, retain dependency gate, then extend semantic layout editing and preview.

## Maintenance
Append or supersede entries when milestone state changes. Include date, source SHA, PR/CI/test links, failures and next action. Do not paste private chat or secrets. Live GitHub evidence overrides stale observations; owner-approved documented decisions govern intent.

## REUSE-01 audit review observation — 2026-11-24
- PR #9 predecessor `268a12dcc8ac9476616abc25f6b3566a2709b678` passed Desktop CI 45432107611 and handoff CI 45432107921; new reconciled head requires fresh CI. Main observed `58d83ca540463d9507c1cd76555d0bac5bb6378b` (#8 and #11 merged). See docs/reuse/AUDIT_SUMMARY.md for owner decisions and partial independent source verification, findings and limitations.
- No private code transferred; no runtime or PLC acceptance. REUSE-01 remains candidate until final review and CI. Next: finish review and merge PR #9 after exact-head CI, then REUSE-02A source/provenance/advisory/sensitive-content preflight in a separate branch. Do not import before clearance or integrate a bridge before applicable decisions and PR #7 integration.
