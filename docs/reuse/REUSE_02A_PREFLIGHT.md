# REUSE-02A — Rust communication engine integration preflight

Status: **in progress; no transfer clearance**. 2026-11-24. Destination base `238fd18a4006ced60b5459a37ff18ac1471a6e2c` (PR #9 merged); source `kandyman991/hmiforge` at `92a612d104693800d7119ee4dc0114636b19002f`. D-009/O-1/O-2/O-7 govern the scope. This PR records evidence only; no source import, driver connection, runtime modification or production acceptance.

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
