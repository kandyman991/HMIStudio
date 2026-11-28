# Runtime Packages — REUSE-04 P1 Specification, O-8 Signing & B5-07 Raster Gate

This document records the engineering specification, reproducible golden fixtures, O-8 signing investigation, B5-07 bounded raster consumer gate, and verified access gaps for **REUSE-04 P1** (format-1/2 `.hmipkg` runtime packages from frozen canonical schema v2).

## 1. Pinned Private Runtime Baseline & Verified Access Gaps

- **Pinned private runtime baseline (from repository records):**
  - `docs/reuse/AUDIT_SUMMARY.md`, `docs/reuse/DEPLOYMENT_AND_PUBLISHING.md`, and `docs/reuse/REUSE_VALIDATION_PLAN.md` pin HMI Forge commit `92a612d104693800d7119ee4dc0114636b19002f` (defining format-1/2 `RuntimePackages.kt`, Tink Ed25519 signing from `RT-01C.3a` PR #48, and `PackageAdmission`) and sanitized fixture merge `03a5887d1d763c936e5377741ee472680b700085` (PR #57).
- **Verified Gap 1 — Private Kotlin repository unavailable in this environment:**
  - `gh repo view kandyman991/hmiforge` returns `GraphQL: Resource not accessible by integration (repository)` and no private checkout exists locally.
  - Consequently, the actual private Kotlin `RuntimePackages.build` serializer and `PackageAdmission` verifier could not be executed in this workspace.
  - No private source, customer projects, credentials, or real signing keys have been transferred into this public repository or CI.
- **Verified Gap 2 — Android API 26 test target unavailable in this environment:**
  - `/opt/android-sdk` provides `platforms/android-34..36`, `build-tools/33.0.2..35.0.0`, `cmdline-tools`, and `platform-tools/adb`, but has no Android `emulator` binary, no `android-26` platform/system image, no attached Android device, and no private `runtime-android` Gradle project.
- **P1 work validated independently:**
  - Byte-for-byte differential verification against the public JDK 21 golden generator (`RuntimePackageFixture.java`), independent transport decoding via `hmi-legacy-import::package::decode_runtime_package`, deterministic canonical schema v2 compilation (`crates/runtime-package`), official RFC 8032 Ed25519 vector verification, and full B5-07 raster consumer gating across `legacy-import`, `engineering-core`, `src-tauri`, and `runtime-package`.

## 2. Reproducible Synthetic Golden Fixtures

The public synthetic JDK 21 fixture generator lives at `crates/legacy-import/tests/fixtures/RuntimePackageFixture.java` (verified against OpenJDK `21.0.13`).

### Reproduction command
```bash
cd crates/legacy-import/tests/fixtures
TZ=UTC java RuntimePackageFixture.java
```

### Verified byte-for-byte outputs
| Fixture | Format | Size | `packageId` | Archive SHA-256 |
| --- | --- | ---: | --- | --- |
| `runtime-unsigned-java.synthetic` | 1 (unsigned) | 935 B | `0dacc3d7e07de017ae6d963ff6414d5f4c0688457c5cbcce7e58b201bc9cb630` | `69c8a76ecf25fd9e230db772436d78391db321e07488327c6fbf0afa859281bf` |
| `runtime-signed-java.synthetic` | 2 (signed) | 1144 B | `cfcef06c70d6fef87bcca84a86e0853f62a671d8285c808cd0584740839ae9f4` | `d10ca7a25c967be500f4c74cb2a4e310ac08aa786f4589bd81c25872483ce2a5` |

### Rust differential compatibility (`crates/runtime-package/src/package_golden_tests.rs`)
- Matches `packageId`, 4-space pretty `manifest.json`, `project.json`, `runtime.json`, `signature.json`, and exact ZIP entry order (`manifest.json`, `project.json`, `runtime.json`, optional `signature.json`) **byte-for-byte** against both JDK 21 golden fixtures.
- Emits ordinary single-disk ZIPs with fixed DOS timestamp `1980-01-01 00:00:00`, no ZIP64, no comments, and no extra fields; repeated builds are bit-for-bit identical in both `Deflated` and `Stored` modes.
- Every built package is self-verified through `hmi_legacy_import::package::decode_runtime_package`.
- **Remaining validation once private runtime access is available:** Generate synthetic format-2 golden packages directly with the pinned private Kotlin `RuntimePackages.build` serializer and confirm live `PackageAdmission` admission of Rust-compiled and Rust-signed packages on JVM and Android API 26.

## 3. Rust Format-2 Compiler (`crates/runtime-package`)

- **Frozen canonical schema v2 preserved:** `crates/project-model/` is unchanged (`schema/hmi-project.v2.schema.json` SHA-256 `b7d49729750bea7a598afa89f64fa57598d5603a88d0fba0dd664abd75ab2738`).
- **Development/simulation vs. production modes (D-004):**
  - `CompileMode::DevelopmentSimulation`: compiles `SIMULATED` packages without production approval or external connections.
  - `CompileMode::ProductionLive`: unconditionally refuses quarantined legacy-import projects (`hmistudio.importQuarantine`, `hmiforge.idMap`, `hmiforge.standardCatalog`, or `legacy-project-*` IDs) and requires at least one valid external source mapping (`LiveMappingRequired`).
  - Quarantined projects are also refused by default in `DevelopmentSimulation` unless `allow_quarantined_simulation: true` is explicitly set.
- **Supported canonical v2 subset (exact minimum legacy `schemaVersion` v1–v10):**
  - Metadata, screens, `screenOrder` (v5), absolute canvases (v1), `Row`/`Column` stacks (v2), equal-fraction root `Grid` (v3).
  - Components: `text` (`label`), `button`, `rectangle`, `ellipse`, `numericDisplay`, `indicator`, `gauge`, `progressBar`, `image`/`svgImage`, `switch`/`slider`/`numericInput`/`textInput`/`dropDown` (v10), v2 `line`/`arrow`/`polygon`, reusable `ComponentDefinition` + `FACEPLATE` (v6), passive industrial symbols (`motor`, `valve`, `conveyor`, `pump`, `tank`, `pipe`, `thermalIndicator`).
  - Tags (6 scalar types: `bool`, `int32`, `int64`, `float32`, `float64`, `string`), engineering units, S7 / OPC UA / loopback connections, guarded v9 write policies (`writeConfirmed: true`), v4 contract-1 expressions, v5/v6/v10 ordered actions, v7 single theme + verified PNG/JPEG/SVG assets, v8 security policy + screen/node permissions, v10 integer enumerations.
- **Explicit fail-closed refusal matrix:**
  - `alarms`, `recipes`, `structDataTypes`, `breakpoints`, `deviceProfiles`, `namedStyles`, `multiLocaleLocalization`, `localizedTextKey`, `themeColorToken`, `multipleThemes`, `popupScreen`, `screenLifecycleEvents`, `boxLayout`, `scrollLayout`, `wrappedRowLayout`, `wrappedColumnLayout`, `nonDefaultFlowAlignment`, `nestedGridLayout`, `asymmetricGridGaps`, `nonUniformGridTracks`, `gridCellSpan`, `asymmetricPadding`, `responsiveLayout`, `zIndex`, `aspectRatio`, `clip`, `collapsedVisibility`, `dataType` (`int8`/`int16`/`uint*`/`dateTime`/`array`/`struct`), `driverKind` (`modbusTcp`/`custom`), `connectionCredentialRef`, `writeOnlyTag`, `unpolicyWritableMapping`, `unconfirmedWritableMapping`, `containerComponent`, `bindingMemberPath`, `bindingTransform`, `writeOnlyBinding`, `valueParameterBinding`, `eventConfirmationPrompt`, `openPopupAction`, `acknowledgeAlarmsAction`, `recipeAction`, `setLocaleAction`, `missingResource`, `resourceIntegrityMismatch`, `invalidSvgResource`, and all B5-07 raster errors.

## 4. O-8 Signing Compatibility Investigation & Recommendation

### Evidence from repository records
- `docs/reuse/DEPLOYMENT_AND_PUBLISHING.md`, `docs/reuse/COMPATIBILITY_MATRIX.md`, `docs/reuse/HMIFORGE_INVENTORY.md`, and `docs/reuse/REUSE_VALIDATION_PLAN.md` document that HMI Forge `RT-01C.3a` (PR #48, merged before `92a612d`) already uses **RFC 8032 Ed25519** via **Google Tink 1.23.0** (`tink` on JVM and `tink-android` on Android, Apache-2.0).
- `runtime-android` executes 10 instrumented tests on **Android API 26 and API 34 emulators**, including shared RFC 8032 Ed25519 vectors and a pinned interop signature (`runtime/deploy/Ed25519Vectors`). Android JCA only added native Ed25519 in API 33, so Tink's pure-Java Ed25519 verifier is what guarantees Android API 26 compatibility.
- Format-2 `signature.json` carries `{"algorithm":"Ed25519","keyId":"<publisherKeyId>","value":"<standard Base64 64-byte signature>"}`.

### Recommendation
- **Adopt RFC 8032 Ed25519** in Rust using **`ed25519-dalek` `=2.2.0`** (`default-features = false, features = ["std"]`, BSD-3-Clause).
- Verified in `crates/runtime-package/src/signing_o8_tests.rs` against official RFC 8032 Section 7.1 test vectors (TV1, TV2, TV3) plus end-to-end signed format-2 package creation, archive cryptographic verification, and tamper refusal (wrong key, wrong `keyId`, bit-flipped signature, unsupported algorithm, redacted `Debug` output).

### Signed payload and remaining validation

Independent source review at pinned HMI Forge revision `92a612d104693800d7119ee4dc0114636b19002f` confirmed that `RuntimePackages.build` signs the exact final manifest bytes, with `packageId` populated. `PackageAdmission` verifies those original bytes. Rust uses the same signing input. The blank ID is used to compute the package identity.

Generate a golden fixture with the pinned private Kotlin serializer and verify Rust-signed packages on the Kotlin JVM and Android API 26 runtimes. Publisher key store migration or re-pairing is a REUSE-05 decision.

## 5. B5-07 Bounded Full Raster Decoding Closure

B5-07 required full bounded pixel decoding for imported PNG and JPEG images before rendering, extraction for use, or runtime consumption, enforcing dimension, pixel-count, and decoded-byte limits while rejecting malformed compressed data and keeping imported bytes inert until each consumer is gated.

### Shared consumer gate (`crates/legacy-import/src/raster.rs`)
- Pure-Rust decoders with `unsafe_code = "forbid"`:
  - PNG: `png = "=0.18.1"` (MIT OR Apache-2.0) with `png::Limits { bytes: limits.max_decoded_bytes }` enforced prior to decompression, full `next_frame` + terminal `finish()` verification, normalized to RGBA8.
  - JPEG: `jpeg-decoder = "=0.3.2"` (`default-features = false`, zero transitive dependencies, MIT OR Apache-2.0), strict envelope scan (`jpeg_dimensions`) + pre-decode geometry bounds + full Huffman/DQT/IDCT decompression, normalized to RGBA8.
- Default limits (`RasterLimits::DEFAULT`):
  - `max_input_bytes`: 4 MiB (`4_194_304` B)
  - `max_width` / `max_height`: `4096` px
  - `max_pixels`: `16_000_000`
  - `max_decoded_bytes`: 64 MiB (`67_108_864` B RGBA8)

### Per-consumer enforcement matrix
| Consumer boundary | Enforcement mechanism | Inertia & negative test coverage |
| --- | --- | --- |
| **Legacy import & review storage** (`crates/legacy-import`, `crates/engineering-core/src/legacy_quarantine.rs`) | Structural envelope verification supports inert evidence retention and preserves historical `0.1.0`–`0.1.5` / envelope v2–v4 replay checksums. `QuarantinedImport::payload.resources` is strictly private (no raw-byte getter). Consumers MUST call `QuarantinedImport::decode_verified_raster` / `decode_verified_raster_with_limits` (verifies SHA-256, declared `legacyWidthPx`×`legacyHeightPx`, and full bounded decoding). | `crates/legacy-import/tests/raster.rs` (3 suites: valid PNG/JPEG, dimension/pixel/decoded-byte/input/mismatch limits, corrupt zlib PNG, truncated PNG rows, illegal Huffman JPEG, corrupt DHT JPEG, invalid DQT JPEG, missing DHT JPEG, invalid scan marker, missing EOI, trailing bytes). `crates/engineering-core/tests/legacy_quarantine.rs::b5_07_raster_consumer_gate_enforces_full_decode_and_bounds`. |
| **Desktop native host & WebView IPC** (`src-tauri/src/persistence_host.rs`, `src/`) | No Tauri IPC command exposes raw imported raster bytes; `ProjectSnapshot` carries metadata + `assets/<sha256>.<ext>` paths only; UI renders no raw imported rasters. | `src-tauri/src/persistence_host.rs::native_review_keeps_imported_rasters_inert_and_b5_07_gated`. |
| **Format-2 runtime package compiler** (`crates/runtime-package/src/compiler.rs`) | Every `image/png` and `image/jpeg` resource MUST match its SHA-256 and pass `decode_bounded_raster_checked(...)` before Base64 emission into `project.json`. SVG resources must pass strict restricted SVG validation. | `crates/runtime-package/tests/compiler.rs::b5_07_compiler_refuses_corrupt_oversized_and_unverified_rasters_and_active_svg`. |

## 6. Dependency, License & CI Security Notes

- **Added dependencies (all pure Rust, permissive licenses, committed lockfiles):**
  - `crates/legacy-import`: `png = "=0.18.1"` (MIT OR Apache-2.0) + `fdeflate`/`miniz_oxide`/`adler2`/`simd-adler32` (MIT/Apache-2.0/Zlib), `jpeg-decoder = "=0.3.2"` (MIT OR Apache-2.0, 0 transitive deps).
  - `crates/runtime-package`: `ed25519-dalek = "=2.2.0"` (BSD-3-Clause) + `curve25519-dalek`/`ed25519`/`signature`/`subtle`/`zeroize` (BSD-3-Clause/MIT/Apache-2.0).
  - `src-tauri/Cargo.lock`: adds `jpeg-decoder v0.3.2` only (`png` was already in the Tauri dependency graph). Under CI-02, updating `src-tauri/Cargo.lock` triggers full desktop + MSI/DEB installer verification.
- **Advisory gate:** `.github/workflows/ci.yml` enforces `cargo audit --version 0.23.0` with zero ignored advisories on `crates/legacy-import/Cargo.lock`, `crates/engineering-core/Cargo.lock`, `crates/runtime-package/Cargo.lock`, and `src-tauri/Cargo.lock`.


## Independent P1 review corrections

The public package writers require a `CompiledProject` and refuse runtime settings that differ from the mode checked by the compiler. Both paths therefore require compiler resource validation before serialization. The raw ZIP serializers are private and exercised directly by the transport golden tests. The Ed25519 verifier uses the original entries returned by the bounded transport decoder. Caller-provided raster limits can tighten the consumer bounds.

## 7. Post-Review CI Verification, Archive vs. Entry Equality & Remaining Blockers (`2bcc3c1`)

- **Hosted CI verified on exact review-fix head `2bcc3c1eb42848e3bd548952c6c9be483b88b0aa` (18/18 checks passed):**
  - [Desktop CI run 46106604087](https://github.com/kandyman991/HMIStudio/actions/runs/46106604087): **17/17 PASS** — `changes`, `frontend`, all 6 crates on `ubuntu-24.04` and `windows-latest` (`project-model`, `engineering-core`, `legacy-import`, `runtime-package`, `hmi-engine`, `tag-engine`), zero-ignored `cargo audit` on all audited lockfiles, `desktop (ubuntu-24.04, deb)` (`5m43s`, artifact `hmi-studio-ubuntu-24.04`, 6,118,503 B), `desktop (windows-latest, msi)` (`11m53s`, artifact `hmi-studio-windows-latest`, 5,640,961 B), and `Required CI`.
  - [Engineering handoff snapshot run 46106604091](https://github.com/kandyman991/HMIStudio/actions/runs/46106604091): **1/1 PASS**.
- **Archive-level vs. entry-level byte equality (JDK 21 `ZipOutputStream` vs. Rust `zip::ZipWriter`):**
  - **Entry-level equality: 100% byte-for-byte identical.** Against `RuntimePackageFixture.java` (`runtime-unsigned-java.synthetic` and `runtime-signed-java.synthetic`), Rust emits byte-identical `manifest.json`, `project.json`, `runtime.json`, `signature.json`, `packageId`, CRC-32 checksums, uncompressed sizes, entry order, and fixed DOS timestamp `(1980, 1, 1, 0, 0, 0)`.
  - **Archive-level equality: differs at the ZIP framing layer (expected and explained).** Inspecting the raw ZIP headers shows two exact differences:
    1. *General-purpose bit flags & data descriptors:* JDK 21 `java.util.zip.ZipOutputStream` writes streaming entries with `flag_bits = 0x0808` (bit 3 `0x0008` trailing 16-byte data descriptor per entry + bit 11 `0x0800` UTF-8 flag), leaving local header CRC/sizes zeroed. Rust `zip::ZipWriter` over seekable `Cursor<Vec<u8>>` writes `flag_bits = 0x0000` (pure-ASCII entry names) with exact CRC/sizes in the local file header and no trailing data descriptors.
    2. *DEFLATE encoder implementation:* JDK 21 uses native `zlib` (`java.util.zip.Deflater`) while Rust uses `flate2` + `zlib-rs`; both produce valid RFC 1951 streams with identical uncompressed bytes and CRCs, though compressed bitstreams may differ across encoders.
  - Both formats are bit-for-bit deterministic across repeated runs within the same serializer.
- **OpenJDK 21 (`21.0.13`) cross-reader & JCA Ed25519 check (non-substitute for private Kotlin Tink admission):**
  - Compiled `crates/project-model/fixtures/valid/minimal.hmi.json` with `compile_project` + `build_signed_package` using deterministic RFC 8032 TV1 test seed (`keyId = "interop-test-key"`, `sequence = 1`, `createdAtMs = 1700000000000`, `createdBy = "hmi-studio-interop-check"`, `Deflated`):
    - `packageId`: `bb2ed9278a7e4a7265a108059d981694e67dd113b8315eeef43b03981100a381`
    - Archive size / SHA-256: `1315 B` / `c453ae8bdccd2cb28b2a38082873399883ab599c779d92453676ebc1f3469171`
    - `project.json` SHA-256: `2f38522fa44107037c38a5d49e9381c1ba4ba3d4d5663f1b46da86c9e5b9da9a`
    - `runtime.json` SHA-256: `6febe16a44ea2f76835d52722560307123be3f19ca8935a778c808fc343f0ab0`
  - Verified with OpenJDK 21 `java.util.zip.ZipFile`, `java.util.zip.ZipInputStream`, `MessageDigest("SHA-256")`, and `Signature.getInstance("Ed25519")`: entry lists and bytes match identically across streaming and random-access ZIP readers, `packageId` recomputes identically, and Ed25519 signature over the finalized `manifest.json` bytes verifies (`true`).
- **Exact blockers remaining before P1 merge (re-verified 2026-11-28):**
  1. **Private HMI Forge checkout (`92a612d104693800d7119ee4dc0114636b19002f`) unavailable in this VM:** GitHub token is scoped to `kandyman991/HMIStudio` only (`gh repo view kandyman991/hmiforge` and `kandyman991/HMIForge` return `GraphQL: Resource not accessible by integration (repository)`), and no local private checkout exists. Executing the actual private Kotlin `RuntimePackages.build` serializer and `PackageAdmission` verifier (JVM and Android) remains blocked on access to an authorized private checkout.
  2. **Android API 26 emulator unavailable in this VM:** `/opt/android-sdk` has only `platforms/android-34..36` and `build-tools/33.0.2..35.0.0`, with no `emulator/` binary, no `system-images/`, no `platforms/android-26`, and `adb devices` empty.

