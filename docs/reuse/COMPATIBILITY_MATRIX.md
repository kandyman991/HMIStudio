# REUSE-01 — Compatibility matrix

Status: **proposed classification**. It was taken from HMI Forge `92a612d` and HMIStudio `264ace7`. A
classification is not a statement that the component is production-ready. In particular, "DIRECT REUSE" means that the
code can be used without functional change; its evidence, licence and acceptance gaps still apply.

## Classes

| Class | Meaning |
| --- | --- |
| DIRECT REUSE | Used unchanged, in its own language and process, behind an existing contract |
| ADAPT | Kept in its language. Changed to fit the canonical HMIStudio contract |
| PORT | Behaviour reimplemented in Rust or TypeScript for HMIStudio. The Kotlin code and its tests are the specification |
| BRIDGE | A translation layer between HMI Forge and HMIStudio contracts (process protocol, legacy import, package compile) |
| REFERENCE ONLY | Read for behaviour, rules, UX or lessons. No code moves; HMIStudio already has or will build its own |
| REPLACE | Superseded by an HMIStudio component; retired after migration |
| INVESTIGATE | Insufficient evidence for a decision; a discriminating check is named |

"Primary" is the class used in the statistics. When the Studio (HMIStudio, Rust/Tauri) and runtime (Kotlin) sides
differ, both are given.

## Matrix

| ID | Component (HMI Forge path) | Primary | Studio side | Runtime side | Evidence, blockers | Target |
| --- | --- | --- | --- | --- | --- | --- |
| E1 | Engine protocol 3, `CONFIG_VERSION` 1, fixtures (`engine/src/protocol.rs`, `engine/protocol/*.json`) | DIRECT REUSE | Rust client speaks it unchanged | unchanged | 4 contract tests pass; value types limited to 6 (see `RUST_ENGINE_INTEGRATION.md` §4) | REUSE-02 |
| E2 | Engine state machine (`engine/src/engine.rs`) | DIRECT REUSE | inside sidecar binary | inside child process | demand, pulses, arming, shutdown disconnects all | REUSE-02 |
| E3 | Engine write guard (`engine/src/guard.rs`) | DIRECT REUSE | same binary | same | `write-guard-cases.json` shared with Kotlin; arming ≤ 900 s, pulse 100–5000 ms | REUSE-02 |
| E4 | OPC UA driver (`driver/opcua.rs`) | DIRECT REUSE | sidecar | child process | 9 tests vs bundled test server only; `async-opcua` MPL-2.0; advisories RUSTSEC-2023-0071 (`rsa`, no fix) and RUSTSEC-2026-0592 (`url`) | REUSE-02 |
| E5 | S7 driver and address parser (`driver/s7.rs`, `s7_address.rs`) | DIRECT REUSE | sidecar | child process | 18 tests vs `snap7-server`; `rust7` =0.1.2 pinned single-author crate; no physical PLC | REUSE-02 |
| E6 | Loopback driver (`driver/loopback.rs`) | DIRECT REUSE | test device | test device | used by runtime self-test | REUSE-02 |
| E7 | OPC UA and S7 test servers (`testserver*.rs`, feature `test-server`) | DIRECT REUSE | test-only | test-only | pull `async-opcua-server` → `heapless` 0.7.17 unsound advisories, test-only | REUSE-02 |
| E8 | Engine integration tests (`engine/tests/`) | DIRECT REUSE | CI | CI | 37 pass in this audit | REUSE-02 |
| E9 | Cross-build scripts (`engine/scripts/`) | ADAPT | Windows/Linux sidecar target names for Tauri | Android/ARM64 unchanged | Tauri sidecar needs target-triple file names | REUSE-02 |
| E10 | Engine ↔ HMIStudio tag engine | BRIDGE | `EngineDriver` in `crates/tag-engine` over protocol 3 | — | tag-engine PR #7 unmerged; sync trait vs async engine | REUSE-02 |
| C1 | `EngineClient`, `LiveSession` (Kotlin) | DIRECT REUSE | PORT client behaviour to Rust | unchanged | 13 end-to-end tests | REUSE-02 |
| C2 | OPC UA online browse (`OnlineTagImporter`) | PORT | Rust over engine `browse` | unchanged | uses engine `browse` request; no new driver code | REUSE-02 |
| M1 | Project model and JSON v1–v10 (`core/model`, `core/io`) | BRIDGE | read-only legacy decoder in Rust for import | stays runtime's in-memory model until REUSE-04 | strict decoding, additive versions; second model risk handled in `PROJECT_MODEL_COMPATIBILITY.md` §6 | REUSE-03 |
| M2 | Expression language contract 1 (`core/binding`) | PORT | Rust evaluator if owner adopts expressions (decision O-4) | unchanged | 22 tests; grammar in `docs/BINDINGS.md` §4 | REUSE-03 |
| M3 | Typed operations, 39 kinds (`core/operations`) | REFERENCE ONLY | HMIStudio 34 commands (RUST-03B) | — | duplicate of RUST-03B; mine gaps only | — |
| M4 | Revision store `.hmistudio` (`core/storage`, `storage-jvm/FileProjectRepository`) | REFERENCE ONLY | RUST-03B persistence; legacy reader for import only | — | D-006; HMIStudio uses different persistence (03B-5/6) | REUSE-03 (reader) |
| M5 | Validation rules (`core/validation`) | REFERENCE ONLY | mine missing rules into project-model validation | — | 509 lines | REUSE-03 |
| M6 | Write commands (`WriteCommand` MOMENTARY/MAINTAINED/SETPOINT, `writeConfirmed`, `commissioningAllowed`, limits) | PORT | canonical model addition (decision O-3) | unchanged | safety-relevant; must never be dropped on import | REUSE-03 |
| M7 | Security model (roles, permissions, `requiredPermission`, anonymous role, auto-logout) | PORT | canonical model addition (O-3) | unchanged | HMIStudio model has no security section | REUSE-03 |
| M8 | Enumerations | PORT | canonical model addition or `map` transform | unchanged | INVESTIGATE mapping to `PropertyType::Enum` | REUSE-03 |
| M9 | Assets and theme (`core/graphics`, model `assets`, `theme`) | ADAPT | map to `resources`, `themes`, `styles` | runtime reads canonical styles | 14 tests | REUSE-03/04 |
| M10 | Reusable definitions and faceplates (`core/reusable`) | ADAPT | map to `ComponentDefinition` + `Instance` | runtime renders canonical instances | faceplate open action missing in HMIStudio | REUSE-03/04 |
| M11 | Layout intent (`ScreenLayout`: Stack, Slot, ScaleGroup) and legacy `RectDp` | ADAPT | import into `Element` tree | runtime layout engine accepts canonical tree | `ScaleGroup` has no canonical equivalent (INVESTIGATE) | REUSE-03/04 |
| M12 | Render model (`core/render`) | ADAPT | — | input from canonical model | 777 lines | REUSE-04 |
| M13 | Navigation (`core/navigation`) | DIRECT REUSE | canonical `Navigation` already | unchanged | `navigate.startup` has no canonical action | REUSE-04 |
| M14 | Operator interaction (`core/interaction`) | DIRECT REUSE | canonical actions/events | unchanged | event and action vocabularies differ (`PROJECT_MODEL_COMPATIBILITY.md` §4) | REUSE-04 |
| M15 | Live values and quality (`core/live`) | DIRECT REUSE | tag-engine has own quality model | unchanged | quality enums map 1:1 GOOD/BAD/UNCERTAIN | REUSE-04 |
| M16 | Simulation (`core/simulation`) | REFERENCE ONLY | tag-engine simulator (RUST-06A) is broader | unchanged | 124 lines | — |
| M17 | Engineering search and cross-reference (`core/search`) | PORT | Rust, RUST-06 search scope | — | 20 tests as specification | RUST-06 |
| M18 | Tag import parsers TIA, L5X (`core/tagimport`, `tag-import-jvm`) | PORT | Rust parsers | — | 12 tests; no third-party code | RUST-06 |
| M19 | Siemens presets (`core/runtime/SiemensPresets.kt`) | PORT | Rust connection presets | — | small | REUSE-02 |
| M20 | Demo projects (`core/demo`) | ADAPT | exported as legacy JSON fixtures | unchanged | needs Gradle run to export; owner-authored synthetic data | REUSE-03 |
| M21 | Runtime package format 1/2 (`core/deploy/RuntimePackages.kt`) | PORT | Rust writer, byte-identical output | unchanged reader | deterministic ZIP order and timestamps; Ed25519 signature over `manifest.json` | REUSE-04 |
| M22 | `core/screens` | REFERENCE ONLY | designer features (RUST-04) | — | — | — |
| M23 | Alarm definitions (no runtime evaluation found) | REFERENCE ONLY | canonical alarms richer | runtime needs alarm runtime | gap on both sides | RUST-06 |
| U1 | Compose renderer (`hmi-ui`) | ADAPT | — | render canonical layout and component kinds | emulators only; 2 direct tests | REUSE-04 |
| R1 | Runtime controller, package archive, live data, write controller, operator access (`runtime`) | DIRECT REUSE | — | unchanged | 44 tests; RT-01B.2 | REUSE-04 |
| R2 | Package store, rollback, power-loss recovery, diagnostics (`runtime/install`) | DIRECT REUSE | — | unchanged | `RuntimeMaintenanceTest` | REUSE-05 |
| R3 | Signed admission, trust list, publisher keys (`runtime/deploy`) | DIRECT REUSE | Studio must produce compatible signatures | unchanged | RFC 8032 vectors shared; D-034 | REUSE-05 |
| R4 | Desktop HTTPS deployment listener and pairing (`runtime-desktop/deploy`) | DIRECT REUSE | Rust client required | unchanged | development-only; disabled by default; no Android listener | REUSE-05 |
| R5 | Studio publish client and dialogs (PR #55, draft, unmerged) | PORT | Rust client + Tauri UI | — | unmerged; base not `main` | REUSE-05 |
| R6 | Studio publisher key store (`studio-desktop/deploy/PublisherKeyStore`) | PORT | Rust, file-format compatible | — | PBKDF2-SHA256 210k + AES-256-GCM backup; compatibility decision O-8 | REUSE-05 |
| R7 | Desktop runtime application (`runtime-desktop`) | DIRECT REUSE | — | unchanged | MSI unsigned; ARM64 under qemu only | REUSE-05 |
| R8 | Android runtime application (`runtime-android`) | DIRECT REUSE | — | unchanged | emulator only; release-secret provisioning unconfirmed | REUSE-05 |
| R9 | Runtime installers (`runtime-installers.yml`, jpackage) | ADAPT | — | move to runtime repository CI on hosted runners | self-hosted runner policy differs (D-006) | REUSE-05 |
| R10 | Android child-process engine (`AndroidEngine`, D-033) | INVESTIGATE | — | keep | D-033 calls it an unverified hypothesis for physical devices; test on one physical device per ABI | REUSE-06 |
| S1 | Local users and audit trail (`storage-jvm/security/LocalSecurity`) | DIRECT REUSE | — | unchanged | PBKDF2; hash-chained | REUSE-04 |
| S2 | Portable project files, runtime package files (`storage-jvm`) | DIRECT REUSE | — | unchanged | — | REUSE-04 |
| S3 | Atomic files (`storage-jvm/AtomicFiles`) | REFERENCE ONLY | 03B persistence exists | unchanged | — | — |
| D1 | Studio application (`studio-desktop`, Compose) | REPLACE | Tauri workspace | — | 11,720 lines; UX reference | REUSE-06 |
| D2 | `StudioSession` and editor state | REFERENCE ONLY | engineering-core `ProjectSession` | — | — | — |
| D3 | Studio live tags, operator runner, Preview | REFERENCE ONLY | future RUST-04/05 preview | — | — | — |
| T1 | `tools/bootstrap-gradle.sh` | ADAPT | — | runtime repository; commit a wrapper | network-dependent | REUSE-04 |
| T2 | `examples/conveyor-demo.hmiforge.json` | DIRECT REUSE | legacy import fixture | — | after licence decision O-1 | REUSE-03 |
| W1 | `ci.yml`, `engine-targets.yml`, `android-runtime.yml` | ADAPT | engine jobs on hosted runners | runtime repository | HMI Forge uses an owner self-hosted runner; HMIStudio policy forbids that for public PRs | REUSE-02/04 |
| W2 | `windows-installer.yml` (Studio MSI) | REPLACE | HMIStudio desktop CI builds Tauri MSI | — | — | — |
| W3 | `update-chat-handoff-state.yml` | REFERENCE ONLY | HMIStudio `handoff.yml` | — | — | — |
| W4 | `runtime-installers.yml` | ADAPT | see R9 | — | — | REUSE-05 |
| X1 | Documentation (46 files) | REFERENCE ONLY | cited as specifications | — | do not copy private operational records | — |
| X2 | Decisions D-001…D-034, failures R-001…R-010 | REFERENCE ONLY | proposed carry-overs in `AUDIT_SUMMARY.md` §8 | — | not automatically accepted in HMIStudio | — |
| X3 | PR #50 3D-01 GLB preflight (draft, conflicting) | INVESTIGATE | RUST-09 scope | — | check after RUST-09 is planned | RUST-09 |
| X4 | PR #51 VIS-00 FigCraft feasibility (docs) | INVESTIGATE | design tooling question | — | not reviewed in depth | — |
| X5 | Branch `tmp/ide-01-screenshot-view` (no PR), closed #9, #18, #33 | REFERENCE ONLY | — | — | superseded or temporary | — |
| X6 | S7CommPlus research (R-004, D3.1) | REFERENCE ONLY | — | — | no implementation; never transfer AGLink or third-party protocol material | — |

## Statistics

Counted from the "Primary" column above (64 rows). Reproduce with `grep -E '^\| [A-Z][0-9]+ \|' COMPATIBILITY_MATRIX.md | awk -F'|' '{print $4}' | sort | uniq -c`.

| Class | Rows |
| --- | --- |
| DIRECT REUSE | 21 |
| ADAPT | 11 |
| PORT | 11 |
| BRIDGE | 2 |
| REFERENCE ONLY | 14 |
| REPLACE | 2 |
| INVESTIGATE | 3 |
| **Total** | **64** |

Rows are components of very different size; counts are not a measure of reused effort. By main lines (methodology in
`HMIFORGE_INVENTORY.md` §1), the modules whose primary disposition is to stay in place unchanged on the runtime side
(`engine`, `engine-client`, `runtime`, `runtime-desktop`, `runtime-android`, `storage-jvm`) total 10,445 of 40,099
main lines; `studio-desktop` (REPLACE plus references) is 11,720; `core`, `hmi-ui` and `tag-import-jvm` (mixed) are
17,934. These are sizes, not reuse percentages.
