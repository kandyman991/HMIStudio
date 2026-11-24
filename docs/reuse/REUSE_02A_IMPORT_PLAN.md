# REUSE-02A — controlled import candidate

**Prepared, not cleared for public transfer.** 2026-11-24. Source `kandyman991/hmiforge@92a612d104693800d7119ee4dc0114636b19002f`; destination base `238fd18a4006ced60b5459a37ff18ac1471a6e2c`. Owner direction D-009/O-1/O-2/O-7 permits conditional first-party reuse, not blanket redistribution. No private source has been pushed.

## Exact candidate allowlist

Each entry was checked against the pinned Git blob, byte length and SHA-256 in a clean detached checkout. The 27 regular files total 304,960 original bytes; 24 mode 100644 and three mode 100755. Copy only these files into `crates/hmi-engine/`, dropping the source `engine/` prefix. Except the documented fixture change, all candidate bytes equal source bytes. This inventory is a candidate allowlist, not permission to publish any file.

| Source path | Mode | Bytes | Source SHA-256 | Candidate SHA-256 if changed |
| --- | --- | ---: | --- | --- |
| `engine/Cargo.lock` | 100644 | 54340 | `0959d7f8bda9a207ed3729b7de0f15d5a57edd963cae347f6ab24fa0d83338ad` | same |
| `engine/Cargo.toml` | 100644 | 1544 | `a16d95f75563574a3612e195683dc594f1f4e76683504b4af7763161275fd84a` | same |
| `engine/protocol/messages.json` | 100644 | 5917 | `839b234c74893da0eaac530c65d7c25cb9618da7cb626ecc11f09886c3cc2bdf` | `d6450d99982882ec2a70e2f4a7d2306cb2c195d4d169be5a0e24faa5e0558a8d` |
| `engine/protocol/write-guard-cases.json` | 100644 | 10100 | `1bb7c5d9e931292959344ff3ec32830308b76e4a1e54c99f009fa23968e21519` | same |
| `engine/scripts/build-android.sh` | 100755 | 2051 | `df1ac69e519841f7e4c03e3d846c978f23c447938265986f9915b85bbbdba1f5` | same |
| `engine/scripts/build-linux-arm64.sh` | 100755 | 827 | `175117ee06c6805689a0861a3899e67689fc86232864f82f21f0a9ed75beb8d3` | same |
| `engine/scripts/hello-smoke.sh` | 100755 | 611 | `4aa81aa8a7729660e9bb11d6d3f6c3beec6aa980e6e9274294b628ce2c6f1194` | same |
| `engine/src/bin/opcua_test_server.rs` | 100644 | 1197 | `2cd46ae72c147b43721d9a962f5e99b27c3a74b321ccf4777ab6eeafd1c91e1b` | same |
| `engine/src/bin/s7_test_server.rs` | 100644 | 1599 | `ea9eb2b783df15d133c7eac535b17948775363e21e61af3c71e78b5144294b08` | same |
| `engine/src/driver/loopback.rs` | 100644 | 5282 | `fd099af456138a5582c345dbebdb714790b9925e25b6417f2c8402ed4c8a51ac` | same |
| `engine/src/driver/mod.rs` | 100644 | 4023 | `f82d1e9bc1922c823953afe3faf281282a0d818f87623b3a50cfe6d8f786e85c` | same |
| `engine/src/driver/opcua.rs` | 100644 | 41258 | `6016159e7a7c9ee1132e2777194d4e6b8fef5b71acf8a6835567f9f81b7e8ea4` | same |
| `engine/src/driver/s7.rs` | 100644 | 21565 | `a360f0a4f82443ee79734de5ae462d147ecfe7568c566a0e3b0613a54d28a046` | same |
| `engine/src/driver/s7_address.rs` | 100644 | 25181 | `dfe5e6a2bd43771b7b46c008957f4f38877ac4a678a0464b2ac8c0422c321fca` | same |
| `engine/src/engine.rs` | 100644 | 23519 | `e5bc05ff7dabde2d9bca620ddc7fcbf1d5dba9e01b7c98ca4771a366edf657be` | same |
| `engine/src/guard.rs` | 100644 | 6056 | `3607fb6c50ff0b56b8e12976f37e5af7de776ec7539e6d0e5080d10025b7e546` | same |
| `engine/src/lib.rs` | 100644 | 588 | `d33c0161d35ff805c7ba09c616807b55bde13037b280b5aefe144563e6e99192` | same |
| `engine/src/main.rs` | 100644 | 2399 | `7cee488cbc1e1e2ee7fcdd54ec69e1e91ff5a3010984f7e554f50a97daa66cc5` | same |
| `engine/src/protocol.rs` | 100644 | 10265 | `bf391289d63cb32a6efa4cbd580ea8adaa1ed128b48ee51620c0ab5bc9284a94` | same |
| `engine/src/testserver.rs` | 100644 | 7530 | `f4862ff295870e2433973db015ade654ecb35a8dc69cc346f71fd5c23dc5b898` | same |
| `engine/src/testserver_s7.rs` | 100644 | 7699 | `27aca9d3adb97718bbfb1c2a287d7ba194dde52e714d41c7ceb064d063b840c3` | same |
| `engine/tests/common/mod.rs` | 100644 | 5742 | `d233cb53ba220850615bc7fef5b7d44cdc276fb8993d96ee1394b47153c5d09b` | same |
| `engine/tests/contract.rs` | 100644 | 3819 | `77fdd5c1d0cd79501130138ece3d480a4d3b75ac6d0a9e3ce9d03af49aa3dbfb` | same |
| `engine/tests/opcua.rs` | 100644 | 22974 | `2ef5f87fcf6fe8d1539713912bd64270539f3a479e62565ca162f136f472d4fa` | same |
| `engine/tests/process.rs` | 100644 | 2523 | `9a3a9085f9578b977e4056857374baba2b010cf1d37ba930ffbdc7b459a140e9` | same |
| `engine/tests/pulse.rs` | 100644 | 9439 | `669e36750f8a732316724cf962f59db8fef154a5770139ee5d8756c82cb155ad` | same |
| `engine/tests/s7.rs` | 100644 | 26912 | `9f99bb01863d07ade83410a2862aa9335cdab8976eae442554ede445ccbeea4e` | same |

The candidate changes only the example OPC UA endpoint in `protocol/messages.json` to a loopback host, retaining scheme and port. JSON structure and all other values are equal; original value intentionally not disclosed. Both contract fixtures remain protocol 3. The sanitized fixture passed all four Rust contract tests; coordinate the new checksum and Kotlin contract test in the private runtime repository before declaring cross-repository parity. Do not import the original unsanitized fixture.

## Exclusion and reproducibility rules

- Use only a clean detached checkout of the pinned commit; verify Git blob modes and SHA-256; reject new or changed paths, links, executable mode drift, and any mismatch. Copy bytes into a private staging directory first. For the one fixture, parse JSON, replace only the identified endpoint with a synthetic loopback host, assert the rest of the parsed JSON is identical, and verify the candidate hash. Repeat secret/disclosure scans on the exact staged bytes.
- Exclude everything outside this allowlist: private history and .git metadata, other modules, proprietary SDKs and reverse-engineering evidence, third-party source trees, customer projects, signing material, credentials, certificates, generated binaries, target directories, private runner details and logs. Do not embed or expose the private clone or credentials in public CI.
- For each file independently confirm first-party rights and disclosure review; history metadata alone is not a legal grant. Collect author/owner evidence without publishing identities, private chat or raw history. Do not silently treat an AI-associated author identity as proof of title. Record any exclusion and adjust the manifest explicitly.
- Before import, finalize a license/notice record for the exact dependency graph, including selected alternative licenses and all applicable copyright/NOTICE terms, MPL-covered source availability for distributed binaries, and transitive/native component obligations. Keep dependencies as Cargo registry references; do not vendor their source. Retain the locked dependency baseline and document any change separately.

## Import execution (only after clearance)

1. Refresh live main, PR #15 and concurrent PRs. Obtain independent review of the exact allowlist, candidate fixture, dependency and rights record. Do not use this plan to bypass unresolved gates. Merge evidence-only PR #15 only with exact-head Required CI, handoff and review; a documentation merge is not transfer authorization.
2. Create a separate branch from verified main and one squashed import commit, with `crates/hmi-engine/PROVENANCE.md` recording source revision, per-file SHA-256, modes, fixture delta, audit and rights/license decisions. No private history, package publication, installer or executable distribution. Add only necessary build discovery/CI wiring and documented attribution; keep bridge, Tauri sidecar, schema and UI out of this PR. Validate scripts before use; do not run cross-build downloads merely for the import.
3. Verify candidate hashes after staging, lockfile unchanged, no hidden extra files, no private paths or credentials. Use hosted Windows/Linux locked tests, including explicit `--features test-server` so OPC UA/S7 test suites are not skipped. Confirm the new crate is discovered by CI-02 and explicitly include applicable advisory/license gates. Review fixture hashes, fmt, Clippy, rustdoc and full tests. No routine MSI/DEB generation unless packaging paths change or a manual milestone request applies. Do not mark skipped jobs passed.
4. Preserve legacy engine and private Kotlin runtime. Record exact-head CI and independent review. Only after rights/notice/disclosure and engineering gates are satisfied may the import merge; no production, runtime, release or hardware acceptance is implied.
5. Before dependent bridge work, integrate reviewed PR #7 against current main, resolve O-5 and relevant model/driver decisions, and implement bounded framing/queues, timeout/child supervision, stale quality, no automatic write replay/rearming, and safe wide-integer write-limit enforcement. Resolve O-6 release advisory disposition before any release. Preserve unrestricted isolated development/simulation; no physical PLC tests during import.

## Outstanding gates

- **Source transfer:** per-file rights and substantive disclosure attestation; final dependency/license choices and notice review. The inventory, Gitleaks results, author metadata and license census are evidence, not a substitute for these decisions.
- **Compatibility:** private Kotlin contract rerun and checksum synchronization; hosted Windows/Linux import tests on the actual import commit.
- **Bridge/release:** process failure and numeric safety tests, PR #7 integration, applicable architecture decisions, and advisory mitigation/release disposition. No hardware acceptance or production claims.

## Owner attestation update — 2026-11-24
Owner confirmed control of the rights to the exact 27-file allowlist from pinned HMI Forge revision 92a612d104693800d7119ee4dc0114636b19002f, including AI-assisted contributions, and confirmed that these files contain no unauthorized third-party proprietary code or customer material. This is the owner rights/disclosure attestation requested in REUSE-02A, not an independent legal certification. Authorization is confined to the reviewed sanitized source candidate under D-009; no private history, SDKs, signing material, third-party vendored source, binaries or release/distribution approval. This resolves the owner-confirmation part of the transfer gate; retain all exact-head CI, private fixture merge, independent review, staged hash/disclosure checks and import-only restrictions. Do not publish source until the preflight/fixture merge gates are met.
