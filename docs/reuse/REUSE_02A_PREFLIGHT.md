# REUSE-02A — Rust communication engine integration preflight

Status: **technical evidence refreshed; controlled candidate prepared; no transfer clearance**. 2026-11-24. Destination base `238fd18a4006ced60b5459a37ff18ac1471a6e2c` (PR #9 merged); source `kandyman991/hmiforge` at `92a612d104693800d7119ee4dc0114636b19002f`. D-009/O-1/O-2/O-7 govern the scope. This PR records evidence only; no source import, driver connection, runtime modification or production acceptance.

## 1. Verified prerequisites

- CI-02 PR #14 merged as `39eba35a90fdfa27bc613a14b15b9e0bb461c639`; exact head `de86b95` passed full Desktop CI 45440535709 and handoff 45440535863. Branch protection and active branch rules were absent when independently queried. Manual dispatch remains unexercised independently.
- REUSE-01 PR #9 merged as `238fd18a4006ced60b5459a37ff18ac1471a6e2c`; exact head `6d346ce` passed Required CI (45445225707) and handoff (45445226131). Frontend/crates/desktop intentionally skipped; artifact API returned zero artifacts. Nine local classifier tests passed. The audit is a planning baseline, not transfer clearance.
- RUST-06A PR #7 remains open; its tag-engine dependency checks must be retained and independently reviewed against current main before bridge integration.

## 2. Pinned source observations

Read-only inspection of the pinned private tree through GitHub API and a separate local detached checkout on an authorized machine. No private history or file contents were copied into HMIStudio. The checkout was clean. Git blob integrity and byte counts were verified for all 27 engine files (304,960 bytes; all UTF-8); SHA-256 was calculated for each. Local checkout independently confirmed 27 tracked files. The engine consists of manifest/lock, two protocol fixtures, three scripts, Rust implementation and test-server code, and tests. Historical author metadata has two distinct identities across seven engine-path commits (4 and 3); this is not proof of ownership or third-party rights. Package license is UNLICENSED; owner direction is proprietary first-party reuse, not open-source licensing. Source attribution and rights review remain mandatory.

| Input | SHA-256 |
| --- | --- |
| engine/Cargo.toml | a16d95f75563574a3612e195683dc594f1f4e76683504b4af7763161275fd84a |
| engine/Cargo.lock | 0959d7f8bda9a207ed3729b7de0f15d5a57edd963cae347f6ab24fa0d83338ad |
| engine/protocol/messages.json | 839b234c74893da0eaac530c65d7c25cb9618da7cb626ecc11f09886c3cc2bdf |
| engine/protocol/write-guard-cases.json | 1bb7c5d9e931292959344ff3ec32830308b76e4a1e54c99f009fa23968e21519 |

The lockfile contains 222 external registry packages, all with checksums, no external Git/path packages. The manifest declares eight direct and two development dependencies, feature `test-server`, release `panic = abort`. Lockfile integrity is not a license or vulnerability assessment.

## 3. Screening and unresolved disclosure candidate

Two limited regex-based screens (GitHub blob bytes and local tracked UTF-8 files) found no matches for private-key headers, selected common token prefixes or quoted credential assignments. Additional local patterns found no copyright/SPDX headers, selected vendor-evidence/customer markers or absolute user paths. These patterns are incomplete: false negatives and false positives are possible. One private-network literal was found in `engine/protocol/messages.json`, within the example configuration endpoint. Its value is intentionally withheld. Do not publish the fixture until its origin is established or it is replaced by a documented synthetic endpoint with coordinated fixture checksums and compatibility tests. Full per-file human review, robust secret scanning (including high-entropy/encoded secrets), fixture/customer-data assessment and rights evidence remain open. No blanket clearance follows from zero matches.

## 4. Dependency and license preflight

Inherited REUSE-01 evidence: 37 tests passed, clean Clippy, cargo audit 2 vulnerabilities and 12 warnings (see AUDIT_SUMMARY and RUST_ENGINE_INTEGRATION). Not rerun during this slice. Local Cargo 1.100.0 is available; cargo-audit, cargo-deny/about and gitleaks were not found in the checked environment. A fresh audit against a dated advisory DB and exact lockfile is mandatory before transfer clearance. Review RSA and URL findings, test-only reachability, exact resolved licensing and complete notices including MPL source-access obligations; record dispositions without treating an unfixed advisory as waived. O-6 release policy remains open. Do not expose private source or credentials to public CI. Do not distribute binaries as part of this preflight.

## 5. Isolated protocol-3 validation plan (not executed)

- Preserve exact protocol-3 handshake, fixtures and message semantics; no implicit version negotiation. Use synthetic loopback/test endpoints only, no physical PLC access. Keep fixture hashes verified after disclosure review.
- Build/test the pinned engine in a clean isolated private checkout with locked dependencies and test-server feature; capture exact commands, versions and results; run contract and rejected-message vectors, frame/queue bounds and version-mismatch cases.
- Verify process lifecycle: normal EOF/shutdown, closed stdout, stalled client, bounded buffering/timeouts, child termination/reaping, restart and stale quality; never auto-retry writes or restore arming. Pulse deassertion cannot be guaranteed under crash; document and test failure behavior.
- Test signed/unsigned conversion bounds and wide-integer write-limit rounding above 2^53; reject unsupported ranges/types pending explicit design. No production write claim. Keep design-time simulator independent and unrestricted by production approval.
- Resolve O-5 process hosting and applicable O-3/O-9 model/driver decisions before integrating the bridge. Keep existing PR #7 separate. Both runtime export modes and Windows/Linux/Android remain requirements.

## 6. Remaining gate and next action

1. Complete a per-file rights/provenance and sensitive-content review of the 27-file pinned set; disposition the fixture endpoint without exposing it. Produce a reproducible allowlisted import manifest and exclusion list (no history, proprietary SDKs or signing material).
2. Run current advisory/license tools and isolated protocol-3 tests; record actual results, unresolved findings and notice obligations. No transfer until review and clearance are recorded.
3. Independently review the preflight and resolve applicable owner decisions, then prepare a separate squashed import PR. PR #7 must be integrated before dependent bridge implementation. Preserve legacy engine until cross-repository parity is verified.

This evidence-only PR is not REUSE-02A completion. No owner installation, manual desktop acceptance or production approval is required for this preflight.

## 7. Independent preflight follow-up — 2026-11-24

Reviewed PR #15 predecessor `d26373a120c3ec6b77500f6022c4bb42581fcfa7` against main `238fd18a4006ced60b5459a37ff18ac1471a6e2c`. Desktop CI 45445542303 completed successfully: classification and Required CI passed; crates/frontend/desktop were skipped. Handoff 45445542430 snapshot passed. PR #7 remained open at `de9e207a43b9c8ab322b1d3db33fd0832febe596`. The following results supersede the unexecuted/limited evidence in sections 3–5 where applicable. Review is evidence-only; neither the source nor private history has been transferred.

### Source and candidate

- Independently verified the clean detached pinned checkout, all 27 Git blob hashes/modes and byte counts (304,960 original bytes). Examined per-path history for seven engine-path commits: one owner-associated identity and one AI-associated identity; metadata is not rights evidence. Per-file automated attribution/reference and IPv4 screening found no attribution markers or additional RFC1918 literals. This is not exhaustive manual source/ownership verification.
- Gitleaks 8.30.1 (official release archive matched its published SHA-256 checksum), default rule/entropy scan of the exact engine directory, fully redacted output, isolated network namespace: exit 0, zero findings. Prepared a private candidate with the sole example endpoint replaced by loopback, preserving all other JSON values and bytes outside that replacement. Reran Gitleaks on the exact candidate: exit 0, zero findings. Do not equate zero findings with absence of secrets or customer data.
- Candidate manifest, file hashes, exclusion list, import steps and open gates are in [REUSE_02A_IMPORT_PLAN.md](REUSE_02A_IMPORT_PLAN.md). Source fixture SHA-256 is unchanged at origin; sanitized candidate fixture SHA-256 is `d6450d99982882ec2a70e2f4a7d2306cb2c195d4d169be5a0e24faa5e0558a8d`. Private Kotlin checksum synchronization and contract retest remain pending. No private endpoint published.

### Fresh dependency and license evidence

- Cargo-audit 0.23.0 official GitHub release archive, downloaded from RustSec; locally measured archive SHA-256 `b77993532b253fcbf550996d7716465ec3d734d4e74e7f87bfb65a19add0fb59` (no independent upstream digest provided). Run offline in a read-only-source/network-isolated sandbox using RustSec advisory DB commit `1d328b6bb9f2f350bf65c1c81ab68781cb9e018a` (2026-11-24 10:21 +01:00; 1,566 advisories): `cargo audit --db <pinned-db> --no-fetch --file Cargo.lock --json`. Exit 1; **2 vulnerabilities, 12 warnings**. RUSTSEC-2023-0071 (rsa 0.9.10) and RUSTSEC-2026-0592 (url 2.5.8) remain unresolved. Unmaintained: atomic-polyfill, chrono, unsafe-libyaml. Nine unsoundness warnings in heapless 0.7.17 (RUSTSEC-2026-0574 through -0582). No advisory waived.
- `cargo tree --locked --offline -e normal,build -i <crate>` verified rsa/url/chrono/unsafe-libyaml in the default host graph; `--target all` with normal/build edges found no heapless or atomic-polyfill. With test-server enabled, heapless is reached through postcard and async-opcua-server. These findings distinguish test-only warnings from default engine dependencies; runtime reachability of the RSA and URL vulnerabilities is not resolved.
- All 222 registry package archives were available in the cache and individually matched their Cargo.lock SHA-256 checksums; extracted license expressions and named license/notice files into a private candidate inventory (398 files). Eleven archives lacked matching notice filenames: nine async-opcua 0.19.0 packages and snap7-client/server 0.1.7. Obtained upstream license texts at their archive-recorded VCS commits: freeopcua/async-opcua `9ad28fc011002398f2e8a95696a50408a14531d9` LICENSE.md (MPL-2.0), SHA-256 `1f256ecad192880510e84ad60474eab7589218784b9a50bc7ceee34c2b91f1d5`; cool0looc/rs-snap7 `7dc36ebc9b53f3e02c69f255d7fc3491ae28ee69` LICENSE (MIT), SHA-256 `a7ea844b24c06ab0038cfdb52afe2faf5b7f5192ede1e0e047b6a76e254bb059`. Inventory includes all 222 packages, not just the shipped subset. Final SPDX alternative selections, bundled/native subcomponent notices, source-availability mechanism and substantive rights/license review remain open; no release/distribution clearance.

### Executed protocol tests

- Cargo/rustc 1.100.0. Clean pinned source mounted read-only in Bubblewrap with a private loopback-only network namespace (no external interface or route), private writable build target, offline locked registry, and synthetic test servers. Initial sandbox setup failed because its mount destination was on a read-only root; corrected with a private temporary mount tree. The corrected run `cargo test --locked --offline --features test-server -- --test-threads=1` exited 0: **37 passed, 0 failed, 0 ignored** (9 library, 4 contract, 9 OPC UA, 2 process, 4 pulse, 9 S7). Includes malformed request, incompatible protocol version, normal EOF/shutdown, guarded simulated writes, link failure and pulse behavior. No physical PLC connection.
- Independently mounted the sanitized candidate at the same read-only source path and reran the compiled contract test executable, which loads JSON at runtime; all 4 passed, exit 0. All code bytes identical; this is fixture compatibility evidence, not a fresh full build of the future import commit.
- Static review of `main.rs`, `guard.rs`, `protocol.rs` and `tests/process.rs` confirms the remaining known limitations: unbounded request/output channels and line reader, stdout failure ends only writer, possible stalled-client/child lifetime issues, and f64 rounding of wide signed integers. Existing tests do not prove bounded framing, pipe failure supervision, abrupt crash pulse release or exact integer limits. These require new regression evidence and mitigation/safe refusal before bridge activation, not a claim that production writes are ready.

### Independent review disposition

The original PR was correctly evidence-only. Corrected incomplete test/audit/disclosure status with the fresh evidence above and prepared the controlled allowlist. The review identifies unresolved transfer gates: per-file substantive rights and sensitive-content attestation, final dependency notice/license choices, private fixture parity. Unresolved process/numeric risks, O-5 and applicable model decisions, PR #7, and O-6 remain downstream gates. The fresh audit is not a passing vulnerability check. No public engine transfer, import PR, production approval, physical connection or installer is authorized by this evidence. Keep #15 draft until transfer gate status and exact-head review/CI are reconciled. Next action: finalize the rights/notice and fixture parity record; then open a separate import-only PR under the controlled plan, not a bridge or release PR.
