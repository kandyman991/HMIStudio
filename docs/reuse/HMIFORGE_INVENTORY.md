# REUSE-01 — HMI Forge inventory

Status: **audit record, proposed**. Nothing here is implemented, migrated or accepted.

| | |
| --- | --- |
| Source | `kandyman991/hmiforge` (private), `main` at `92a612d104693800d7119ee4dc0114636b19002f` (2026-11-24T06:48:56Z, "chore: refresh chat handoff state [skip ci]") |
| Destination | `kandyman991/HMIStudio` (public), `main` at `264ace7e47d3c16c21c8415875fb64db1bf7c1d0` (merge of PR #6) |
| Audit branch | `docs/reuse-01-hmiforge-audit` |
| Access | Read-only clone of the source. No commit, branch, PR, issue or setting was created or changed in HMI Forge. Builds ran from a separate target directory (`CARGO_TARGET_DIR` outside the clone). |

The source and the destination both call their product "HMI Studio". In this audit, **HMI Forge** means the private
`hmiforge` repository and **HMIStudio** means this public repository.

This document lists what exists. Classifications are in `COMPATIBILITY_MATRIX.md`. Nothing in this directory
contains source code, credentials, signing material, customer projects or private chat content from HMI Forge.
It contains repository paths, counts, interface names and summaries of behaviour, so that each conclusion can be
traced to a source file.

## 1. Measurement methodology

Every number in the REUSE documents comes from one of these commands, run at the source SHA above. No reuse
percentage is claimed anywhere.

| Quantity | Method |
| --- | --- |
| Source files and lines | `find <module> -name '*.kt'` (Kotlin) or `-name '*.rs'` (Rust), excluding `build/` and `target/`; lines are physical lines from `wc -l` (blank lines and comments included) |
| Main versus test | A file is a test file when its path contains a source-set directory named `test`, `tests` or ending in `Test` (`commonTest`, `jvmTest`, `androidTest`). Rust `#[cfg(test)]` modules inside `src/` count as main lines (two files: `driver/opcua.rs`, `driver/s7_address.rs`). |
| Test count | Kotlin: occurrences of `@Test` in test files. Rust: occurrences of `#[test]`/`#[tokio::test]` (37), matching the 37 tests that `cargo test` ran. |
| Dependencies | Rust: `Cargo.lock` and `cargo tree --locked -e normal`; licences: `cargo metadata`; advisories: `cargo audit` 0.23.0 with the advisory database fetched on 2026-11-24. Kotlin: `build.gradle.kts` of each module. |
| Pull requests and branches | GitHub REST API (`pulls?state=all`, `branches`), 2026-11-24 |
| CI status | GitHub Actions run list for the repository, 2026-11-24 |

## 2. Repository-level facts

- Build: Gradle Kotlin DSL, 9 Gradle modules (`settings.gradle.kts`) plus the Cargo crate `engine/`. Kotlin
  2.3.20, Compose Multiplatform 1.12.0, AGP 9.1.0, JDK 21, Gradle 9.5.0. No Gradle wrapper is committed;
  `tools/bootstrap-gradle.sh` downloads Gradle.
- 371 tracked files, 222 commits on `main`.
- Continuity records read: `AGENTS.md`, `CHAT_HANDOFF.md` (latest entry RT-01C.3b), `ARCHITECTURE.md`,
  `DEVELOPMENT.md`, `README.md`, `ROADMAP.md`, `docs/ENGINEERING_DECISIONS.md` (D-001 … D-034, IDE-12A/12B),
  `docs/FAILED_APPROACHES.md` (R-001 … R-010), `docs/MILESTONE_PLAN.md`, `docs/RUNTIME.md`,
  `docs/RT-01C3_BRIEF.md`, `docs/RUNTIME_ENGINE.md`, `docs/WRITE_POLICY.md`, `docs/SECURITY.md`,
  `docs/SECURITY_CRYPTO.md`, `docs/BINDINGS.md`, `docs/runtime-deployment.md`, `THIRD_PARTY_NOTICES.md`.
- Licence: no project licence. `engine/Cargo.toml` declares `license = "UNLICENSED"`. `licenses/Apache-2.0.txt`
  is the text for third-party Apache-2.0 components (Google Tink and others). `THIRD_PARTY_NOTICES.md` says that
  it is not yet a complete inventory.

## 3. Module inventory

The 12 attributes recorded for each module are: (1) purpose, (2) language and platforms, (3) size, (4) tests,
(5) internal dependencies, (6) external dependencies, (7) maturity evidence, (8) CI coverage, (9) licence and
provenance, (10) sensitive content, (11) HMIStudio counterpart or overlap, (12) proposed disposition. The
classification of the disposition is in `COMPATIBILITY_MATRIX.md`.

Size totals:

| Module | Main files / lines | Test files / lines | Tests |
| --- | --- | --- | --- |
| `engine` (Rust) | 14 / 4,308 | 6 / 2,205 | 37 |
| `engine-client` | 3 / 696 | 7 / 880 | 13 |
| `core` | 66 / 14,641 | 28 / 5,813 | 245 |
| `hmi-ui` | 11 / 2,855 | 1 / 65 | 2 |
| `runtime` | 20 / 3,679 | 6 / 1,016 | 44 |
| `runtime-desktop` | 5 / 700 | 2 / 278 | 11 |
| `runtime-android` | 3 / 237 | 6 / 555 | 10 |
| `storage-jvm` | 5 / 825 | 6 / 733 | 27 |
| `tag-import-jvm` | 4 / 438 | 1 / 203 | 5 |
| `studio-desktop` | 31 / 11,720 | 37 / 6,677 | 143 |
| **Total** | **162 / 40,099** | **100 / 18,425** | **537** (500 Kotlin, 37 Rust) |

### 3.1 `engine` — Rust communication engine (`hmi-engine`)

1. **Purpose:** hosts the communication drivers behind a versioned message boundary. It never starts a write on its
   own. Every write is the direct result of a `write` or `pulse` request that passes `guard::check_write`.
2. **Language and platforms:** Rust 2021, a library (`src/lib.rs`: `driver`, `engine`, `guard`, `protocol`) plus the
   binary `hmi-engine`. Built for Linux x64 (CI), Windows x64 (installer workflow), Linux ARM64 (cross build) and
   Android arm64-v8a, armeabi-v7a and x86_64 (NDK, API 26).
3. **Size:** `driver/opcua.rs` 1,048, `driver/s7_address.rs` 711, `engine.rs` 621, `driver/s7.rs` 569,
   `protocol.rs` 352, `testserver.rs` 214, `testserver_s7.rs` 201, `guard.rs` 168, `driver/loopback.rs` 157,
   `driver/mod.rs` 111, `main.rs` 59, plus the test-server binaries.
4. **Tests:** 37, re-run in this audit with `cargo test --locked --features test-server` (all passed; §5 of
   `REUSE_VALIDATION_PLAN.md`): 9 S7-address unit tests, 4 contract, 9 OPC UA, 2 process, 4 pulse and 9 S7. Shared
   fixtures in `engine/protocol/messages.json` and `engine/protocol/write-guard-cases.json` are also used by the
   Kotlin `EngineContractTest`.
5. **Internal dependencies:** none. Kotlin reaches it only through the process protocol.
6. **External dependencies:** `async-opcua` 0.19 (client only in the binary; server only with the `test-server`
   feature), `rust7` =0.1.2 (pure-Rust S7 client), `tokio` 1, `serde`/`serde_json`, `async-trait`, `futures-util`;
   `snap7-server` 0.1.7 is test-only. 222 locked external crates, all from crates.io. 200 distinct crates are in
   the normal (non-dev) dependency tree.
7. **Maturity evidence:** milestones D1 (#13), D2 (#14), D3a–c (#15–#17), IDE-07 guarded writes (#34) and RT-01B.1
   (#41) were merged. It has been exercised against the loopback driver, the bundled OPC UA test server and the
   `snap7-server` test PLC, on Android x86_64 emulators (API 26 and 34), and under qemu for ARM64. **No physical
   PLC has been tested**: owner decision 14 is still open for physical machinery.
8. **CI coverage:** `ci.yml` (Linux build and tests), `engine-targets.yml` (cross targets),
   `android-runtime.yml` (emulator), `runtime-installers.yml` (bundled engine).
9. **Licence and provenance:** owner-authored and `UNLICENSED`. Dependency licences are mostly MIT and/or Apache-2.0,
   with 9 MPL-2.0 crates (the `async-opcua` family), Unicode-3.0, CDLA-Permissive-2.0, Zlib and BSL-1.0 options.
10. **Sensitive content:** none found in the engine sources. Credentials enter only at run time (`configure`), and the
    test `credentials_never_appear_in_debug_output` checks that they are redacted.
11. **HMIStudio counterpart:** `crates/tag-engine` (RUST-06A, PR #7, **not merged**). Its synchronous `Driver` trait
    and in-process simulator overlap with the engine's driver layer. HMIStudio has no OPC UA or S7 implementation.
12. **Disposition:** reuse as a separate executable, with a Rust protocol client in HMIStudio (`RUST_ENGINE_INTEGRATION.md`).

### 3.2 `engine-client` — Kotlin JVM client of the engine

1. **Purpose:** starts `hmi-engine` as a child process (`EngineClient`), runs the live session (`LiveSession`) and
   performs OPC UA online browse for tag import (`OnlineTagImporter`).
2. **Language and platforms:** Kotlin JVM (Java 8 / Android API 26 compatible calls).
3. **Size:** 3 / 696 main.
4. **Tests:** 13, end-to-end against the real engine binary (`EngineEndToEndTest`, `ControlEndToEndTest`,
   `LiveDataEndToEndTest`, `S7EndToEndTest`, `EngineContractTest`, `PackagedEngineLocationTest`, `LiveSessionTest`).
5. **Internal dependencies:** `core`.
6. **External dependencies:** none beyond `core`.
7. **Maturity evidence:** used by Studio, `runtime-desktop` and `runtime-android` (RT-01B.2). Engine restart
   supervision is in `runtime/live/LiveData`.
8. **CI coverage:** `ci.yml`, `android-runtime.yml`.
9. **Licence and provenance:** owner-authored, no licence.
10. **Sensitive content:** none.
11. **HMIStudio counterpart:** none. HMIStudio needs a Rust client that speaks the same protocol.
12. **Disposition:** keep unchanged for the Kotlin runtime. Port its client behaviour (launch, handshake, framing,
    supervision) to Rust for Studio.

### 3.3 `core` — Kotlin Multiplatform model and rules (JVM + Android)

1. **Purpose:** the project model, serialization (schema v1–v10), validation, typed operations, bindings and the
   expression language, layout, rendering model, navigation, operator interaction, write policy, security rules,
   simulation, search and cross-reference, tag import parsers, runtime packages (`deploy/RuntimePackages.kt`) and
   demo projects.
2. **Language and platforms:** Kotlin Multiplatform `commonMain` (JVM and Android).
3. **Size by package (main lines):** `operations` 1,783 · `model` 1,414 · `graphics` 1,308 · `reusable` 1,199 ·
   `binding` 889 · `storage` 802 · `interaction` 779 · `render` 777 · `live` 713 · `runtime` 632 · `demo` 594 ·
   `layout` 585 · `tagimport` 583 · `screens` 533 · `validation` 509 · `security` 480 · `search` 322 ·
   `deploy` 281 · `navigation` 193 · `io` 141 · `simulation` 124.
4. **Tests:** 245. By package: operations 32, binding 22, layout 22, storage 21, search 20, reusable 18, screens 18,
   graphics 14, interaction 14, live 14, runtime 13, deploy 10, security 9, tagimport 7, simulation 5, plus 6 in
   `jvmTest`.
5. **Internal dependencies:** none.
6. **External dependencies:** `kotlinx-serialization-json` 1.7.3 only.
7. **Maturity evidence:** F0 (#12) to IDE-12B (#39), UX-02 (#49) and RT-01 were merged with CI. The model has 10 schema
   versions, and every older file decodes without migration (each version only adds optional fields). Strict
   decoding (`ignoreUnknownKeys = false`).
8. **CI coverage:** `ci.yml` (JVM tests), `android-runtime.yml` (Android compilation and emulator tests).
9. **Licence and provenance:** owner-authored, no licence.
10. **Sensitive content:** `demo/DemoProjects.kt` holds synthetic demo projects (pump station, food line, live
    conveyor). No customer data was found.
11. **HMIStudio counterpart:** `crates/project-model` (RUST-03A) and `crates/engineering-core` (RUST-03B) overlap with
    `model`, `io`, `validation`, `operations` and `storage`. `crates/tag-engine` (PR #7) overlaps with `binding`
    (transforms) and `simulation`.
12. **Disposition:** split per package (see the matrix). The Kotlin runtime keeps using it as it is. HMIStudio uses it
    as a legacy import source and as a behavioural reference, and does not run a second model.

### 3.4 `hmi-ui` — shared Compose renderer

1. **Purpose:** the one renderer used by Studio Preview and both runtimes (`AdaptiveScreen`, `IndustrialGraphics`,
   `InputControls`, `ComponentStyle`, `Viewports`, editing overlays).
2. **Language and platforms:** Compose Multiplatform (desktop JVM and Android). Canvas rendering was rejected by
   design (D-007, R-003, `RendererGuardTest`).
3. **Size:** 11 / 2,855 main.
4. **Tests:** 2 (`jvmTest`). Most rendering behaviour is tested through `runtime` and `studio-desktop`.
5. **Internal dependencies:** `core`.
6. **External dependencies:** Compose runtime, foundation, ui and material3.
7. **Maturity evidence:** RT-01A.1 (#38). It runs on Android emulators in CI. It has not been tried on physical panels.
8. **CI coverage:** `ci.yml`, `android-runtime.yml`.
9. **Licence and provenance:** owner-authored. Compose is Apache-2.0.
10. **Sensitive content:** none.
11. **HMIStudio counterpart:** none. HMIStudio has no Compose code. The HMIStudio designer is HTML (Tauri).
12. **Disposition:** reuse as the runtime renderer, adapted to the canonical layout tree.

### 3.5 `runtime` — shared runtime logic (desktop + Android)

1. **Purpose:** opens packages (`PackageArchive`), runs projects (`RuntimeController`, `RuntimeRoot`), handles live
   data and supervision (`live/LiveData`), the write path (`live/WriteController`), operators and audit
   (`security/OperatorAccess`), the installation store with rollback (`install/PackageStore`), diagnostics,
   device settings, and signed-package admission (`deploy/PackageAdmission`, `TrustList`, `PublisherKeys`,
   `DeploymentSecurity`, `DeploymentControl`).
2. **Language and platforms:** Kotlin Multiplatform. `jvmAndAndroidMain` is shared by both targets.
3. **Size:** 20 / 3,679 main.
4. **Tests:** 44 (`RuntimeTest`, `RuntimeLiveTest`, `RuntimeMaintenanceTest`, `SignedPackageTrustTest`,
   `Ed25519VectorsTest`, `DeploymentSecurityTest`).
5. **Internal dependencies:** `hmi-ui`, `engine-client`, `storage-jvm`.
6. **External dependencies:** Google Tink 1.23.0 (`tink` and `tink-android`, Apache-2.0).
7. **Maturity evidence:** RT-01A.2 (#40), B.2 (#42), C.1 (#43), C.3a (#48) and C.3b (#54) were merged. The status of
   each part is in `DEPLOYMENT_AND_PUBLISHING.md` §2.
8. **CI coverage:** `ci.yml`, `android-runtime.yml`, `runtime-installers.yml`.
9. **Licence and provenance:** owner-authored.
10. **Sensitive content:** none committed. Device data (accounts, audit trail, PKI, trust lists) is written at run
    time, outside packages.
11. **HMIStudio counterpart:** none. RUST-07 deployment is still planned.
12. **Disposition:** reuse as the runtime.

### 3.6 `runtime-desktop` — Windows and Linux runtime application

1. **Purpose:** command line, full screen or kiosk, device store, start at boot, `--self-test`, and the desktop
   HTTPS deployment listener with pairing (`deploy/RuntimeDeploymentServer`, `DeviceTlsIdentity`, RT-01C.3b).
2. **Language and platforms:** Compose Desktop (JVM). Packaged with jpackage: MSI, .deb, and an ARM64 tarball.
3. **Size:** 5 / 700 main.
4. **Tests:** 11.
5. **Internal dependencies:** `runtime`.
6. **External dependencies:** Compose Desktop; jpackage at build time.
7. **Maturity evidence:** installers built and self-tested in CI (#44, run recorded in HMI Forge `docs/RUNTIME.md` §10); listener merged (#54) as development-only, disabled by default (brief §11).
8. **CI coverage:** `ci.yml`, `runtime-installers.yml` (Windows MSI and ARM64 only on manual request or tag).
9. **Licence and provenance:** owner-authored, no licence.
10. **Sensitive content:** none committed; TLS identity and trust lists are created at run time on the device.
11. **HMIStudio counterpart:** none.
12. **Disposition:** reuse as the desktop runtime. Windows MSI unsigned; ARM64 checked only under qemu.

### 3.7 `runtime-android` — Android runtime application

1. **Purpose:** an immersive or kiosk activity (`RuntimeActivity`), the engine as a child process from the native
   library directory (`AndroidEngine`), device-owner lock-task mode, and a boot receiver.
2. **Language and platforms:** Compose for Android, Android 8.0+ (API 26). ABIs: arm64-v8a, armeabi-v7a and x86_64.
3. **Size:** 3 / 237 main. **Tests:** 10 instrumented (emulator API 26 and 34).
4. **External dependencies:** `androidx.activity:activity-compose` 1.10.1.
5. **Maturity evidence:** emulator acceptance only. D-033 itself labels the Android child-process engine "an
   unverified deployment hypothesis" for physical devices. Release signing is configured, but per HMI Forge records,
   provisioning of the secrets is not confirmed.
6. **Disposition:** reuse, with physical-device acceptance still open.

### 3.8 `storage-jvm` — local storage

1. **Purpose:** atomic files (`AtomicFiles`), the project revision store (`FileProjectRepository`, D-006 `.hmistudio`
   bundle with immutable checksummed revisions and an atomic HEAD), portable project files, runtime package files,
   local users and the audit trail (`security/LocalSecurity`: PBKDF2 user store, hash-chained audit log).
2. **Language and platforms:** Kotlin JVM, Java 8 / API 26 compatible.
3. **Size:** 5 / 825 main. **Tests:** 27.
4. **HMIStudio counterpart:** `crates/engineering-core` persistence (RUST-03B: staging, verified backup, sidecar and
   autosave envelope) covers project storage. HMIStudio has no user or audit store.
5. **Disposition:** the runtime keeps it. For HMIStudio, it is a reference and a legacy import input.

### 3.9 `tag-import-jvm` — tag file importers

1. **Purpose:** TIA Portal tag tables, Rockwell Logix L5X, and a wizard (`TagImportWizard`). The parsers are in
   `core/tagimport`. The OPC UA online browse is in `engine-client`.
2. **Size:** 4 / 438 main. **Tests:** 5 (plus 7 in `core/tagimport`).
3. **External dependencies:** none (JDK only).
4. **HMIStudio counterpart:** none. Planned under RUST-06.
5. **Disposition:** port to Rust.

### 3.10 `studio-desktop` — Compose Desktop engineering application

1. **Purpose:** the current HMI Forge editor: menus and commands (`StudioCommands` 1,195), the main window (`Main.kt`
   2,294), panels (graphics, reusable components, interaction, screens, connections, security, writes, engineering,
   bindings, tag browser, search), the editor state, live tags, tag import, the operator runner, and the publisher
   key store (`deploy/PublisherKeyStore`).
2. **Size:** 31 / 11,720 main. **Tests:** 143.
3. **HMIStudio counterpart:** the Tauri and React workspace (RUST-01/02) and `engineering-core` commands. HMIStudio's
   architecture replaces this module (D-001).
4. **Disposition:** replace it. Keep it as the behavioural and UX reference. Port the publisher key store and
   deployment client logic.

## 4. Tools, examples, tests, CI and documentation

| Item | Contents | Note |
| --- | --- | --- |
| `tools/bootstrap-gradle.sh` | Downloads Gradle 9.5.0 | Needs network access to `services.gradle.org`, which this audit's sandbox refuses |
| `examples/conveyor-demo.hmiforge.json` | One schema-v1 project with legacy `RectDp` geometry | The only committed project file. More demos are built in code (`core/demo`) |
| `engine/scripts/` | `build-android.sh`, `build-linux-arm64.sh`, `hello-smoke.sh` | Cross builds and a qemu handshake smoke test |
| Tests | 537 test functions (§3), plus Android instrumented tests | Inventory and gaps: `REUSE_VALIDATION_PLAN.md` |
| `.github/workflows/` | `ci.yml`, `android-runtime.yml`, `engine-targets.yml`, `runtime-installers.yml`, `windows-installer.yml`, `update-chat-handoff-state.yml` | Most jobs run on an owner self-hosted Linux runner. The Windows jobs run only on manual request. See matrix rows W1–W4. |
| `docs/` | 46 Markdown files (6,185 lines), including `docs/evidence/` | Behavioural specifications cited throughout this audit |
| `handoff/state.json` | An automated snapshot, refreshed on every push to main | Possibly stale by design |

## 5. Pull requests, branches and unmerged work

- **52 pull requests** (#2 … #56): 46 merged, 3 closed without merge, 3 open.
- **Closed without merge:** #9 local development shortcuts, #18 D3.1 S7CommPlus research (superseded by #19), and #33
  write-policy docs (its content arrived through #34/#35).
- **Open:**
  - **#50** 3D-01 glTF/GLB preflight foundation. Draft, head `20e0caa`, 7 files, +442. Based on `main`, but GitHub
    reports it as conflicting with current `main`.
  - **#51** VIS-00 FigCraft feasibility, docs only. Head `dfcb2ba`, 2 files, +84.
  - **#55** RT-01C.3c Studio Publish, pinned-device pairing and publisher keys UI. Draft, head `c1ec06d`, 9 files,
    +726. Its base is `feat/rt-01c3b-device-deploy`, whose PR #54 is merged. It still has to be retargeted to `main`.
    CI on `c1ec06d` passed (2026-11-23).
- **52 branches.** Besides `main` and the merged or closed PR branches, there is one branch without any PR:
  `tmp/ide-01-screenshot-view` (`da94fb1`). It was not analysed beyond its name and is classified REFERENCE ONLY.
- **Latest CI evidence:** `main` `92a612d` differs from `1c8da07` (PR #56) only in `handoff/state.json`. PR #56's
  head `63a9167` passed **CI** and **Android runtime (emulator)** on 2026-11-24. The audit did not re-run the Kotlin
  tests: the sandbox network refuses the Gradle distribution and the Maven repositories (`REUSE_VALIDATION_PLAN.md`
  §5).

## 6. Things not found

- **Recipes:** no recipe model or runtime. "Recipe" appears only in policy text and tag-import notes.
- **Alarms:** there are alarm *definitions* (a boolean source variable, a message and a severity) and operations to
  edit them. No alarm evaluation, history or acknowledgement runtime was found in `core` or `runtime`.
- **Trends and history:** not found.
- **Modbus:** not found in the engine or `core`. HMIStudio's `DriverKind::ModbusTcp` has no implementation anywhere.
- **S7CommPlus:** not implemented. R-004 records it as REJECTED_BY_DESIGN, with an INCONCLUSIVE D3.1 addendum.
  Nothing from third-party S7CommPlus or AGLink material may be transferred.
- **Customer projects or credentials:** none in the tracked files.
