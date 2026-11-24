# REUSE-01 — Reuse milestone plan (REUSE-01 … REUSE-06)

Status: **proposed**. This plan does not authorize REUSE-02 or any later milestone. Each needs owner authorization.
Owner decisions O-1 … O-12 are listed in `AUDIT_SUMMARY.md` §6.

## Relation to the RUST series

| RUST milestone | Conflict or overlap | Proposed resolution |
| --- | --- | --- |
| RUST-02 (PR #8 RUST-02C open) | none. REUSE touches no `src/`, `src-tauri/` or shared records | REUSE PRs keep their records in `docs/reuse/` until RUST-02C merges |
| RUST-03A/03B (merged) | model lacks S7, security, write semantics, faceplate action | canonical schema v2 in REUSE-03 through the migration registry, with engineering-core commands |
| RUST-05 import and Compose parity | **same scope** as REUSE-03 and REUSE-04 | treat REUSE-03 as RUST-05A (legacy import) and REUSE-04 as RUST-05B (runtime parity) |
| RUST-06 drivers, tags, simulation (PR #7) | `tag-engine` driver layer would duplicate the engine | re-scope RUST-06 driver work to the `EngineDriver` bridge (REUSE-02); keep simulator, bindings, alarms, recipes and search in RUST-06 |
| RUST-07 deployment | **same scope** as REUSE-05 | REUSE-05 = RUST-07, built on RT-01C instead of from scratch |
| RUST-08 migration acceptance | same as REUSE-06 | REUSE-06 = RUST-08 reuse part |
| RUST-09 3D, AI | HMI Forge PR #50 3D-01 | INVESTIGATE when RUST-09 is planned |

The edits to `ROADMAP.md`, `docs/MILESTONE_PLAN.md` and `docs/ENGINEERING_DECISIONS.md` are **proposed** in
`AUDIT_SUMMARY.md` §8. They are not applied, because PR #8 is editing those files right now.

---

## REUSE-01 — Inventory and compatibility audit (this PR)

- **Scope:** documents only, the 11 files in `docs/reuse/`.
- **Dependencies:** read access to HMI Forge.
- **Deliverables:** inventory, matrix, integration designs, validation and milestone plan, and the handoff record.
- **Approach:** read-only audit, re-running engine tests outside the clone.
- **Acceptance:** independent review. Every number can be reproduced by the stated method. No source is transferred.
- **Tests:** the engine suite was re-run (37 passed).
- **Risks:** the publication detail of the private repository (O-7).
- **Parallelism:** independent of RUST-02C and RUST-06A.
- **Out of scope:** any code, any HMI Forge change, and REUSE-02.

## REUSE-02 — Engine bridge

- **Scope:**
  - import the `hmi-engine` crate into `crates/hmi-engine` (vendored, with provenance);
  - a Rust protocol 3 client crate;
  - `EngineDriver` in `tag-engine`;
  - Tauri sidecar packaging for Windows and Linux;
  - S7 and loopback driver kinds in the canonical model, or deferred to REUSE-03;
  - Siemens presets;
  - OPC UA online browse through the engine.
- **Dependencies:**
  - **O-1 licence**, **O-2 repository strategy** and O-5 hosting;
  - PR #7 merged;
  - for packaging, RUST-02C merged.
- **Deliverables:**
  - the crates;
  - CI workflow `hmi-engine.yml` on hosted ubuntu and windows;
  - sidecar in the desktop installers;
  - `docs/ENGINE_BRIDGE.md` and a validation record;
  - `PROVENANCE.md`.
- **Approach:**
  - import unchanged and prove it with the engine's 37 tests;
  - build the client against `messages.json`;
  - build the bridge against loopback and the test servers;
  - add type conversion per `RUST_ENGINE_INTEGRATION.md` §4.
- **Acceptance:**
  - the engine tests pass on hosted ubuntu and windows at the exact head;
  - the client and bridge tests pass;
  - the advisory triage is recorded (O-6);
  - the licence notices are generated;
  - no protocol 3 change.
- **Tests:** `REUSE_VALIDATION_PLAN.md` §3.
- **Risks:**
  - advisories without fixes (`rsa`);
  - the `rust7` single maintainer;
  - Windows named-pipe and stdio behaviour in the sidecar;
  - a sync/async mismatch in the bridge.
- **Parallelism:** can run in parallel with REUSE-03.
- **Out of scope:** protocol 4, Studio write policy UI, Modbus.

## REUSE-03 — Legacy import and canonical model v2 (RUST-05A)

- **Scope:**
  - canonical schema v2 additions (`PROJECT_MODEL_COMPATIBILITY.md` §6.4) with engineering-core commands and
    validation;
  - a read-only Rust decoder for HMI Forge v1–v10;
  - the `ImportLegacyProject` command;
  - the migration report;
  - fixtures.
- **Dependencies:** **O-3 model additions**, O-4 expressions and O-1 (for the fixtures from HMI Forge).
- **Deliverables:**
  - `project-model` v2 with schema and migration;
  - a crate `legacy-import`;
  - fixtures;
  - `docs/LEGACY_IMPORT.md`.
- **Approach:**
  - model changes first, in their own PR;
  - then the decoder, the mapping and the report.
- **Acceptance:** `LEGACY_PROJECT_MIGRATION.md` §6.
- **Tests:** per-version, negative, determinism and leaf-accounting tests.
- **Risks:**
  - expression semantics if not adopted;
  - `ScaleGroup`;
  - industrial symbols without canonical kinds;
  - id collisions.
- **Parallelism:**
  - the model-v2 PR is serial with other `project-model` changes;
  - the decoder can be written in parallel against fixtures.
- **Out of scope:** writing HMI Forge JSON, and runtime changes.

## REUSE-04 — Runtime compatibility (RUST-05B)

- **Scope:**
  - P1: a Rust compiler from canonical to runtime package format 2, with signing;
  - P2: a canonical decoder in the Kotlin runtime, `hmi-ui` semantic layout and package format 3;
  - the parity harness.
- **Dependencies:** REUSE-03 model v2 frozen; O-2 runtime repository; O-8 key compatibility (for signing).
- **Deliverables:**
  - the crate `runtime-package`;
  - golden packages;
  - runtime repository PRs;
  - a parity report;
  - `docs/RUNTIME_PACKAGES.md`.
- **Approach:**
  - first, byte-identical format-2 packages;
  - then the refusal rules;
  - then P2 behind a format version, with P1 kept until P2 is accepted.
- **Acceptance:**
  - golden packages identical;
  - the Kotlin reader admits signed packages;
  - parity within tolerance on JVM and an Android emulator;
  - manual RUST-05 acceptance (D-008).
- **Tests:** `REUSE_VALIDATION_PLAN.md` §3.
- **Risks:**
  - HTML versus Compose layout differences;
  - fonts;
  - Android emulator availability in hosted CI.
- **Parallelism:**
  - Kotlin P2 work runs in parallel with the Rust compiler once the schema is frozen;
  - the parity harness can start with REUSE-03 fixtures.
- **Out of scope:** generated Kotlin source (`COMPOSE_RUNTIME_INTEGRATION.md` §2), alarms runtime, recipes.

## REUSE-05 — Deployment Mode A and Mode B (RUST-07)

- **Scope:**
  - a Rust publisher key store;
  - pairing and a signed deployment client;
  - Tauri Publish and Add device UI;
  - a standalone bundle (installer plus signed package plus settings preset);
  - the runtime installers in the runtime repository's hosted CI.
- **Dependencies:**
  - REUSE-04 P1;
  - HMI Forge #55 resolved or the protocol frozen;
  - O-8;
  - signing decisions (release signing remains separate).
- **Deliverables:** crates `publisher-keys` and `deploy-client`; UI; `docs/DEPLOYMENT.md`.
- **Approach:** reproduce #55's client against the merged C.3b listener in CI.
- **Acceptance:**
  - pairing, deploy, staged activation and rollback work end to end against a desktop runtime in CI;
  - all refusal cases pass;
  - manual RUST-07 deployment acceptance.
- **Risks:**
  - certificate pinning across platforms;
  - clock skew;
  - Android still file-only until C.4.
- **Parallelism:** the key-store and client work can start once REUSE-04's signing format is fixed.
- **Out of scope:**
  - production or field acceptance;
  - Windows code signing;
  - Android listener (HMI Forge C.4);
  - runtime self-update.

## REUSE-06 — Migration acceptance and retirement (RUST-08 part)

- **Scope:**
  - end-to-end: legacy project → HMIStudio → publish → desktop and Android runtimes on simulation and test servers;
  - physical-device check of D-033;
  - a retirement plan for HMI Forge `studio-desktop`;
  - final notices.
- **Dependencies:** REUSE-02 … 05.
- **Deliverables:** acceptance record; retirement decision record; updated roadmap.
- **Acceptance:**
  - all automated gates pass;
  - owner manual acceptance at RUST-08;
  - every unverified item in `DEPLOYMENT_AND_PUBLISHING.md` §2 either verified or explicitly carried forward.
- **Risks:** physical hardware availability; decision 14 (physical machinery) remains separate.
- **Out of scope:** production commissioning.

## Sequence and parallelism

```
REUSE-01 ──► owner decisions O-1..O-12
               ├─► REUSE-02 (engine bridge) ───────────────┐
               └─► REUSE-03a (model v2) ─► REUSE-03b (import)
                                          └─► REUSE-04 P1 ─┴─► REUSE-05 ─► REUSE-06
                                               REUSE-04 P2 (Kotlin, parallel) ──┘
```

Independent tasks that can start without waiting for others, each once the owner authorizes it:

- the engine licence and notices generation (O-1);
- exporting the demo projects to JSON in HMI Forge or the runtime repository;
- writing synthetic v1–v10 fixtures;
- the Rust protocol 3 client against `messages.json`, using only the fixtures;
- the Ed25519 vector tests in Rust;
- the advisory triage (O-6).
