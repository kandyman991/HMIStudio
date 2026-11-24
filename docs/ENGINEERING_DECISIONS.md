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
