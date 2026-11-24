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

## D-CI-02 — Change-aware CI and on-demand installer generation (accepted direction, 2026-11-24)
Owner-directed: replace universal desktop packaging with conservative path classification, always-evaluated Required CI, applicable frontend/crate/native checks, manual and packaging-sensitive Windows/Linux installer builds, three-day development artifact retention, dependency compilation caching and superseded-run cancellation. Preserve hosted-only runners, exact-head CI, independent review, dependency review and D-008 milestone acceptance. Rejected alternative: package both platforms for every documentation or routine source edit. CI-02 merged in PR #14 (`39eba35`) after exact-head full CI and review; no branch protection contexts were enforced. Documentation-only demonstration and independent manual dispatch remain unverified; no measured savings claimed. See CI_RUNNER_POLICY.md.

## D-009 — Selective HMI Forge reuse boundary (owner direction recorded in PR #9, 2026-11-24)
Accepted O-1/O-2/O-7: proprietary first-party direction with no open-source license chosen; selectively import audited engine files from pinned HMI Forge source in a squashed provenance-recorded import, keeping the private Kotlin/Compose runtime repository and public CI free of private credentials; authorize publication of the audit detail only. Each transfer requires documented rights, dependency obligations, sensitive-content screening and review. No blanket third-party redistribution or release authorization. O-3 through O-6 and O-8 through O-12 remain open; process hosting, protocol/model changes and runtime generation approach are proposals. D-001, D-002, D-004 and D-008 remain binding. See reuse/AUDIT_SUMMARY.md.

### D-009 follow-up — REUSE-02A owner attestation (2026-11-24)
Owner confirmed control of the rights to the exact 27-file allowlist from pinned HMI Forge revision 92a612d104693800d7119ee4dc0114636b19002f, including AI-assisted contributions, and confirmed that these files contain no unauthorized third-party proprietary code or customer material. This is the owner rights/disclosure attestation requested in REUSE-02A, not an independent legal certification. Authorization is confined to the reviewed sanitized source candidate under D-009; no private history, SDKs, signing material, third-party vendored source, binaries or release/distribution approval. The recorded dependency choices and source-only restrictions remain binding; O-6 and the other outstanding architecture/release decisions are unchanged.

### D-009 follow-up — REUSE-02B dependency remediation candidate (2026-11-24)

Owner requested resolution of PR #16 dependency security findings. The initial approved source-only import remains immutable in its first commit; dependency remediation is separately committed. Engineering proposes a narrowly scoped temporary exception to the original registry-reference preference for the nine MPL-2.0 packages from the exact official public FreeOpcUa/async-opcua revision `ca18bb73bac4ff2627943c760c97123062988468`, with no vendoring or private credentials. The release 0.19.0 cannot select patched rsa 0.10 through a lockfile update alone. Retain exact Git source/commit/tree/license validation, registry checksums, explicit test-server CI and fresh RustSec audit without ignored advisories. Return to a reviewed compatible registry release when available. The candidate is not production clearance; independent review and green exact-head CI required for merge. Chrono maintenance warning and full binary notices/covered source availability remain separate release work. No other Git dependency exception or architecture decision is implied.
