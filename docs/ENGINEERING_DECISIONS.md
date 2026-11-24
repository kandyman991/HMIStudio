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
Use standard GitHub-hosted Windows/Linux jobs in this public repository, no private self-hosted runner for untrusted PRs. Exact-head evidence required; lockfiles pending. See CI_RUNNER_POLICY.md.

## D-007 — Semantic layout (accepted direction)
Nested layout containers, constraints and adaptive rules; allow absolute positioning where explicitly needed, not as the sole project representation. The runtime mapping is a future validation task.
