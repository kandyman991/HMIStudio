# REUSE-02A — dependency notice and provenance review

2026-11-24. Source-only import scope; no binaries or third-party source vendoring. Exact engine lockfile SHA-256 `0959d7f8bda9a207ed3729b7de0f15d5a57edd963cae347f6ab24fa0d83338ad`. 222 registry archives independently checksum-verified. The following selections are proposed engineering license choices for dependency use, not legal certification or release clearance. All applicable upstream notices remain preserved.

## Dependency obligations

- Select MIT where expressly offered, otherwise the selected alternatives listed below. Legacy MIT/Apache-2.0 syntax is treated as dual licensing; verify the included grant and preserve its notices. For conjunctions retain every mandatory term; for aws-lc-sys retain the complete bundled component license texts, not a simplified package label.
- Nine async-opcua packages: MPL-2.0. Preserve notices and the exact upstream covered source availability and recipient instructions before distributing an executable. Upstream LICENSE.md at revision `9ad28fc011002398f2e8a95696a50408a14531d9` recovered and hashed in preflight; no upstream code copied into the destination.
- snap7-client/server 0.1.7: upstream MIT grant recovered at revision `7dc36ebc9b53f3e02c69f255d7fc3491ae28ee69`; test dependency graph. rust7 0.1.2 declares MIT with archive LICENSE. r-efi 6.0.0 archive AUTHORS contains the MIT grant and copyright attribution, resolving the apparent missing license filename.
- Keep Apache, ISC, BSD, Unicode-3.0, Zlib, CDLA-Permissive-2.0 and applicable native component notices. Keep complete license and attribution texts for each actual platform graph, including ring/aws-lc, before binary distribution. Data license obligations for webpki-roots must be retained. Do not assume test-only packages ship or that all target-specific packages are used on each platform.
- Scope limit: registry metadata and archived license files are inventory evidence; full native/bundled per-platform binary notice assembly and source availability verification remain release gates. The source-only PR must not publish binary artifacts or vendored dependency code. Two vulnerabilities and twelve warnings remain as documented in the fresh audit; no advisory waived.

## Exact package selections

| Package | Version | Declared license expression | Selected terms |
| --- | --- | --- | --- |
| aes | 0.8.4 | MIT OR Apache-2.0 | MIT |
| aho-corasick | 1.1.5 | Unlicense OR MIT | MIT |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 | MIT |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 | MIT |
| arc-swap | 1.9.5 | MIT OR Apache-2.0 | MIT |
| async-opcua | 0.19.0 | MPL-2.0 | MPL-2.0 |
| async-opcua-client | 0.19.0 | MPL-2.0 | MPL-2.0 |
| async-opcua-core | 0.19.0 | MPL-2.0 | MPL-2.0 |
| async-opcua-core-namespace | 0.19.0 | MPL-2.0 | MPL-2.0 |
| async-opcua-crypto | 0.19.0 | MPL-2.0 | MPL-2.0 |
| async-opcua-macros | 0.19.0 | MPL-2.0 | MPL-2.0 |
| async-opcua-nodes | 0.19.0 | MPL-2.0 | MPL-2.0 |
| async-opcua-server | 0.19.0 | MPL-2.0 | MPL-2.0 |
| async-opcua-types | 0.19.0 | MPL-2.0 | MPL-2.0 |
| async-trait | 0.1.92 | MIT OR Apache-2.0 | MIT |
| atomic-polyfill | 1.0.3 | MIT OR Apache-2.0 | MIT |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | MIT |
| aws-lc-rs | 1.19.0 | ISC AND (Apache-2.0 OR ISC) | ISC |
| aws-lc-sys | 0.47.0 | ISC AND (Apache-2.0 OR ISC) AND Apache-2.0 AND MIT AND BSD-3-Clause AND (Apache-2.0 OR ISC OR MIT) AND (Apache-2.0 OR ISC OR MIT-0) | ISC AND Apache-2.0 AND MIT AND BSD-3-Clause; retain all bundled upstream notices |
| base64 | 0.22.1 | MIT OR Apache-2.0 | MIT |
| base64ct | 1.8.3 | Apache-2.0 OR MIT | MIT |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | MIT |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | MIT |
| block-padding | 0.3.3 | MIT OR Apache-2.0 | MIT |
| bumpalo | 3.20.4 | MIT OR Apache-2.0 | MIT |
| byteorder | 1.5.0 | Unlicense OR MIT | MIT |
| bytes | 1.12.1 | MIT | MIT |
| cbc | 0.1.2 | MIT OR Apache-2.0 | MIT |
| cc | 1.11.2 | MIT OR Apache-2.0 | MIT |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | MIT |
| chrono | 0.4.45 | MIT OR Apache-2.0 | MIT |
| cipher | 0.4.4 | MIT OR Apache-2.0 | MIT |
| cmake | 0.1.58 | MIT OR Apache-2.0 | MIT |
| cobs | 0.3.0 | MIT OR Apache-2.0 | MIT |
| const-oid | 0.9.6 | Apache-2.0 OR MIT | MIT |
| convert_case | 0.11.0 | MIT | MIT |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | MIT |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | MIT |
| critical-section | 1.2.0 | MIT OR Apache-2.0 | MIT |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | MIT |
| der | 0.7.10 | Apache-2.0 OR MIT | MIT |
| der_derive | 0.7.3 | Apache-2.0 OR MIT | MIT |
| digest | 0.10.7 | MIT OR Apache-2.0 | MIT |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | MIT |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 | Apache-2.0 |
| embedded-io | 0.4.0 | MIT OR Apache-2.0 | MIT |
| embedded-io | 0.6.1 | MIT OR Apache-2.0 | MIT |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | MIT |
| errno | 0.3.15 | MIT OR Apache-2.0 | MIT |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 | MIT |
| flagset | 0.4.7 | Apache-2.0 | Apache-2.0 |
| foldhash | 0.1.5 | Zlib | Zlib |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | MIT |
| fs_extra | 1.3.0 | MIT | MIT |
| futures | 0.3.34 | MIT OR Apache-2.0 | MIT |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | MIT |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | MIT |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 | MIT |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | MIT |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | MIT |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | MIT |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | MIT |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | MIT |
| generic-array | 0.14.7 | MIT | MIT |
| gethostname | 0.5.0 | Apache-2.0 | Apache-2.0 |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | MIT |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | MIT |
| hash32 | 0.2.1 | MIT OR Apache-2.0 | MIT |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 | MIT |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | MIT |
| heapless | 0.7.17 | MIT OR Apache-2.0 | MIT |
| hmac | 0.12.1 | MIT OR Apache-2.0 | MIT |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 | MIT |
| iana-time-zone-haiku | 0.1.2 | MIT OR Apache-2.0 | MIT |
| icu_collections | 2.4.0 | Unicode-3.0 | Unicode-3.0 |
| icu_locale_core | 2.4.0 | Unicode-3.0 | Unicode-3.0 |
| icu_normalizer | 2.4.0 | Unicode-3.0 | Unicode-3.0 |
| icu_normalizer_data | 2.4.0 | Unicode-3.0 | Unicode-3.0 |
| icu_properties | 2.4.0 | Unicode-3.0 | Unicode-3.0 |
| icu_properties_data | 2.4.0 | Unicode-3.0 | Unicode-3.0 |
| icu_provider | 2.4.0 | Unicode-3.0 | Unicode-3.0 |
| idna | 1.1.0 | MIT OR Apache-2.0 | MIT |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | MIT |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | MIT |
| inout | 0.1.4 | MIT OR Apache-2.0 | MIT |
| itoa | 1.0.19 | MIT OR Apache-2.0 | MIT |
| jobserver | 0.1.35 | MIT OR Apache-2.0 | MIT |
| js-sys | 0.3.107 | MIT OR Apache-2.0 | MIT |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 | MIT |
| libc | 0.2.191 | MIT OR Apache-2.0 | MIT |
| libm | 0.2.16 | MIT | MIT |
| linux-raw-sys | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | MIT |
| litemap | 0.8.3 | Unicode-3.0 | Unicode-3.0 |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | MIT |
| log | 0.4.35 | MIT OR Apache-2.0 | MIT |
| memchr | 2.8.3 | Unlicense OR MIT | MIT |
| mio | 1.2.5 | MIT | MIT |
| num-bigint-dig | 0.8.6 | MIT/Apache-2.0 | MIT (legacy dual-license syntax) |
| num-integer | 0.1.47 | MIT OR Apache-2.0 | MIT |
| num-iter | 0.1.46 | MIT OR Apache-2.0 | MIT |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | MIT |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | MIT |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | MIT |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | MIT |
| pem-rfc7468 | 0.7.0 | Apache-2.0 OR MIT | MIT |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | MIT |
| percent-encoding-rfc3986 | 0.1.3 | MIT/Apache-2.0 | MIT (legacy dual-license syntax) |
| pin-project | 1.1.13 | Apache-2.0 OR MIT | MIT |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT | MIT |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | MIT |
| pkcs1 | 0.7.5 | Apache-2.0 OR MIT | MIT |
| pkcs8 | 0.10.2 | Apache-2.0 OR MIT | MIT |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | MIT |
| postcard | 1.1.3 | MIT OR Apache-2.0 | MIT |
| potential_utf | 0.1.6 | Unicode-3.0 | Unicode-3.0 |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 | MIT |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | MIT |
| quote | 1.0.47 | MIT OR Apache-2.0 | MIT |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | MIT |
| rand | 0.8.8 | MIT OR Apache-2.0 | MIT |
| rand_chacha | 0.3.1 | MIT OR Apache-2.0 | MIT |
| rand_core | 0.6.4 | MIT OR Apache-2.0 | MIT |
| redox_syscall | 0.5.18 | MIT | MIT |
| regex | 1.13.1 | MIT OR Apache-2.0 | MIT |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | MIT |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | MIT |
| ring | 0.17.14 | Apache-2.0 AND ISC | Apache-2.0 AND ISC |
| rsa | 0.9.10 | MIT OR Apache-2.0 | MIT |
| rust7 | 0.1.2 | MIT | MIT |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 | MIT |
| rustix | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | MIT |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT | MIT |
| rustls-pki-types | 1.16.0 | MIT OR Apache-2.0 | MIT |
| rustls-webpki | 0.103.17 | ISC | ISC |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | MIT |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | Apache-2.0 |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | MIT |
| semver | 1.0.28 | MIT OR Apache-2.0 | MIT |
| serde | 1.0.229 | MIT OR Apache-2.0 | MIT |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | MIT |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | MIT |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | MIT |
| serde_yaml | 0.9.34+deprecated | MIT OR Apache-2.0 | MIT |
| sha1 | 0.10.7 | MIT OR Apache-2.0 | MIT |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | MIT |
| shlex | 2.0.1 | MIT OR Apache-2.0 | MIT |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | MIT |
| signature | 2.2.0 | Apache-2.0 OR MIT | MIT |
| slab | 0.4.12 | MIT | MIT |
| smallvec | 1.16.5 | MIT OR Apache-2.0 | MIT |
| snap7-client | 0.1.7 | MIT | MIT |
| snap7-server | 0.1.7 | MIT | MIT |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | MIT |
| spin | 0.9.9 | MIT | MIT |
| spki | 0.7.3 | Apache-2.0 OR MIT | MIT |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | MIT |
| subtle | 2.6.1 | BSD-3-Clause | BSD-3-Clause |
| syn | 2.0.119 | MIT OR Apache-2.0 | MIT |
| syn | 3.0.10 | MIT OR Apache-2.0 | MIT |
| synstructure | 0.14.0 | MIT | MIT |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | MIT |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | MIT |
| tinystr | 0.8.5 | Unicode-3.0 | Unicode-3.0 |
| tls_codec | 0.4.2 | Apache-2.0 OR MIT | MIT |
| tls_codec_derive | 0.4.2 | Apache-2.0 OR MIT | MIT |
| tokio | 1.54.1 | MIT | MIT |
| tokio-macros | 2.7.2 | MIT | MIT |
| tokio-rustls | 0.26.6 | MIT OR Apache-2.0 | MIT |
| tokio-util | 0.7.20 | MIT | MIT |
| tracing | 0.1.44 | MIT | MIT |
| tracing-attributes | 0.1.31 | MIT | MIT |
| tracing-core | 0.1.36 | MIT | MIT |
| tracing-futures | 0.2.5 | MIT | MIT |
| typenum | 1.20.1 | MIT OR Apache-2.0 | MIT |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | MIT AND Unicode-3.0 |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | MIT |
| unsafe-libyaml | 0.2.11 | MIT | MIT |
| untrusted | 0.9.0 | ISC | ISC |
| url | 2.5.8 | MIT OR Apache-2.0 | MIT |
| utf8_iter | 1.1.0 | Apache-2.0 OR MIT | MIT |
| utf_types | 1.0.0 | Apache-2.0 OR MIT | MIT |
| uuid | 1.29.2 | Apache-2.0 OR MIT | MIT |
| version_check | 0.9.5 | MIT/Apache-2.0 | MIT (legacy dual-license syntax) |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | MIT |
| wasm-bindgen | 0.2.130 | MIT OR Apache-2.0 | MIT |
| wasm-bindgen-macro | 0.2.130 | MIT OR Apache-2.0 | MIT |
| wasm-bindgen-macro-support | 0.2.130 | MIT OR Apache-2.0 | MIT |
| wasm-bindgen-shared | 0.2.130 | MIT OR Apache-2.0 | MIT |
| webpki-roots | 0.26.11 | CDLA-Permissive-2.0 | CDLA-Permissive-2.0 |
| webpki-roots | 1.0.12 | CDLA-Permissive-2.0 | CDLA-Permissive-2.0 |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | MIT |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | MIT |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | MIT |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | MIT |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | MIT |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | MIT |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 | MIT |
| windows-sys | 0.59.0 | MIT OR Apache-2.0 | MIT |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | MIT |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | MIT |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | MIT |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 | MIT |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 | MIT |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 | MIT |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 | MIT |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 | MIT |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | MIT |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | MIT |
| writeable | 0.6.4 | Unicode-3.0 | Unicode-3.0 |
| x509-cert | 0.2.5 | Apache-2.0 OR MIT | MIT |
| yoke | 0.8.4 | Unicode-3.0 | Unicode-3.0 |
| yoke-derive | 0.8.4 | Unicode-3.0 | Unicode-3.0 |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | MIT |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | MIT |
| zerofrom | 0.1.8 | Unicode-3.0 | Unicode-3.0 |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | Unicode-3.0 |
| zeroize | 1.9.1 | Apache-2.0 OR MIT | MIT |
| zeroize_derive | 1.5.0 | Apache-2.0 OR MIT | MIT |
| zerotrie | 0.2.5 | Unicode-3.0 | Unicode-3.0 |
| zerovec | 0.11.9 | Unicode-3.0 | Unicode-3.0 |
| zerovec-derive | 0.11.7 | Unicode-3.0 | Unicode-3.0 |
| zmij | 1.0.23 | MIT | MIT |

## Source provenance assessment

Owner direction D-009 authorizes selective first-party transfer in principle, not third-party redistribution. The present instruction authorizes proceeding subject to the recorded gates. All 27 paths have positive project-local Git history; seven development commits cover the original engine, typed S7 addresses and driver, demand/browse/status, guarded writes and target scripts. History identifies owner-associated and AI-associated contributions. The history is retained privately. This does not establish title or prove that all code was independently authored. No new owner rights attestation is on record.

Reviewed manifest, lockfile, fixture structure, driver imports/interfaces, selected implementation sections, synthetic test servers, test harnesses, process and guard logic and scripts. The S7 test byte vectors are generated synthetic protocol frames in loopback test code, not an imported customer capture as far as inspected. No vendor SDK linkage, embedded binary, external source tree or proprietary evidence identified in the candidate. Gitleaks original and sanitized candidate returned zero findings. The example network address was removed; test credentials are explicit synthetic examples used by loopback test code, not deployment secrets. No absolute claim of absence of copied code or customer data.

Per-path assessment remains conditional on the owner confirming rights and absence of third-party/customer material outside the declared registry dependencies. No formal legal certification is claimed. Do not publish the private source until this condition is recorded.

## Candidate path assessment

| Path | Technical provenance category | Disposition |
| --- | --- | --- |
| `engine/Cargo.lock` | Manifest / generated registry dependency lock | Candidate hash verified; rights confirmation pending |
| `engine/Cargo.toml` | Manifest / generated registry dependency lock | Candidate hash verified; rights confirmation pending |
| `engine/protocol/messages.json` | Shared project protocol test fixture | Sanitized loopback candidate; rights confirmation pending |
| `engine/protocol/write-guard-cases.json` | Shared project protocol test fixture | Candidate hash verified; rights confirmation pending |
| `engine/scripts/build-android.sh` | Project target-build helper (legacy paths; not activated) | Candidate hash verified; rights confirmation pending |
| `engine/scripts/build-linux-arm64.sh` | Project target-build helper (legacy paths; not activated) | Candidate hash verified; rights confirmation pending |
| `engine/scripts/hello-smoke.sh` | Project target-build helper (legacy paths; not activated) | Candidate hash verified; rights confirmation pending |
| `engine/src/bin/opcua_test_server.rs` | Project-local synthetic test harness/server | Candidate hash verified; rights confirmation pending |
| `engine/src/bin/s7_test_server.rs` | Project-local synthetic test harness/server | Candidate hash verified; rights confirmation pending |
| `engine/src/driver/loopback.rs` | Project-local Rust implementation | Candidate hash verified; rights confirmation pending |
| `engine/src/driver/mod.rs` | Project-local Rust implementation | Candidate hash verified; rights confirmation pending |
| `engine/src/driver/opcua.rs` | Project-local Rust implementation | Candidate hash verified; rights confirmation pending |
| `engine/src/driver/s7.rs` | Project-local Rust implementation | Candidate hash verified; rights confirmation pending |
| `engine/src/driver/s7_address.rs` | Project-local Rust implementation | Candidate hash verified; rights confirmation pending |
| `engine/src/engine.rs` | Project-local Rust implementation | Candidate hash verified; rights confirmation pending |
| `engine/src/guard.rs` | Project-local Rust implementation | Candidate hash verified; rights confirmation pending |
| `engine/src/lib.rs` | Project-local Rust implementation | Candidate hash verified; rights confirmation pending |
| `engine/src/main.rs` | Project-local Rust implementation | Candidate hash verified; rights confirmation pending |
| `engine/src/protocol.rs` | Project-local Rust implementation | Candidate hash verified; rights confirmation pending |
| `engine/src/testserver.rs` | Project-local synthetic test harness/server | Candidate hash verified; rights confirmation pending |
| `engine/src/testserver_s7.rs` | Project-local synthetic test harness/server | Candidate hash verified; rights confirmation pending |
| `engine/tests/common/mod.rs` | Project-local synthetic test harness/server | Candidate hash verified; rights confirmation pending |
| `engine/tests/contract.rs` | Project-local synthetic test harness/server | Candidate hash verified; rights confirmation pending |
| `engine/tests/opcua.rs` | Project-local synthetic test harness/server | Candidate hash verified; rights confirmation pending |
| `engine/tests/process.rs` | Project-local synthetic test harness/server | Candidate hash verified; rights confirmation pending |
| `engine/tests/pulse.rs` | Project-local synthetic test harness/server | Candidate hash verified; rights confirmation pending |
| `engine/tests/s7.rs` | Project-local synthetic test harness/server | Candidate hash verified; rights confirmation pending |

No public code transfer, runtime distribution, production connectivity or release approval.

## Owner attestation — 2026-11-24
Owner confirmed control of the rights to the exact 27-file allowlist from pinned HMI Forge revision 92a612d104693800d7119ee4dc0114636b19002f, including AI-assisted contributions, and confirmed that these files contain no unauthorized third-party proprietary code or customer material. This is the owner rights/disclosure attestation requested in REUSE-02A, not an independent legal certification. Authorization is confined to the reviewed sanitized source candidate under D-009; no private history, SDKs, signing material, third-party vendored source, binaries or release/distribution approval. This supersedes the pending-owner-attestation language and the conditional path dispositions above; the technical evidence and limitations remain unchanged.
