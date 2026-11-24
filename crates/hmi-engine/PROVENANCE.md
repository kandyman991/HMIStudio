# Controlled Rust engine import — REUSE-02B

Source-only, not integrated or released. First-party proprietary direction; no open-source license grant for this crate. Owner rights/disclosure attestation for the exact 27-file allowlist is recorded in D-009 and the preflight notice review merged in HMIStudio #15. Preserve declared third-party terms; this file is not a substitute for redistribution notices. No third-party code vendored.

- Source revision: `92a612d104693800d7119ee4dc0114636b19002f`.
- HMI Forge fixture synchronization #57 merged at `03a5887d1d763c936e5377741ee472680b700085`; all applicable Linux, Android emulator API 26/34 and Android ARM CI passed (45448770963, 45448770573, 45448770554); Linux ARM64 manual job skipped. Current private main `b812bf85d86df7edfa89b335cd00e87efe52a8e0` has no engine changes since that merge. Both shared fixtures verified against main.
- HMIStudio #15 merged at `5173d9a60ffb87b52edde1719043bfcbff4d3be5`; Required CI 45449616282 and snapshot 45449616387 passed; independent review 6044266368. Source rights and dependency inventory recorded in merged preflight.
- [Original allowlist and exclusions](../../docs/reuse/REUSE_02A_IMPORT_PLAN.md), [notice review](../../docs/reuse/REUSE_02A_NOTICE_REVIEW.md), [preflight](../../docs/reuse/REUSE_02A_PREFLIGHT.md).

## Imported files

The following 27 original payload paths describe the immutable initial import commit `e14451343ed5bfe020d75ffde97fd557f872230b`. They retained their mode and candidate SHA-256 at that commit. The explicitly documented dependency-only remediation below supersedes the two Cargo-file hashes; the other 25 payload files are unchanged. Only the protocol fixture example endpoint changed from the pinned source; see original allowlist for original hashes. No private history, SDK, customer data, binaries, artifacts or Kotlin sources copied.

| Relative path | Mode | SHA-256 |
| --- | --- | --- |
| `Cargo.lock` | 100644 | `0959d7f8bda9a207ed3729b7de0f15d5a57edd963cae347f6ab24fa0d83338ad` |
| `Cargo.toml` | 100644 | `a16d95f75563574a3612e195683dc594f1f4e76683504b4af7763161275fd84a` |
| `protocol/messages.json` | 100644 | `d6450d99982882ec2a70e2f4a7d2306cb2c195d4d169be5a0e24faa5e0558a8d` |
| `protocol/write-guard-cases.json` | 100644 | `1bb7c5d9e931292959344ff3ec32830308b76e4a1e54c99f009fa23968e21519` |
| `scripts/build-android.sh` | 100755 | `df1ac69e519841f7e4c03e3d846c978f23c447938265986f9915b85bbbdba1f5` |
| `scripts/build-linux-arm64.sh` | 100755 | `175117ee06c6805689a0861a3899e67689fc86232864f82f21f0a9ed75beb8d3` |
| `scripts/hello-smoke.sh` | 100755 | `4aa81aa8a7729660e9bb11d6d3f6c3beec6aa980e6e9274294b628ce2c6f1194` |
| `src/bin/opcua_test_server.rs` | 100644 | `2cd46ae72c147b43721d9a962f5e99b27c3a74b321ccf4777ab6eeafd1c91e1b` |
| `src/bin/s7_test_server.rs` | 100644 | `ea9eb2b783df15d133c7eac535b17948775363e21e61af3c71e78b5144294b08` |
| `src/driver/loopback.rs` | 100644 | `fd099af456138a5582c345dbebdb714790b9925e25b6417f2c8402ed4c8a51ac` |
| `src/driver/mod.rs` | 100644 | `f82d1e9bc1922c823953afe3faf281282a0d818f87623b3a50cfe6d8f786e85c` |
| `src/driver/opcua.rs` | 100644 | `6016159e7a7c9ee1132e2777194d4e6b8fef5b71acf8a6835567f9f81b7e8ea4` |
| `src/driver/s7.rs` | 100644 | `a360f0a4f82443ee79734de5ae462d147ecfe7568c566a0e3b0613a54d28a046` |
| `src/driver/s7_address.rs` | 100644 | `dfe5e6a2bd43771b7b46c008957f4f38877ac4a678a0464b2ac8c0422c321fca` |
| `src/engine.rs` | 100644 | `e5bc05ff7dabde2d9bca620ddc7fcbf1d5dba9e01b7c98ca4771a366edf657be` |
| `src/guard.rs` | 100644 | `3607fb6c50ff0b56b8e12976f37e5af7de776ec7539e6d0e5080d10025b7e546` |
| `src/lib.rs` | 100644 | `d33c0161d35ff805c7ba09c616807b55bde13037b280b5aefe144563e6e99192` |
| `src/main.rs` | 100644 | `7cee488cbc1e1e2ee7fcdd54ec69e1e91ff5a3010984f7e554f50a97daa66cc5` |
| `src/protocol.rs` | 100644 | `bf391289d63cb32a6efa4cbd580ea8adaa1ed128b48ee51620c0ab5bc9284a94` |
| `src/testserver.rs` | 100644 | `f4862ff295870e2433973db015ade654ecb35a8dc69cc346f71fd5c23dc5b898` |
| `src/testserver_s7.rs` | 100644 | `27aca9d3adb97718bbfb1c2a287d7ba194dde52e714d41c7ceb064d063b840c3` |
| `tests/common/mod.rs` | 100644 | `d233cb53ba220850615bc7fef5b7d44cdc276fb8993d96ee1394b47153c5d09b` |
| `tests/contract.rs` | 100644 | `77fdd5c1d0cd79501130138ece3d480a4d3b75ac6d0a9e3ce9d03af49aa3dbfb` |
| `tests/opcua.rs` | 100644 | `2ef5f87fcf6fe8d1539713912bd64270539f3a479e62565ca162f136f472d4fa` |
| `tests/process.rs` | 100644 | `9a3a9085f9578b977e4056857374baba2b010cf1d37ba930ffbdc7b459a140e9` |
| `tests/pulse.rs` | 100644 | `669e36750f8a732316724cf962f59db8fef154a5770139ee5d8756c82cb155ad` |
| `tests/s7.rs` | 100644 | `9f99bb01863d07ade83410a2862aa9335cdab8976eae442554ede445ccbeea4e` |

## Verification and constraints

- Destination payload verified 27/27 hashes and executable modes, zero extra payload paths. Gitleaks 8.30.1 exact destination scan exit 0, zero findings. Rust fmt and all three shell scripts syntax passed. Scripts retain legacy paths and are not activated or cross-run by Studio.
- Preflight isolated locked suite 37/37 and candidate Rust contracts 4/4; Kotlin contracts 3/3. These are predecessor/local evidence, not hosted acceptance of this import commit.
- The 222 registry lock entries match the reviewed license selections. CI explicitly runs Clippy, tests (serial, including test-server), and rustdoc on Windows/Linux, plus Linux dependency inventory and fresh unwaived RustSec audit. Local repeat audit against pinned advisory DB `1d328b6bb9f2f350bf65c1c81ab68781cb9e018a` returned exit 1: RUSTSEC-2023-0071, RUSTSEC-2026-0592 and 12 warnings. Import PR remains draft; no merge while a required check fails. No advisory ignore list or blanket approval.
- Existing CI-02 conservative fallback selects the full suite, including unrelated desktop installer checks for workflow changes; the imported engine is not referenced by Tauri or included in its installers. No engine artifact upload or packaging change.
- No bridge, model changes, UI, deployment or physical PLC use. Before bridge, resolve applicable decisions and PR #7, bound framing/queues and supervise child failure/timeouts/stale quality; prohibit automatic write replay/rearming and handle wide integer limits safely. Before release, resolve audit and binary/native notice/source availability obligations.


## Dependency remediation — 2026-11-24

The first exact-head hosted run 45455020647 passed Windows/Linux engine compilation, test-server tests, Clippy, rustdoc and notice inventory, as well as other applicable functional checks. Required CI failed only because the Linux engine advisory gate failed. Handoff 45455020664 passed. Do not treat the first head as accepted.

- The released async-opcua 0.19.0 pins rsa ^0.9 in client/crypto; rsa 0.10.0 is required by RUSTSEC-2023-0071. The upstream project's public issue #257 tracks the update. Released 0.19.0 also depends on deprecated serde_yaml/unsafe-libyaml and postcard's heapless defaults.
- Remediation candidate: both client and test-server dependencies use the same exact public upstream Git revision `ca18bb73bac4ff2627943c760c97123062988468` (tree `3b8b55a602b7c2818965625bacee645dc9c0fdc9`, GitHub verified commit signature). No floating branch, private dependency, fork, or vendored source. The upstream development revision includes the RSA 0.10 migration, YAML replacement and postcard default-feature removal. This is an unreleased revision and broader than a patch-only update (103 changed upstream files relative to the packaged revision), requiring compatibility tests and independent review. Prefer a compatible reviewed registry release when available. No cryptographic certification is claimed.
- Lockfile updates url to 2.5.9 and form_urlencoded to 1.2.3; rsa to 0.10.0. The pinned advisory DB `1d328b6bb9f2f350bf65c1c81ab68781cb9e018a` gives exit 0, zero vulnerabilities and one unmaintained warning (chrono RUSTSEC-2026-0543). Both original vulnerabilities and eleven warnings are absent from the new graph; remaining chrono warning is tracked, not ignored or claimed fixed. Upstream uses chrono throughout and replacing it requires separate compatibility work. CI still runs fresh cargo audit with no ignore configuration and the existing exit policy; release remains blocked on appropriate disposition of the warning and binary notices.
- 211 registry archive SHA-256 values verified against new lock; 37 changed/new registry packages have reviewed declared licenses and MIT license text checksums, and inherited exact checksums preserve prior notice evidence. Nine public Git packages are MPL-2.0; exact commit, tree, license SHA-256 and upstream declared package licenses verified. Current machine-readable inventory: [REUSE_02B_DEPENDENCY_INVENTORY.json](../../docs/reuse/REUSE_02B_DEPENDENCY_INVENTORY.json). Original REUSE-02A review remains historical evidence. Gate verifies complete exact package identities/checksums, rejects unreviewed sources and checks the upstream Git checkout and license. No third-party source copied into HMIStudio; preserve covered-source availability and all binary/platform notice obligations before distribution.
- Current Cargo.toml SHA-256 `29d6a378a59bb4dcd8dbfef0413002c8946babd7e180ad6f945dbbf6b2f99a17`; Cargo.lock SHA-256 `08572972bdad44a16a2f6227457bb0ff654047cc0d5bfade99ef15669ee00497`. The remaining 25 imported source/test/fixture/script files are byte-for-byte identical to the initial import. Private Kotlin runtime and protocol version are unchanged.
- Local isolated locked offline suite: 37/37 passed (including encrypted authenticated OPC UA, S7, process, pulse and contract tests); loopback-only network namespace, no external routes/physical PLC. First sandbox attempt lacked the linker symlink target and failed during setup; rerun with read-only /etc/alternatives and loader cache succeeded. Notice inventory gate and 15 Python tests passed (6 notice regressions, 9 routing). Hosted exact-head validation and independent review still mandatory.

- Local isolated fmt, Clippy (--all-targets, --features test-server, -D warnings) and rustdoc (-D warnings) passed on the exact remediation candidate.
