//! Deterministic compiler and serializer for HMI Forge format-1 (unsigned) and
//! format-2 (signed) `.hmipkg` runtime packages from frozen canonical schema v2.
//!
//! ## Safety & Policy Boundaries
//! - **B5-07 raster gate:** Every PNG and JPEG resource MUST pass full bounded
//!   pixel decoding through `hmi_legacy_import::raster` before any bytes are
//!   emitted into `project.json`.
//! - **Quarantine isolation:** Quarantined legacy-import projects are
//!   unconditionally refused for [`CompileMode::ProductionLive`]. Isolated
//!   development and simulation packages (`SIMULATED`) require no production
//!   approval (D-004).
//! - **Signing (O-8):** Uses standard RFC 8032 Ed25519 (`ed25519-dalek`) matching
//!   Google Tink `Ed25519` used by the existing Kotlin runtime on JVM and Android
//!   API 26+. This crate ships test-key signing/verification only; production
//!   publisher key storage, rotation, and device pairing remain gated on REUSE-05
//!   and the owner O-8 decision.

pub mod compiler;
pub mod package;
pub mod signing;

pub use compiler::{compile_project, CompileError, CompileMode, CompileOptions, CompiledProject};
pub use package::{
    build_signed_package, build_unsigned_package, BuiltPackage, Compression, ManifestFile,
    PackageBuildError, PackageFormat, PackageManifest, RuntimeSettings, RuntimeValues,
    SignedPackageMeta, UnsignedPackageMeta,
};
pub use signing::{
    Ed25519TestSigner, ManifestSigner, PackageSignature, SigningError, ED25519_ALGORITHM,
};
