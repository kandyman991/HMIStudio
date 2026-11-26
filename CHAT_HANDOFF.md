# HMI Studio — curated engineering handoff

## DEV-01 acceptance — 2026-11-25
- Verified PR #21 merge, owner-configured synchronization credential, automatic PR #22 merge and successful no-change post-merge reconciliation. See docs/DEV_01_ACCEPTANCE.md for exact evidence. Formal acceptance recorded in canonical roadmap source; this acceptance PR still requires independent review and exact-head CI.
- Next: reconcile legacy PRs #20, #13 and #12 with current main and applicable claims; preserve scope and review each independently.

## DEV-01 review corrections — 2026-11-25
- PR #21 predecessor c08424c passed hosted Required CI and handoff. The corrected branch adds required scoped milestone changes and shared reservations, trusted GitHub reconciliation through reviewed PRs, explicit parent/related scopes and generated view consistency. No new application or installer work.
- Reconciliation reads verified GitHub observations; publication requires an approved repository-scoped ROADMAP_SYNC_TOKEN. Offline dashboard remains supported; public hosting not authorized. Shared reservation race handling is conservative; coordinate legacy PRs #12, #13 and #20 before integration.
- Next: independently inspect corrected head and exact-head CI, review synchronization permissions and activation. Do not claim a merge or acceptance before verified evidence.

## DEV-01 completion proposal — 2026-11-25
- Based on observed main `d1571fc5cd801a2b7d65b63dd3de7d25bf531817`. REUSE-02C.3 merged in [PR #19](https://github.com/kandyman991/HMIStudio/pull/19). Its exact source `9560d35eaae751a5352cb40c78015931f448a8f2` passed Desktop CI 45575498171, tag CI 45575498358 and handoff 45575498315. [PR #20](https://github.com/kandyman991/HMIStudio/pull/20) remains a separate draft in the observed snapshot. Preserve its handoff and roadmap changes when reconciling. PRs #12 and #13 remain separate.
- DEV-01 uses per-milestone structured records. Generate the detailed plan and offline dashboard with `node roadmap/generate.mjs`. Three original plans are preserved as exact migration inputs. The prior roadmap HTML was unavailable. See [roadmap/README.md](roadmap/README.md) and D-DEV-01.
- Next for DEV-01: verify exact-head hosted Required CI and handoff, resolve independent review findings and follow the existing merge policy. No desktop installer acceptance or public website deployment is needed. Issue #10 remains tracked under RUST-02/RUST-04.


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
## RUST-04A merged / RUST-04B candidate — 2026-11-24
- PR #11 merged at 58d83ca540463d9507c1cd76555d0bac5bb6378b. Reviewed source 6030fa387a3fee3bd2b13ae8779ff2c690db0db5; desktop CI 45421552926 and handoff 45421552619 passed, 7 frontend tests on both platforms, 9 Windows / 10 Linux native tests. No blocking findings.
- Issue #10 records owner Windows checklist 9/9, Linux CI and isolated launch/close, waiver of duplicate Linux manual tests. Owner explicitly authorized #11 merge while rejecting current visual direction. Visual acceptance carried into RUST-04; original HTML was unavailable at that historical observation, since recovered in PR #13. D-008 follow-up records scope.
- RUST-04B branch feat/rust-04b-layout-rendering adds base flow CSS mapping, size/arrangement editing via Rust commands, grouped toolbars/library/ruler/inspector and pure tests. No new dependencies, native permissions or schema changes. Unsupported layout/live/responsive features explicitly marked; no visual parity or local test claim.
## Next exact action
Verify fresh exact-head hosted CI and final review of corrected RUST-04B, then engineering merge when authorized. Continue RUST-04B.1 visual alignment before RUST-04C pointer manipulation; retain #10/RUST-04 visual acceptance.

## Maintenance
Append or supersede entries when milestone state changes. Include date, source SHA, PR/CI/test links, failures and next action. Do not paste private chat or secrets. Live GitHub evidence overrides stale observations; owner-approved documented decisions govern intent.

## CI-02 candidate — 2026-11-24
- Started from verified main `58d83ca540463d9507c1cd76555d0bac5bb6378b`. Owner requested change-aware CI ahead of REUSE-02A; existing PRs #7/#9/#12/#13 left intact. PR #9 exact head `34ae0c6e73b54ac30b11e3b6f72f4306922ea9af` desktop run 45438220109 and handoff 45438220116 now passed; remaining independent audit scope is not waived.
- CI-02 introduces routing tests, stable Required CI, conditional hosted jobs, manual installer dispatch, three-day artifact retention and pinned Rust cache. CI workflow changes deliberately force full validation on this PR. No local native or workflow execution claimed. Branch protection API returned 403; verify required contexts before transition. Existing #7 tag-engine audit must be preserved when reconciled. Next: independently review and verify CI-02 exact-head CI and branch-protection transition, then merge before REUSE-02A. No private source transferred.

## REUSE-01 audit review observation — 2026-11-24
- PR #9 predecessor `268a12dcc8ac9476616abc25f6b3566a2709b678` passed Desktop CI 45432107611 and handoff CI 45432107921; new reconciled head requires fresh CI. Main observed `58d83ca540463d9507c1cd76555d0bac5bb6378b` (#8 and #11 merged). See docs/reuse/AUDIT_SUMMARY.md for owner decisions and partial independent source verification, findings and limitations.
- No private code transferred; no runtime or PLC acceptance. REUSE-01 remains candidate until final review and CI. Next: finish review and merge PR #9 after exact-head CI, then REUSE-02A source/provenance/advisory/sensitive-content preflight in a separate branch. Do not import before clearance or integrate a bridge before applicable decisions and PR #7 integration.

## CI-02 merge and REUSE-01 reconciliation — 2026-11-24
- CI-02 PR #14 independently reviewed at `de86b95bdea35f08e89963ec92cfefe002976a47`; Desktop CI 45440535709 (including Required CI and both installers) and handoff 45440535863 passed; merged as `39eba35a90fdfa27bc613a14b15b9e0bb461c639`. Read-only GitHub API verification found no enforced branch protection or active branch rules on main. Manual dispatch not exercised independently.
- PR #9 predecessor `34ae0c6e73b54ac30b11e3b6f72f4306922ea9af` passed Desktop CI 45438220109 and handoff 45438220116. Reconciled four shared record conflicts retaining both tracks; new exact-head Required CI and handoff pending. Verify documentation-only routing without installer artifacts. Audit is a planning baseline, not transfer clearance. REUSE-02A remains separate after merge; no source transferred.

## REUSE-01 merged; REUSE-02A started — 2026-11-24
- PR #9 exact head `6d346ce` passed Required CI 45445225707 and handoff 45445226131; all non-applicable jobs skipped and zero installer artifacts. Merged as `238fd18a4006ced60b5459a37ff18ac1471a6e2c`. CI-02 documentation-only demonstration complete; independent manual dispatch remains pending.
- REUSE-02A evidence-only branch: pinned source and lockfile verified, limited sensitive-content scans identified one fixture endpoint requiring disposition. See docs/reuse/REUSE_02A_PREFLIGHT.md for evidence and remaining rights, advisory, notice and isolated contract validation gates. No private source transferred. Next: finish preflight checks and independent review; no import until cleared. PR #7 remains a bridge dependency.

## REUSE-02A technical validation follow-up — 2026-11-24
- PR #15 predecessor `d26373a120c3ec6b77500f6022c4bb42581fcfa7` passed Desktop Required CI 45445542303 and handoff 45445542430; non-applicable jobs skipped. Pinned private engine `92a612d104693800d7119ee4dc0114636b19002f`: isolated offline locked suite 37/37 passed; sanitized candidate contract 4/4 passed; Gitleaks original/candidate zero findings; fresh RustSec audit 2 vulnerabilities/12 warnings; 222/222 archive checksums and license census verified. No private source transferred.
- Controlled 27-file candidate allowlist and import steps: docs/reuse/REUSE_02A_IMPORT_PLAN.md; evidence and limitations: docs/reuse/REUSE_02A_PREFLIGHT.md. Transfer still blocked by substantive per-file rights/disclosure attestation and final dependency notice/license review; private Kotlin fixture parity pending. No production, device, release or bridge acceptance. PR #7 remains open; process/numeric safety and applicable owner decisions retained. Next: complete remaining clearance and review exact new #15 head/CI before separate import-only PR.

## REUSE-02A notice and fixture follow-up — 2026-11-24
- PR #15 predecessor `a8c7f9fedf8421621571a7106dfc3fd31d37ac0d` passed Required CI 45447543946 and snapshot 45447543876; unrelated jobs skipped. Private HMI Forge #57 `c8c7ef9b11729d0f603e9459946a596d1b10f2a1` contains matching sanitized fixture; Kotlin contract 3/3 and preflight routing passed locally; exact-head private CI and merge pending.
- Exact dependency license selections and source provenance assessment recorded in docs/reuse/REUSE_02A_NOTICE_REVIEW.md. Source-only scope is distinct from binary notice/release gates. Owner rights attestation for exact 27-file allowlist remains required; no source pushed to public repo. Next: record attestation, finish #57 and #15 CI/review, then separate import-only PR; retain PR #7 and safety gates for bridge.

## REUSE-02A owner rights attestation — 2026-11-24
- Owner confirmed the exact 27-file allowlist rights and no unauthorized proprietary/customer material; see D-009 follow-up and REUSE_02A_NOTICE_REVIEW.md. No production/release clearance.
- #15 predecessor `a82bc361f56087187264836dc961f8ec26a01c94` passed Required CI 45449132831 and snapshot 45449133015; new documentation head needs exact-head checks. Private #57 at `c8c7ef9b11729d0f603e9459946a596d1b10f2a1` still awaiting CI. Next: review/merge #57 and #15 after respective gates, then separate source-only import PR; no source transfer yet.

## REUSE-02B source import — 2026-11-24
- HMI Forge #57 merged `03a5887d1d763c936e5377741ee472680b700085` after exact-head Linux CI, Android API 26/34 and Android ARM passed. HMIStudio #15 merged `5173d9a60ffb87b52edde1719043bfcbff4d3be5` after Required CI and snapshot; owner authorization and both exact-head reviews recorded. Private main fixture checksums reverified.
- Branch feat/reuse-02b-engine-import: only 27 approved payload files plus PROVENANCE.md, CI validation wiring and engineering documentation. Final destination hash/mode checks, disclosure scan, fmt, shell syntax and 222-package license inventory pass. Classifier 9/9 passed. Repeat pinned audit exit 1 with 2 vulnerabilities/12 warnings; hosted audit is enforced without ignores. No engine binary artifact, sidecar, Tauri/UI wiring, runtime publishing or physical PLC use.
- Next exact action: verify import PR head, hosted Windows/Linux test-server suite and all applicable exact-head gates, address security advisories in separately reviewed changes as needed, independent review before merge; no bridge until PR #7 and supervision/numeric requirements are resolved. No manual installer acceptance for this scope. CI workflow changes trigger existing full fallback including unrelated installer checks; no engine is bundled.

## REUSE-02B dependency remediation — 2026-11-24
- PR #16 initial head `e14451343ed5bfe020d75ffde97fd557f872230b`: Desktop CI 45455020647 Windows/Linux functional tests and all other selected jobs passed except Linux engine audit and aggregate Required CI. Handoff 45455020664 passed. No merge.
- Remediation batch: exact official public async-opcua Git revision `ca18bb73bac4ff2627943c760c97123062988468` (verified GitHub commit, tree `3b8b55a602b7c2818965625bacee645dc9c0fdc9`) with rsa 0.10.0, url 2.5.9, YAML and postcard dependency corrections. Local pinned RustSec DB audit exit 0, zero vulnerabilities; one chrono maintenance warning remains (no ignore). Source-only registry/Git inventory 211+9 and 37 new/changed registry license notices verified; strengthened exact source/checksum/license gate and 15 Python tests pass. Isolated locked test-server suite 37/37 passed, including encrypted OPC UA. Original 25 non-Cargo payload files unchanged, protocol/Kotlin unchanged.
- See crates/hmi-engine/PROVENANCE.md, new dependency inventory and D-009 follow-up. This is a reviewed candidate using an unreleased upstream revision, not a new general Git-dependency policy. Return to compatible registry release when available. Next: verify new exact-head CI including audit, review the pinned upstream and dependency/notice changes, merge only after gates. No release, hardware, bridge or runtime publishing acceptance; retain PR #7 and process/numeric requirements for bridge.

- Isolated fmt, Clippy and rustdoc passed; exact final lockfile repeat audit exit 0, zero vulnerabilities, one chrono maintenance warning.

## RUST-06A review and reconciliation — 2026-11-24
- REUSE-02B PR #16 merged at `78f3a64c06d00afcc392edccad9da9fd757d0392` preserving import and remediation commits; exact-head Desktop CI 45458106267 and handoff 45458106080 passed, review 6044916091. Chrono maintenance warning and pinned unreleased OPC UA dependency remain documented release limitations.
- PR #7 reconciled from `de9e207a43b9c8ab322b1d3db33fd0832febe596` with that main, clean ordinary merge. Independent review fixes exact range/map integer comparison, safe linear transform limits, bounded operator input, shutdown/cancel admission and result handling, default allocation limit and extreme duration cancellation. See docs/TAG_ENGINE_VALIDATION.md; 50 local Rust tests, fmt/Clippy/rustdoc, 15 CI regressions passed; strict local RustSec audit zero vulnerabilities/warnings, 20 archive checksums/licenses verified. Dedicated CI exact-head checkout and cache align with CI-02; strict independent audit retained. No changed tag lockfile, UI, model, physical drivers or engine bridge.
- Next exact action: verify reconciled PR #7 exact head, fresh Required CI, handoff and dedicated audit/checks, independent final review and merge. No manual installer acceptance. REUSE-02C bridge preflight thereafter: bounded/coalesced subscriptions/resync, process supervision, framing, exact numeric limits or safe refusal, no stale command replay; preserve simulator and production boundary.

## REUSE-02C.1 bridge supervision — PR #17 candidate — 2026-11-24
- Branch `feat/reuse-02c1-bridge-supervision` at base `5fa32afa14d2b5d765e6563bef9a840c4adf2b2a` (main after PR #7). Original candidate pushed at a7277f4e9f1ffb05a2c8e1824e481eec1269c47b; author results below are historical. Earlier uncommitted bounded-transport, event-sink, framing, supervisor and O-5 changes are preserved.
- Added:
  - bounded S7 command queue (8) with not-transmitted overflow and a coalesced demand wake;
  - stop by flag rather than queue, with a bounded join;
  - transmit-time refusal of stopping, withdrawn and pre-connection writes;
  - engine supervision of in-flight `write`/`browse` against client disconnect, closed output, event failure and `shutdown`, with ordered deferral (max 32);
  - `OUTCOME_UNKNOWN:` / `NOT_TRANSMITTED:` driver outcomes;
  - immediate release of pulses whose start outcome is unknown;
  - no replay or re-arming.
  
  Protocol 3 and Kotlin are unchanged. A test-only driver factory and a simulated PLC transport were also added. Details are in docs/reuse/REUSE_02C1_BRIDGE_SUPERVISION.md.
- Local Linux results (rustc 1.100.0, `--locked`):
  - fmt clean, and strict Clippy (`-D warnings`) clean for hmi-engine (default and test-server) and tag-engine;
  - hmi-engine 53/53 with test-server, 35/35 with default features; tag-engine 61/61;
  - 13 new deterministic tests; 7 of 8 re-introduced defects caught.
  
  The intermittent tag-engine `bridge_supervisor` failure (an `ETXTBSY` fixture race) was fixed: 0 failures in 200 parallel runs, against 41 in 100 before. Simulated drivers only.
- Open findings F-1 to F-9: arming across reconnection, unconfirmed release after an unknown start, release bounds of non-S7 drivers, no explicit protocol-3 uncertain code, bridge still not wired. Next: verify corrected-head hosted CI and complete independent review; owner decisions on F-1 and F-2 before any physical write; then bounded subscription delivery, resync and request correlation.

- Independent corrective review: found starvation of due pulses under deferred/ready traffic; added explicit dispatch-boundary deadline priority, fair ordinary-work selection preserving request FIFO, and a simulated deferred-browse regression. Corrected-head CI and final review required. No real PLC or installer acceptance. REUSE-02C.2 follows merge.

- Independent corrective Linux preflight: 54 engine test-server, 36 default and 61 tag-engine tests passed, engine strict test-server Clippy and fmt passed; original-scheduler mutation failed the added regression as expected. No lockfile change. Hosted gate still required.

## REUSE-02C.1 merged; REUSE-02C.2 dispatch foundation candidate — 2026-11-25
- PR #17 merged at `dbdeeb2ea74555ba879380168e17c923a1c41da2`; exact source `8e900bc7c1883c0ad9fac3103e21cbb322915b97`, Required CI 45489970600 (Windows/Linux engine tests, Linux notice and advisory gate), dedicated tag CI/audit 45489971060 and handoff 45489970760 passed. Installer builds skipped. Independent review verdict recorded as GitHub review 6053509356 (comment; formal self-approval refused by GitHub). No hardware acceptance.
- Branch feat/reuse-02c2-bridge-dispatch starts from that verified merge. Typed protocol-3 messages, single-flight request correlation, absolute reply deadline, bounded event mailbox, sticky failure and explicit resync-required invalidation, read-only browse fixture operation. No dependencies changed; 70 local locked tag-engine tests, strict Clippy, rustdoc and fmt passed. See docs/reuse/REUSE_02C2_BRIDGE_DISPATCH.md.
- Next exact action: verify new PR exact-head CI and independent review, then implement coalesced bounded tag subscriptions, generation-scoped cache invalidation and authoritative resynchronization baseline in isolated simulation. No configure/start/writes or Studio wiring exposed; no complete resynchronization or milestone acceptance claimed.

## REUSE-02C.2 PR #18 completion candidate — 2026-11-25
- Predecessor d2b9b96 passed Desktop CI 45569339342, tag CI 45569339255 and handoff 45569339262. Main remained dbdeeb2; no concurrent branch changes observed.
- Replaced unbounded tag subscriptions with bounded latest-state mailboxes (64 subscribers, 256 events and 4 MiB serialized payload per mailbox), ordered coalescing barriers, explicit baseline generation marker, overflow/resubscribe and discard on unsubscribe. Isolated read-only loopback bridge requires complete good/type-correct values for each expected point before publishing; generation-aware refresh and fresh process recovery reject stale data. Maximum four synthetic scalar points for this slice; no physical endpoints, write or arming controls.
- Local locked/offline Linux: 80 ordinary tag-engine tests plus 1 explicitly invoked real freshly built engine-child test passed; strict Clippy, rustdoc and fmt passed. No dependency or protocol changes. Added cross-platform real-child check to dedicated hosted CI; exact-head CI pending. See docs/reuse/REUSE_02C2_BRIDGE_DISPATCH.md.
- Next exact action: verify corrected exact-head hosted CI and dependency gates, independently review PR #18 and fix findings before merge; preserve isolated scope and subsequent general EngineDriver/numeric/termination/UI work. No hardware or production acceptance.

- Independent corrective review on PR #18 predecessor e12569e: reproduced false initial publication from pre-acknowledgement loopback events; reject events queued before configure/start acknowledgements and invalidate. Added deterministic fixture covering both phases; original implementation failed regression, corrected passed. Isolated locked/offline Linux: 81 ordinary tag-engine tests, strict Clippy, fmt, diff and lockfile checks passed. Corrected exact-head hosted CI, final independent review and merge pending; real-child test required on hosted Windows/Linux.

## REUSE-02C.2 merged; REUSE-02C.3 isolated EngineDriver candidate — 2026-11-25
- PR #18 merged at 9dc343b052f456ad507dfc69e2a82fb28345f108; exact reviewed head 8d81c149dffdd5c0112356de9a76dbc9f4c03e95, review 6053747116, Desktop Required CI 45571815356, dedicated Windows/Linux real child and audit 45571815369, handoff 45571815372 passed. Installer jobs ran due workflow change; no manual or hardware acceptance.
- Branch feat/reuse-02c3-engine-driver from verified merge: explicitly registered isolated read-only EngineDriverFactory, exact checked scalar protocol-3 conversions, cancellable startup, generation-aware reads, child-loss bad quality and fresh baseline recovery. No dependency/schema/wire changes or write/physical/UI functionality. See docs/reuse/REUSE_02C3_ENGINE_DRIVER.md.
- Linux locked/offline: 85 ordinary tag-engine tests, strict Clippy, rustdoc, fmt, lockfile/diff checks passed. Two explicit real-child smoke tests passed with an existing previously built engine binary; exact-source fresh binary tests required on hosted Windows/Linux. Next exact action: independent review and exact-head CI before merge; preserve simulation scope and follow up with general mapping/quality/write termination.

- Independent static review of PR #19 predecessor 8a78179c5f7b3a702ecf68c979168e2c3bf47e69: found that a post-baseline wire-valid but out-of-range scalar was returned as an item TypeMismatch without invalidating the generation. Corrected to validate the complete decoded snapshot before returning any read results and disconnect on failure, returning ConnectionLost. Added a deterministic synthetic child regression (negative Long for UInt32, bad quality/fault, fresh changed baseline). New regression and corrected source require hosted validation; no local execution claimed by reviewer. No physical endpoints, writes, dependencies or protocol changes. Next: verify exact corrected head Required CI, tag CI (including fresh children on Windows/Linux and audit), handoff and complete independent review before guarded merge.

## REUSE-02C.4A write lifecycle — draft candidate — 2026-11-25
- Branch `feat/reuse-02c4a-write-lifecycle` starts from verified main `9dc343b052f456ad507dfc69e2a82fb28345f108` (PR #18 merge). PR #19 then merged as `d1571fc5cd801a2b7d65b63dd3de7d25bf531817`; reconciled by ordinary merge (shared-record conflicts only, both entries kept). Open PRs observed: #19 REUSE-02C.3 (`8a78179`, draft, under independent review, branch untouched), #12, #13. Only `crates/hmi-engine/` and docs changed; `crates/tag-engine/` EngineDriver untouched. Protocol 3, Kotlin message set, dependencies and `Cargo.lock` unchanged.
- Added:
  - engine-side `Limits`, with every driver wait bounded: write/pulse start/browse 10 s, each release 5 s, each driver stop 5 s;
  - write and pulse-start timeouts reported as `DRIVER_ERROR`/`OUTCOME_UNKNOWN:`, with a timed-out pulse start released at once;
  - connection loss releases that source's pulses and disarms, and reconnection never re-arms (D-02C4A-1, resolves F-1);
  - failed or timed-out releases reported and marked unconfirmed, then blocked with `BUSY` until an explicit re-arm, never rewritten automatically (D-02C4A-2, mitigates F-2);
  - bounded termination with hung drivers (resolves F-3 for the engine).
  
  Details are in docs/reuse/REUSE_02C4A_WRITE_LIFECYCLE.md.
- Local Linux results (rustc 1.97.0, `--locked`):
  - fmt clean; strict Clippy (default and test-server) and rustdoc `-D warnings` clean;
  - hmi-engine 67/67 with test-server, 49/49 with default features;
  - 13 new deterministic simulated tests, repeated 10× green; 8/8 re-introduced defects caught;
  - `cargo audit`: 0 vulnerabilities, one known chrono warning.
  
  Simulated and synthetic drivers only. No hosted CI claimed.
- After the #19 merge, on the merged tree: hmi-engine 67/67; tag-engine 86 passed (2 ignored by design); both real-child tests passed against an engine freshly built from this branch; lockfiles unchanged.
- Open: owner confirmation of D-02C4A-1/2; whether to add a protocol-4 explicit outcome code or "release now" request; PLC-side command watchdog before physical use; Kotlin not exercised against the new disarm/BUSY behaviour; F-4/F-6/F-7 unchanged. Next: verify exact-head Required CI and handoff, independent review (reconcile with #19 if it merges first), then REUSE-02C.4B write-capable EngineDriver integration in isolated simulation.

## REUSE-02C.4A owner decisions applied — 2026-11-25
- Owner decisions:
  - D-02C4A-1 accepted: global disarm on connection loss under protocol 3; healthy-source monitoring kept; no automatic re-arming or stale replay; per-connection arming is a protocol-4 requirement.
  - D-02C4A-2 accepted with modifications: bounded release attempts and explicit uncertain outcomes; only the affected momentary command is blocked. Ordinary re-arming must not clear it, because authorization and command recovery are separate. Protocol 3 has no recovery operation, so the block is retained for the engine session. An explicit outcome-unknown status and command-specific recovery are protocol-4 requirements. PLC-side watchdogs are still required for physical momentary commands.
  
  Both are recorded in docs/ENGINEERING_DECISIONS.md.
- Correction on PR #20, from the first candidate `26910c1`, whose CI does not carry over:
  - `armWrites` no longer clears unconfirmed commands; nothing in protocol 3 clears them;
  - the refusal message is now `COMMAND_UNCONFIRMED:`.
  
  Tests now cover two sources (one healthy), a second momentary command, re-arm/disarm/stop/start/configure/reconnection not clearing the block, and only the affected command being blocked. The superseded clear-on-arm rule is caught by a regression test.
- Local Linux results (rustc 1.97.0, `--locked`):
  - hmi-engine 71/71 with test-server, 53/53 with default features; `write_lifecycle` 17 tests, repeated 10× green; 11/11 re-introduced defects caught;
  - fmt, strict Clippy and rustdoc clean; `cargo audit` 0 vulnerabilities (chrono warning);
  - tag-engine 86 passed (2 ignored) plus 2 real-child tests against the branch engine;
  - tag-engine and lockfiles unchanged.
- Open: the block is per engine session (L-4, must be addressed before REUSE-02C.4B exposes writes); Kotlin not exercised. Next: corrected-head hosted CI and independent review before merge; REUSE-02C.4B only on separate authorization.

## REUSE-02C.4A independent review and reconciliation — 2026-11-25
- Verified PR #20 predecessor 7b0166c: all 17 hosted checks passed; main advanced to a4babf7 (DEV-01 acceptance). Reconciled generated dashboard conflict through canonical milestone inputs, preserving accepted DEV-01.
- Integration remains blocked by static findings: ambiguous OPC UA pulse-start failures can skip release, queued requests can overtake connection loss, and startup/demand waits are not engine-bounded. See docs/reuse/REUSE_02C4A_WRITE_LIFECYCLE.md §9 and canonical milestone blockers. Correct with scoped reservation and simulation regressions; require fresh exact-head CI and independent review. No physical connectivity or write-capable EngineDriver exposure.

## REUSE-02C.4A review corrections — 2026-11-25
- Scope extension: retained the REUSE-02C.4A reservation and atomically added only crates/hmi-engine/src/driver/opcua.rs. Corrected the three §9 findings (OPC UA ambiguous response markers and compensation, bounded event drain and invalidation of stale queued control requests, cancellable start/demand and bounded failed-start cleanup). No protocol/dependency/tag-engine changes.
- Local Linux isolated locked test-server engine suite: 77 passed, including 22 write lifecycle and 7 session safety; formatting passed. See docs/reuse/REUSE_02C4A_WRITE_LIFECYCLE.md §10. Fresh hosted Required CI, Tag engine CI, handoff and final exact-head review before guarded merge. L-4 remains a blocker before write-capable EngineDriver integration. No physical PLC or production acceptance.

## PR #13 visual reference reconciliation — 2026-11-25
- Reviewed original reference at de777d7368fa2ffe6c95c3f27c49a89802989c8d against main 9c09e363d5df08114202a42ec0c9aa8bca664b80. Verified 116579 bytes, documented SHA-256 and Git blob hash; no application code or dependencies changed. Previous Desktop CI 45431128554 and handoff 45431128532 passed on predecessor only.
- RUST-04 scope reserved for docs/original-visual-reference; canonical milestone and generated views updated. PR #12 remains separate. Visual fidelity and issue #10 acceptance are not claimed. Re-review exact-head hosted checks before merge; do not release the reservation until merge or abandonment.

## RUST-04B independent review corrections — 2026-11-25
- PR #12 predecessor 1dd36b5 reconciled with main 0a74f65 (PR #13 original HTML reference merged). Scope reserved as RUST-04B under RUST-04. Corrected visual baseline, fit/zoom, hidden interaction and preview limits; no Rust/IPC/dependency changes.
- Local 13 frontend tests, TypeScript, Vite and isolated Chromium fixture passed; details in docs/LAYOUT_DESIGNER.md. Original reference hash verified. No native visual acceptance, runtime parity or PLC operation claimed.
- Fresh exact-head hosted Required CI and handoff required; previous passing runs do not carry over. Keep reservation until merge or explicit abandonment. RUST-04B.1 then RUST-04C planned in canonical roadmap.

## RUST-04B.1 workspace alignment candidate — 2026-11-25
- Verified main 9f20def9 (PR #12 merged; exact-head Desktop CI 45624121728 and handoff 45624121871 successful). PR #24 is conflicting with failed Required CI; leave separate. RUST-04B.1 scoped reservation recorded; canonical RUST-04B predecessor reconciled.
- Reference menu hierarchy, dock/catalog/inspector chrome, command search, grid and minimum-height layout refined; no Rust/IPC/model/dependency changes. 14 frontend and 9 roadmap tests passed, TypeScript/Vite and isolated Chromium screenshot/interaction checks passed. See docs/LAYOUT_DESIGNER.md for metrics and limitations.
- Actual native screenshot comparison still required (no local WebKitGTK development prerequisites or Xvfb); do not mark completion or owner acceptance. Submit draft, verify exact-head hosted CI and independent review, obtain native visual comparison, then integrate before RUST-04C.

## RUST-04B.1 independent review — 2026-11-25

- Independent source review of predecessor 5c8459fa69fa5fd060dc52422cbf6ec3c0eb3e9c found outside menu dismissal lost when preview stopped click propagation, command focus/empty search issues, and compact dock resize steps collapsed by the 25% cap. Corrected within existing reserved frontend scope.
- Fresh isolated clone: locked npm install; 14/14 frontend tests with Node 22.23.5 (system Node 22.22.1 lacks TypeScript stripping), TypeScript and Vite passed; unchanged package lock and reference SHA-256. Headless Chromium fixture with unchanged Tauri CSP and synthetic snapshot/persistence IPC verified outside pointer and focus dismissal, selection, Ctrl+K reset, no-match message, Escape/action focus restoration, grid, and six distinct widths per side at 850x500 (127.5, 153, 178.5, 204, 229.5, 255 px); no mutation IPC or page errors. Reproduce by opening File then clicking/focusing a preview component, searching a nonexistent command, Escape, searching Toggle grid and executing it, then keyboard Home and successive arrows on each splitter at 850x500. This is not native acceptance.
- Predecessor Required CI and handoff succeeded ([Desktop CI](https://github.com/kandyman991/HMIStudio/actions/runs/45627392049), [handoff](https://github.com/kandyman991/HMIStudio/actions/runs/45627392150)). Native desktop screenshot/interaction comparison is still pending; a separate manual Desktop CI dispatch 45627669773 was observed queued for the predecessor, not the corrected source. Host has neither webkit2gtk-4.1 development metadata nor Xvfb. Do not treat browser or build evidence as native acceptance. Fresh hosted CI and final-head review required after this correction.

## RUST-04C direct manipulation candidate — 2026-11-25

- Verified PR #25 merged at bfd81fc (reviewed 9efb5e1; Desktop CI 45628584254, handoff 45628583943 succeeded). Reconciled engineering merge in canonical roadmap, released previous reservation and claimed RUST-04C. Native prototype comparison remains at RUST-04 acceptance, not waived.
- Implemented flow pointer reorder/reparent, constrained fixed-size resize with optional 10 dp snap, scroll pan and semantic cross-axis alignment using existing typed transactions. Immutable gesture preview, one command on release, cancellation and revision/generation checks. Unsupported non-flow/wrapped/responsive layouts explicitly excluded; no Rust/IPC/production changes.
- 17 frontend tests, TypeScript/Vite and isolated synthetic browser gesture fixture passed. See docs/LAYOUT_DESIGNER.md. Final independent review and exact-head CI still required; combined native designer acceptance remains open.
- Existing Rust core commands/transactions/undo_redo suites 29/29 and roadmap tests 9/9 passed.
- Draft PR #26 opened at e59e154 as the supported base-flow completion candidate. Verify the latest head after the roadmap snapshot commit; do not treat this predecessor SHA as final CI evidence. Keep reservation until merge.

- Independent PR #26 review of c7f384c against bfd81fc corrected nested resize grip hit-testing, low-zoom grip extent, stale viewport/source geometry detection and layout-affecting feedback. Fresh isolated frontend build and synthetic Chromium regression passed; see docs/LAYOUT_DESIGNER.md. Predecessor hosted CI passed but does not cover this correction. Require corrected-head CI before merge; native RUST-04 acceptance remains open.
- Follow-up final diff review explicitly made the resize grip border-box, repeated the frontend build and browser fixture, and retained the exact-head CI gate.

## Roadmap reconciliation after PR #26 — 2026-11-25

- Verified PR #26 merged at dfd18a07a9c908b242b5de92cde5563fc378b553 on main, corrected head 01fb1a19159e1851341d0bd52cf6ab6630ce8175, successful exact-head Desktop CI 45631813805 and handoff 45631813614; RUST-04C reservation released. Canonical RUST-04C now reflects verified merge. Live synchronization also verified PR #20 merged at 9c09e363d5df08114202a42ec0c9aa8bca664b80 with exact-head Desktop CI, Tag engine CI and handoff successful, but the live-data-dependent roadmap test 6 fails when its REUSE-02C.4A proposal fixture is promoted. Keep that milestone observation unchanged until scoped DEV-02 fixes the test and reconciles it. No acceptance granted.
- Older evidence-only PR #24 is conflicting and has failed Required CI; leave it unmerged pending DEV-02 reconciliation. This partial reconciliation is based on verified current main and live GitHub observations. No application or CI policy changes. Next action: combined RUST-04 native desktop designer validation against preserved HTML reference, including direct manipulation; record acceptance only with actual evidence.

## DEV-02 roadmap regression correction — 2026-11-25

- PR #27 merged at 3ab9d4644e2479bf6d9ede948e2a14ff1643f831 with exact-head Required CI and handoff successful. Reserved DEV-02 from that main. Fixed roadmap validator tests that depended on the mutable REUSE-02C.4A and RUST-01 records and live reservation context; synthetic fixtures retain rejection checks and explicitly test reservation eligibility.
- Live synchronization verified PR #20 exact-head Desktop CI, Tag engine CI and handoff and merge ancestry; updated REUSE-02C.4A to merged and removed stale CI blocker. PR #24 remains conflicting; supersede only after corrected reconciliation is integrated. No acceptance or application changes.

## RUST-04 native Windows acceptance — 2026-11-25

- Owner tested Windows MSI from main a707bf568c9d71ade77fd3e6a9d457ef03f6f713, Desktop CI run 45640095067. Eight of ten checks passed, including original HTML visual fidelity; drag/reorder and nested reparent failed. Full checklist in docs/LAYOUT_DESIGNER.md. RUST-04 remains in progress and blocked; register RUST-04C.1, reserve exact scope after this record integrates, correct and retest. No overall acceptance or Linux acceptance inferred.

## RUST-04C.1 correction candidate — 2026-11-25

- PR #30 merged at 55b3f04; reserved RUST-04C.1 on fix/rust-04c1-native-drag. Reproduced compact center drag misclassified as resize and zero-area empty flow target in predecessor frontend. Corrected grip geometry, empty auto-flow editor target, main-axis background insertion and capture/focus handling. Local 20 frontend tests, TypeScript/Vite and rendered synthetic browser pointer and prior resize regression passed. See docs/LAYOUT_DESIGNER.md for evidence and retest steps. Independent review and exact-head hosted CI required; native Windows retest remains outstanding and RUST-04 blocked.

## RUST-04C.1 Windows retest — 2026-11-25

- Owner reported PASS for both previously failing Windows checks (selection/drag/reorder and nested reparenting), no additional observations. Latest successful main installer run 45652237283 at a204ba08926ce4bc27a742d90602c27d5a7c4ab3 contains PR #32; Windows artifact 15040567730. Installed binary checksum not independently inspected. Cumulative Windows checklist 10/10 across original and targeted retest; no full corrected-build regression or Linux manual acceptance inferred. See docs/LAYOUT_DESIGNER.md. RUST-04 remains in progress; next action resolve remaining Linux native evidence or scoped owner decision, then formal acceptance under D-008.

## RUST-04 scoped acceptance — 2026-11-25

- Owner directed skipping Linux installation and removing it as an acceptance prerequisite. RUST-04 accepted for supported designer scope on cumulative owner Windows 10/10 and verified successful build a204ba0 (run 45652237283); Linux installation/manual interactions deferred, not passed. RUST-02 deferred visual fidelity gate satisfied by prior Windows visual PASS; retain historical issue #10 limitations. RUST-05 acceptance dependency on RUST-04 is satisfied; remaining reuse prerequisites unchanged. See D-008 follow-up and docs/LAYOUT_DESIGNER.md.

## Review depth decision — 2026-11-25

Owner approved lightweight independent consistency review for documentation/roadmap-only PRs; full technical review remains required for implementation or control changes. Both retain exact-head Required CI, handoff and applicable gates. Apply to PR #34 evidence with explicit scrutiny of its owner-approved policy changes; do not self-exempt.

## REUSE-03A started — 2026-11-25

Owner approved O-3, O-4 and synthetic-only O-1 fixture scope. Reserved REUSE-03 in RUST-05 group from main f86f433; initial compatibility tests and proposed model contract in docs/reuse/REUSE_03A_MODEL_V2.md. Keep schema v1 active until v2 is complete. No private transfer, decoder or runtime work. Next exact action: typed industrial/security additions and validated commands, then expression/action/catalog contracts and migration freeze.

## REUSE-03A industrial/security declarations — 2026-11-25

Typed candidate model and deterministic validation added; existing v1 serialization and behavior unchanged. Seven new synthetic regressions cover command and limit validation, loopback isolation and permission references. Conservative float32 narrowing refusal is a candidate engineering default, not production enforcement. Next exact action: complete typed engineering integration and remaining action/enumeration/component/expression contracts before schema activation. PR #35 remains draft.

## REUSE-03A expression/enumeration candidate — 2026-11-25

Added staged typed Rust expression parser/type checker/snapshot evaluator and discrete enum mappings; eight new original synthetic test groups pass. Contract-1 numerical behavior is f64; refuse lossy canonical integer input. No Kotlin transfer or runtime binding; cross-runtime edge parity remains unverified. Next exact action: action/component contracts, canonical typed engineering integration and v1-to-v2 migration. PR #35 stays draft and reservation remains active.

## REUSE-03A components/actions — 2026-11-25

Added nine passive reusable symbol definitions and staged line/arrow/polygon/text-input/dropdown contracts, faceplate classification/arguments, ordered actions and dynamic startup resolution. Initial createIndustrialSymbol typed session command integrates allocation, undo/redo and atomic rollback using v1-compatible data. Classification/placement and remaining typed commands still need canonical integration; schema stays v1. Next exact action: canonical integration, reference/remap/transaction tests and strict migration activation. PR #35 remains draft; reservation active.

## REUSE-03A canonical v2 candidate — 2026-11-25

Draft branch now writes schema 2; v1 fixtures/schema immutable, additive strict migration and typed project groups integrated. Group replacement commands use strict validation, normalized references and existing persistence/undo; duplicate operations refused on nonempty v2 until remap is complete. Unsupported desktop and tag-engine consumers refuse nonempty v2; native Tauri offline dependency unavailable. Scope extended atomically to consumer paths. Next exact action: finish canonical component placement, copy/remap, consumer validation and cross-runtime parity before freezing v2. PR #35 remains draft.

## REUSE-03A canonical placement and copying — 2026-11-25

Five typed leaf kinds now occupy canonical nodes with one matching contract. insertV2Component is atomic; classified symbol creation persists metadata. Screen/node/definition copies preserve and remap declarations; inherited permissions cannot be lost on move/copy. Cross-definition reparenting remains refused and reference-bearing deletion requires explicit cleanup. Existing desktop/tag consumers reject unsupported features. Next exact action: downstream compatibility and expression conformance before freeze. PR #35 remains draft; reservation active.

## REUSE-03A desktop v2 engineering integration — 2026-11-25

Valid v2 open/save/recovery, static five-component preview, typed inspector and insert/duplicate operations now integrated. Single-contract edits avoid full JS project round trip; deletion removes intrinsic contracts only. Runtime activation refusal unchanged. Frontend 23 tests via temporary transpilation and isolated native host 10 tests passed; full native Tauri/visual acceptance not claimed. Scope extended to src/ and tests/. Next exact action: cross-runtime expression conformance and exact-head hosted CI/technical review before schema freeze. PR #35 remains draft.

## REUSE-03A expression conformance — 2026-11-25

Pinned actual private Kotlin/JVM contract-1 oracle 92a612d compiled separately; 12,664 original synthetic vectors match Rust after numeric text formatting fixes. Public fixture replay is automated; no private source or project fixture transfer. Exact-head hosted CI and independent full technical review remain prerequisites for schema freeze; no runtime or Android parity claim. Next exact action: independently review final PR #35, correct any findings, verify exact-head checks, record freeze and merge only when gates satisfied. Reservation active.

## REUSE-03A independent review correction — 2026-11-25
- PR #35 predecessor d0c68d14018dca7a3d0ba3e68e031ad90a9bef11 passed Desktop CI 45686447457, Tag engine CI 45686447214 and handoff 45686446711. Corrected canonical v2 diagnostic paths; 104 local locked offline model tests passed (including corpus replay and schema checks). V1 schema bytes unchanged and v2 hash recorded.
- Formal freeze proposal in docs/reuse/REUSE_03A_MODEL_V2.md; do not certify freeze or merge before completing independent technical review and exact corrected-head hosted checks. REUSE-03 remains in progress; no runtime, device or visual acceptance.

## REUSE-03A merge / REUSE-03B start — 2026-11-25
- PR #35 verified merged at b9ae1d5; exact head 1ddf0b4 passed all 17 jobs including Required CI. Schema v2 formally frozen; see docs/reuse/REUSE_03A_MODEL_V2.md. RUST-05 reservation retained for REUSE-03B. Synthetic-only fixtures; no legacy source transfer, production authority or runtime acceptance.
- Next: read-only legacy decoder, deterministic canonical mapping and complete migration report; retain unsupported-feature refusal.
- REUSE-03B initial portable decoder crate and six synthetic regression groups implemented; locked offline tests, Clippy and rustdoc passed. See docs/LEGACY_IMPORT.md for narrow supported subset and strict refusal of all unmapped content. Scope narrowed to crates/legacy-import/ using compare-and-set on the retained claim. No schema change, private fixture transfer or runtime approval. Continue full source validation and mappings; draft PR requires review and exact-head CI.

## REUSE-03B.2 — 2026-11-25
- PR #36 predecessor 8458d45 passed Desktop CI 45699525142 and handoff 45699525091. RUST-05 reservation retained. Added conservative industrial/security mapping and exact limits/reference validation in crates/legacy-import; 11 local regression groups, Clippy and rustdoc passed. Frozen schema unchanged.
- Explicitly refuse unsupported credentials, ambiguous endpoints/access, critical reason/readback override/max-change/decimal/write-only policy gaps; no runtime activation. See docs/LEGACY_IMPORT.md. REUSE-03 remains in progress; next component/layout and expression/enumeration mapping, then remaining import containers/engineering integration. Exact-head hosted CI and independent review remain required.

## REUSE-03B.3 — 2026-11-25
- PR #36 extends graphics mapping: selected component contracts, nested explicit row/column layout intent (warnings, retained decoded absolute geometry), reusable composites with read-only bool/text tag parameters, direct and parser-checked expression remapping, standalone exact int64 enumerations. 17 local regression groups, Clippy and rustdoc passed. Schema unchanged; no renderer or runtime acceptance.
- Unsupported standard definitions, variable/value parameter variants, adaptive/grid/scale groups, dynamic visibility reflow, per-component enumeration use and remaining graphics/actions/alarms/assets/themes are explicitly refused. See docs/LEGACY_IMPORT.md. Next: remaining legacy declaration mappings and safe unsupported preservation/publish guard, then containers and engineering integration. Retain RUST-05 and REUSE-03 in_progress. Exact-head CI and independent review required.

## REUSE-03B.7 independent review — 2026-11-26
- Verified PR #36 predecessor 9047525 all 19 hosted jobs, 54 importer, three quarantine and two engine differential groups on both Windows and Linux. Review found report/source evidence replay, catalog-only marker, bounded reads, post-commit warning visibility and save-copy regression gaps. Correction candidate within existing RUST-05 scope; see docs/LEGACY_IMPORT.md. Review-only bundle v2 retains exact source locally; never publish customer data or review bundles.
- Blocking gate remains: no desktop import/review or production/live consumer integration; predicate alone is not non-bypassable end-to-end enforcement. Preserve simulation freedom and frozen canonical schema. Verify new exact-head CI and independently review corrected source before any formal acceptance. No reservation release or merge.

## REUSE-03B.8 candidate — 2026-11-26
- Verified b3b5898: all 19 hosted jobs, seven quarantine, 54 importer, two address differential tests per platform. RUST-05 reservation extended before desktop changes. Native review lifecycle, guarded edit requests, save-copy and import-specific production screening added; synthetic host regression. Review bundles are not plain projects or ordinary autosaves. No real publisher/live project connector or simulation exporter exists; future consumers must enforce distinct trusted admission paths. No commissioning, schema change or merge.
- Next: verify corrected-head hosted CI and native lifecycle regression, review actual scope and limitations, reconcile remaining archive/semantic and downstream enforcement gates. See docs/LEGACY_IMPORT.md.

## REUSE-03B.8 independent review — 2026-11-26

- Verified [draft PR #36](https://github.com/kandyman991/HMIStudio/pull/36) at `fc2a531fea09fb4ae21723062e7bb240a6dc00e6` against main `b9ae1d5ac102effbcf6f0e2f1cdd8d66fb7facb8`. Hosted Windows and Linux desktop and crate checks passed on that predecessor. [Desktop CI](https://github.com/kandyman991/HMIStudio/actions/runs/45829699850), [Tag engine CI](https://github.com/kandyman991/HMIStudio/actions/runs/45829699781), [handoff](https://github.com/kandyman991/HMIStudio/actions/runs/45829699807).
- Corrections cover quarantined bases and autosaves, save-copy undo boundaries, stale native requests, source protection and committed bundle reopen after interrupted staging cleanup. Native production screening borrows the active project and stays inside Rust. The shared RUST-05 reservation is unchanged. See [review and test scope](docs/LEGACY_IMPORT.md).
- Verified corrected source `13aeca71746a926101c410bc92bce8c477a3d69d` in all 19 hosted jobs. Windows and Linux native tests and Required CI succeeded. The evidence commit records that observed predecessor. Verify its own exact head separately. [Desktop CI](https://github.com/kandyman991/HMIStudio/actions/runs/45835880081), [Tag engine CI](https://github.com/kandyman991/HMIStudio/actions/runs/45835880330), [handoff](https://github.com/kandyman991/HMIStudio/actions/runs/45835880113).
- Next: verify the documentation successor's hosted checks and keep REUSE-03 in progress. Actual live/publish enforcement and isolated runtime admission remain open, along with archive/package and migration coverage. Apply D-004 to isolated reads, writes and operator actions. Formal acceptance and the RUST-05 interoperability milestone remain open.

## REUSE-03B.9 — isolated engine candidate (2026-11-26)
- Verified exact head 2bd11e6 in all 19 hosted jobs; native 14/15 and engineering-core 72/73 tests on Windows/Linux, nine quarantine per platform. RUST-05 extended before engine entry-point edits. Added fixed launch-time isolated mode, loopback-only configuration and factory, no credentials/settings or live driver selection, simulation writes/pulses without production approval; preserve ordinary protocol and production write guards. Two synthetic regression groups; exact-head hosted CI and independent review pending.
- Next: verify mode tests and CI; integrate native project-to-engine isolated launcher and actual production consumer admission, then archive/package format and remaining migration coverage and review-edit recovery. No formal acceptance or merge. See docs/LEGACY_IMPORT.md.

## REUSE-03B.10 - native isolated project simulation (2026-11-26)

- Verified draft [PR #36](https://github.com/kandyman991/HMIStudio/pull/36) at predecessor `8f7d06377e7ac988b7f483d0e3a39654b2d50e33` and main `b9ae1d5`. Predecessor [Desktop CI](https://github.com/kandyman991/HMIStudio/actions/runs/45848389015), [Tag engine CI](https://github.com/kandyman991/HMIStudio/actions/runs/45848388871) and [handoff](https://github.com/kandyman991/HMIStudio/actions/runs/45848388892) passed. Retained RUST-05 and extended reservation `e7b6ea0` to `e36947b` for the owner-approved draft packaging deferral.
- Native Studio projects, including import reviews, now compile to validated memory-only configurations for a host-selected isolated child. The active project generation, revision and simulation session scope commands. Edits and successful switches retire the old child. The native Simulated tags panel exposes reads, writes and momentary commands. Production/live admission and frozen schema v2 are preserved.
- Next: verify the pushed head's Windows and Linux native real-child regression, Required CI, Tag engine CI and handoff. The owner requested zero installers on this draft. Independently review before merge. Actual publishing/live entry-point enforcement, broader runtime compatibility, archive admission and review-edit crash recovery remain open. See [implementation and reproduction](docs/LEGACY_IMPORT.md#reuse-03b10---native-isolated-project-simulation).
