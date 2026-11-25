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
