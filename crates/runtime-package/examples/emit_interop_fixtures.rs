//! Emits deterministic synthetic format-1 and format-2 `.hmipkg` packages
//! compiled through the actual `hmi-runtime-package` compiler and public
//! package writers for cross-runtime interoperability validation.
//!
//! Usage:
//!   cargo run --locked --example emit_interop_fixtures -- <output-dir>

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmi_project_model::id::{
    ConnectionId, EnumerationId, NodeId, PermissionId, ProjectId, ResourceId, RoleId, ScreenId,
    TagId, UnitId,
};
use hmi_project_model::load_project;
use hmi_project_model::model::v2::{
    CommandKind, ConnectionPolicy, DriverKindV2, Permission, Role, SecurityPolicy, TagWritePolicy,
    WriteLimits,
};
use hmi_project_model::model::v2_bindings::{EnumerationDefinition, EnumerationEntry};
use hmi_project_model::model::{
    ComponentKind, Connection, DataType, DriverKind, Element, EngineeringUnit, LocalizedText, Node,
    Position, Project, Resource, ResourceKind, Screen, ScreenKind, Size, Tag, TagAccess,
};
use hmi_runtime_package::{
    build_signed_package, build_unsigned_package, compile_project, BuiltPackage, CompileMode,
    CompileOptions, Compression, Ed25519TestSigner, RuntimeSettings, RuntimeValues,
    SignedPackageMeta, UnsignedPackageMeta,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

// Official RFC 8032 Section 7.1 Test Vector 1 & 2 seeds (public test constants only).
const TV1_SEED: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
const TV2_SEED: [u8; 32] = [
    0x4c, 0xcd, 0x08, 0x9b, 0x28, 0xff, 0x96, 0xda, 0x9d, 0xb6, 0xc3, 0x46, 0xec, 0x11, 0x4e, 0x0f,
    0x5b, 0x8a, 0x31, 0x9f, 0x35, 0xab, 0xa6, 0x24, 0xda, 0x8c, 0xf6, 0xed, 0x4f, 0xb8, 0xa6, 0xfb,
];

// Valid 1x1 red RGBA PNG (70 bytes).
const VALID_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

// Valid 2x3 baseline JPEG.
const VALID_JPEG_B64: &str = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAMCAgICAgMCAgIDAwMDBAYEBAQEBAgGBgUGCQgKCgkICQkKDA8MCgsOCwkJDRENDg8QEBEQCgwSExIQEw8QEBD/wAALCAADAAIBAREA/8QAFAABAAAAAAAAAAAAAAAAAAAACf/EABQQAQAAAAAAAAAAAAAAAAAAAAD/2gAIAQEAAD8AVN//2Q==";

const VALID_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect x="0" y="0" width="8" height="8" fill="#00aa55"/></svg>"##;

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn build_industrial_live_v10() -> Result<(Project, BTreeMap<String, Vec<u8>>), String> {
    let mut root = Node::new(
        NodeId::new("n-root").map_err(|e| e.to_string())?,
        Element::Absolute {},
    );
    root.layout.width = Size::fixed(800.0);
    root.layout.height = Size::fixed(480.0);

    let mut lbl = Node::new(
        NodeId::new("n-lbl").map_err(|e| e.to_string())?,
        Element::Component {
            kind: ComponentKind::Text,
        },
    );
    lbl.layout.position = Some(Position { x: 16.0, y: 24.0 });
    lbl.layout.width = Size::fixed(200.0);
    lbl.layout.height = Size::fixed(36.0);
    lbl.properties.insert("text".into(), json!("Boiler Ready"));
    root.children.push(lbl);

    let screen = Screen {
        id: ScreenId::new("scr-main").map_err(|e| e.to_string())?,
        name: "Main".into(),
        kind: ScreenKind::Page,
        title: Some(LocalizedText::Literal("Main Screen".into())),
        design_profile: None,
        events: Vec::new(),
        root,
        extensions: BTreeMap::new(),
    };
    let mut project = Project::new(
        ProjectId::new("prj-boiler").map_err(|e| e.to_string())?,
        "Boiler HMI",
        screen,
    );

    project.units.push(EngineeringUnit {
        id: UnitId::new("u-bar").map_err(|e| e.to_string())?,
        symbol: "bar".into(),
        name: Some(LocalizedText::Literal("Bar".into())),
        quantity: Some("pressure".into()),
    });
    project.connections.push(Connection {
        id: ConnectionId::new("conn-plc").map_err(|e| e.to_string())?,
        name: "Line 1 S7".into(),
        driver: DriverKind::SiemensS7,
        endpoint: Some("10.0.0.10:102".into()),
        parameters: BTreeMap::from([("rack".into(), json!(0)), ("slot".into(), json!(1))]),
        credential_ref: None,
        extensions: BTreeMap::new(),
    });
    project.v2.industrial.connections.push(ConnectionPolicy {
        connection: ConnectionId::new("conn-plc").map_err(|e| e.to_string())?,
        driver: DriverKindV2::SiemensS7,
        commissioning_allowed: false,
    });
    project.tags.push(Tag {
        id: TagId::new("tag-pressure").map_err(|e| e.to_string())?,
        name: "Boiler/Pressure".into(),
        data_type: DataType::Float32,
        connection: Some(ConnectionId::new("conn-plc").map_err(|e| e.to_string())?),
        address: Some("DB10.REAL0".into()),
        access: TagAccess::Read,
        unit: Some(UnitId::new("u-bar").map_err(|e| e.to_string())?),
        range: None,
        initial_value: Some(json!(1.5)),
        scan_rate_ms: 250,
        description: Some("Steam pressure".into()),
        extensions: BTreeMap::new(),
    });
    project.tags.push(Tag {
        id: TagId::new("tag-setpoint").map_err(|e| e.to_string())?,
        name: "Boiler/Setpoint".into(),
        data_type: DataType::Int32,
        connection: Some(ConnectionId::new("conn-plc").map_err(|e| e.to_string())?),
        address: Some("DB10.DINT4".into()),
        access: TagAccess::ReadWrite,
        unit: None,
        range: None,
        initial_value: Some(json!(50)),
        scan_rate_ms: 250,
        description: None,
        extensions: BTreeMap::new(),
    });
    project.v2.industrial.tags.push(TagWritePolicy {
        tag: TagId::new("tag-setpoint").map_err(|e| e.to_string())?,
        command: CommandKind::Setpoint,
        confirmation_required: true,
        critical: false,
        write_confirmed: true,
        pulse_ms: None,
        limits: WriteLimits {
            min: Some(serde_json::Number::from(0)),
            max: Some(serde_json::Number::from(100)),
            step: Some(serde_json::Number::from(5)),
        },
    });
    project.v2.industrial.security = SecurityPolicy {
        permissions: vec![Permission {
            id: PermissionId::new("perm-operate").map_err(|e| e.to_string())?,
            name: "Operate".into(),
        }],
        roles: vec![Role {
            id: RoleId::new("role-op").map_err(|e| e.to_string())?,
            name: "Operator".into(),
            permissions: vec![PermissionId::new("perm-operate").map_err(|e| e.to_string())?],
        }],
        anonymous_role: None,
        auto_logout_seconds: Some(300),
    };
    project
        .v2
        .bindings
        .enumerations
        .push(EnumerationDefinition {
            id: EnumerationId::new("enum-mode").map_err(|e| e.to_string())?,
            name: "Mode".into(),
            data_type: DataType::Int32,
            entries: vec![
                EnumerationEntry {
                    value: json!(0),
                    label: LocalizedText::Literal("Off".into()),
                    color: Some("#888888".into()),
                },
                EnumerationEntry {
                    value: json!(1),
                    label: LocalizedText::Literal("Auto".into()),
                    color: Some("#00aa00".into()),
                },
            ],
        });

    let valid_jpeg = STANDARD
        .decode(VALID_JPEG_B64)
        .map_err(|e| format!("jpeg b64: {e}"))?;
    let png_sha = sha256_hex(VALID_PNG);
    let jpg_sha = sha256_hex(&valid_jpeg);
    let svg_sha = sha256_hex(VALID_SVG);
    let png_path = format!("assets/{png_sha}.png");
    let jpg_path = format!("assets/{jpg_sha}.jpg");
    let svg_path = format!("assets/{svg_sha}.svg");
    project.resources.push(Resource {
        id: ResourceId::new("res-png").map_err(|e| e.to_string())?,
        kind: ResourceKind::Image,
        path: png_path.clone(),
        media_type: Some("image/png".into()),
        sha256: Some(png_sha),
        extensions: BTreeMap::new(),
    });
    project.resources.push(Resource {
        id: ResourceId::new("res-jpg").map_err(|e| e.to_string())?,
        kind: ResourceKind::Image,
        path: jpg_path.clone(),
        media_type: Some("image/jpeg".into()),
        sha256: Some(jpg_sha),
        extensions: BTreeMap::new(),
    });
    project.resources.push(Resource {
        id: ResourceId::new("res-svg").map_err(|e| e.to_string())?,
        kind: ResourceKind::Vector,
        path: svg_path.clone(),
        media_type: Some("image/svg+xml".into()),
        sha256: Some(svg_sha),
        extensions: BTreeMap::new(),
    });
    let resources = BTreeMap::from([
        (png_path, VALID_PNG.to_vec()),
        (jpg_path, valid_jpeg),
        (svg_path, VALID_SVG.to_vec()),
    ]);
    Ok((project, resources))
}

fn write_pkg(out_dir: &Path, name: &str, pkg: &BuiltPackage) -> Result<Value, String> {
    let path = out_dir.join(name);
    fs::write(&path, pkg.archive_bytes()).map_err(|e| format!("write {name}: {e}"))?;
    Ok(json!({
        "file": name,
        "format": match pkg.format() {
            hmi_runtime_package::PackageFormat::Format1Unsigned => 1,
            hmi_runtime_package::PackageFormat::Format2Signed => 2,
        },
        "packageId": pkg.package_id(),
        "archiveSha256": pkg.archive_sha256(),
        "archiveBytes": pkg.archive_bytes().len(),
        "manifestSha256": sha256_hex(pkg.manifest_json()),
        "runtimeSha256": sha256_hex(pkg.runtime_json()),
        "signatureSha256": pkg.signature_json().map(sha256_hex),
    }))
}

fn make_fingerprint_signer(seed: [u8; 32]) -> Result<Ed25519TestSigner, String> {
    let probe = Ed25519TestSigner::from_seed("probe", seed).map_err(|e| e.to_string())?;
    let key_id = probe.public_key_sha256();
    Ed25519TestSigner::from_seed(key_id, seed).map_err(|e| e.to_string())
}

fn build_f01_f02_boundaries_v10() -> Result<Project, String> {
    let mut root = Node::new(
        NodeId::new("n-root").map_err(|e| e.to_string())?,
        Element::Absolute {},
    );
    root.layout.width = Size::fixed(800.0);
    root.layout.height = Size::fixed(480.0);

    let mut lbl = Node::new(
        NodeId::new("n-lbl").map_err(|e| e.to_string())?,
        Element::Component {
            kind: ComponentKind::Text,
        },
    );
    lbl.layout.position = Some(Position { x: 16.0, y: 24.0 });
    lbl.layout.width = Size::fixed(240.0);
    lbl.layout.height = Size::fixed(36.0);
    lbl.properties
        .insert("text".into(), json!("F01/F02 Write Boundaries"));
    root.children.push(lbl);

    let screen = Screen {
        id: ScreenId::new("scr-main").map_err(|e| e.to_string())?,
        name: "Main".into(),
        kind: ScreenKind::Page,
        title: Some(LocalizedText::Literal("Boundaries".into())),
        design_profile: None,
        events: Vec::new(),
        root,
        extensions: BTreeMap::new(),
    };
    let mut project = Project::new(
        ProjectId::new("prj-f01-f02").map_err(|e| e.to_string())?,
        "Write Policy Boundaries",
        screen,
    );

    let conn_id = ConnectionId::new("conn-opc").map_err(|e| e.to_string())?;
    project.connections.push(Connection {
        id: conn_id.clone(),
        name: "Boundaries OPC UA".into(),
        driver: DriverKind::OpcUa,
        endpoint: Some("opc.tcp://10.0.0.10:4840".into()),
        parameters: BTreeMap::from([
            ("securityPolicy".into(), json!("None")),
            ("securityMode".into(), json!("None")),
        ]),
        credential_ref: None,
        extensions: BTreeMap::new(),
    });
    project.v2.industrial.connections.push(ConnectionPolicy {
        connection: conn_id.clone(),
        driver: DriverKindV2::OpcUa,
        commissioning_allowed: false,
    });

    // Momentary lower boundary (100 ms) and upper boundary (5000 ms).
    for (id, name, addr, pulse, confirm) in [
        (
            "tag-pulse-min",
            "Cmd/PulseMin",
            "ns=2;s=PulseMin",
            100u32,
            false,
        ),
        (
            "tag-pulse-max",
            "Cmd/PulseMax",
            "ns=2;s=PulseMax",
            5_000u32,
            true,
        ),
    ] {
        let tid = TagId::new(id).map_err(|e| e.to_string())?;
        project.tags.push(Tag {
            id: tid.clone(),
            name: name.into(),
            data_type: DataType::Bool,
            connection: Some(conn_id.clone()),
            address: Some(addr.into()),
            access: TagAccess::ReadWrite,
            unit: None,
            range: None,
            initial_value: Some(json!(false)),
            scan_rate_ms: 250,
            description: None,
            extensions: BTreeMap::new(),
        });
        project.v2.industrial.tags.push(TagWritePolicy {
            tag: tid,
            command: CommandKind::Momentary,
            confirmation_required: confirm,
            critical: false,
            write_confirmed: true,
            pulse_ms: Some(pulse),
            limits: WriteLimits::default(),
        });
    }

    // Exact binary64 INT64 setpoint spanning [-2^53, 2^53 + 2] with step 2.
    let tid_i64 = TagId::new("tag-sp-i64").map_err(|e| e.to_string())?;
    project.tags.push(Tag {
        id: tid_i64.clone(),
        name: "Cmd/SetpointI64".into(),
        data_type: DataType::Int64,
        connection: Some(conn_id.clone()),
        address: Some("ns=2;s=SetpointI64".into()),
        access: TagAccess::ReadWrite,
        unit: None,
        range: None,
        initial_value: Some(json!(0_i64)),
        scan_rate_ms: 250,
        description: None,
        extensions: BTreeMap::new(),
    });
    project.v2.industrial.tags.push(TagWritePolicy {
        tag: tid_i64,
        command: CommandKind::Setpoint,
        confirmation_required: true,
        critical: false,
        write_confirmed: true,
        pulse_ms: None,
        limits: WriteLimits {
            min: Some(serde_json::Number::from(-9_007_199_254_740_992_i64)),
            max: Some(serde_json::Number::from(9_007_199_254_740_994_i64)),
            step: Some(serde_json::Number::from(2_i64)),
        },
    });

    // Exact binary64 FLOAT64 setpoint without step rounding.
    let tid_f64 = TagId::new("tag-sp-f64").map_err(|e| e.to_string())?;
    project.tags.push(Tag {
        id: tid_f64.clone(),
        name: "Cmd/SetpointF64".into(),
        data_type: DataType::Float64,
        connection: Some(conn_id),
        address: Some("ns=2;s=SetpointF64".into()),
        access: TagAccess::ReadWrite,
        unit: None,
        range: None,
        initial_value: Some(json!(0.0)),
        scan_rate_ms: 250,
        description: None,
        extensions: BTreeMap::new(),
    });
    project.v2.industrial.tags.push(TagWritePolicy {
        tag: tid_f64,
        command: CommandKind::Setpoint,
        confirmation_required: true,
        critical: false,
        write_confirmed: true,
        pulse_ms: None,
        limits: WriteLimits {
            min: Some(serde_json::Number::from_f64(-12.5).ok_or("min f64")?),
            max: Some(serde_json::Number::from_f64(12.5).ok_or("max f64")?),
            step: Some(serde_json::Number::from_f64(0.5).ok_or("step f64")?),
        },
    });

    Ok(project)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    let out_dir = args
        .first()
        .map(PathBuf::from)
        .ok_or("usage: emit_interop_fixtures <output-dir> [--f01-f02-boundaries]")?;
    let include_boundaries = args.iter().skip(1).any(|a| a == "--f01-f02-boundaries");
    fs::create_dir_all(&out_dir)?;

    let tv1 = make_fingerprint_signer(TV1_SEED)?;
    let tv2 = make_fingerprint_signer(TV2_SEED)?;

    // 1. Minimal v2 project (SIMULATED)
    let minimal = load_project(include_str!(
        "../../project-model/fixtures/valid/minimal.hmi.json"
    ))?
    .project;
    let minimal_compiled = compile_project(&minimal, &BTreeMap::new(), CompileOptions::default())?;
    let sim_settings = RuntimeSettings::default();

    let min_v1 = build_unsigned_package(
        &minimal_compiled,
        sim_settings,
        UnsignedPackageMeta {
            version: "1.0.0".into(),
            created_at_ms: 1_700_000_000_000,
            created_by: "reuse-04-p1-interop".into(),
        },
        Compression::Deflated,
    )?;
    let min_v2_seq1 = build_signed_package(
        &minimal_compiled,
        sim_settings,
        SignedPackageMeta {
            version: "1.0.0".into(),
            created_at_ms: 1_700_000_000_000,
            created_by: "reuse-04-p1-interop".into(),
            sequence: 1,
        },
        &tv1,
        Compression::Deflated,
    )?;
    let min_v2_seq2 = build_signed_package(
        &minimal_compiled,
        sim_settings,
        SignedPackageMeta {
            version: "1.0.1".into(),
            created_at_ms: 1_700_000_000_500,
            created_by: "reuse-04-p1-interop".into(),
            sequence: 2,
        },
        &tv1,
        Compression::Deflated,
    )?;
    let min_v2_untrusted = build_signed_package(
        &minimal_compiled,
        sim_settings,
        SignedPackageMeta {
            version: "1.0.2".into(),
            created_at_ms: 1_700_000_000_900,
            created_by: "reuse-04-p1-interop".into(),
            sequence: 3,
        },
        &tv2,
        Compression::Deflated,
    )?;

    // 2. Industrial LIVE v10 project with B5-07 verified PNG/JPEG/SVG assets
    let (industrial, industrial_res) = build_industrial_live_v10()?;
    let industrial_compiled = compile_project(
        &industrial,
        &industrial_res,
        CompileOptions {
            mode: CompileMode::ProductionLive,
            ..CompileOptions::default()
        },
    )?;
    let live_settings = RuntimeSettings {
        values: RuntimeValues::Live,
        full_screen: false,
        status_bar: true,
        keep_screen_on: false,
    };

    let ind_v1 = build_unsigned_package(
        &industrial_compiled,
        live_settings,
        UnsignedPackageMeta {
            version: "2.0.0".into(),
            created_at_ms: 1_700_000_001_000,
            created_by: "reuse-04-p1-interop".into(),
        },
        Compression::Deflated,
    )?;
    let ind_v2_seq1 = build_signed_package(
        &industrial_compiled,
        live_settings,
        SignedPackageMeta {
            version: "2.0.0".into(),
            created_at_ms: 1_700_000_001_000,
            created_by: "reuse-04-p1-interop".into(),
            sequence: 1,
        },
        &tv1,
        Compression::Deflated,
    )?;
    let ind_v2_seq2 = build_signed_package(
        &industrial_compiled,
        live_settings,
        SignedPackageMeta {
            version: "2.0.1".into(),
            created_at_ms: 1_700_000_001_500,
            created_by: "reuse-04-p1-interop".into(),
            sequence: 2,
        },
        &tv1,
        Compression::Deflated,
    )?;

    let mut packages = vec![
        write_pkg(&out_dir, "minimal_v2_unsigned.hmipkg", &min_v1)?,
        write_pkg(&out_dir, "minimal_v2_signed_seq1.hmipkg", &min_v2_seq1)?,
        write_pkg(&out_dir, "minimal_v2_signed_seq2.hmipkg", &min_v2_seq2)?,
        write_pkg(
            &out_dir,
            "minimal_v2_signed_untrusted.hmipkg",
            &min_v2_untrusted,
        )?,
        write_pkg(&out_dir, "industrial_live_v10_unsigned.hmipkg", &ind_v1)?,
        write_pkg(
            &out_dir,
            "industrial_live_v10_signed_seq1.hmipkg",
            &ind_v2_seq1,
        )?,
        write_pkg(
            &out_dir,
            "industrial_live_v10_signed_seq2.hmipkg",
            &ind_v2_seq2,
        )?,
    ];

    if include_boundaries {
        let boundaries = build_f01_f02_boundaries_v10()?;
        let boundaries_compiled = compile_project(
            &boundaries,
            &BTreeMap::new(),
            CompileOptions {
                mode: CompileMode::ProductionLive,
                ..CompileOptions::default()
            },
        )?;
        let boundaries_pkg = build_signed_package(
            &boundaries_compiled,
            live_settings,
            SignedPackageMeta {
                version: "2.1.0".into(),
                created_at_ms: 1_700_000_002_000,
                created_by: "reuse-04-p1-interop".into(),
                sequence: 1,
            },
            &tv1,
            Compression::Deflated,
        )?;
        packages.push(write_pkg(
            &out_dir,
            "f01_f02_boundaries_v10_signed_seq1.hmipkg",
            &boundaries_pkg,
        )?);
    }

    let meta = json!({
        "keys": {
            "tv1": {
                "seedHex": hex_encode(&TV1_SEED),
                "publicKeyHex": hex_encode(&tv1.public_key_bytes()),
                "publicKeyBase64": STANDARD.encode(tv1.public_key_bytes()),
                "keyId": tv1.public_key_sha256(),
            },
            "tv2": {
                "seedHex": hex_encode(&TV2_SEED),
                "publicKeyHex": hex_encode(&tv2.public_key_bytes()),
                "publicKeyBase64": STANDARD.encode(tv2.public_key_bytes()),
                "keyId": tv2.public_key_sha256(),
            }
        },
        "packages": packages,
    });
    fs::write(
        out_dir.join("fixtures-meta.json"),
        serde_json::to_vec_pretty(&meta)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&meta)?);
    Ok(())
}
