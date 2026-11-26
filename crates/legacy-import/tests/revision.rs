use hmi_legacy_import::revision::*;
use hmi_legacy_import::{import_portable, MAX_INPUT_BYTES};
use sha2::{Digest, Sha256};

const DESCRIPTOR: &[u8] = br#"{"format":"hmistudio-project-bundle","formatVersion":1}"#;
const ID: &str = "0000000002-1234abcd";
const HEAD: &str = r#"{"formatVersion":1,"sequence":7,"currentRevisionId":"0000000002-1234abcd","undo":["0000000001-abcdef00"],"redo":[],"nextRevisionNumber":3,"recentTransactionIds":["synthetic"]}"#;
const PROJECT: &str = r#"{ "schemaVersion":1, "metadata":{"name":"Revision sample","defaultScreenId":"main"}, "screens":{"main":{"id":"main","title":"Main","widthDp":640,"heightDp":480,"components":{}}}, "variables":{}, "externalSources":{}, "externalMappings":{} }"#;

fn record(body: &str) -> Vec<u8> {
    format!("{:x}\n{body}", Sha256::digest(body.as_bytes())).into_bytes()
}
fn revision(project: &str) -> Vec<u8> {
    record(&format!(
        r#"{{"formatVersion":1,"id":"{ID}","number":2,"parentId":"0000000001-abcdef00","kind":"TRANSACTION","label":"Original synthetic history","transaction":{{"version":1,"id":"synthetic","baseRevision":6,"operations":[],"label":"NEVER-TRANSFER"}},"project":{project}}}"#
    ))
}

#[test]
fn selected_revision_preserves_exact_project_bytes_and_deterministic_portable_result() {
    let head = record(HEAD);
    let s = select_current_revision(DESCRIPTOR, &head).unwrap();
    assert_eq!(s.revision_id(), ID);
    assert_eq!(s.revision_relative_path(), format!("revisions/{ID}.json"));
    assert!(s.matches_source(DESCRIPTOR, &head));
    assert!(!s.matches_source(DESCRIPTOR, &record(&HEAD.replace("7", "8"))));
    assert!(!s.matches_source(b"{}", &head));
    let bytes = revision(PROJECT);
    let v = decode_current_revision(&s, &bytes).unwrap();
    assert_eq!(v.project_bytes(), PROJECT.as_bytes());
    assert_eq!(v.revision_sha256(), format!("{:x}", Sha256::digest(&bytes)));
    let direct = import_portable(PROJECT.as_bytes());
    let imported = import_portable(v.project_bytes());
    assert!(
        imported.report.has_complete_mapping(),
        "{:?}",
        imported.report
    );
    assert_eq!(direct.report, imported.report);
    assert_eq!(direct.project, imported.project);
    assert!(!serde_json::to_string(&imported.report)
        .unwrap()
        .contains("NEVER-TRANSFER"));
}

#[test]
fn descriptor_head_and_path_admission_fail_closed() {
    assert_eq!(
        select_current_revision(
            DESCRIPTOR,
            &record(&HEAD.replace("formatVersion\":1", "formatVersion\":2"))
        )
        .err(),
        Some(RevisionError::UnsupportedStorageVersion)
    );
    for id in [
        "../secret",
        "0000000002-1234ABCD",
        "0000000002-1234abcd/..",
        "0000000000-1234abcd",
    ] {
        assert!(select_current_revision(DESCRIPTOR, &record(&HEAD.replace(ID, id))).is_err());
    }
    for bad in [
        HEAD.replace("\"sequence\":7", "\"sequence\":-1"),
        HEAD.replace("\"nextRevisionNumber\":3", "\"nextRevisionNumber\":2"),
        HEAD.replace("\"sequence\":7", "\"sequence\":7,\"sequence\":8"),
        HEAD.replace("0000000001-abcdef00", "../secret"),
        HEAD.replace("\"sequence\":7", "\"sequence\":7,\"unknown\":true"),
    ] {
        assert!(select_current_revision(DESCRIPTOR, &record(&bad)).is_err());
    }
    for marker in [
        br#"{"format":"foreign","formatVersion":1}"#.as_slice(),
        br#"{"format":"hmistudio-project-bundle","formatVersion":2}"#,
        br#"{"format":"hmistudio-project-bundle","formatVersion":1,"formatVersion":1}"#,
        b"{}",
    ] {
        assert!(select_current_revision(marker, &record(HEAD)).is_err());
    }
    let mut broken = record(HEAD);
    broken.push(b' ');
    assert_eq!(
        select_current_revision(DESCRIPTOR, &broken).err(),
        Some(RevisionError::InvalidChecksum)
    );
    assert_eq!(
        select_current_revision(DESCRIPTOR, &vec![0; MAX_HEAD_BYTES + 1]).err(),
        Some(RevisionError::SizeLimit)
    );
    assert_eq!(
        select_current_revision(&vec![0; MAX_DESCRIPTOR_BYTES + 1], &record(HEAD)).err(),
        Some(RevisionError::SizeLimit)
    );
}

#[test]
fn selected_record_must_be_intact_matching_and_bounded() {
    let s = select_current_revision(DESCRIPTOR, &record(HEAD)).unwrap();
    let good = revision(PROJECT);
    let mut damaged = good.clone();
    damaged.push(b' ');
    assert_eq!(
        decode_current_revision(&s, &damaged).err(),
        Some(RevisionError::InvalidChecksum)
    );
    let body = std::str::from_utf8(&good[65..]).unwrap();
    for bad in [
        body.replace(ID, "0000000002-1234abce"),
        body.replace("\"number\":2", "\"number\":3"),
        body.replace("\"formatVersion\":1", "\"formatVersion\":2"),
        body.replace("\"kind\":\"TRANSACTION\"", "\"kind\":\"UNKNOWN\""),
        body.replace("\"number\":2", "\"number\":2,\"number\":2"),
        body.replace("0000000001-abcdef00", "../secret"),
        body.replace("\"label\":\"NEVER-TRANSFER\"", "\"label\":1,\"label\":2"),
        body.replace("\"number\":2", "\"number\":2,\"unknown\":true"),
    ] {
        assert!(decode_current_revision(&s, &record(&bad)).is_err());
    }
    assert!(
        decode_current_revision(&s, &revision(r#"{"schemaVersion":1,"schemaVersion":2}"#)).is_err()
    );
    assert!(decode_current_revision(&s, &revision("null")).is_err());
    let oversize = format!(r#"{{"padding":"{}"}}"#, "x".repeat(MAX_INPUT_BYTES));
    assert_eq!(
        decode_current_revision(&s, &revision(&oversize)).err(),
        Some(RevisionError::SizeLimit)
    );
    assert_eq!(
        decode_current_revision(&s, &vec![0; MAX_REVISION_BYTES + 1]).err(),
        Some(RevisionError::SizeLimit)
    );
    let unsupported = revision(r#"{"schemaVersion":11}"#);
    let snapshot = decode_current_revision(&s, &unsupported).unwrap();
    assert!(import_portable(snapshot.project_bytes()).project.is_none());
}

#[test]
fn bounds_history_metadata_and_json_structure() {
    let head = record(HEAD);
    let s = select_current_revision(DESCRIPTOR, &head).unwrap();
    let mut h: serde_json::Value = serde_json::from_str(HEAD).unwrap();
    h["sequence"] = 0.into();
    assert_eq!(
        select_current_revision(DESCRIPTOR, &record(&h.to_string())).err(),
        Some(RevisionError::InvalidSequence)
    );
    h["sequence"] = 7.into();
    h["recentTransactionIds"] = serde_json::json!(vec!["x"; 513]);
    assert_eq!(
        select_current_revision(DESCRIPTOR, &record(&h.to_string())).err(),
        Some(RevisionError::SizeLimit)
    );
    h["recentTransactionIds"] = serde_json::json!([]);
    h["undo"] = serde_json::json!(vec!["0000000001-abcdef00"; 201]);
    assert_eq!(
        select_current_revision(DESCRIPTOR, &record(&h.to_string())).err(),
        Some(RevisionError::SizeLimit)
    );

    let good = revision(PROJECT);
    let body = std::str::from_utf8(&good[65..]).unwrap();
    let many_nodes = format!("[{}0]", "0,".repeat(200_000));
    let large = body.replace(r#""NEVER-TRANSFER""#, &many_nodes);
    assert_eq!(
        decode_current_revision(&s, &record(&large)).err(),
        Some(RevisionError::SizeLimit)
    );
    let nested = format!("{}0{}", "[".repeat(150), "]".repeat(150));
    assert!(
        decode_current_revision(&s, &record(&body.replace(r#""NEVER-TRANSFER""#, &nested)))
            .is_err()
    );
    let escaped_duplicate =
        body.replace(r#""label":"NEVER-TRANSFER""#, r#""label":1,"la\u0062el":2"#);
    assert!(decode_current_revision(&s, &record(&escaped_duplicate)).is_err());
}

#[test]
fn revision_kind_requires_the_pinned_writer_record_shape() {
    let head = record(HEAD);
    let s = select_current_revision(DESCRIPTOR, &head).unwrap();
    let good = revision(PROJECT);
    let body = std::str::from_utf8(&good[65..]).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(body).unwrap();
    value["transaction"] = serde_json::Value::Null;
    assert!(decode_current_revision(&s, &record(&value.to_string())).is_err());
    value["kind"] = "RESTORE".into();
    value["restoredFromId"] = "0000000001-abcdef00".into();
    assert!(decode_current_revision(&s, &record(&value.to_string())).is_ok());
    value["parentId"] = serde_json::Value::Null;
    assert!(decode_current_revision(&s, &record(&value.to_string())).is_err());

    let first = record(
        &HEAD
            .replace(ID, "0000000001-abcdef00")
            .replace("\"nextRevisionNumber\":3", "\"nextRevisionNumber\":2")
            .replace("\"undo\":[\"0000000001-abcdef00\"]", "\"undo\":[]"),
    );
    let first_selection = select_current_revision(DESCRIPTOR, &first).unwrap();
    value["kind"] = "IMPORTED".into();
    value["number"] = 1.into();
    value["id"] = "0000000001-abcdef00".into();
    value["restoredFromId"] = serde_json::Value::Null;
    assert!(decode_current_revision(&first_selection, &record(&value.to_string())).is_ok());
    value["kind"] = "CREATED".into();
    assert!(decode_current_revision(&first_selection, &record(&value.to_string())).is_ok());
}
