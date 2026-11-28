#![allow(clippy::unwrap_used)]

use std::io::{Cursor, Read};

use crate::{
    Compression, Ed25519TestSigner, ManifestSigner, PackageBuildError, PackageFormat,
    PackageSignature, RuntimeSettings, RuntimeValues, SignedPackageMeta, SigningError,
    UnsignedPackageMeta,
};
use hmi_legacy_import::package::{decode_runtime_package, LegacyValues, SignatureStatus};
use zip::ZipArchive;

const JAVA_UNSIGNED_ARCHIVE: &[u8] =
    include_bytes!("../../legacy-import/tests/fixtures/runtime-unsigned-java.synthetic");
const JAVA_SIGNED_ARCHIVE: &[u8] =
    include_bytes!("../../legacy-import/tests/fixtures/runtime-signed-java.synthetic");

fn read_zip_entry(archive: &[u8], name: &str) -> Vec<u8> {
    let mut zip = ZipArchive::new(Cursor::new(archive)).unwrap();
    let mut file = zip.by_name(name).unwrap();
    let mut out = Vec::new();
    file.read_to_end(&mut out).unwrap();
    out
}

fn zip_entry_names(archive: &[u8]) -> Vec<String> {
    let mut zip = ZipArchive::new(Cursor::new(archive)).unwrap();
    (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().unwrap().into_owned())
        .collect()
}

struct FixedFixtureSigner {
    key_id: &'static str,
    algorithm: &'static str,
    value: &'static str,
}

impl ManifestSigner for FixedFixtureSigner {
    fn key_id(&self) -> &str {
        self.key_id
    }
    fn sign_manifest(&self, _manifest_bytes: &[u8]) -> Result<PackageSignature, SigningError> {
        Ok(PackageSignature {
            algorithm: self.algorithm.to_owned(),
            key_id: self.key_id.to_owned(),
            value: self.value.to_owned(),
        })
    }
}

#[test]
fn format1_and_format2_match_kotlin_runtime_packages_wire_format() {
    // 1. Format 1 (unsigned): 4-space pretty-printed manifest.json & runtime.json
    //    matching pinned Kotlin RuntimePackages.build (92a612d104693800d7119ee4dc0114636b19002f).
    let java_project_v1 = read_zip_entry(JAVA_UNSIGNED_ARCHIVE, "project.json");
    let java_decoded_v1 = decode_runtime_package(JAVA_UNSIGNED_ARCHIVE).unwrap();

    let expected_runtime_4space = "{\n    \"values\": \"SIMULATED\",\n    \"fullScreen\": true,\n    \"statusBar\": true,\n    \"keepScreenOn\": true\n}";

    let rust_v1 = super::build_unsigned_bytes(
        &java_project_v1,
        RuntimeSettings {
            values: RuntimeValues::Simulated,
            full_screen: true,
            status_bar: true,
            keep_screen_on: true,
        },
        UnsignedPackageMeta {
            version: " 1.0 ".into(),
            created_at_ms: 1000,
            created_by: "synthetic".into(),
        },
        Compression::Deflated,
    )
    .unwrap();

    assert_eq!(rust_v1.format(), PackageFormat::Format1Unsigned);
    assert_eq!(
        std::str::from_utf8(rust_v1.runtime_json()).unwrap(),
        expected_runtime_4space
    );
    assert_eq!(
        zip_entry_names(rust_v1.archive_bytes()),
        zip_entry_names(JAVA_UNSIGNED_ARCHIVE)
    );
    assert_eq!(
        read_zip_entry(rust_v1.archive_bytes(), "project.json"),
        java_project_v1
    );
    assert_eq!(
        read_zip_entry(rust_v1.archive_bytes(), "runtime.json"),
        expected_runtime_4space.as_bytes()
    );
    let parsed_manifest_v1: serde_json::Value =
        serde_json::from_slice(rust_v1.manifest_json()).unwrap();
    assert_eq!(parsed_manifest_v1["version"], "1.0");

    let rust_decoded_v1 = decode_runtime_package(rust_v1.archive_bytes()).unwrap();
    assert_eq!(rust_decoded_v1.package_id(), rust_v1.package_id());
    assert_eq!(
        rust_decoded_v1.project_bytes(),
        java_decoded_v1.project_bytes()
    );
    assert_eq!(
        rust_decoded_v1.signature_status(),
        SignatureStatus::Unsigned
    );
    assert_eq!(
        rust_decoded_v1.source_settings().values,
        LegacyValues::Simulated
    );

    // 2. Format 2 (signed): 4-space pretty-printed manifest.json, runtime.json, and signature.json.
    let java_project_v2 = read_zip_entry(JAVA_SIGNED_ARCHIVE, "project.json");
    let _java_decoded_v2 = decode_runtime_package(JAVA_SIGNED_ARCHIVE).unwrap();

    let fixture_signer = FixedFixtureSigner {
        key_id: "synthetic-key",
        algorithm: "TEST",
        value: "AQID",
    };
    let expected_sig_4space = "{\n    \"algorithm\": \"TEST\",\n    \"keyId\": \"synthetic-key\",\n    \"value\": \"AQID\"\n}";
    let rust_v2 = super::build_signed_bytes(
        &java_project_v2,
        RuntimeSettings::default(),
        SignedPackageMeta {
            version: "1.0".into(),
            created_at_ms: 1000,
            created_by: "synthetic".into(),
            sequence: 7,
        },
        &fixture_signer,
        Compression::Deflated,
    )
    .unwrap();

    assert_eq!(rust_v2.format(), PackageFormat::Format2Signed);
    assert_eq!(
        std::str::from_utf8(rust_v2.runtime_json()).unwrap(),
        expected_runtime_4space
    );
    assert_eq!(
        std::str::from_utf8(rust_v2.signature_json().unwrap()).unwrap(),
        expected_sig_4space
    );
    assert_eq!(
        zip_entry_names(rust_v2.archive_bytes()),
        zip_entry_names(JAVA_SIGNED_ARCHIVE)
    );
    assert_eq!(
        read_zip_entry(rust_v2.archive_bytes(), "signature.json"),
        expected_sig_4space.as_bytes()
    );

    let rust_decoded_v2 = decode_runtime_package(rust_v2.archive_bytes()).unwrap();
    assert_eq!(rust_decoded_v2.package_id(), rust_v2.package_id());
    assert_eq!(
        rust_decoded_v2.signature_status(),
        SignatureStatus::PresentUnverified
    );
}

#[test]
fn package_serialization_is_bit_for_bit_deterministic() {
    let project = read_zip_entry(JAVA_SIGNED_ARCHIVE, "project.json");
    let signer = Ed25519TestSigner::from_seed("publisher-det-1", [0x42u8; 32]).unwrap();
    for compression in [Compression::Deflated, Compression::Stored] {
        let a = super::build_signed_bytes(
            &project,
            RuntimeSettings::default(),
            SignedPackageMeta {
                version: "2.1.0".into(),
                created_at_ms: 1_700_000_000_000,
                created_by: "hmi-studio-test".into(),
                sequence: 42,
            },
            &signer,
            compression,
        )
        .unwrap();
        let b = super::build_signed_bytes(
            &project,
            RuntimeSettings::default(),
            SignedPackageMeta {
                version: "2.1.0".into(),
                created_at_ms: 1_700_000_000_000,
                created_by: "hmi-studio-test".into(),
                sequence: 42,
            },
            &signer,
            compression,
        )
        .unwrap();
        assert_eq!(a.archive_bytes(), b.archive_bytes());
        assert_eq!(a.archive_sha256(), b.archive_sha256());
        assert_eq!(a.package_id(), b.package_id());
    }
}

#[test]
fn package_builder_explicit_refusal_matrix() {
    let project = read_zip_entry(JAVA_UNSIGNED_ARCHIVE, "project.json");
    let signer = Ed25519TestSigner::from_seed("pub-key-1", [0x11u8; 32]).unwrap();

    // LIVE without external mapping fails closed.
    assert_eq!(
        super::build_unsigned_bytes(
            &project,
            RuntimeSettings {
                values: RuntimeValues::Live,
                ..RuntimeSettings::default()
            },
            UnsignedPackageMeta {
                version: "1.0".into(),
                created_at_ms: 1000,
                created_by: "test".into(),
            },
            Compression::Deflated,
        ),
        Err(PackageBuildError::LiveMappingRequired)
    );

    // Format 2 requires strictly positive sequence.
    assert_eq!(
        super::build_signed_bytes(
            &project,
            RuntimeSettings::default(),
            SignedPackageMeta {
                version: "1.0".into(),
                created_at_ms: 1000,
                created_by: "test".into(),
                sequence: 0,
            },
            &signer,
            Compression::Deflated,
        ),
        Err(PackageBuildError::InvalidMetadata(
            "format-2 sequence must be > 0"
        ))
    );

    // Empty version, >40-char version, empty createdBy, negative createdAtMs fail closed.
    assert!(matches!(
        super::build_unsigned_bytes(
            &project,
            RuntimeSettings::default(),
            UnsignedPackageMeta {
                version: "   ".into(),
                created_at_ms: 1000,
                created_by: "test".into(),
            },
            Compression::Deflated,
        ),
        Err(PackageBuildError::InvalidMetadata(_))
    ));
    assert!(matches!(
        super::build_unsigned_bytes(
            &project,
            RuntimeSettings::default(),
            UnsignedPackageMeta {
                version: "v".repeat(41),
                created_at_ms: 1000,
                created_by: "test".into(),
            },
            Compression::Deflated,
        ),
        Err(PackageBuildError::InvalidMetadata(_))
    ));
    assert!(matches!(
        super::build_unsigned_bytes(
            &project,
            RuntimeSettings::default(),
            UnsignedPackageMeta {
                version: "1.0".into(),
                created_at_ms: -1,
                created_by: "test".into(),
            },
            Compression::Deflated,
        ),
        Err(PackageBuildError::InvalidMetadata(_))
    ));
    assert!(matches!(
        super::build_unsigned_bytes(
            &project,
            RuntimeSettings::default(),
            UnsignedPackageMeta {
                version: "1.0".into(),
                created_at_ms: 1000,
                created_by: "".into(),
            },
            Compression::Deflated,
        ),
        Err(PackageBuildError::InvalidMetadata(_))
    ));

    // Invalid project JSON fails closed.
    assert_eq!(
        super::build_unsigned_bytes(
            b"{not json",
            RuntimeSettings::default(),
            UnsignedPackageMeta {
                version: "1.0".into(),
                created_at_ms: 1000,
                created_by: "test".into(),
            },
            Compression::Deflated,
        ),
        Err(PackageBuildError::InvalidProjectJson)
    );
}
