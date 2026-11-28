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

## 8. Verified Kotlin Golden Comparison, JVM Admission & Android API 26 Instrumentation (2026-11-28)

Both environment blockers recorded in §7 were resolved in an authorized local validation environment with the pinned private HMI Forge checkout (`92a612d104693800d7119ee4dc0114636b19002f`), OpenJDK `21.0.13.1`, Gradle `9.5.0`, and the `hmistudio-api-26` KVM-accelerated Android emulator (`ro.build.version.sdk = 26`, Android `8.0.0`, `x86_64`). All private Kotlin source and private-side test harnesses remain strictly outside this public repository; only the original public fixture emitter (`crates/runtime-package/examples/emit_interop_fixtures.rs`), Rust corrections/regression tests, and sanitized synthetic evidence are recorded here.

### Exact tested revisions
- **Rust predecessor baseline:** `bea6ec36043c88b56db725e442187155bc4641d1` (PR #40 branch `feat/reuse-04-p1-runtime-package`; corrections applied on successor commit).
- **Pinned private Kotlin runtime baseline:** `92a612d104693800d7119ee4dc0114636b19002f` (`RuntimePackages.build`, `RuntimePackages.read`, `HmiProjectJson`, `HmiProjectValidator`, `PackageAdmission`, `PackageStore`, `TrustList`, Google Tink `Ed25519Sign`/`Ed25519Verify` on JVM and `tink-android` on Android API 26).
- **Frozen canonical schema v2 preserved:** `crates/project-model/` unchanged (`schema/hmi-project.v2.schema.json` SHA-256 `b7d49729750bea7a598afa89f64fa57598d5603a88d0fba0dd664abd75ab2738`).

### Discrepancies discovered on `bea6ec3` and corrections applied
1. **Confirmed semantic defect (`crates/runtime-package/src/compiler.rs`):**
   - *Finding:* Rust emitted `"confirm": true` on `SETPOINT` and `MAINTAINED` write commands whenever `TagWritePolicy.confirmation_required == true`. Running the actual Kotlin `RuntimePackages.read` -> `HmiProjectValidator.validate` rejected the compiled `industrial_live_v10` package with `The project has 1 error(s), first: Setpoints are always confirmed` (`WriteRules.kt` reserves `WriteCommand.confirm` strictly for `MOMENTARY` commands).
   - *Correction:* Emit `"confirm": true` only for `CommandKind::Momentary` when `confirmation_required == true`; omit `"confirm"` for `CommandKind::Maintained` and `CommandKind::Setpoint` when `confirmation_required == true`, and fail closed (`CompileError::UnsupportedFeature("unconfirmedSetpointOrMaintained")`) if `confirmation_required == false`. Added regression test `write_command_confirm_matches_kotlin_write_rules` in `crates/runtime-package/tests/compiler.rs`.
2. **Confirmed wire formatting & metadata parity defect (`crates/runtime-package/src/package.rs`):**
   - *Finding:* Rust serialized `runtime.json` and `signature.json` as compact JSON (77 B / 197 B) instead of 4-space pretty-printed JSON (102 B / 216 B) matching Kotlin `RuntimePackages.build` (`Json { prettyPrint = true; encodeDefaults = false; ignoreUnknownKeys = false }`), and did not trim `version` or enforce Kotlin's 40-character maximum length.
   - *Correction:* Generalized `format_json_4space` in `crates/runtime-package/src/package.rs` for `manifest.json`, `runtime.json`, and `signature.json`; trimmed `version` and enforced `1..=40` UTF-16 code units. Updated `crates/runtime-package/src/package_golden_tests.rs` to verify exact 4-space Kotlin wire formatting while preserving historical `crates/legacy-import/tests/fixtures/runtime-*-java.synthetic` replay fixtures untouched.
3. **Harmless / expected differences distinguished from semantic defects:**
   - *`project.json` serialization:* Rust `compile_project` emits compact JSON omitting default fields (`501 B` for `minimal`, `2,652 B` for `industrial_live_v10`), whereas Kotlin `HmiProjectJson.encode` emits 4-space pretty-printed JSON with `encodeDefaults = true` (`1,931 B` / `5,343 B`). Deserialized `HmiProject` semantics are **100% identical** (`kotlinReloaded.project == rustDecoded.project`), `HmiProjectValidator.validate` reports **0 errors**, and because `manifest.json` hashes the actual `project.json` bytes, `runtime.json`, `manifest.json` 4-space serialization, `packageId` formula, ZIP entry order, and Tink `Ed25519Sign` vs. Rust `ed25519-dalek` `signature.json` bytes are **100% byte-for-byte identical**.
   - *ZIP archive framing:* Rust `zip::ZipWriter` (`flag_bits = 0x0000`, header CRC/sizes, `zlib-rs`) vs. JDK/Android `ZipOutputStream` (`flag_bits = 0x0808`, trailing data descriptors, native `zlib`). Entry names, order, and unpacked entry bytes match identically across both serializers and readers.

### Synthetic interop fixtures (`cargo run --locked --example emit_interop_fixtures -- <dir>`)
Deterministic public RFC 8032 test keys (non-production):
- **TV1 (trusted publisher):** `keyId = 21fe31dfa154a261626bf854046fd2271b7bed4b6abe45aa58877ef47f9721b9` (`publicKeyHex = d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a`)
- **TV2 (untrusted publisher):** `keyId = 39f713d0a644253f04529421b9f51b9b08979d08295959c4f3990ee617f5139f` (`publicKeyHex = 3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c`)

| Fixture | Format | Size | `packageId` | Archive SHA-256 |
| --- | --- | ---: | --- | --- |
| `minimal_v2_unsigned.hmipkg` | 1 | 1035 B | `1cefbb4db5f5870227a2e3cea74ad0d899a75c2ae4de25fd31a57fbf1e423f7c` | `607331a9836008db0bf976531021e73d6d15f795b5635fe60b384faea5a89680` |
| `minimal_v2_signed_seq1.hmipkg` | 2 (TV1, seq 1) | 1394 B | `932b10e150a30cde8908af2c5f34659eb0131ffe66eabcfdfa2a0dba691de9b5` | `0adfae75e0ef3f0b911b5b396bc1a48c28629697d25adda48332fd2cd1d0b26a` |
| `minimal_v2_signed_seq2.hmipkg` | 2 (TV1, seq 2) | 1393 B | `4d29b0e31c491d1094faf69ae2018add2d011e42442153281c337fa0d7519c97` | `cc0153a6ea868822934984e17206c571916665fdb5993a598dcbe057064bd93f` |
| `minimal_v2_signed_untrusted.hmipkg` | 2 (TV2, seq 3) | 1395 B | `9d94cbeeaca184f0360e68d0cae168c55d0513424cf79cccccb0d89bc0382598` | `172107b6cec7069bf08bdc08474dad1646feb5f16e5a672c85b91d602fd7bac2` |
| `industrial_live_v10_unsigned.hmipkg` | 1 | 2057 B | `4373c0183f11fcf1ab6f30f2f83cf7eb9c6a8f6f5adb637784a5f7b27f09532c` | `6bc4c041c3109e53ea97d384c4384103dc47813a3cfef0ea4ca654f5cf84e8a1` |
| `industrial_live_v10_signed_seq1.hmipkg` | 2 (TV1, seq 1) | 2411 B | `f8f069096a5919cc83558c4db5e11c257c9dbf464b467a70b776bd2dafa1ddc9` | `7f388dfd4e9f647ed92985b1e098383838da3a344b0fe6a05194d0ebf4db93d9` |
| `industrial_live_v10_signed_seq2.hmipkg` | 2 (TV1, seq 2) | 2416 B | `3f20deccec4c723bd25e9c3e81ee50314254aa1bbb9c22c5066adbd320a94c16` | `e790f1d734d0fd96ef0a23b69da3a141409524761e018e043ec356cdc8ea182a` |

### Execution evidence & test counts
- **Kotlin JVM golden & `PackageAdmission` (`:runtime:jvmTest`, private harness `Reuse04P1InteropTest` against pinned `92a612d` + Google Tink):** **3/3 PASS** (`0` failures, `0` skipped):
  - `goldenDifferentialComparisonAgainstKotlinRuntimePackagesBuild`: PASS across format-1 & format-2 `minimal` (v2, `SIMULATED`) and `industrial_live_v10` (v10, `LIVE`, S7 connection, `TagWritePolicy`, `SecurityPolicy`, `EnumerationDefinition`, B5-07 verified PNG/JPEG/SVG assets). Confirmed byte-for-byte `runtime.json`, 4-space `manifest.json`, `packageId` formula, ZIP entry order, Tink `Ed25519Sign` vs. Rust `signature.json`, and 100% identical deserialized `HmiProject` semantics.
  - `jvmPackageAdmissionAcceptsValidRustSignedPackagesAndEnforcesSequenceAndPolicy`: PASS (unsigned `FILE`/`NETWORK` policy, pre-pairing refusal, TV1 pairing, `minimal` & `industrial_live_v10` `seq=1` & `seq=2` `Trusted` admission, `NETWORK` replay/downgrade refusal, TV2 untrusted key refusal on `FILE` and `NETWORK`).
  - `jvmPackageAdmissionRejectsAllTamperedAndMalformedRustPackages`: PASS (modified `project.json`, modified `runtime.json`, modified `manifest.json` with recomputed `packageId`, bit-flipped Ed25519 signature, mismatched `signature.json` `keyId`, legacy `ECDSA-P256-SHA256`, corrupt ZIP, empty ZIP, missing `signature.json`, path traversal entry).
- **Android API 26 instrumentation (`:runtime-android:connectedDebugAndroidTest` on `hmistudio-api-26` AVD, verified `ro.build.version.sdk = 26`, Android `8.0.0`, `x86_64`, `Android/sdk_phone_x86_64/generic_x86_64:8.0.0/OSR1.180418.004/4931640:userdebug/test-keys`, Tink Android pure-Java Ed25519):** **5/5 PASS** (`0` failures, `0` errors, `0` skipped, `1.51s`):
  - `Reuse04P1AndroidAdmissionTest`: **3/3 PASS** (`verifyRunningOnAndroidApi26`, `androidApi26AdmitsValidRustSignedPackagesAndEnforcesSequenceAndPolicy`, `androidApi26RejectsAllTamperedAndMalformedRustPackages`).
  - `SignedPackagesOnAndroidTest`: **2/2 PASS** (`rfc8032VectorsAndTheDesktopInteropSignatureMatch`, `aSignedPackageIsAdmittedOnlyFromAPairedPublisherAndUnchanged`).
- **Rust regression checks:**
  - `crates/runtime-package`: `cargo fmt --check` PASS, `cargo clippy --locked --all-targets -- -D warnings` PASS, `cargo test --locked` **10/10 PASS** (5 unit + 5 integration).
  - `crates/legacy-import`: `tests/package.rs` **11/11 PASS**, `tests/raster.rs` **4/4 PASS**.
  - `crates/engineering-core`: `tests/legacy_quarantine.rs` **28/28 PASS**.

### Remaining limitations
- Production publisher key store, passphrase KDF, hardware-backed keystore, and device pairing remain gated on REUSE-05 and the owner O-8 decision.
- Format-3 decoding, Compose layout parity, and manual RUST-05 desktop/device acceptance belong to REUSE-04 P2.
- PR #40 remains draft with `RUST-05` reserved pending exact-head hosted CI on the corrected commit and final independent pre-merge review.

## 9. Independent Review Findings F01 & F02 Resolution (2026-11-28)

Independent review of PR #40 predecessor `f506dfdcbeec681816e1cd4dd28a06ebcd42e6eb` (18/18 hosted CI pass: Desktop CI `46129480588`, Engineering handoff snapshot `46129480577`) identified two compiler-boundary write-policy compatibility gaps between canonical schema v2 (`crates/project-model/src/validation/v2.rs`) and the pinned Kotlin runtime (`92a612d104693800d7119ee4dc0114636b19002f`, `WriteRules.kt` / `WriteCommands.kt`).

### Fail-closed compiler rules implemented (`crates/runtime-package/src/compiler.rs`)
Frozen canonical schema v2 (`crates/project-model/schema/hmi-project.v2.schema.json`, SHA-256 `b7d49729750bea7a598afa89f64fa57598d5603a88d0fba0dd664abd75ab2738`) was **not modified**, and no engineering limits or pulse durations are silently clamped, rounded, or adjusted:

1. **F01 — Momentary pulse duration bounds & setpoint limit requirements:**
   - `CommandKind::Momentary`: enforces empty write limits (`unexpectedWriteLimits`), requires `pulse_ms` (`missingMomentaryPulse`), and enforces inclusive bounds `100..=5000` ms (`MIN_MOMENTARY_PULSE_MS = 100`, `MAX_MOMENTARY_PULSE_MS = 5_000`; refuses `< 100` or `> 5000` with `CompileError::UnsupportedFeature("momentaryPulseOutOfRange")`).
   - `CommandKind::Maintained`: enforces `confirmation_required == true` (`unconfirmedSetpointOrMaintained`), omits `"confirm"`, and rejects `pulse_ms` (`unexpectedPulseDuration`) and write limits (`unexpectedWriteLimits`).
   - `CommandKind::Setpoint`: enforces `confirmation_required == true` (`unconfirmedSetpointOrMaintained`), omits `"confirm"`, rejects `pulse_ms` (`unexpectedPulseDuration`), requires **both** `limits.min` and `limits.max` (`CompileError::UnsupportedFeature("missingSetpointLimits")`), enforces strict ordering `min < max` (`CompileError::UnsupportedFeature("invalidSetpointRange")`), and requires optional `step > 0` (`CompileError::UnsupportedFeature("invalidSetpointStep")`).
2. **F02 — Exact IEEE 754 binary64 representability of `INT64` setpoint limits:**
   - Because Kotlin `WriteCommand.min`, `max`, and `step` are IEEE 754 binary64 (`Double`), any canonical `INT64` limit not exactly representable in binary64 would silently round or collapse adjacent endpoints (for example `[9007199254740992, 9007199254740993]` collapsing to `[2^53, 2^53]`).
   - `is_exact_binary64_i64(v: i64) -> bool` verifies exact representability in integer arithmetic (`let mag = v.unsigned_abs(); mag == 0 || (mag >> mag.trailing_zeros()) < (1u64 << 53)`), avoiding saturating float-to-int cast edge cases (such as `i64::MAX`). Non-exact `INT64` `min`, `max`, or `step` values fail closed with `CompileError::UnsupportedFeature("inexactSetpointLimit")`. Exact values are emitted verbatim from the validated `serde_json::Number`.

### Committed source digests & Rust regression verification
- **Modified public source SHA-256 digests:**
  - `crates/runtime-package/src/compiler.rs`: `97c1512250754fe932d77f3b8dfad46956b8ddd0706237309ccc2d0bfb486803`
  - `crates/runtime-package/tests/compiler.rs`: `f689bf37bdcb6897bb81dcb05a32bc519f600bd23404af77e6a6c52e5fa1f71b`
  - `crates/runtime-package/examples/emit_interop_fixtures.rs`: `d3164b2b7bdd0c85aad855f917479471f78297a83f93fe5648ecc7d8bf36bf80`
  - `crates/project-model/schema/hmi-project.v2.schema.json`: `b7d49729750bea7a598afa89f64fa57598d5603a88d0fba0dd664abd75ab2738` (zero diff in `crates/project-model/`)
- **New Rust regression tests (`crates/runtime-package/tests/compiler.rs`):**
  - `f01_momentary_pulse_duration_boundaries_fail_closed`: confirms canonical schema v2 still accepts `1, 99, 5001, 60000` ms, compiler refuses all four with `momentaryPulseOutOfRange`, accepts exact boundaries `100` ms and `5000` ms verbatim, and round-trips both via `hmi_legacy_import::import_portable`.
  - `f01_setpoint_missing_and_equal_limits_fail_closed`: confirms canonical schema v2 still accepts missing `min`/`max` and equal `min == max` (`Int32`, `Int64`, `Float32`, `Float64`), compiler refuses missing with `missingSetpointLimits` and equal with `invalidSetpointRange`, and strictly ordered limits without `step` compile verbatim.
  - `f02_int64_setpoint_exact_binary64_representability`: confirms canonical schema v2 still accepts lossy `INT64` limits (`[2^53, 2^53+1]`, `-(2^53+1)`, `2^53+3`, `i64::MAX`, `i64::MIN+1`, `i64::MAX-1022`, and lossy `step = 2^53+1`), compiler refuses all seven with `inexactSetpointLimit`, and exact binary64 `INT64` ranges (`[-2^53, 2^53]` step `1`, `[2^53, 2^53+2]` step `2`, `[i64::MIN, i64::MAX-1023]` step `1024`) compile verbatim.
- **Local verification passed:**
  - `crates/runtime-package`: `cargo fmt --check` PASS, `cargo clippy --all-targets --locked -- -D warnings` PASS, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked` PASS, `cargo test --locked` **13/13 PASS** (5 unit + 8 integration).
  - `crates/legacy-import`: `tests/package.rs` **11/11 PASS**, `tests/raster.rs` **4/4 PASS**.
  - `crates/engineering-core`: `tests/legacy_quarantine.rs` **28/28 PASS**.

### Preserved interoperability evidence & boundary fixture for Legion execution
- **Prior §8 evidence preserved unchanged:** Running `emit_interop_fixtures` produces **100% byte-for-byte identical** archives, `packageId`s, and SHA-256 digests for all 7 default fixtures (`minimal_v2_*` and `industrial_live_v10_*`) validated on `f506dfd` (Kotlin JVM 3/3 PASS, Android API 26 5/5 PASS).
- **Optional boundary fixture (`cargo run --manifest-path crates/runtime-package/Cargo.toml --locked --example emit_interop_fixtures -- <dir> --f01-f02-boundaries`):**
  - `f01_f02_boundaries_v10_signed_seq1.hmipkg` (format 2, TV1, seq 1, OPC UA `LIVE`, containing `pulseMs = 100`, `pulseMs = 5000`, `Int64` setpoint `[-9007199254740992, 9007199254740994]` `step = 2`, and `Float64` setpoint `[-12.5, 12.5]` `step = 0.5`):
    - Size: `1846 B`
    - `packageId`: `8cf7abe0c24f1026712ca31b05ed2fee078450ebad3fd2ea28860e688a410ef9`
    - Archive SHA-256: `30ec7b88efbc95fb781812509cc2978e8586a4b82cc6f1459b5b1c78a1a1e843`
    - `manifest.json` SHA-256: `1559feecf45c0964e64bb42b8eddfdc1f830503104fe829a6a004243230e98b9`
    - `signature.json` SHA-256: `2bf60ce54f2f376bcd5818859d40a0422aab078c2512f2df8ff0952fcff21f52`
- **Pending Legion execution (environment gap honestly recorded):** Focused Kotlin JVM (`:runtime:jvmTest`) and Android API 26 (`:runtime-android:connectedDebugAndroidTest` on `hmistudio-api-26`) differential regression checks against the corrected Rust commit and `f01_f02_boundaries_v10_signed_seq1.hmipkg` were **not** executed from this cloud VM because the pinned private HMI Forge checkout (`92a612d104693800d7119ee4dc0114636b19002f`) and the `hmistudio-api-26` AVD exist only on the owner's Legion machine. Execute the focused Legion differential check against this pushed commit and record its exact observed pass evidence before final P1 sign-off and merge.
