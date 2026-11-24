# REUSE-01 — Licensing, provenance and repository strategy

Status: **proposed; owner decisions required**. This document is not legal advice. It records facts and options so
that the owner can decide. Source: HMI Forge `92a612d`. Destination: HMIStudio `264ace7`.

## 1. Facts

| Item | State |
| --- | --- |
| HMIStudio licence | none selected (D-003, AGENTS.md). The repository is public |
| HMI Forge licence | none; private repository; `engine/Cargo.toml` `license = "UNLICENSED"` |
| Authorship | Per HMI Forge records, owner-directed work produced by AI agents. No third-party contributor PRs were seen in the 52 PRs (not independently verified per commit author) |
| HMI Forge third-party notices | `THIRD_PARTY_NOTICES.md` lists Google Tink 1.23.0 (Apache-2.0) and its dependencies (gson, error_prone annotations, jsr305, androidx.annotation, protobuf-java BSD-3-Clause), and says it is not yet complete |
| HMIStudio issue #4 | open: dependency advisories and third-party redistribution obligations |

### Engine dependency licences

Taken from `cargo tree --locked -e normal`, which covers what ships in `hmi-engine`:

- MIT and/or Apache-2.0 for most crates;
- **MPL-2.0** for the `async-opcua` family: `async-opcua`, `-client`, `-core`, `-crypto`, `-types`, `-nodes` and
  `-macros`;
- **Unicode-3.0** for the ICU4X crates, `unicode-ident` and others;
- Zlib (`foldhash`), BSD-3-Clause (`subtle`), Apache-2.0 OR BSL-1.0 (`ryu`) and Unlicense OR MIT.

Test-only dependencies (`snap7-server`, the `async-opcua` server) add no obligations to shipped binaries.

### Runtime dependency licences (Kotlin)

Kotlin stdlib, kotlinx-serialization, Compose Multiplatform, AndroidX and Google Tink are all Apache-2.0.
protobuf-java is BSD-3-Clause. A Temurin JRE (GPLv2 with Classpath Exception) is bundled in the Linux ARM64 tarball,
and jpackage runtimes are bundled in the MSI and .deb.

## 2. Obligations if distributed

- **MPL-2.0** is file-level copyleft. The binary can be distributed under any licence. The source of the MPL-covered
  files, including any modifications to them, must be made available, and recipients must be told how to get it. The
  crates are used unmodified from crates.io, so naming the versions and their source location in the notices is
  normally enough. Any patch to an `async-opcua` file would have to be published.
- **Apache-2.0**: include the licence text and any NOTICE files.
- **Unicode-3.0 and BSD**: include the copyright and licence text.
- **JRE (GPLv2 with Classpath Exception)** in the runtime installers: include its licence and make the source
  available, as the JDK distributor requires.
- **Generation:** produce a complete notices file automatically (`cargo about` or `cargo deny` for Rust, a Gradle
  licence report for Kotlin) and check it in CI. Neither repository has one yet.

## 3. Provenance rules for any transfer

1. A transfer happens only after the owner has chosen licences for the destination and for the transferred code
   (decision O-1).
2. Import is a single squashed commit per component, with a `PROVENANCE.md` that records the source repository,
   source SHA, source paths, file list with SHA-256, the licence decision reference, and the audit PR. No private
   history is imported (D-003: "no unaudited private source/history").
3. Before the transfer, rescan the files for secrets (gitleaks or similar) and for customer data, and record the
   result in the import PR.
4. Never transfer:
   - signing material: keystores, private keys, certificate fingerprints of real release keys, secret names;
   - self-hosted runner identities or labels;
   - private chat content;
   - the AGLink or S7CommPlus evidence (R-004/D3.1);
   - proprietary SDKs.
5. Third-party crates stay as `Cargo.lock` dependencies. They are not vendored as copies.

**This audit transferred no source code.** It publishes module and file paths, interface names, counts and behaviour
summaries of the private repository. **Owner confirmation that publishing this level of detail is acceptable is
requested (decision O-7).** If it is not, the PR should be closed rather than merged.

## 4. Repository strategy options

| Option | How | For | Against |
| --- | --- | --- | --- |
| Git subtree of `engine/` into HMIStudio | history-preserving merge | provenance in Git | imports private history into a public repository (violates D-003 unless the whole history is audited); messy two-way sync |
| Git submodule to private HMI Forge | HMIStudio references a private repository | no copy | public CI cannot fetch a private submodule without a secret; outside contributors can never build; conflicts with D-006 hosted CI and "minimize secrets" |
| Published packages | engine published as a crate (private registry) or release binaries | clean versioning | a private registry or private releases need credentials in public CI; a public crate needs a licence first |
| Vendoring (squashed import with provenance) | copy `engine/` into `crates/hmi-engine/` at a pinned SHA | builds in hosted CI; single source for Studio; provenance file | needs a licence; HMI Forge copy must be frozen or kept in sync deliberately |
| Separate runtime repository | Kotlin runtime (and optionally the engine) live in their own repository; HMIStudio consumes released artifacts and shared contract fixtures | runtime keeps Gradle/Android toolchain and its own CI; Studio stays Rust/TypeScript | needs a release process and cross-repository contract tests |

**Recommendation (subject to O-1 and O-2):**

1. **Engine: vendor** it into HMIStudio as `crates/hmi-engine`, a squashed import with provenance, at a pinned SHA
   from HMI Forge, once it has a licence. After that, HMIStudio is the engine's home. HMI Forge freezes its `engine/`
   directory and consumes released engine binaries from HMIStudio.
2. **Kotlin runtime: separate runtime repository.** HMI Forge itself can become that repository: keep the `runtime*`,
   `hmi-ui`, `core`, `engine-client` and `storage-jvm` modules, and retire `studio-desktop` after REUSE-06. Whether it
   stays private or becomes public is decision O-2. If it stays private, HMIStudio's public CI tests against the
   **published contract fixtures** (canonical schema, package format, protocol fixtures) only, and cross-repository
   end-to-end tests run in the runtime repository.
3. **Contracts duplicated by checksum, not by reference:**
   - the canonical JSON Schema;
   - the `.hmipkg` format description and golden packages;
   - the protocol fixtures;
   - the Ed25519 vectors.

   Each repository checks the SHA-256 of its copy against a manifest, so drift fails CI.
