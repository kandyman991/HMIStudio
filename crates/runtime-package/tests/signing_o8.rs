#![allow(clippy::unwrap_used)]

use std::io::{Cursor, Read, Write};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmi_runtime_package::package::verify_signed_package_archive;
use hmi_runtime_package::signing::verify_manifest_signature;
use hmi_runtime_package::{
    build_signed_package, Compression, Ed25519TestSigner, ManifestSigner, PackageBuildError,
    PackageSignature, RuntimeSettings, SignedPackageMeta, SigningError, ED25519_ALGORITHM,
};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

fn decode_hex<const N: usize>(hex: &str) -> [u8; N] {
    assert_eq!(hex.len(), N * 2);
    let mut out = [0u8; N];
    for i in 0..N {
        out[i] = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    out
}

/// Official RFC 8032 Section 7.1 Test Vectors 1, 2, and 3.
///
/// Google Tink 1.23.0 `Ed25519Sign`/`Ed25519Verify` (JVM and Android API 26+)
/// implements standard RFC 8032 pure Ed25519. Verifying exact bit-for-bit match
/// against these official vectors establishes cryptographic wire compatibility.
#[test]
fn rfc8032_official_ed25519_vectors_match_exactly() {
    // TV1: empty message
    let tv1_seed =
        decode_hex::<32>("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
    let tv1_pub =
        decode_hex::<32>("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
    let tv1_sig = decode_hex::<64>(
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155\
         5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
    );
    let s1 = Ed25519TestSigner::from_seed("rfc8032-tv1", tv1_seed).unwrap();
    assert_eq!(s1.public_key_bytes(), tv1_pub);
    let sig1 = s1.sign_manifest(b"").unwrap();
    assert_eq!(sig1.algorithm, ED25519_ALGORITHM);
    assert_eq!(STANDARD.decode(&sig1.value).unwrap(), tv1_sig);
    verify_manifest_signature(&tv1_pub, "rfc8032-tv1", b"", &sig1).unwrap();

    // TV2: 1-byte message 0x72
    let tv2_seed =
        decode_hex::<32>("4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb");
    let tv2_pub =
        decode_hex::<32>("3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c");
    let tv2_sig = decode_hex::<64>(
        "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da\
         085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
    );
    let s2 = Ed25519TestSigner::from_seed("rfc8032-tv2", tv2_seed).unwrap();
    assert_eq!(s2.public_key_bytes(), tv2_pub);
    let sig2 = s2.sign_manifest(&[0x72]).unwrap();
    assert_eq!(STANDARD.decode(&sig2.value).unwrap(), tv2_sig);
    verify_manifest_signature(&tv2_pub, "rfc8032-tv2", &[0x72], &sig2).unwrap();

    // TV3: 2-byte message 0xaf82
    let tv3_seed =
        decode_hex::<32>("c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7");
    let tv3_pub =
        decode_hex::<32>("fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025");
    let tv3_sig = decode_hex::<64>(
        "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac\
         18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a",
    );
    let s3 = Ed25519TestSigner::from_seed("rfc8032-tv3", tv3_seed).unwrap();
    assert_eq!(s3.public_key_bytes(), tv3_pub);
    let sig3 = s3.sign_manifest(&[0xaf, 0x82]).unwrap();
    assert_eq!(STANDARD.decode(&sig3.value).unwrap(), tv3_sig);
    verify_manifest_signature(&tv3_pub, "rfc8032-tv3", &[0xaf, 0x82], &sig3).unwrap();

    // Debug output must never leak secret seed bytes.
    let dbg = format!("{s1:?}");
    assert!(!dbg.contains("9d61b19d"));
    assert!(dbg.contains("rfc8032-tv1"));
}

#[test]
fn signed_format2_package_verifies_and_refuses_tampering() {
    let project = br#"{"schemaVersion":1,"metadata":{"name":"O-8 Signed Demo","defaultScreenId":"main","targetProfiles":[]},"screens":{"main":{"id":"main","title":"Main","widthDp":640,"heightDp":480,"components":{}}},"variables":{},"externalSources":{},"externalMappings":{}}"#;
    let signer = Ed25519TestSigner::from_seed("pub-o8-test", [0x5au8; 32]).unwrap();
    let other = Ed25519TestSigner::from_seed("pub-o8-other", [0xa5u8; 32]).unwrap();

    let built = build_signed_package(
        project,
        RuntimeSettings::default(),
        SignedPackageMeta {
            version: "1.0.0".into(),
            created_at_ms: 1_700_000_000_000,
            created_by: "o8-test".into(),
            sequence: 1,
        },
        &signer,
        Compression::Deflated,
    )
    .unwrap();

    let verified = verify_signed_package_archive(
        built.archive_bytes(),
        &signer.public_key_bytes(),
        "pub-o8-test",
    )
    .unwrap();
    assert_eq!(verified.package_id, built.package_id());
    assert_eq!(verified.sequence, 1);
    assert_eq!(verified.publisher_key_id, "pub-o8-test");

    // Wrong public key refuses verification.
    assert_eq!(
        verify_signed_package_archive(
            built.archive_bytes(),
            &other.public_key_bytes(),
            "pub-o8-test",
        ),
        Err(PackageBuildError::Signing(SigningError::VerificationFailed))
    );

    // Wrong expected key id refuses verification.
    assert_eq!(
        verify_signed_package_archive(
            built.archive_bytes(),
            &signer.public_key_bytes(),
            "pub-o8-other",
        ),
        Err(PackageBuildError::Signing(SigningError::KeyIdMismatch))
    );

    // Tampered signature byte inside re-packed ZIP refuses verification.
    let mut entries = Vec::new();
    {
        let mut zip = ZipArchive::new(Cursor::new(built.archive_bytes())).unwrap();
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).unwrap();
            let name = f.name().unwrap().into_owned();
            let mut data = Vec::new();
            f.read_to_end(&mut data).unwrap();
            if name == "signature.json" {
                let mut sig: PackageSignature = serde_json::from_slice(&data).unwrap();
                let mut raw = STANDARD.decode(&sig.value).unwrap();
                raw[0] ^= 0x01;
                sig.value = STANDARD.encode(raw);
                data = serde_json::to_vec(&sig).unwrap();
            }
            entries.push((name, data));
        }
    }
    let mut tampered = Cursor::new(Vec::new());
    {
        let mut w = ZipWriter::new(&mut tampered);
        let opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0).unwrap())
            .large_file(false);
        for (name, data) in &entries {
            w.start_file(name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }
    assert_eq!(
        verify_signed_package_archive(
            &tampered.into_inner(),
            &signer.public_key_bytes(),
            "pub-o8-test",
        ),
        Err(PackageBuildError::Signing(SigningError::VerificationFailed))
    );

    // Unsupported signature algorithm refuses cleanly.
    let bad_alg = PackageSignature {
        algorithm: "RSA-PSS".into(),
        key_id: "pub-o8-test".into(),
        value: STANDARD.encode([0u8; 64]),
    };
    assert_eq!(
        verify_manifest_signature(
            &signer.public_key_bytes(),
            "pub-o8-test",
            built.manifest_json(),
            &bad_alg
        ),
        Err(SigningError::UnsupportedAlgorithm)
    );
}
