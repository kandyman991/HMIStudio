# HMI Studio — engineering rules

- Read this file, README.md, ROADMAP.md and docs/ARCHITECTURE.md before changes. Check live main, PRs and CI; branch from current main. One scoped milestone per PR. Do not overwrite concurrent work.
- Document decisions, acceptance criteria, limitations and reproducible tests. Require exact-head CI and independent review before merge. Never claim unrun checks passed. No force pushes to main.
- Public repository: never transfer private source, customer projects, proprietary drivers, credentials, signing material or SDKs without documented authorization and license review. No project license is assumed until owner chooses one.
- Prefer standard hosted CI; never run untrusted public PRs on private self-hosted machines. Minimize permissions, artifact retention and secrets. Commit lockfiles; review dependency licenses and vulnerabilities.
- Tauri 2 desktop shell; HTML/CSS/TypeScript frontend; Rust authoritative engineering core. Typed commands only, no generic shell or privileged filesystem bridge.
- Versioned framework-independent project model; semantic nested layout constraints, distinct editor/runtime responsibilities. Verify Kotlin/Compose interoperability for Windows, Linux and Android; support standalone bundles and project-only publishing.
- Development/simulation reads and writes, operator commands, setpoints, alarms, recipes and runtime tests need no production approval. Actual machinery commissioning, trusted deployment, release signing and licensing are separate.
- Label placeholders; do not claim production connectivity, deployment or milestone acceptance until verified. Build, test, record evidence and update roadmap.
