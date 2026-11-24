# HMI Studio — engineering rules

## Mandatory resumption
1. Read AGENTS.md, CHAT_HANDOFF.md, README.md, ROADMAP.md, docs/MILESTONE_PLAN.md and docs/CONTINUITY_PLAYBOOK.md.
2. Read relevant docs/ARCHITECTURE.md, docs/ARCHITECTURE_MEMORY.md, docs/ENGINEERING_DECISIONS.md, docs/FAILED_APPROACHES.md, docs/CI_RUNNER_POLICY.md and docs/DEVELOPMENT_SIMULATION_POLICY.md before applicable work.
3. Treat handoff/state.json and automated snapshot output as possibly stale observations. Verify live GitHub main SHA, branches, PR heads, issues, CI and releases before changes. Inspect code and tests when records disagree.
4. Resume the next exact action only after reconciliation. Distinguish proposed, implemented, CI-tested, merged and desktop/device-accepted.
5. Git-tracked engineering records and verified live GitHub evidence are the continuity source. Do not rely on ChatGPT memory, Claude/Gemini conversation history, artifact memory, temporary workspaces, previous agent sessions or external notes to recover engineering context or authorization. If evidence is missing, record the gap and seek the actual source or owner decision. Do not silently invent or reconstruct facts.

## Workflow
- Branch from verified main; one scoped milestone per PR. Do not overwrite concurrent work.
- Document decisions, acceptance criteria, limitations and reproducible tests. Require exact-head CI and independent review before merge. Never claim unrun checks passed. No force pushes to main.
- Batch reviewed fixes and run targeted preflight before pushing. Follow CI-02 in docs/CI_RUNNER_POLICY.md: require exact-head Required CI and handoff, applicable independent checks and review; use change-aware routing, cache dependencies and skip irrelevant installer jobs. Packaging-affecting changes, release tags and milestone/manual installer requests still require both bundles. Do not call a skipped check passed.
- Public repository: never transfer private source, customer projects, proprietary drivers, credentials, signing material or SDKs without documented authorization and license review. No project license is assumed until owner chooses one. Never publish raw private chats or private operational records.
- Prefer standard hosted CI; never run untrusted public PRs on private self-hosted machines. Minimize permissions, artifact retention and secrets. Commit lockfiles; review dependency licenses and vulnerabilities.
- Tauri 2 desktop shell; HTML/CSS/TypeScript frontend; Rust authoritative engineering core. Typed commands only, no generic shell or privileged filesystem bridge.
- Every HMI project must be editable and buildable without AI. UI and optional AI must use the same validated engineering operations.
- Versioned framework-independent project model; semantic nested layout constraints, distinct editor/runtime responsibilities. Verify Kotlin/Compose interoperability for Windows, Linux and Android; support standalone bundles and project-only publishing.
- Development/simulation reads and writes, operator commands, setpoints, alarms, recipes and runtime tests need no production approval. Actual machinery commissioning, trusted deployment, release signing and licensing are separate.
- Label placeholders; do not claim production connectivity, deployment or milestone acceptance until verified.

## Engineering merge versus milestone acceptance (binding D-008)
- Routine development PRs do not require owner installation, manual desktop testing or visual sign-off. Require compilation, automated tests, exact-head CI, independent review, applicable dependency security/license checks and installer generation when packaging is affected.
- Record deferred manual evidence explicitly, with the milestone that owns it. Do not request an owner install merely to unblock an intermediate PR; do not label deferred evidence as passed.
- RUST-01 Windows/Linux install, launch, close and visual/prototype comparison are deferred to RUST-02. Continue development and merge after the engineering gates pass. The reconstructed UI is not proof of original-prototype fidelity.
- Manual desktop acceptance is required at RUST-02 workspace, RUST-04 designer, RUST-05 runtime interoperability, RUST-07 deployment and RUST-08 full release acceptance, not every intermediate PR within those milestones. RUST-03 uses automated engineering tests; RUST-06 uses simulation first, with hardware acceptance at explicitly defined hardware milestones.
- Do not defer a known failing required automated check, unresolved blocking review finding, or material security/safety defect. Production approval, trusted deployment, physical commissioning and release acceptance remain separate. An explicitly scoped owner-approved acceptance requirement overrides the routine default.

## Architecture and handoff gates
- Before architectural changes, search accepted decisions and failed approaches. Do not revive rejected designs without explicit owner approval, changed assumptions and regression evidence.
- In the same PR, record material decisions and failures, limitations, test evidence and the next exact action. Classify CONFIRMED_FAILURE, REJECTED_BY_DESIGN, SUPERSEDED and INCONCLUSIVE separately.
- At milestone/PR transitions update CHAT_HANDOFF.md and milestone statuses with verified source SHA, PR and CI links; refresh handoff/state.json with observed facts and timestamp when useful. Do not claim the current commit contains its own hash; record the observed predecessor and verify live head.
- Automated snapshots cannot replace curated engineering decisions or approval. Do not use uploaded artifacts as project memory.
- Resume phrase: "Resume HMI Studio (kandyman991/HMIStudio)" means follow mandatory resumption, not permission to bypass review, testing or acceptance.
