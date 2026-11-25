# REUSE-01 — Audit summary and handoff record

Status: **audit complete, proposed for independent review**. Nothing is implemented, migrated or accepted. REUSE-02
is not started and needs owner authorization.

| | |
| --- | --- |
| Date | 2026-11-24 |
| Source | `kandyman991/hmiforge` `main` `92a612d104693800d7119ee4dc0114636b19002f` (read-only) |
| Destination base | `kandyman991/HMIStudio` `main` `264ace7e47d3c16c21c8415875fb64db1bf7c1d0` |
| Branch | `docs/reuse-01-hmiforge-audit` |
| Concurrent work observed | HMIStudio PR #7 RUST-06A (`de9e207`, open), PR #8 RUST-02C (`de884bb`, open; edits `CHAT_HANDOFF.md`, `ROADMAP.md`, `docs/MILESTONE_PLAN.md`, `docs/ENGINEERING_DECISIONS.md`) |

## 1. Executive summary

HMI Forge is a mature private Kotlin Multiplatform and Rust codebase: about 40,000 main lines and 537 tests. Its value
for HMIStudio is concentrated in three places:

- **the Rust communication engine:** OPC UA, S7, the write guard, and a stable, fixture-tested protocol 3;
- **the complete Compose runtime:** desktop and Android, with live data, operator accounts, audit, guarded writes,
  install store, rollback and kiosk;
- **the signed deployment chain:** Ed25519 packages, trust lists, and desktop pairing with an authenticated
  listener.

None of these exist in HMIStudio, and all of them can be reused without rewriting them.

The HMI Forge **Studio** (Compose Desktop, about 11,700 lines) and its **project model, operations and storage** are
superseded by HMIStudio's Tauri workspace and RUST-03A/03B. They become a behavioural reference and a **legacy import
source**. They are not a second model.

Recommended architecture:

1. **Engine as a separate executable everywhere.** Studio runs it as a Tauri sidecar, bridged into `tag-engine` by an
   `EngineDriver`. Protocol 3 stays unchanged.
2. **One canonical editable model (HMIStudio).** A one-way legacy importer brings in HMI Forge projects and never
   silently discards anything.
3. **The data-driven Kotlin runtime is kept.** It is fed first by compiled format-2 packages, later by the
   canonical model natively.
4. **RT-01C deployment is reused** for Mode B, and Mode A is completed as runtime + signed package bundles.

The two blocking prerequisites are owner decisions:

- **licensing** (O-1). Neither repository has a licence and the engine is `UNLICENSED`;
- **repository strategy** (O-2).

## 2. Statistics

Full methodology is in `HMIFORGE_INVENTORY.md` §1.

- **Modules:** 10 code modules, plus `tools`, `examples`, 6 CI workflows and 46 documentation files.
- **Size:** 162 main files with 40,099 main lines; 100 test files with 18,425 test lines.
- **Tests:** 537 (500 Kotlin, 37 Rust).
- **Pull requests:** 52 (46 merged, 3 closed unmerged, 3 open: #50, #51, #55).
- **Branches:** 52, including one temporary branch with no PR.
- **Engine dependencies:** 222 locked external crates; 2 advisories (vulnerabilities) and 12 warnings.
- **Classification:** 64 components (`COMPATIBILITY_MATRIX.md`).

  | Class | Components |
  | --- | --- |
  | DIRECT REUSE | 21 |
  | ADAPT | 11 |
  | PORT | 11 |
  | BRIDGE | 2 |
  | REFERENCE ONLY | 14 |
  | REPLACE | 2 |
  | INVESTIGATE | 3 |

No reuse percentage is claimed.

## 3. Most valuable reusable components

1. `hmi-engine` with its protocol 3 and shared fixtures, OPC UA and S7 drivers, and write guard (E1–E8). It has 37
   tests, all of which passed when re-run in this audit.
2. The Kotlin runtime stack (`runtime`, `runtime-desktop`, `runtime-android`, `hmi-ui`, `storage-jvm` security),
   covering RT-01A–C.1 (R1–R8, S1).
3. Signed packages and device trust (Ed25519 through Tink, admission, trust list, desktop pairing and listener), from
   RT-01C.3a/b (R3, R4).
4. The specifications encoded in tests:
   - the write policy (IDE-07);
   - the expression language contract;
   - package determinism;
   - tag-import parsers (TIA, L5X).

## 4. Major compatibility issues

1. **Model shape:** HMI Forge has variables plus mappings plus expressions. HMIStudio has tags with addresses plus
   transforms.
2. **Missing in the HMIStudio model:**
   - a security section (roles, permissions, `requiredPermission`);
   - write semantics (MOMENTARY/MAINTAINED/SETPOINT commands, confirmation, write limits, `writeConfirmed`,
     `commissioningAllowed`);
   - the `open.faceplate` action;
   - the `S7` driver kind;
   - 14 of the 28 HMI Forge component types (`levelIndicator` only approximately).
3. **Value types:** the engine has 6 scalars. HMIStudio has 13 scalars plus `DateTime`, arrays and structs, so
   protocol 4 is needed for full coverage (O-9).
4. **Driver-layer duplication risk:** the RUST-06A `tag-engine` `Driver` trait overlaps with the engine. Re-scope it
   to a bridge.
5. **Layout:** legacy `RectDp` and `ScaleGroup` against the semantic node tree. Parity between the HTML designer and
   the Compose runtime must now be proven by tests. It no longer comes from a shared renderer.
6. **Deployment:** Mode B is merged for the desktop device only. The Studio client is in draft PR #55. Android has
   file import only. Mode A has no combined bundle.

## 5. Risks and blockers

| Item | Type |
| --- | --- |
| No licence for either repository, engine `UNLICENSED` | **blocker** for any transfer (O-1) |
| Private repository cannot be fetched by public CI | blocker for submodule or private-package strategies (O-2) |
| RUSTSEC-2023-0071 `rsa` (no fix) and RUSTSEC-2026-0592 `url` in the shipped engine | release blocker until triaged (O-6) |
| No physical PLC, panel, ARM hardware or Android device evidence; D-033 Android child process is a hypothesis for physical devices | acceptance risk (REUSE-06) |
| `rust7` pinned single-author crate | supply-chain risk |
| Windows MSI unsigned; Android release signing secrets unconfirmed | release risk (separate from development) |
| HTML designer versus Compose runtime layout parity | technical risk (REUSE-04) |
| Kotlin suite not re-run in this audit (network allowlist) | evidence gap; HMI Forge CI on `63a9167` passed |
| Publishing private module structure in a public repository | owner confirmation needed (O-7) |

## 6. Owner decisions needed

| ID | Decision | Blocks |
| --- | --- | --- |
| O-1 | Licence for HMIStudio and for code transferred from HMI Forge (engine, fixtures, demo projects) | REUSE-02, fixtures |
| O-2 | Repository strategy: vendor the engine into HMIStudio, and keep the Kotlin runtime in a separate (private or public) runtime repository | REUSE-02, -04 |
| O-3 | Canonical model v2 additions: S7 and loopback kinds, security, write semantics, faceplate and startup actions, enumerations, industrial symbol library | REUSE-03 |
| O-4 | Adopt the HMI Forge expression language (contract 1) in the canonical model, or keep transforms only (unconvertible expressions then stay blocking) | REUSE-03 |
| O-5 | Studio engine hosting: sidecar process (recommended), not in-process | REUSE-02 |
| O-6 | Acceptance or mitigation of the engine advisories | REUSE-02 release |
| O-7 | Confirm that publishing this audit's level of detail about the private repository is acceptable | merging this PR |
| O-8 | Publisher key and trust compatibility: reuse existing HMI Forge key files and paired devices, or start fresh and re-pair | REUSE-05 |
| O-9 | Protocol 4 for DateTime, UInt64, arrays and structs | REUSE-02/04 |
| O-10 | Re-scope RUST-06 driver work to the engine bridge; map REUSE-03/04/05/06 onto RUST-05A/05B/07/08 | roadmap |
| O-11 | Runtime approach: data-driven (recommended); generated Kotlin is rejected for now; hybrid is deferred | REUSE-04 |
| O-12 | Fate of HMI Forge open PRs #50, #51 and #55 (and whether #55 is finished in HMI Forge first) | REUSE-05 |

Not changed: HMI Forge decision 14 (the first physical-machinery write). Production commissioning, trusted
deployment, release signing and licensing all remain separate. Development and simulation stay free of production
approval (HMIStudio D-004).

### Owner decision record — 2026-11-24

The owner approved the three recommendations presented for O-1, O-2 and O-7. This dated record supersedes pending-owner wording for these three decisions elsewhere in the original audit; the other audit statements remain historical observations.

- **O-1 — ACCEPTED direction:** retain proprietary ownership of first-party HMIStudio and HMI Forge code; do not select an open-source license at this stage. Public visibility is not an open-source license. Selective transfer of owner-controlled first-party engine code is authorized in principle under O-2, subject to documented rights/provenance, dependency license review, redistribution obligations and secret/customer-data screening. No blanket authorization for third-party or proprietary SDK code, and no release/distribution clearance is implied.
- **O-2 — ACCEPTED direction:** selectively import the audited Rust communication engine into HMIStudio using a pinned source revision and squashed provenance-recorded import, without private history. Retain HMI Forge as the private Kotlin/Compose runtime repository. Keep public CI independent of private credentials; use reviewed public contract fixtures and private cross-repository integration checks. Both runtime delivery modes and Windows/Linux/Android targets remain required. Do not retire working legacy functionality prematurely.
- **O-7 — ACCEPTED:** the owner authorizes publication of the architectural audit detail in this PR. This does not authorize publishing secrets, customer data, signing material, proprietary SDKs, private history or unaudited source.

O-3 through O-6 and O-8 through O-12 are not approved by this decision. Preserve the Rust desktop architecture, the canonical project model, and development/simulation policy. Exact source selection and import checks remain REUSE-02 deliverables; resolve applicable outstanding decisions before implementing affected work. No migration, release acceptance or production approval is claimed.

Next: independent source-level audit review, reconcile the PR with live main and concurrent RUST work, update the shared engineering records without overwriting concurrent changes, and obtain exact-head CI before merge. Do not equate owner approval with technical acceptance.

## 7. Implementation sequence and independent tasks

The sequence and the parallel tasks are in `REUSE_MILESTONE_PLAN.md`:

O-decisions → REUSE-02 ∥ REUSE-03a → REUSE-03b → REUSE-04 (P1, then P2 in parallel in Kotlin) → REUSE-05 → REUSE-06.

These tasks are independent and can start now once authorized:

- notices generation;
- demo-project export;
- synthetic v1–v10 fixtures;
- a fixture-only Rust protocol client;
- Rust Ed25519 vector tests;
- advisory triage.

## 8. Proposed changes to shared records (not applied)

These are **proposals only**. PR #8 (RUST-02C) is editing these files right now, so the edits should be made by
whoever merges after PR #8, after owner decisions.

**`ROADMAP.md` / `docs/MILESTONE_PLAN.md`:**

- add a row "REUSE-01 HMI Forge audit: candidate (this PR)";
- annotate RUST-05 as "RUST-05A legacy import (REUSE-03) / RUST-05B runtime parity (REUSE-04)";
- annotate RUST-06 drivers as "through hmi-engine bridge (REUSE-02)";
- annotate RUST-07 as "built on RT-01C deployment (REUSE-05)".

**`docs/ENGINEERING_DECISIONS.md`:** the proposed entries carry temporary IDs, so that they do not collide with
numbering in PR #8.

- **P-REUSE-1 (proposed):** the communication engine runs as a separate executable in Studio and runtime; there are
  no in-process production drivers; protocol 3 is kept.
- **P-REUSE-2 (proposed):** HMIStudio `project-model` is the only editable model; HMI Forge JSON is legacy input
  only; runtime packages are derived build outputs that are refused when lossy.
- **P-REUSE-3 (proposed):** the runtime stays data-driven Kotlin/Compose; generated per-project Kotlin source is not
  pursued.
- **P-REUSE-4 (proposed):** legacy import never silently discards data; unmapped data is preserved in `extensions`
  as a blocking finding.
- **P-REUSE-5 (proposed):** transfers from HMI Forge are squashed imports with provenance, after licence selection;
  no private history.

**`docs/FAILED_APPROACHES.md`:** no new failures were established. The HMI Forge records R-001…R-010 (for example
R-003 Canvas renderer, REJECTED_BY_DESIGN, and R-004 S7CommPlus, REJECTED_BY_DESIGN with an INCONCLUSIVE addendum)
are noted as context. They are not imported as HMIStudio records.

## 9. Handoff record

- **Verified:**
  - HMIStudio `main` is `264ace7` and its open PRs are #7 and #8 (2026-11-24);
  - HMI Forge `main` is `92a612d`;
  - HMI Forge PR #54 was merged on 2026-11-24, and #55 is a draft whose base is the merged C.3b branch.
- **Run:** the engine tests (37 passed), clippy (clean), `cargo audit` and the licence census. The Kotlin tests were
  not run (network).
- **Not done (by design):**
  - no HMI Forge change;
  - no code transfer;
  - no edits to `CHAT_HANDOFF.md`, `ROADMAP.md`, `MILESTONE_PLAN.md`, `ENGINEERING_DECISIONS.md` or
    `handoff/state.json` (concurrent PR #8);
  - no component migration;
  - REUSE-02 not started.
- **Next exact action:**
  - an independent review of this PR;
  - owner answers to O-1, O-2 and O-7 (merge prerequisite), then O-3 … O-12;
  - after PR #8 merges, the reviewer copies §8 into the shared records, adds a `CHAT_HANDOFF.md` entry citing this
    PR's verified head, and merges;
  - REUSE-02 starts only on explicit authorization.

## Independent review findings and corrections — 2026-11-24

This is a partial independent static review, not approval or a completed acceptance gate. The following corrections supersede conflicting claims elsewhere in the audit. Source inspected at the pinned HMI Forge revision, including `engine/Cargo.toml`, `engine/src/protocol.rs`, `engine/src/engine.rs`, `engine/src/guard.rs` and `engine/src/main.rs`. No test execution or source migration was performed during this review.

| Finding | Correction / required acceptance |
| --- | --- |
| Protocol negotiation | `Engine::handle` compares the requested protocol against exactly `PROTOCOL_VERSION`; mismatch returns `UNSUPPORTED_PROTOCOL`. This is a version handshake, not multi-version negotiation. REUSE-02 must keep protocol 3 unchanged. Any later protocol 4 must explicitly implement and test a backward-compatible handshake/codec strategy. |
| Numeric write limits | `Value::as_f64` casts i64 to f64, and `check_write` compares that rounded value to f64 bounds. Adjacent integers above 2^53 may round identically. Treat Long/UInt32/UInt64 conversions separately from write-limit enforcement: UInt32 fits exactly, but unrestricted i64/u64 limit checking is not lossless. Before enabling guarded wide-integer writes, use an exact comparison or reject unsupported ranges and test boundary, type, sign and overflow cases. Do not declare the write guard production-ready. |
| Process lifecycle | stdin EOF and explicit shutdown trigger normal disconnect; stdout write failure only exits the writer task. Request/output channels are unbounded. Do not assume a crashed, stalled or disconnected client can never leave the engine running, or that pulses are guaranteed to deassert after a crash or failed driver write. Test bounded queues/framing, supervision, timeout, child termination/reaping, failed output, loss of client and stale write state. Never automatically retry a write or re-arm after restart. |
| Runtime design | Data-driven Compose is a proposal for the normal runtime; it does not revoke the requirement to evaluate generated Kotlin/Compose projects or standalone application exports. Preserve Mode A and Mode B; O-11 remains open. Performance superiority and zero runtime risk are unmeasured and must not be claimed. |
| Licensing | O-1/O-2/O-7 approved as recorded above; proprietary first-party ownership does not automatically grant third-party redistribution rights. Review exact dependency graph, notices and provenance, including MPL-covered source availability and bundled JRE obligations. Do not import private history. |
| Current GitHub state | At review, main was `58d83ca540463d9507c1cd76555d0bac5bb6378b`, two commits ahead of the audit fork point; PR #8 and #11 merged, #7 and #12 open. Legacy PR #55 remains open and #56 merged. Earlier inventory statuses are pinned historical observations, not live state. Reconcile from current main without clobbering parallel work. |

Remaining audit gate: independently verify the full inventory and model/runtime/deployment mappings against pinned source, review dependency and disclosure evidence, reconcile shared decisions/roadmap/handoff against live main, and obtain exact-head CI. The originally reported 37 passing engine tests and Kotlin CI are inherited evidence only. Do not mark REUSE-01 accepted on this partial review.

REUSE-02 preparation is authorized by the owner's request, but implementation remains a separate branch/PR after REUSE-01 merge. Start with REUSE-02A: pinned-source inventory, provenance/rights and sensitive-content checks, dependency advisory and notice triage, and an isolated protocol-3 contract test plan. Resolve O-5 hosting and applicable model/driver decisions before integrating the bridge; keep O-6 release disposition separate. Preserve existing simulator behavior; PR #7 must be reviewed and integrated before dependent bridge work. No source transfer, PLC connection, production approval or deployment is claimed.

## 10. Documents

`HMIFORGE_INVENTORY.md`, `COMPATIBILITY_MATRIX.md`, `RUST_ENGINE_INTEGRATION.md`, `PROJECT_MODEL_COMPATIBILITY.md`,
`COMPOSE_RUNTIME_INTEGRATION.md`, `LEGACY_PROJECT_MIGRATION.md`, `DEPLOYMENT_AND_PUBLISHING.md`,
`LICENSING_AND_PROVENANCE.md`, `REUSE_VALIDATION_PLAN.md`, `REUSE_MILESTONE_PLAN.md`, and this file.


## Follow-up review and reconciliation — 2026-11-24

Reviewed the pinned legacy tree via GitHub: 371 tracked blobs, nine Gradle modules plus Rust engine, all per-module Kotlin/Rust main/test file counts match the inventory, and six workflow files. Inspected the engine manifest, guard, process entry point, legacy project model, component catalog, package loader, package admission and deployment request/server source. Confirmed the destination project-model tree is unchanged between the audit fork and observed main `58d83ca540463d9507c1cd76555d0bac5bb6378b`. Corrected working documents to distinguish exact version handshake, normal shutdown versus failure supervision, unverified package signatures versus trusted admission, sensitive imported fields, and generated-runtime export evaluation. Historical owner decisions remain as recorded above. Shared roadmap/decision/handoff records reconciled without replacing concurrent changes. Prior exact-head CI on `268a12dcc8ac9476616abc25f6b3566a2709b678`: Desktop CI 45432107611 and handoff snapshot 45432107921 succeeded; new head requires fresh CI.

Limits: source inventory file counts are independently verified; physical line totals, test counts/results, full dependency graph/advisories, individual authorship and exhaustive field-by-field/runtime behavior remain inherited or unverified. No full local clone, tests, secret scan or hardware execution was performed in this review. The audit is a planning baseline, not proof of production readiness or transfer clearance. Further source/rights checks are mandatory before REUSE-02A import and each component's later acceptance. No private source or secrets transferred. Do not mark final merge gate passed until fresh CI and remaining review are verified.
