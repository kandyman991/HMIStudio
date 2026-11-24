# REUSE-01 — Test inventory and reuse validation plan

Status: **plan**. Only §5 records checks that were actually run in REUSE-01.

## 1. HMI Forge test inventory (`92a612d`)

The counting method is in `HMIFORGE_INVENTORY.md` §1.

| Module | Tests | Kind | Runs in HMI Forge CI |
| --- | --- | --- | --- |
| engine | 37 | unit, contract, OPC UA against the test server, S7 against `snap7-server`, process, pulse | `ci.yml` |
| engine-client | 13 | end-to-end against the real engine binary | `ci.yml` |
| core | 245 | unit (commonTest and jvmTest) | `ci.yml` |
| hmi-ui | 2 | JVM UI | `ci.yml` |
| runtime | 44 | JVM: packages, live, maintenance, signed trust, Ed25519 vectors, deployment security | `ci.yml` |
| runtime-desktop | 11 | JVM | `ci.yml` |
| runtime-android | 10 | instrumented, on API 26 and 34 emulators | `android-runtime.yml` |
| storage-jvm | 27 | JVM | `ci.yml` |
| tag-import-jvm | 5 | JVM | `ci.yml` |
| studio-desktop | 143 | JVM, Compose UI tests | `ci.yml` |
| Installers | — | install, `--self-test` and uninstall jobs | `runtime-installers.yml` (manual or tag for Windows and ARM64) |

**Shared fixtures that cross the language boundary:**

- `engine/protocol/messages.json` and `engine/protocol/write-guard-cases.json`, checked in Rust and Kotlin;
- Ed25519 vectors (RFC 8032 plus a pinned interop signature), checked in desktop and Android tests.

**Gaps:**

- No physical device, panel, ARM hardware or physical PLC test exists (decision 14 is open).
- No test checks that Studio Preview and the runtime render the same way: they share one renderer, so it was never
  needed. HMIStudio needs such a test.
- No licence or notice test exists.

## 2. HMIStudio test inventory (`264ace7`, plus PR #7)

| Crate | Tests | State |
| --- | --- | --- |
| project-model | 76 (per RUST-03A records) | merged |
| engineering-core | 58 (per RUST-03B records) | merged |
| tag-engine | 44 | PR #7, CI passed on `de9e207`, not merged |
| Desktop shell | `tests/workspace.test.mjs`, Rust host tests | merged; RUST-02C in PR #8 |

## 3. Validation per reuse milestone

| Milestone | Automated checks | Where |
| --- | --- | --- |
| REUSE-02 engine | engine's 37 tests on hosted Ubuntu and Windows; Rust client passes `messages.json` round-trip and rejected-request fixtures; `EngineDriver` against loopback and the OPC UA and S7 test servers (read, demand, arming, write, pulse, refusal codes, restart supervision); type-conversion table §4 of `RUST_ENGINE_INTEGRATION.md` as table-driven tests; `cargo audit` and a licence report; fixture SHA-256 manifest check | HMIStudio CI (hosted) |
| REUSE-03 import | per-version fixtures v1–v10 and negative fixtures; determinism (double import, byte-identical); leaf-accounting test; RUST-03A validation of every output; model v1→v2 migration tests | HMIStudio CI |
| REUSE-04 runtime | Rust package writer vs Kotlin `RuntimePackages.read`: golden packages byte-identical; signature verifies with Kotlin `PackageAdmission`; compiler refusal tests; semantic layout parity (bounds within tolerance) on JVM and Android emulator; behaviour scripts | both repositories |
| REUSE-05 deployment | Rust client pairs with and deploys to the desktop listener in CI (loopback network); replay, clock-window, pinned certificate mismatch, downgrade and unpaired-publisher refusals; key-store file compatibility (O-8) | runtime repository CI plus HMIStudio |
| REUSE-06 acceptance | everything above on final heads; installers; manual acceptance per D-008 (RUST-05/07/08); at least one physical Android device per ABI and one Linux ARM64 panel for D-033 (INVESTIGATE row R10) | owner |

## 4. CI placement rules

- HMIStudio public PRs run on GitHub-hosted runners only (D-006, `CI_RUNNER_POLICY.md`).
- Android emulators and self-hosted runners stay in the runtime repository, under its owner's policy.
- Every claim of "passed" quotes the exact head SHA and run.

## 5. Checks run in REUSE-01

| Check | Command | Result |
| --- | --- | --- |
| Engine tests | `CARGO_TARGET_DIR=/tmp/hf-target cargo test --locked --features test-server` in a read-only clone at `92a612d`, cargo 1.97.0, Linux x64 | **37 passed, 0 failed** (OPC UA suite 51 s) |
| Engine lints | `cargo clippy --locked --all-targets --features test-server` | no warnings or errors |
| Engine advisories | `cargo audit` (0.23.0, database fetched 2026-11-24) | 2 vulnerabilities (RUSTSEC-2023-0071 `rsa`, RUSTSEC-2026-0592 `url`), 12 warnings (`heapless` test-only, `chrono`, `unsafe-libyaml`, `atomic-polyfill`) |
| Licence census | `cargo tree --locked -e normal --format "{p} {l}"` | see `LICENSING_AND_PROVENANCE.md` §1 |
| Kotlin tests | not run | Gradle 9.5.0 (`services.gradle.org`), Maven Central and Google Maven are refused by this sandbox's network allowlist (HTTP 403 from the proxy). The latest HMI Forge CI evidence is PR #56 head `63a9167`: **CI** and **Android runtime (emulator)** succeeded on 2026-11-24. `main` `92a612d` differs from the merged `1c8da07` only in `handoff/state.json` |
| PR, branch and CI state | GitHub REST API | `HMIFORGE_INVENTORY.md` §5 |
| HMIStudio docs build | none needed (Markdown only) | — |

Nothing from these runs was written into HMI Forge.
