use hmi_legacy_import::package::{
    decode_runtime_package, LegacyValues, PackageError, SignatureStatus, MAX_PACKAGE_BYTES,
};
use hmi_legacy_import::{import_portable, MAX_INPUT_BYTES};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Write};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

const PROJECT: &[u8] = br#"{"schemaVersion":1,"metadata":{"name":"Synthetic","defaultScreenId":"main"},"screens":{"main":{"id":"main","title":"Main","widthDp":640,"heightDp":480,"components":{}}},"variables":{},"externalSources":{},"externalMappings":{}}"#;
const SETTINGS: &[u8] =
    br#"{"values":"SIMULATED","fullScreen":true,"statusBar":true,"keepScreenOn":true}"#;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn manifest(project: &[u8], settings: &[u8]) -> Value {
    let a = digest(project);
    let b = digest(settings);
    let id = digest(
        format!(
            "project.json:{a}:{}\nruntime.json:{b}:{}",
            project.len(),
            settings.len()
        )
        .as_bytes(),
    );
    json!({
        "format":1, "packageId":id, "projectName":"Synthetic", "version":"1",
        "schemaVersion":1, "createdAtMs":1000, "createdBy":"synthetic",
        "startupScreen":"main", "files":[
            {"path":"project.json","sha256":a,"sizeBytes":project.len()},
            {"path":"runtime.json","sha256":b,"sizeBytes":settings.len()}
        ]
    })
}

fn archive(entries: &[(&str, &[u8])], compression: CompressionMethod) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zip.start_file(
            *name,
            SimpleFileOptions::default().compression_method(compression),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn package(m: &Value, project: &[u8], settings: &[u8]) -> Vec<u8> {
    let text = serde_json::to_vec(m).unwrap();
    archive(
        &[
            ("manifest.json", &text),
            ("project.json", project),
            ("runtime.json", settings),
        ],
        CompressionMethod::Stored,
    )
}

fn u16_at(bytes: &[u8], at: usize) -> usize {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) as usize
}

fn u32_at(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}

fn central_headers(bytes: &[u8]) -> Vec<usize> {
    let eocd = bytes.len() - 22;
    let count = u16_at(bytes, eocd + 10);
    let mut at = u32_at(bytes, eocd + 16);
    (0..count)
        .map(|_| {
            let header = at;
            at += 46 + u16_at(bytes, at + 28) + u16_at(bytes, at + 30) + u16_at(bytes, at + 32);
            header
        })
        .collect()
}

#[test]
fn original_java_streaming_packages_decode_without_granting_trust() {
    for (bytes, trust) in [
        (
            include_bytes!("fixtures/runtime-unsigned-java.synthetic").as_slice(),
            SignatureStatus::Unsigned,
        ),
        (
            include_bytes!("fixtures/runtime-signed-java.synthetic").as_slice(),
            SignatureStatus::PresentUnverified,
        ),
    ] {
        let p = decode_runtime_package(bytes).unwrap();
        assert_eq!(p.signature_status(), trust);
        assert_eq!(p.manifest_schema_version(), 1);
        assert_eq!(p.archive_sha256(), digest(bytes));
        assert_eq!(p.source_settings().values, LegacyValues::Simulated);
        assert!(p.source_settings().full_screen);
        let direct = import_portable(p.project_bytes());
        assert!(direct.report.has_complete_mapping(), "{:?}", direct.report);
        assert_eq!(direct.report.source_sha256, digest(p.project_bytes()));
        assert!(p
            .project_bytes()
            .windows("Synthetic café".len())
            .any(|x| x == "Synthetic café".as_bytes()));
    }
}

#[test]
fn stored_and_deflated_transport_preserve_portable_bytes_and_inert_settings() {
    for method in [CompressionMethod::Stored, CompressionMethod::Deflated] {
        let settings =
            br#"{"values":"SIMULATED","fullScreen":false,"statusBar":true,"keepScreenOn":false}"#;
        let m = manifest(PROJECT, settings);
        let text = serde_json::to_vec(&m).unwrap();
        let bytes = archive(
            &[
                ("manifest.json", &text),
                ("project.json", PROJECT),
                ("runtime.json", settings),
            ],
            method,
        );
        let p = decode_runtime_package(&bytes).unwrap();
        assert_eq!(p.project_bytes(), PROJECT);
        assert_eq!(p.package_id(), m["packageId"].as_str().unwrap());
        assert_eq!(p.signature_status(), SignatureStatus::Unsigned);
        assert_eq!(p.source_settings().values, LegacyValues::Simulated);
        assert!(!p.source_settings().full_screen);
        assert!(import_portable(p.project_bytes())
            .report
            .has_complete_mapping());
    }
}

#[test]
fn malformed_manifest_content_and_signing_metadata_fail_closed() {
    let m = manifest(PROJECT, SETTINGS);
    for (field, value) in [
        ("format", json!(3)),
        ("packageId", json!("wrong")),
        ("schemaVersion", json!(2)),
        ("projectName", json!("Other")),
        ("startupScreen", json!("other")),
        ("sequence", json!(1)),
        ("publisherKeyId", json!("key")),
        ("files", json!([])),
    ] {
        let mut bad = m.clone();
        bad[field] = value;
        assert!(
            decode_runtime_package(&package(&bad, PROJECT, SETTINGS)).is_err(),
            "{field}"
        );
    }
    for key in ["path", "sha256", "sizeBytes"] {
        let mut bad = m.clone();
        bad["files"][0][key] = json!("invalid");
        assert!(decode_runtime_package(&package(&bad, PROJECT, SETTINGS)).is_err());
    }
    let mut duplicate = m.clone();
    duplicate["files"][1] = duplicate["files"][0].clone();
    assert!(decode_runtime_package(&package(&duplicate, PROJECT, SETTINGS)).is_err());
    let mut changed_project = PROJECT.to_vec();
    changed_project.push(b' ');
    assert!(decode_runtime_package(&package(&m, &changed_project, SETTINGS)).is_err());
    let mut duplicate_json = serde_json::to_string(&m).unwrap();
    duplicate_json.insert_str(1, "\"format\":1,");
    assert!(decode_runtime_package(&archive(
        &[
            ("manifest.json", duplicate_json.as_bytes()),
            ("project.json", PROJECT),
            ("runtime.json", SETTINGS)
        ],
        CompressionMethod::Stored
    ))
    .is_err());
}

#[test]
fn strict_runtime_json_and_project_metadata_are_checked() {
    for settings in [
        br#"{"values":"UNKNOWN","fullScreen":true,"statusBar":true,"keepScreenOn":true}"#.as_slice(),
        br#"{"values":"SIMULATED","values":"LIVE","fullScreen":true,"statusBar":true,"keepScreenOn":true}"#,
        br#"{"values":"SIMULATED","fullScreen":true,"statusBar":true,"keepScreenOn":true,"extra":false}"#,
    ] {
        assert_eq!(decode_runtime_package(&package(&manifest(PROJECT, settings), PROJECT, settings)).err(), Some(PackageError::InvalidSettings));
    }
    let project = br#"{"schemaVersion":1,"metadata":{"name":"Synthetic"},"screens":{"definition:temp":{},"z":{},"a":{}}}"#;
    let mut m = manifest(project, SETTINGS);
    m["startupScreen"] = json!("z");
    assert!(decode_runtime_package(&package(&m, project, SETTINGS)).is_ok());
    m["startupScreen"] = json!("a");
    assert_eq!(
        decode_runtime_package(&package(&m, project, SETTINGS)).err(),
        Some(PackageError::InvalidProject)
    );
    let duplicate = br#"{"schemaVersion":1,"metadata":{"name":"Synthetic","name":"Other"},"screens":{"main":{}}}"#;
    assert!(decode_runtime_package(&package(
        &manifest(duplicate, SETTINGS),
        duplicate,
        SETTINGS
    ))
    .is_err());
}

#[test]
fn archive_allowlist_and_raw_header_consistency_are_enforced() {
    let text = serde_json::to_vec(&manifest(PROJECT, SETTINGS)).unwrap();
    for name in [
        "../evil.json",
        "C:/bad.json",
        "project.json/",
        "Project.json",
        "a\\b",
        "signature.json",
    ] {
        assert!(
            decode_runtime_package(&archive(
                &[
                    ("manifest.json", &text),
                    ("project.json", PROJECT),
                    ("runtime.json", SETTINGS),
                    (name, b"bad")
                ],
                CompressionMethod::Stored
            ))
            .is_err(),
            "{name}"
        );
    }
    let valid = package(&manifest(PROJECT, SETTINGS), PROJECT, SETTINGS);
    let headers = central_headers(&valid);
    let runtime = headers[2];
    let local = u32_at(&valid, runtime + 42);
    let mut duplicate = valid.clone();
    duplicate[runtime + 46..runtime + 58].copy_from_slice(b"project.json");
    duplicate[local + 30..local + 42].copy_from_slice(b"project.json");
    assert_eq!(
        decode_runtime_package(&duplicate).err(),
        Some(PackageError::UnexpectedEntry)
    );
    let mut mismatch = valid.clone();
    mismatch[local + 30..local + 42].copy_from_slice(b"../evil.json");
    assert!(decode_runtime_package(&mismatch).is_err());
    let mut flags = valid.clone();
    flags[runtime + 8] |= 1;
    flags[local + 6] |= 1;
    assert!(decode_runtime_package(&flags).is_err());
    let mut overlap = valid.clone();
    overlap[runtime + 42..runtime + 46].copy_from_slice(&0u32.to_le_bytes());
    assert!(decode_runtime_package(&overlap).is_err());
    let mut crc = valid.clone();
    let data = local + 30 + u16_at(&crc, local + 26) + u16_at(&crc, local + 28);
    crc[data] ^= 1;
    assert!(decode_runtime_package(&crc).is_err());
}

#[test]
fn bounded_archive_sizes_and_forged_inflation_claims_are_refused() {
    assert_eq!(
        decode_runtime_package(&vec![0; MAX_PACKAGE_BYTES + 1]).err(),
        Some(PackageError::SizeLimit)
    );
    let big = vec![b' '; MAX_INPUT_BYTES + 1];
    let text = serde_json::to_vec(&manifest(&big, SETTINGS)).unwrap();
    let bomb = archive(
        &[
            ("manifest.json", &text),
            ("project.json", &big),
            ("runtime.json", SETTINGS),
        ],
        CompressionMethod::Deflated,
    );
    assert_eq!(
        decode_runtime_package(&bomb).err(),
        Some(PackageError::SizeLimit)
    );
    let mut forged = bomb;
    let h = central_headers(&forged)[1];
    let local = u32_at(&forged, h + 42);
    // Both headers claim a tiny expansion. Real decompressed output must
    // still be bounded and rejected, not silently truncated to the claim.
    forged[h + 24..h + 28].copy_from_slice(&1u32.to_le_bytes());
    forged[local + 22..local + 26].copy_from_slice(&1u32.to_le_bytes());
    assert!(decode_runtime_package(&forged).is_err());
    let valid = package(&manifest(PROJECT, SETTINGS), PROJECT, SETTINGS);
    let mut trailer = valid.clone();
    trailer.push(0);
    assert!(decode_runtime_package(&trailer).is_err());
    let mut prefix = vec![0];
    prefix.extend(valid.clone());
    assert!(decode_runtime_package(&prefix).is_err());
    for end in 0..valid.len() {
        assert!(decode_runtime_package(&valid[..end]).is_err(), "{end}");
    }
}

#[test]
fn signed_envelope_changes_and_descriptor_tampering_are_refused() {
    let valid = include_bytes!("fixtures/runtime-signed-java.synthetic");
    let mut zip = zip::ZipArchive::new(Cursor::new(valid)).unwrap();
    let mut content = Vec::new();
    for index in 0..zip.len() {
        use std::io::Read;
        let mut file = zip.by_index(index).unwrap();
        let name = file.name().unwrap().into_owned();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        content.push((name, bytes));
    }
    for invalid in [
        br#"{"algorithm":"TEST","keyId":"other","value":"AQID"}"#.as_slice(),
        br#"{"algorithm":"TEST","keyId":"synthetic-key","value":"?"}"#,
        br#"{"algorithm":"","keyId":"synthetic-key","value":"AQID"}"#,
    ] {
        let entries = [
            ("manifest.json", content[0].1.as_slice()),
            ("project.json", content[1].1.as_slice()),
            ("runtime.json", content[2].1.as_slice()),
            ("signature.json", invalid),
        ];
        assert_eq!(
            decode_runtime_package(&archive(&entries, CompressionMethod::Stored)).err(),
            Some(PackageError::InvalidSignature)
        );
    }
    let mut m: Value = serde_json::from_slice(&content[0].1).unwrap();
    m["sequence"] = json!(8);
    let changed = serde_json::to_vec(&m).unwrap();
    assert!(decode_runtime_package(&archive(
        &[
            ("manifest.json", &changed),
            ("project.json", &content[1].1),
            ("runtime.json", &content[2].1),
            ("signature.json", &content[3].1)
        ],
        CompressionMethod::Stored
    ))
    .is_err());
    let mut broken = valid.to_vec();
    let central = central_headers(&broken)[0];
    let local = u32_at(&broken, central + 42);
    let data = local + 30 + u16_at(&broken, local + 26) + u16_at(&broken, local + 28);
    let descriptor = data + u32_at(&broken, central + 20);
    assert_eq!(&broken[descriptor..descriptor + 4], b"PK\x07\x08");
    broken[descriptor + 4] ^= 1;
    assert!(decode_runtime_package(&broken).is_err());
}

#[test]
fn deflate_stream_must_consume_the_entire_declared_payload() {
    let text = serde_json::to_vec(&manifest(PROJECT, SETTINGS)).unwrap();
    let valid = archive(
        &[
            ("manifest.json", &text),
            ("project.json", PROJECT),
            ("runtime.json", SETTINGS),
        ],
        CompressionMethod::Deflated,
    );
    assert!(decode_runtime_package(&valid).is_ok());
    let central = u32_at(&valid, valid.len() - 22 + 16);
    let last = central_headers(&valid)[2];
    let local = u32_at(&valid, last + 42);
    let compressed = u32_at(&valid, last + 20);
    assert_eq!(u16_at(&valid, local + 6) & 8, 0);
    for append in [true, false] {
        let mut bad = valid.clone();
        let delta = if append {
            bad.insert(central, 0x42);
            1isize
        } else {
            bad.remove(central - 1);
            -1isize
        };
        let updated = (compressed as isize + delta) as u32;
        let shifted = (last as isize + delta) as usize;
        let eocd = bad.len() - 22;
        bad[local + 18..local + 22].copy_from_slice(&updated.to_le_bytes());
        bad[shifted + 20..shifted + 24].copy_from_slice(&updated.to_le_bytes());
        bad[eocd + 16..eocd + 20]
            .copy_from_slice(&((central as isize + delta) as u32).to_le_bytes());
        assert!(decode_runtime_package(&bad).is_err(), "append={append}");
    }
}

#[test]
fn package_schema_is_the_feature_floor_not_the_declared_version() {
    let baseline: Value = serde_json::from_slice(PROJECT).unwrap();
    // Raw shape fixtures isolate package feature-floor semantics. Full
    // portable validation is independent and still required for admission.
    for (version, pointer, feature) in [
        (
            2,
            "/screens/main/layout",
            json!({"root":{"node":"stack","direction":"ROW"}}),
        ),
        (
            3,
            "/screens/main/layout",
            json!({"root":{"node":"stack","direction":"GRID"}}),
        ),
        (
            4,
            "/screens/main/components",
            json!({"c":{"expressions":{"value":"x"}}}),
        ),
        (5, "/screenOrder", json!(["main"])),
        (6, "/componentDefinitions", json!({"d":{"kind":"SYMBOL"}})),
        (7, "/theme", json!({})),
        (8, "/security", json!({})),
        (
            9,
            "/externalSources",
            json!({"s":{"commissioningAllowed":true}}),
        ),
        (10, "/enumerations", json!({"e":{}})),
        (
            10,
            "/screens/main/components",
            json!({"c":{"actions":{"press":[{"action":"navigate.back"},{"action":"navigate.back"}]}}}),
        ),
    ] {
        let mut source = baseline.clone();
        source["schemaVersion"] = json!(10);
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        source.pointer_mut(parent).unwrap()[key] = feature;
        let bytes = serde_json::to_vec(&source).unwrap();
        let mut m = manifest(&bytes, SETTINGS);
        m["schemaVersion"] = json!(version);
        assert!(
            decode_runtime_package(&package(&m, &bytes, SETTINGS)).is_ok(),
            "{pointer}: {version}"
        );
        m["schemaVersion"] = json!(version - 1);
        assert_eq!(
            decode_runtime_package(&package(&m, &bytes, SETTINGS)).err(),
            Some(PackageError::InvalidProject),
            "{pointer}: understated"
        );
        if version < 10 {
            m["schemaVersion"] = json!(version + 1);
            assert_eq!(
                decode_runtime_package(&package(&m, &bytes, SETTINGS)).err(),
                Some(PackageError::InvalidProject),
                "{pointer}: overstated"
            );
        }
    }
    let mut source = baseline;
    source["schemaVersion"] = json!(10);
    let bytes = serde_json::to_vec(&source).unwrap();
    assert!(
        decode_runtime_package(&package(&manifest(&bytes, SETTINGS), &bytes, SETTINGS)).is_ok()
    );
}

#[test]
fn live_package_requires_a_source_and_variable_reference() {
    let settings = br#"{"values":"LIVE","fullScreen":false,"statusBar":true,"keepScreenOn":false}"#;
    assert_eq!(
        decode_runtime_package(&package(&manifest(PROJECT, settings), PROJECT, settings)).err(),
        Some(PackageError::InvalidSettings)
    );
    let mut source: Value = serde_json::from_slice(PROJECT).unwrap();
    source["externalSources"] = json!({"s":{}});
    source["variables"] = json!({"v":{}});
    for (device, variable, valid) in [
        ("s", "v", true),
        ("other", "v", false),
        ("s", "other", false),
    ] {
        source["externalMappings"] = json!({"m":{"sourceId":device,"internalVariableId":variable}});
        let bytes = serde_json::to_vec(&source).unwrap();
        let result =
            decode_runtime_package(&package(&manifest(&bytes, settings), &bytes, settings));
        if valid {
            let decoded = result.unwrap();
            assert_eq!(decoded.source_settings().values, LegacyValues::Live);
            // The transport gate must not be confused with portable validation.
            assert!(import_portable(decoded.project_bytes()).project.is_none());
        } else {
            assert_eq!(result.err(), Some(PackageError::InvalidSettings));
        }
    }
}

#[test]
fn legacy_component_slot_alignment_requires_schema_three() {
    let baseline: Value = serde_json::from_slice(PROJECT).unwrap();
    for (field, value) in [
        ("maxHeightDp", json!(100)),
        ("alignH", json!("CENTER")),
        ("alignV", json!("MIDDLE")),
    ] {
        let mut source = baseline.clone();
        source["schemaVersion"] = json!(3);
        source["screens"]["main"]["layout"] = json!({
            "root": {
                "node": "stack",
                "direction": "ROW",
                "children": [{"node":"component","componentId":"c"}]
            }
        });
        source["screens"]["main"]["layout"]["root"]["children"][0][field] = value;
        let bytes = serde_json::to_vec(&source).unwrap();
        let mut m = manifest(&bytes, SETTINGS);
        m["schemaVersion"] = json!(2);
        assert_eq!(
            decode_runtime_package(&package(&m, &bytes, SETTINGS)).err(),
            Some(PackageError::InvalidProject),
            "{field}"
        );
        m["schemaVersion"] = json!(3);
        assert!(decode_runtime_package(&package(&m, &bytes, SETTINGS)).is_ok());
    }
}
