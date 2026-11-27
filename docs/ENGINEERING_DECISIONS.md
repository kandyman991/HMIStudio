# Engineering decision register

Each new entry records ID, date, status (accepted/proposed/superseded), rationale, alternatives, evidence, implications and validation gaps. Accepted requirements do not imply implementation.

## D-001 — Desktop and runtime separation (accepted direction, 2026-11-24)
Rust authoritative engineering engine; Tauri 2 and HTML/CSS/TypeScript desktop UI; Kotlin/Compose runtime targets Windows/Linux/Android. Independent project format, not arbitrary HTML translation. React is the selected initial implementation; layout parity still needs testing.

## D-002 — Two runtime delivery modes (accepted requirement)
Support standalone runtime+project bundle and project-only publishing to installed runtime. Pairing, signing and interoperability need independent validation. Neither mode is implemented in the initial shell.

## D-003 — Public successor boundary (accepted)
Clean public repository; no unaudited private source/history, proprietary SDKs or customer data. No project license selected. Selective authorized migration only.

## D-004 — Development versus production (accepted)
Simulation and development operations do not require production approval. Real machinery commissioning and trusted deployment are separate; see DEVELOPMENT_SIMULATION_POLICY.md.

## D-005 — Repository-only continuity (accepted, 2026-11-24)
Curated Git records plus live GitHub evidence are the source of engineering continuity. Do not rely on artifact memory or chat memory. Read-only automated snapshots supplement, not replace, curated records.

## D-006 — Hosted desktop CI (initial direction)
Use standard GitHub-hosted Windows/Linux jobs in this public repository, no private self-hosted runner for untrusted PRs. Exact-head evidence required; lockfiles committed; predecessor 0f2dddb passed locked CI. Reverify final head. See CI_RUNNER_POLICY.md.

## D-007 — Semantic layout (accepted direction)
Nested layout containers, constraints and adaptive rules; allow absolute positioning where explicitly needed, not as the sole project representation. The runtime mapping is a future validation task.

## D-008 — Engineering merge gates and deferred manual acceptance (accepted, owner-directed 2026-11-24)
Routine early-development PRs must not block on manual installation or owner visual sign-off. This supersedes the original RUST-00/01 manual merge gates, not the need for those tests. Require compilation, automated tests, exact-head CI, independent review, applicable dependency security/license checks and installer generation when packaging changes. Record unresolved evidence honestly.

RUST-01 Windows/Linux install, launch, close, interaction and original prototype comparison (or approved deviation) move to the RUST-02 workspace acceptance boundary. RUST-03 engineering core uses automated tests; RUST-04 designer, RUST-05 interoperability, RUST-07 deployment and RUST-08 release require planned manual acceptance at their milestone boundaries. RUST-06 simulation first, physical PLC tests at explicitly defined hardware milestones. Intermediate PRs can merge before manual milestone acceptance if engineering gates pass. Explicitly scoped owner-required testing, known blocking defects, security/safety controls and production/release approval are not waived.

Rationale: avoid repetitive manual installs of incomplete development snapshots while retaining verifiable engineering quality and meaningful UI/runtime/hardware acceptance. Rejected alternative: require owner installation and visual sign-off on each PR. Evidence: predecessor 3767484 passed desktop CI and handoff CI; actual desktop installation/visual tests have not run. This decision does not claim they passed.

## RUST-03B engineering implementation decisions (2026-11-24)
Adopted in the PR #3 candidate, subject to final CI; not production or owner release acceptance. See docs/ENGINEERING_CORE.md and docs/ENGINEERING_CORE_VALIDATION.md for alternatives, evidence and limitations.
- 03B-1: Snapshot-based transactions and undo, not inverse commands; memory cost remains measurable.
- 03B-2: RUST-03A validation gate, rejectNewErrors default with strict available; repair invalid projects incrementally.
- 03B-3: Monotonic revision distinct from content state id; dirty tracking follows save and undo.
- 03B-4: Session-wide ID retirement, including explicit IDs and within committed transactions.
- 03B-5: Engine-owned persistence through narrow FileSystem trait; no webview filesystem bridge. Exclusive staging, verified backup and conflict detection; trusted storage and single-writer enforcement still required at host boundary.
- 03B-6: Sidecar for backups and atomic v2 autosave envelope; reject stale recovery overwrites absent explicit consent; preserve recovery on close. Reject two-file update for loss of prior snapshot on interruption. Untitled recovery directory host-selected.

## RUST-02C implementation decisions (candidate, 2026-11-24)
- Native-only project selection and recovery; no webview path or filesystem capabilities. Advisory app-data and per-project sidecar locks with persistent lock filenames; guarded filesystem checks reject links and Windows reparse points. Trusted local storage only, not adversarial concurrent directory replacement.
- 30-second native autosave cadence; recovery must be explicitly resolved before edits can overwrite its slot. Save As refuses unrelated existing projects or recovery data; external modification is never force-overwritten through the UI. Recovery of a deleted original requires selecting the sidecar envelope and saving to a different filename.
- Native Save/Discard/Cancel before project replacement or close; session generation protects stale engineering edits after replacement. See docs/NATIVE_PERSISTENCE.md for test evidence, alternatives and limits. Candidate only, pending exact-head native CI and independent review; no change to D-008.

## D-DEV-01 — Living development roadmap

Owner-approved direction (2026-11-25). The per-milestone structured records are authoritative for milestone scope, dependencies, ownership, blockers, status and evidence. The deterministic generator produces `docs/MILESTONE_PLAN.md` and the offline HTML dashboard. Review scope and historical decisions in the existing linked records.

A review candidate and its CI remain a proposal. Verify the merge commit on the default branch before recording integration, then require evidence for milestone acceptance under D-008. Canonical edits, generated outputs and the curated handoff travel in the scoped PR. CI validates generation, migration, dependency and evidence consistency in the existing Required CI path. No automated branch writes or site deployments are used.

Use repository-safe observations and label their freshness. The public repository has no supported private Pages configuration. The dashboard is operated from a local checkout.


## D-CI-02 — Change-aware CI and on-demand installer generation (accepted direction, 2026-11-24)
Owner-directed: replace universal desktop packaging with conservative path classification, always-evaluated Required CI, applicable frontend/crate/native checks, manual and packaging-sensitive Windows/Linux installer builds, three-day development artifact retention, dependency compilation caching and superseded-run cancellation. Preserve hosted-only runners, exact-head CI, independent review, dependency review and D-008 milestone acceptance. Rejected alternative: package both platforms for every documentation or routine source edit. CI-02 merged in PR #14 (`39eba35`) after exact-head full CI and review; no branch protection contexts were enforced. Documentation-only demonstration and independent manual dispatch remain unverified; no measured savings claimed. See CI_RUNNER_POLICY.md.

## D-009 — Selective HMI Forge reuse boundary (owner direction recorded in PR #9, 2026-11-24)
Accepted O-1/O-2/O-7: proprietary first-party direction with no open-source license chosen; selectively import audited engine files from pinned HMI Forge source in a squashed provenance-recorded import, keeping the private Kotlin/Compose runtime repository and public CI free of private credentials; authorize publication of the audit detail only. Each transfer requires documented rights, dependency obligations, sensitive-content screening and review. No blanket third-party redistribution or release authorization. O-3 through O-6 and O-8 through O-12 remain open; process hosting, protocol/model changes and runtime generation approach are proposals. D-001, D-002, D-004 and D-008 remain binding. See reuse/AUDIT_SUMMARY.md.

### D-009 follow-up — REUSE-02A owner attestation (2026-11-24)
Owner confirmed control of the rights to the exact 27-file allowlist from pinned HMI Forge revision 92a612d104693800d7119ee4dc0114636b19002f, including AI-assisted contributions, and confirmed that these files contain no unauthorized third-party proprietary code or customer material. This is the owner rights/disclosure attestation requested in REUSE-02A, not an independent legal certification. Authorization is confined to the reviewed sanitized source candidate under D-009; no private history, SDKs, signing material, third-party vendored source, binaries or release/distribution approval. The recorded dependency choices and source-only restrictions remain binding; O-6 and the other outstanding architecture/release decisions are unchanged.

### D-009 follow-up — REUSE-02B dependency remediation candidate (2026-11-24)

Owner requested resolution of PR #16 dependency security findings. The initial approved source-only import remains immutable in its first commit; dependency remediation is separately committed. Engineering proposes a narrowly scoped temporary exception to the original registry-reference preference for the nine MPL-2.0 packages from the exact official public FreeOpcUa/async-opcua revision `ca18bb73bac4ff2627943c760c97123062988468`, with no vendoring or private credentials. The release 0.19.0 cannot select patched rsa 0.10 through a lockfile update alone. Retain exact Git source/commit/tree/license validation, registry checksums, explicit test-server CI and fresh RustSec audit without ignored advisories. Return to a reviewed compatible registry release when available. The candidate is not production clearance; independent review and green exact-head CI required for merge. Chrono maintenance warning and full binary notices/covered source availability remain separate release work. No other Git dependency exception or architecture decision is implied.

### RUST-06A review follow-up (PR #7)
For the standalone tag-engine simulation foundation, exact integer range/map checks and safe refusal of wide linear transforms are required, with operator property bounds and shutdown/cancellation admission enforced. A conservative 100,000 expanded-node per-default allocation limit is introduced as a review-selected engineering default; aggregate project sizing remains a later validation concern. Preserve 06A-1…8 scope and the existing simulation-only boundary. Unbounded subscription delivery is not accepted for UI/bridge integration; require bounded/coalesced delivery and resync before integration. No production or release acceptance is implied.

### O-5 — Separate supervised communication engine (owner approved)
Studio hosts the communication engine as a separate executable, not an in-process driver library. Preserve protocol 3 and Kotlin compatibility; use typed native IPC and no generic frontend shell. REUSE-02C.1 must establish bounded framing, process lifecycle, bounded subscription delivery with resync, exact or safely refused numeric transport, and no stale write replay or automatic rearming. Simulation remains permitted without production approval. Packaging, actual PLC connectivity and release acceptance remain separate. Other open reuse decisions are not resolved by O-5.

### REUSE-02C.2 — Dispatch foundation (candidate)
Single-flight monotonic IDs within one supervised session, one absolute reply deadline, strict protocol-3 decoding against the shared fixture, separate bounded event delivery (32 messages/4 MiB serialized payload) and terminal invalidation on overflow or unmatched replies. Chosen over silent lossy queues and ID reuse; caps are engineering defaults pending load validation, not real-time guarantees. A fresh handshake is required after invalidation but is not a complete cache resync; define authoritative baseline and coalesced bounded subscriptions before Studio wiring. No automatic write replay, no arming restoration, no configure/start or writes exposed by this slice. Preserve O-5 and D-004.

### REUSE-02C.2 follow-up — Bounded subscriptions and isolated baseline (candidate)
Latest-state subscriptions replace unbounded channels. Choose 64 subscribers, 256 pending events and 4 MiB serialized payload per mailbox; coalesce only between lifecycle/connection barriers, preserve initial baseline boundary, terminate invalid generation on overflow rather than deliver partial history. Initial status and value snapshot are protected by the documented lock order. Isolated bridge recovery launches a fresh child with generated loopback-only read configuration (1–4 points), publishes only a complete good/type-correct baseline, rejects stale generations and never replays writes. These are scoped engineering defaults pending load and general EngineDriver validation; not a new wire protocol, physical-driver or production acceptance.

- Independent REUSE-02C.2 correction: isolated loopback startup must reject events observed before configure/start acknowledgements; a new generation must not reuse premature state. The existing engine sends start OK before loopback reports. Generic correlated requests still permit interleaved events. Reproduced on predecessor e12569e and verified regression on corrected source; hosted gate pending.

### REUSE-02C.3 — Explicit isolated driver and checked scalar translation (candidate)
Host explicitly supplies and registers the separate engine executable for Simulator only; no project-supplied executable/endpoints/credentials. Initial adapter supports 1–4 unique read-only zero-initialized scalar points and rejects unsupported configurations. Use signed integer protocol variants with checked narrowing and unsigned range checks; UInt64 above signed Long capacity is refused rather than rounded. Preserve exact float variants and finite-only values. No writes or arming exposed; this is scoped incremental implementation, not a new simulation approval requirement. On source/transport loss, discard generation, mark TagEngine connection bad and obtain fresh baseline through normal reconnect. Cancellation extends into startup waits; full driver mapping, richer quality semantics and writes/termination are later work. Evidence and limits: docs/reuse/REUSE_02C3_ENGINE_DRIVER.md.

### DEV-01 review corrections — proposed

Implementation PRs require a matching shared reservation and canonical milestone update. GitHub observation synchronization may propose merge-state changes only from explicitly designated completion PRs and verified exact-head evidence. The automation must use reviewed pull requests, not direct default-branch writes; acceptance remains explicit. Scope relationships distinguish summary acceptance gates from counted implementation slices. No public hosting or credential configuration is authorized by this change.

### D-02C4A-1 — Global disarm on connection loss (owner accepted, 2026-11-25)
Under protocol 3, a source that leaves the connected state ends the control period for the whole engine:

- that source's running pulses are released, with the result reported;
- arming is withdrawn (`writesArmed {armed:false}`);
- monitoring of healthy sources continues.

Automatic re-arming after reconnection and replay of stale commands are prohibited; writes need an explicit new `armWrites`. Arming is engine-wide in protocol 3, so this disarm is global by design. **Protocol 4 requirement:** per-connection arming.

### D-02C4A-2 — Unconfirmed momentary-command release (owner accepted with modifications, 2026-11-25)
A pulse release is attempted once and bounded. Release failure, timeout, or a missing driver is reported explicitly (`pulseReleased {released:false}` with `NOT_TRANSMITTED:` or `OUTCOME_UNKNOWN:`), and only the affected momentary command is blocked. The engine never rewrites FALSE automatically.

Communication authorization and command recovery are separate operations. Ordinary `armWrites`, `disarmWrites`, `stop`, `start`, `configure` and reconnection never clear an unconfirmed command. Clearing requires an explicit, command-specific recovery with appropriate verification of the device state. Protocol 3 has no such operation, so the block is retained conservatively for the rest of the engine session; a refused pulse carries `BUSY` and a `COMMAND_UNCONFIRMED:` message.

**Protocol 4 requirements:**
- an explicit outcome-unknown status, instead of the `DRIVER_ERROR` message prefix;
- an operator-initiated, command-specific recovery operation that verifies state before clearing.

PLC-side watchdogs remain required for physical momentary commands. Development and isolated simulation reads and writes remain permitted without production approval. The engine-session scope of the block, which is lost when the engine process ends, is a recorded limitation that the EngineDriver integration and protocol 4 must address.

### D-008 follow-up — Scoped RUST-02 acceptance and visual continuation (owner direction, 2026-11-24)

Issue #10 records Windows functional checklist 9/9, Linux CI and isolated X11 launch/clean close, and owner waiver of duplicate manual Linux functional checks. Owner rejected the earlier visual direction and authorized PR #11 engineering merge. Visual acceptance remains open in RUST-04; not waived. The recovered original HTML is now preserved by PR #13. This entry was called D-009 in the original PR #12 draft; use D-008 follow-up to avoid colliding with the existing selective-reuse D-009. No generalized Linux or later acceptance waiver.

### D-008 follow-up — Scoped RUST-04 Linux acceptance deferral (owner direction, 2026-11-25)

The owner directed skipping Linux installation for now and removing it as an acceptance dependency. For RUST-04 only, accept the supported designer scope based on the recorded cumulative Windows 10/10 owner checklist and successful exact-revision Windows/Linux hosted build and automated CI (run 45652237283). Linux installation and manual designer interaction acceptance remain deferred, not passed; no later platform, runtime or release acceptance is waived. The previously deferred RUST-02 visual comparison is satisfied by the recorded Windows prototype fidelity PASS; existing RUST-02 scoped Linux exception and unperformed-case limitations remain unchanged. Keep all automated gates and later platform-specific acceptance requirements. See docs/LAYOUT_DESIGNER.md.

### D-008 follow-up — Scope-proportionate independent review (owner direction, 2026-11-25)

Owner approved distinguishing documentation/roadmap PRs from implementation PRs: use lightweight evidence and consistency review for documentation-only changes, and full technical review for application code or executable/control changes. Preserve independent review, exact-head Required CI and handoff, applicable checks and explicit owner acceptance. Verify actual scope and escalate mixed or uncertain changes; a policy/control change must be scrutinized under the existing rules, not self-exempted. No change to Linux CI, release or production safeguards. See AGENTS.md and docs/CI_RUNNER_POLICY.md.

### D-009 follow-up — REUSE-03 owner decisions (2026-11-25)

Owner approved O-3 canonical model v2 additions, O-4 HMI Forge expression contract 1 in validated Rust, and the O-1 fixture approach of newly authored synthetic fixtures only initially. No private demo/customer projects, private code, secrets or signing material may be transferred under this approval. Preserve a single canonical editable Rust model; v1 migration must preserve semantics and never grant physical write or trust authorization. Separate actual fixture transfers require documented exact rights and review. See docs/reuse/REUSE_03A_MODEL_V2.md.

### D-008 follow-up — Milestone-based comprehensive independent review (owner direction, 2026-11-27)

Owner directed adoption of the Electric Studio cadence: routine implementation slices within a draft milestone PR use author self-review, automated regression tests and successful exact-head applicable CI; reserve comprehensive independent technical review for the major milestone boundary before merge, with risk-triggered earlier independent review for material architecture, schema/protocol, security/trust boundaries, safety-critical behavior and serious regressions. PR #36 uses REUSE-03B completion as its comprehensive review gate. Retain the scope-based pre-merge review classification, review binding policy changes under the preceding control, and retain all manual, production, release, dependency, packaging, security and CI gates. No known blocking finding may be deferred. This owner-approved policy is recorded on the draft branch and is not yet integrated on main. No independent review or merge of this policy change is claimed.
