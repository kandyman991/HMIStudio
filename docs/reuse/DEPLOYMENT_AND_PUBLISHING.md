# REUSE-01 — Deployment and publishing

Status: **proposed**. This covers HMI Forge RT-01 status as observed at `92a612d`, and HMIStudio D-002 (two delivery
modes), RUST-07.

## 1. Modes

- **Mode A, standalone.** The runtime and the project are delivered together. HMIStudio D-002 calls this a
  "standalone runtime + project bundle".
- **Mode B, project-only publishing.** A project is published to a runtime that is already installed and paired.

## 2. RT-01 status

The status was checked against the GitHub merge state and the HMI Forge records. Terms:

- **completed**: merged with CI evidence recorded in HMI Forge;
- **partial**: some of the stated scope is merged;
- **proposed**: specified but not started;
- **unverified**: claimed or intended, but without evidence on the stated target.

This audit re-ran only the engine tests. Kotlin evidence comes from HMI Forge CI (`REUSE_VALIDATION_PLAN.md` §5).

| Step | Scope | Status | Evidence | Unverified |
| --- | --- | --- | --- | --- |
| RT-01A.1 | Shared `hmi-ui`, Android targets | completed | #38 merged | — |
| RT-01A.2 | `.hmipkg` format 1, desktop and Android runtimes on simulated values | completed | #40 merged | physical devices |
| RT-01B.1 | Engine on every target | completed | #41 merged; Android x86_64 emulators API 26/34; ARM64 qemu handshake | ARM hardware, physical Android (D-033 hypothesis) |
| RT-01B.2 | Live values, accounts, guarded writes, engine supervision | completed | #42 merged; emulator tests | physical panels; physical PLC (decision 14 open) |
| RT-01C.1 | Install store, rollback, power-loss recovery, kiosk, start at boot, diagnostics | completed | #43 merged | real power loss on hardware; physical panels |
| RT-01C.2 | Installers: MSI, .deb, ARM64 tarball, Android APK per ABI | completed (with limits) | #44 merged; CI run 44365768756 recorded by HMI Forge | Windows code signing (deferred by owner); Android release signing secrets not confirmed; ARM64 under qemu only |
| RT-01C.3a | Signed packages format 2 (Ed25519, Tink), trust list, admission | completed | #48 merged | — |
| RT-01C.3b | Desktop HTTPS listener, pairing, authenticated deploy, replay protection | completed (desktop, development-only) | #54 merged 2026-11-24 | Android listener (deferred to C.4); field or production use (brief §11) |
| RT-01C.3c | Studio Publish, pinned-device pairing, publisher keys UI | partial | PR #55 draft, unmerged, CI passed on `c1ec06d`, base branch not `main` | everything until merged |
| RT-01C.4 | Device credential store (DPAPI, libsecret, Keystore), Android foreground service | proposed | — | — |

## 3. Mode A in HMI Forge today

The runtime installers (C.2) and a package opened on the device from a file, USB or file share (A.2, C.1, C.3a) are
**two artifacts**. A single "runtime + project" installer was not found. A device chooses whether it accepts
unsigned format-1 files from a maintainer (`allowUntrustedFiles`, default on).

**Mode A status: partial.** All the parts exist. A combined bundle is not implemented.

Proposal for HMIStudio:

- A "standalone bundle" is the runtime installer for the target together with a signed package and a
  device-settings preset.
- Option 1: delivered as an archive, plus a first-start rule that installs the bundled package (simplest).
- Option 2: built as a per-project installer. This means jpackage or APK rebuilds per project, with signing for
  each; it is not recommended (§2 of `COMPOSE_RUNTIME_INTEGRATION.md`).
- Both still need the HMI Forge package checks and admission rules.

## 4. Mode B in HMI Forge today

1. Pair: the device shows its certificate fingerprint and an 8-digit code valid for 10 minutes. The engineer checks
   the fingerprint and enters the code. Studio sends its Ed25519 publisher key with proof of possession. The device
   stores it in `trust/` and audits the pairing.
2. Deploy: every request carries an Ed25519 signature over method, route, timestamp, nonce and body hash. A
   five-minute window and a durable nonce store prevent replay. `POST /v1/packages` stages a package for the next
   controlled start. Immediate or idle-based activation is refused.
3. Sequence anti-downgrade per project and publisher. Rollback is a separate, audited action.

The device side is merged for desktop only. The Studio side is the unmerged draft #55.

**Mode B status: partial.** Android has no listener; it imports files and verifies signatures.

## 5. Reuse plan

| Part | Disposition |
| --- | --- |
| Device side: listener, pairing, admission, store, rollback, kiosk, diagnostics | DIRECT REUSE in the runtime repository |
| Package writer (format 2) | PORT to Rust: byte-identical to `RuntimePackages.build` for the same input (deterministic entry order and timestamps); verified with a golden package and the Kotlin reader |
| Ed25519 signing | PORT with a reviewed Rust crate (for example `ed25519-dalek`); shared RFC 8032 vectors plus the pinned interop signature from `runtime/deploy/Ed25519Vectors` |
| Publisher key store | PORT with a file-format decision (O-8): compatible with HMI Forge key files and backups (PBKDF2-SHA256 210,000 rounds, AES-256-GCM), or a new format plus re-pairing |
| Deployment client (pairing, pinning, signed requests) | PORT from #55 after #55's own review, or once the protocol is frozen if #55 stays unmerged |
| Studio UI (Publish, Add device) | new in Tauri |

## 6. Constraints carried over

- Development and simulation deploys to isolated test runtimes need no production approval (HMIStudio D-004, HMI
  Forge D-031).
- Trusted deployment acceptance, release signing, physical commissioning (decision 14) and licensing stay separate
  and are not claimed.
- No signing material, keystores, secret names, runner identities or fingerprints of real keys are copied into this
  public repository.
