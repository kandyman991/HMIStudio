# REUSE-01 — Compose runtime integration

Status: **proposed**. Source: HMI Forge `92a612d` (`hmi-ui`, `runtime`, `runtime-desktop`, `runtime-android`).

## 1. What exists

HMI Forge already has one multiplatform runtime (D-033, RT-01):

- `hmi-ui`: the Compose Multiplatform renderer shared with Studio Preview. It is Compose-only by decision: Canvas
  rendering was rejected (D-007, R-003) and a guard test enforces this.
- `core`: the platform-neutral rules (bindings, expressions, navigation, operator actions, write policy, access
  rules).
- `runtime`: the package loader, controller, live data with engine supervision, the write controller, operator
  accounts and audit, the install store, and signed admission.
- `runtime-desktop` (Windows x64, Linux x64/ARM64) and `runtime-android` (API 26+, three ABIs).

The runtime is **data-driven**. It interprets the project JSON inside a `.hmipkg` package at run time. No
per-project code generation or compilation takes place.

## 2. Three approaches compared

| Criterion | (1) Generated Kotlin source per project | (2) Data-driven reusable runtime (HMI Forge today) | (3) Hybrid: data-driven plus optional generated or compiled extensions |
| --- | --- | --- | --- |
| Toolchain on engineer machine | Kotlin compiler, Gradle, Android SDK and signing keys for every publish | none (package is data) | none for normal projects; toolchain only for extensions |
| Mode B project-only publishing | a new app binary each time, so not "project-only" | native fit: send the package, device keeps runtime | fits for data; extensions need a runtime update |
| Android | rebuild and re-sign the APK per project; Play/MDM update cycle | one signed APK per ABI, packages are data | as (2) |
| Signed deployment (D-034) | binary signing per project | package signature (exists) | both |
| Editor ↔ runtime parity | generator must reproduce semantics; second implementation of rules | same `core` rules as Preview (HMI Forge); for HMIStudio the Studio preview is HTML, so parity must be tested | as (2) |
| Performance | best | sufficient for HMI screens (not measured on physical panels) | as (2), with fast paths |
| Safety and review | generated code needs review per project | fixed reviewed runtime; project cannot add code | extensions need their own review gate |
| Existing evidence | none | RT-01A–C tests, emulator acceptance | none |

**Recommendation: (2) data-driven, keeping (3) as a later and separately approved option.** Approach (2) is what
exists and is tested. It is the only one that satisfies D-002 Mode B without a toolchain on the engineer's machine,
and it keeps the operator runtime free of per-project code, which is better for safety review. Approach (1) is not
recommended. Approach (3) is the place for custom widgets later, behind its own decision and review gate, and is out
of scope for REUSE-02 … 06.

## 3. Adapting the runtime to the canonical model

The HMIStudio designer is HTML (Tauri) and the runtime is Compose. Parity can no longer come from sharing a
renderer, so it has to be specified and tested. Two phases:

| Phase | Runtime input | Change in the runtime (Kotlin, runtime repository) | Change in HMIStudio (Rust) |
| --- | --- | --- | --- |
| **P1 compile** (REUSE-04a) | HMI Forge runtime package (format 2) | none, or minimal, for already-mapped features | Rust compiler canonical → `HmiProject` JSON (current runtime schema) → `.hmipkg` writer and signer; refuses unmappable features |
| **P2 native** (REUSE-04b) | canonical project JSON inside a package (new package format 3) | a Kotlin decoder for the canonical schema, validated against `hmi-project.v1.schema.json`, and a mapping into the existing render and runtime model; `hmi-ui` adds semantic `Element` tree layout (Row, Column, Box, Grid, Scroll, Absolute, Instance), responsive rules and missing kinds | package format 3 writer |

Phase 1 delivers a working runtime early, with zero runtime risk. Phase 2 removes the derived model. The
divergence controls are in `PROJECT_MODEL_COMPATIBILITY.md` §6.

## 4. Parity verification

- **Fixture corpus:** canonical projects that cover every component kind, layout element, binding transform, action
  and event, plus the legacy fixtures (`LEGACY_PROJECT_MIGRATION.md` §5).
- **Runtime side:** Compose UI tests run headless on JVM and on an Android emulator. They assert the semantics
  tree (bounds within tolerance, text, enabled and visible state) for each fixture and target profile.
- **Studio side:** the same fixtures are rendered in the Tauri preview, and the layout boxes are exported as JSON.
  Bounds are compared with the runtime's boxes per profile, with a documented tolerance.
- **Behaviour:** scripted event sequences (press, release, value change, navigation) run against the simulated tag
  source. The resulting tag writes and navigation are compared between Studio preview and runtime.
- **Manual acceptance:** at RUST-05, as required by D-008.

## 5. Gaps that block runtime parity

| Gap | Location | Milestone |
| --- | --- | --- |
| Semantic node tree layout (`Box`, `Scroll`, tracks, responsive rules) | `hmi-ui` | REUSE-04b |
| Value types beyond protocol 3 (DateTime, UInt64, arrays, structs) | engine protocol, runtime model | REUSE-02 (decision O-9) |
| Alarm runtime (evaluation, acknowledgement) | both | RUST-06 |
| Recipes | both | RUST-06 |
| Localization (`LocalizedText` keys) | runtime | REUSE-04b |
| Physical-panel and ARM-hardware runs | all targets | REUSE-06 |
