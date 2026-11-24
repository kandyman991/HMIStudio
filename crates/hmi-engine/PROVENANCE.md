# Controlled Rust engine import — REUSE-02B

Source-only, not integrated or released. First-party proprietary direction; no open-source license grant for this crate. Owner rights/disclosure attestation for the exact 27-file allowlist is recorded in D-009 and the preflight notice review merged in HMIStudio #15. Preserve declared third-party terms; this file is not a substitute for redistribution notices. No third-party code vendored.

- Source revision: `92a612d104693800d7119ee4dc0114636b19002f`.
- HMI Forge fixture synchronization #57 merged at `03a5887d1d763c936e5377741ee472680b700085`; all applicable Linux, Android emulator API 26/34 and Android ARM CI passed (45448770963, 45448770573, 45448770554); Linux ARM64 manual job skipped. Current private main `b812bf85d86df7edfa89b335cd00e87efe52a8e0` has no engine changes since that merge. Both shared fixtures verified against main.
- HMIStudio #15 merged at `5173d9a60ffb87b52edde1719043bfcbff4d3be5`; Required CI 45449616282 and snapshot 45449616387 passed; independent review 6044266368. Source rights and dependency inventory recorded in merged preflight.
- [Original allowlist and exclusions](../../docs/reuse/REUSE_02A_IMPORT_PLAN.md), [notice review](../../docs/reuse/REUSE_02A_NOTICE_REVIEW.md), [preflight](../../docs/reuse/REUSE_02A_PREFLIGHT.md).

## Imported files

The following 27 original payload paths retain their mode and candidate SHA-256. Only the protocol fixture example endpoint changed from the pinned source; see original allowlist for original hashes. No private history, SDK, customer data, binaries, artifacts or Kotlin sources copied.

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
