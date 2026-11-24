# HMI Studio — engineering rules

## Scope
This public repository is the Rust/Tauri desktop successor. The legacy private repository is read-only migration reference, not a dependency. Never copy private source, customer projects, proprietary drivers, credentials or signing material into this repository without documented authorization and license review.

## Workflow
- Read this file, README.md, ROADMAP.md and docs/ARCHITECTURE.md when present before changes.
- Check live main, open PRs and CI; branch from current main. Keep one scoped milestone per PR. Do not overwrite concurrent work.
- Document decisions, acceptance criteria, limitations and reproducible test results. Never claim unrun checks passed.
- Review and fix findings; require exact-head CI and independent review before merge. No force pushes to main.
- Prefer standard GitHub-hosted runners for public CI. Never run untrusted public PRs on private self-hosted machines. Minimize workflow permissions, artifact retention and secret exposure.
- Keep dependency lockfiles committed; audit dependency licenses and vulnerabilities. No project license is assumed until owner chooses one.
- Do not commit secrets, signing keys, personal/customer data, or proprietary SDKs.

## Architecture
- Tauri 2 desktop shell, HTML/CSS/TypeScript frontend, Rust authoritative engineering core. UI calls typed engineering commands; no privileged generic shell or filesystem access.
- Versioned framework-independent project model; editor and runtime have distinct responsibilities. Prefer semantic nested layout constraints over fixed-coordinate-only design.
- Preserve Kotlin/Compose runtime compatibility for Windows, Linux and Android, including standalone bundles and project-only publishing. Verify interoperability rather than assume it.
- Development and simulation reads/writes, commands, setpoints, alarms, recipes and runtime tests do not require production approval. Actual machinery commissioning, trusted deployment, release signing and licensing are separate.
- No production PLC connectivity or deployment claims before actual acceptance. Do not weaken trust controls to make simulation work.

## Definition of done
Build and test on relevant targets, document acceptance and remaining gaps, keep UI placeholders visibly labeled, and update roadmap status using evidence. Never claim a milestone complete on source presence alone.
