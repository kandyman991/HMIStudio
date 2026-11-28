#![allow(clippy::unwrap_used)]

use std::io::{Cursor, Read};

use hmi_legacy_import::package::{decode_runtime_package, LegacyValues, SignatureStatus};
use hmi_runtime_package::{
    build_signed_package, build_unsigned_package, Compression, Ed25519TestSigner, ManifestSigner,
    PackageBuildError, PackageFormat, PackageSignature, RuntimeSettings, RuntimeValues,
    SignedPackageMeta, SigningError, UnsignedPackageMeta,
};
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
fn format1_and_format2_match_jdk21_golden_fixtures_byte_for_byte() {
    // 1. Format 1 (unsigned) differential comparison against JDK 21 golden fixture.
    let java_manifest_v1 = read_zip_entry(JAVA_UNSIGNED_ARCHIVE, "manifest.json");
    let java_project_v1 = read_zip_entry(JAVA_UNSIGNED_ARCHIVE, "project.json");
    let java_runtime_v1 = read_zip_entry(JAVA_UNSIGNED_ARCHIVE, "runtime.json");
    let java_decoded_v1 = decode_runtime_package(JAVA_UNSIGNED_ARCHIVE).unwrap();

    let rust_v1 = build_unsigned_package(
        &java_project_v1,
        RuntimeSettings {
            values: RuntimeValues::Simulated,
            full_screen: true,
            status_bar: true,
            keep_screen_on: true,
        },
        UnsignedPackageMeta {
            version: "1.0".into(),
            created_at_ms: 1000,
            created_by: "synthetic".into(),
        },
        Compression::Deflated,
    )
    .unwrap();

    assert_eq!(rust_v1.format(), PackageFormat::Format1Unsigned);
    assert_eq!(rust_v1.package_id(), java_decoded_v1.package_id());
    assert_eq!(rust_v1.manifest_json(), java_manifest_v1.as_slice());
    assert_eq!(rust_v1.runtime_json(), java_runtime_v1.as_slice());
    assert_eq!(
        zip_entry_names(rust_v1.archive_bytes()),
        zip_entry_names(JAVA_UNSIGNED_ARCHIVE)
    );
    assert_eq!(
        read_zip_entry(rust_v1.archive_bytes(), "manifest.json"),
        java_manifest_v1
    );
    assert_eq!(
        read_zip_entry(rust_v1.archive_bytes(), "project.json"),
        java_project_v1
    );
    assert_eq!(
        read_zip_entry(rust_v1.archive_bytes(), "runtime.json"),
        java_runtime_v1
    );

    let rust_decoded_v1 = decode_runtime_package(rust_v1.archive_bytes()).unwrap();
    assert_eq!(rust_decoded_v1.package_id(), java_decoded_v1.package_id());
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

    // 2. Format 2 (signed) differential comparison against JDK 21 golden fixture.
    let java_manifest_v2 = read_zip_entry(JAVA_SIGNED_ARCHIVE, "manifest.json");
    let java_project_v2 = read_zip_entry(JAVA_SIGNED_ARCHIVE, "project.json");
    let java_runtime_v2 = read_zip_entry(JAVA_SIGNED_ARCHIVE, "runtime.json");
    let java_sig_v2 = read_zip_entry(JAVA_SIGNED_ARCHIVE, "signature.json");
    let java_decoded_v2 = decode_runtime_package(JAVA_SIGNED_ARCHIVE).unwrap();

    let fixture_signer = FixedFixtureSigner {
        key_id: "synthetic-key",
        algorithm: "TEST",
        value: "AQID",
    };
    let rust_v2 = build_signed_package(
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
    assert_eq!(rust_v2.package_id(), java_decoded_v2.package_id());
    assert_eq!(rust_v2.manifest_json(), java_manifest_v2.as_slice());
    assert_eq!(rust_v2.runtime_json(), java_runtime_v2.as_slice());
    assert_eq!(rust_v2.signature_json().unwrap(), java_sig_v2.as_slice());
    assert_eq!(
        zip_entry_names(rust_v2.archive_bytes()),
        zip_entry_names(JAVA_SIGNED_ARCHIVE)
    );
    assert_eq!(
        read_zip_entry(rust_v2.archive_bytes(), "manifest.json"),
        java_manifest_v2
    );
    assert_eq!(
        read_zip_entry(rust_v2.archive_bytes(), "signature.json"),
        java_sig_v2
    );

    let rust_decoded_v2 = decode_runtime_package(rust_v2.archive_bytes()).unwrap();
    assert_eq!(rust_decoded_v2.package_id(), java_decoded_v2.package_id());
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
        let a = build_signed_package(
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
        let b = build_signed_package(
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
        build_unsigned_package(
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
        build_signed_package(
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

    // Empty version, empty createdBy, negative createdAtMs fail closed.
    assert!(matches!(
        build_unsigned_package(
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
        build_unsigned_package(
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
        build_unsigned_package(
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
        build_unsigned_package(
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
