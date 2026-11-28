#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmi_legacy_import::import_portable;
use hmi_legacy_import::package::decode_runtime_package;
use hmi_legacy_import::raster::{RasterError, RasterLimits};
use hmi_project_model::id::{
    AlarmId, BreakpointId, ConnectionId, DataTypeId, DeviceProfileId, EnumerationId, NodeId,
    PermissionId, ProjectId, RecipeFieldId, RecipeId, ResourceId, RoleId, ScreenId, TagId, ThemeId,
    UnitId,
};
use hmi_project_model::load_project;
use hmi_project_model::model::v2::{
    CommandKind, ConnectionPolicy, DriverKindV2, Permission, Role, SecurityPolicy, TagWritePolicy,
    WriteLimits,
};
use hmi_project_model::model::v2_bindings::{EnumerationDefinition, EnumerationEntry};
use hmi_project_model::model::{
    Action, AlarmCondition, AlarmDefinition, AlarmSeverity, Arrangement, BindingMode,
    BindingSource, Breakpoint, ComponentKind, Connection, CrossAlignment, DataType, DeviceProfile,
    DriverKind, Element, EngineeringUnit, EventHandler, EventKind, LayoutParams, LocalizedText,
    Node, Orientation, Platform, Position, Project, PropertyBinding, RecipeDefinition, RecipeField,
    Resource, ResourceKind, ResponsiveCondition, ResponsiveRule, Screen, ScreenKind, Size,
    StructMember, StructType, Tag, TagAccess, TagRef, Theme, ValueTransform,
};
use hmi_runtime_package::package::verify_signed_package_archive;
use hmi_runtime_package::{
    build_signed_package, build_unsigned_package, compile_project, CompileError, CompileMode,
    CompileOptions, Compression, Ed25519TestSigner, PackageBuildError, RuntimeSettings,
    RuntimeValues, SignedPackageMeta, UnsignedPackageMeta,
};
use serde_json::json;
use sha2::{Digest, Sha256};

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

const VALID_SVG: &[u8] =
    br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect x="0" y="0" width="8" height="8" fill="#00aa55"/></svg>"##;

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn minimal_project() -> Project {
    load_project(include_str!(
        "../../project-model/fixtures/valid/minimal.hmi.json"
    ))
    .unwrap()
    .project
}

fn absolute_project_with_label() -> Project {
    let mut root = Node::new(NodeId::new("n-root").unwrap(), Element::Absolute {});
    root.layout.width = Size::fixed(800.0);
    root.layout.height = Size::fixed(480.0);
    let mut lbl = Node::new(
        NodeId::new("n-lbl").unwrap(),
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
        id: ScreenId::new("scr-main").unwrap(),
        name: "Main".into(),
        kind: ScreenKind::Page,
        title: Some(LocalizedText::Literal("Main Screen".into())),
        design_profile: None,
        events: Vec::new(),
        root,
        extensions: BTreeMap::new(),
    };
    Project::new(ProjectId::new("prj-boiler").unwrap(), "Boiler HMI", screen)
}

fn corrupt_deflate_png_2x2() -> Vec<u8> {
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &b in data {
            crc ^= u32::from(b);
            for _ in 0..8 {
                let m = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xedb8_8320 & m);
            }
        }
        !crc
    }
    fn chunk(kind: [u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + body.len());
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(&kind);
        out.extend_from_slice(body);
        out.extend_from_slice(&crc32(&out[4..]).to_be_bytes());
        out
    }
    let mut out = Vec::from(*b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    out.extend_from_slice(&chunk(*b"IHDR", &ihdr));
    out.extend_from_slice(&chunk(*b"IDAT", &[0xde, 0xad, 0xbe, 0xef]));
    out.extend_from_slice(&chunk(*b"IEND", &[]));
    out
}

#[test]
fn minimal_canonical_v2_compiles_deterministically_and_roundtrips_format2() {
    let project = minimal_project();
    let resources = BTreeMap::new();
    let compiled_a = compile_project(&project, &resources, CompileOptions::default()).unwrap();
    let compiled_b = compile_project(&project, &resources, CompileOptions::default()).unwrap();
    assert_eq!(compiled_a, compiled_b);
    assert_eq!(compiled_a.schema_version(), 2); // Column adaptive layout -> v2
    assert_eq!(compiled_a.project_name(), "Minimal");
    assert_eq!(compiled_a.startup_screen(), "scr-main");
    assert!(!compiled_a.has_live_mapping());

    // Build format-2 signed package for isolated development/simulation (D-004).
    let signer = Ed25519TestSigner::from_seed("dev-sim-key", [0x33u8; 32]).unwrap();
    let pkg = build_signed_package(
        &compiled_a,
        RuntimeSettings::default(),
        SignedPackageMeta {
            version: "0.1.0".into(),
            created_at_ms: 1_700_000_000_000,
            created_by: "hmi-studio-dev".into(),
            sequence: 1,
        },
        &signer,
        Compression::Deflated,
    )
    .unwrap();

    // Independent strict transport decoder AND Ed25519 cryptographic verifier accept it.
    let decoded = decode_runtime_package(pkg.archive_bytes()).unwrap();
    assert_eq!(decoded.manifest_schema_version(), 2);
    let verified = verify_signed_package_archive(
        pkg.archive_bytes(),
        &signer.public_key_bytes(),
        "dev-sim-key",
    )
    .unwrap();
    assert_eq!(verified.package_id, pkg.package_id());
}

#[test]
fn industrial_live_v10_project_with_b5_07_verified_assets_compiles_and_packages() {
    let mut project = absolute_project_with_label();
    project.units.push(EngineeringUnit {
        id: UnitId::new("u-bar").unwrap(),
        symbol: "bar".into(),
        name: Some(LocalizedText::Literal("Bar".into())),
        quantity: Some("pressure".into()),
    });
    project.connections.push(Connection {
        id: ConnectionId::new("conn-plc").unwrap(),
        name: "Line 1 S7".into(),
        driver: DriverKind::SiemensS7,
        endpoint: Some("10.0.0.10:102".into()),
        parameters: BTreeMap::from([("rack".into(), json!(0)), ("slot".into(), json!(1))]),
        credential_ref: None,
        extensions: BTreeMap::new(),
    });
    project.v2.industrial.connections.push(ConnectionPolicy {
        connection: ConnectionId::new("conn-plc").unwrap(),
        driver: DriverKindV2::SiemensS7,
        commissioning_allowed: false,
    });
    project.tags.push(Tag {
        id: TagId::new("tag-pressure").unwrap(),
        name: "Boiler/Pressure".into(),
        data_type: DataType::Float32,
        connection: Some(ConnectionId::new("conn-plc").unwrap()),
        address: Some("DB10.REAL0".into()),
        access: TagAccess::Read,
        unit: Some(UnitId::new("u-bar").unwrap()),
        range: None,
        initial_value: Some(json!(1.5)),
        scan_rate_ms: 250,
        description: Some("Steam pressure".into()),
        extensions: BTreeMap::new(),
    });
    project.tags.push(Tag {
        id: TagId::new("tag-setpoint").unwrap(),
        name: "Boiler/Setpoint".into(),
        data_type: DataType::Int32,
        connection: Some(ConnectionId::new("conn-plc").unwrap()),
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
        tag: TagId::new("tag-setpoint").unwrap(),
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
            id: PermissionId::new("perm-operate").unwrap(),
            name: "Operate".into(),
        }],
        roles: vec![Role {
            id: RoleId::new("role-op").unwrap(),
            name: "Operator".into(),
            permissions: vec![PermissionId::new("perm-operate").unwrap()],
        }],
        anonymous_role: None,
        auto_logout_seconds: Some(300),
    };
    project
        .v2
        .bindings
        .enumerations
        .push(EnumerationDefinition {
            id: EnumerationId::new("enum-mode").unwrap(),
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

    // Attach valid PNG, valid JPEG, and valid restricted SVG resources.
    let valid_jpeg = STANDARD.decode(VALID_JPEG_B64).unwrap();
    let png_sha = sha256_hex(VALID_PNG);
    let jpg_sha = sha256_hex(&valid_jpeg);
    let svg_sha = sha256_hex(VALID_SVG);
    let png_path = format!("assets/{png_sha}.png");
    let jpg_path = format!("assets/{jpg_sha}.jpg");
    let svg_path = format!("assets/{svg_sha}.svg");
    project.resources.push(Resource {
        id: ResourceId::new("res-png").unwrap(),
        kind: ResourceKind::Image,
        path: png_path.clone(),
        media_type: Some("image/png".into()),
        sha256: Some(png_sha),
        extensions: BTreeMap::new(),
    });
    project.resources.push(Resource {
        id: ResourceId::new("res-jpg").unwrap(),
        kind: ResourceKind::Image,
        path: jpg_path.clone(),
        media_type: Some("image/jpeg".into()),
        sha256: Some(jpg_sha),
        extensions: BTreeMap::new(),
    });
    project.resources.push(Resource {
        id: ResourceId::new("res-svg").unwrap(),
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

    let compiled = compile_project(
        &project,
        &resources,
        CompileOptions {
            mode: CompileMode::ProductionLive,
            ..CompileOptions::default()
        },
    )
    .unwrap();
    assert_eq!(compiled.schema_version(), 10);
    assert!(compiled.has_live_mapping());

    // Round-trip compiled JSON through the strict legacy importer!
    let imported = import_portable(compiled.project_json());
    assert!(
        imported.report.has_complete_mapping(),
        "unexpected import findings: {:?}",
        imported.report.findings
    );
    assert!(imported.project.is_some());

    // Package as LIVE format-2 signed package.
    let signer = Ed25519TestSigner::from_seed("prod-test-key", [0x77u8; 32]).unwrap();
    let pkg = build_signed_package(
        &compiled,
        RuntimeSettings {
            values: RuntimeValues::Live,
            ..RuntimeSettings::default()
        },
        SignedPackageMeta {
            version: "1.2.3".into(),
            created_at_ms: 1_700_000_000_000,
            created_by: "hmi-studio-ci".into(),
            sequence: 10,
        },
        &signer,
        Compression::Deflated,
    )
    .unwrap();
    let decoded = decode_runtime_package(pkg.archive_bytes()).unwrap();
    assert_eq!(decoded.manifest_schema_version(), 10);
}

#[test]
fn b5_07_compiler_refuses_corrupt_oversized_and_unverified_rasters_and_active_svg() {
    // 1. Structurally valid PNG envelope with corrupt DEFLATE stream -> refused.
    let corrupt_png = corrupt_deflate_png_2x2();
    let sha = sha256_hex(&corrupt_png);
    let path = format!("assets/{sha}.png");
    let mut project = absolute_project_with_label();
    project.resources.push(Resource {
        id: ResourceId::new("res-bad-png").unwrap(),
        kind: ResourceKind::Image,
        path: path.clone(),
        media_type: Some("image/png".into()),
        sha256: Some(sha),
        extensions: BTreeMap::new(),
    });
    let mut res_map = BTreeMap::from([(path.clone(), corrupt_png)]);
    assert_eq!(
        compile_project(&project, &res_map, CompileOptions::default()),
        Err(CompileError::RasterRefused(
            RasterError::MalformedCompressedData
        ))
    );

    // 2. Missing resource bytes -> refused.
    assert_eq!(
        compile_project(&project, &BTreeMap::new(), CompileOptions::default()),
        Err(CompileError::MissingResource)
    );

    // 3. SHA-256 mismatch -> refused.
    res_map.insert(path, VALID_PNG.to_vec());
    assert_eq!(
        compile_project(&project, &res_map, CompileOptions::default()),
        Err(CompileError::ResourceIntegrityMismatch)
    );

    // 4. Valid JPEG exceeding compiler RasterLimits -> DimensionLimit / PixelCountLimit / DecodedByteLimit.
    let valid_jpeg = STANDARD.decode(VALID_JPEG_B64).unwrap();
    let jpg_sha = sha256_hex(&valid_jpeg);
    let jpg_path = format!("assets/{jpg_sha}.jpg");
    let mut jpg_project = absolute_project_with_label();
    jpg_project.resources.push(Resource {
        id: ResourceId::new("res-jpg").unwrap(),
        kind: ResourceKind::Image,
        path: jpg_path.clone(),
        media_type: Some("image/jpeg".into()),
        sha256: Some(jpg_sha),
        extensions: BTreeMap::new(),
    });
    let jpg_map = BTreeMap::from([(jpg_path, valid_jpeg.clone())]);
    assert_eq!(
        compile_project(
            &jpg_project,
            &jpg_map,
            CompileOptions {
                raster_limits: RasterLimits {
                    max_height: 2,
                    ..RasterLimits::DEFAULT
                },
                ..CompileOptions::default()
            }
        ),
        Err(CompileError::RasterRefused(RasterError::DimensionLimit))
    );
    assert_eq!(
        compile_project(
            &jpg_project,
            &jpg_map,
            CompileOptions {
                raster_limits: RasterLimits {
                    max_pixels: 5,
                    ..RasterLimits::DEFAULT
                },
                ..CompileOptions::default()
            }
        ),
        Err(CompileError::RasterRefused(RasterError::PixelCountLimit))
    );
    assert_eq!(
        compile_project(
            &jpg_project,
            &jpg_map,
            CompileOptions {
                raster_limits: RasterLimits {
                    max_decoded_bytes: 20,
                    ..RasterLimits::DEFAULT
                },
                ..CompileOptions::default()
            }
        ),
        Err(CompileError::RasterRefused(RasterError::DecodedByteLimit))
    );

    // 5. Corrupt JPEG Huffman stream -> MalformedCompressedData.
    let sos = valid_jpeg
        .windows(2)
        .position(|w| w == [0xff, 0xda])
        .unwrap();
    let sos_len = u16::from_be_bytes([valid_jpeg[sos + 2], valid_jpeg[sos + 3]]) as usize;
    let mut bad_jpeg = valid_jpeg[..sos + 2 + sos_len].to_vec();
    bad_jpeg.extend_from_slice(&[0xff, 0x00, 0xff, 0x00, 0xff, 0x00, 0xff, 0xd9]);
    let bad_sha = sha256_hex(&bad_jpeg);
    let bad_path = format!("assets/{bad_sha}.jpg");
    let mut bad_jpg_proj = absolute_project_with_label();
    bad_jpg_proj.resources.push(Resource {
        id: ResourceId::new("res-bad-jpg").unwrap(),
        kind: ResourceKind::Image,
        path: bad_path.clone(),
        media_type: Some("image/jpeg".into()),
        sha256: Some(bad_sha),
        extensions: BTreeMap::new(),
    });
    assert_eq!(
        compile_project(
            &bad_jpg_proj,
            &BTreeMap::from([(bad_path, bad_jpeg)]),
            CompileOptions::default()
        ),
        Err(CompileError::RasterRefused(
            RasterError::MalformedCompressedData
        ))
    );

    // 6. Active SVG content -> InvalidSvgResource.
    let bad_svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><script>alert(1)</script></svg>"#;
    let svg_sha = sha256_hex(bad_svg);
    let svg_path = format!("assets/{svg_sha}.svg");
    let mut svg_proj = absolute_project_with_label();
    svg_proj.resources.push(Resource {
        id: ResourceId::new("res-bad-svg").unwrap(),
        kind: ResourceKind::Vector,
        path: svg_path.clone(),
        media_type: Some("image/svg+xml".into()),
        sha256: Some(svg_sha),
        extensions: BTreeMap::new(),
    });
    assert_eq!(
        compile_project(
            &svg_proj,
            &BTreeMap::from([(svg_path, bad_svg.to_vec())]),
            CompileOptions::default()
        ),
        Err(CompileError::InvalidSvgResource)
    );
}

#[test]
fn quarantine_and_explicit_v2_feature_refusal_matrix() {
    let empty = BTreeMap::new();

    // Quarantine refusal: refused unconditionally in ProductionLive, refused by default in DevSim.
    let mut quarantined = absolute_project_with_label();
    quarantined.extensions.insert(
        "hmistudio.importQuarantine".into(),
        json!({"version":1,"state":"reviewRequired"}),
    );
    assert_eq!(
        compile_project(&quarantined, &empty, CompileOptions::default()),
        Err(CompileError::QuarantinedProjectRefused)
    );
    assert_eq!(
        compile_project(
            &quarantined,
            &empty,
            CompileOptions {
                mode: CompileMode::ProductionLive,
                allow_quarantined_simulation: true,
                ..CompileOptions::default()
            }
        ),
        Err(CompileError::QuarantinedProjectRefused)
    );
    let simulation = compile_project(
        &quarantined,
        &empty,
        CompileOptions {
            mode: CompileMode::DevelopmentSimulation,
            allow_quarantined_simulation: true,
            ..CompileOptions::default()
        },
    )
    .unwrap();
    let live_settings = RuntimeSettings {
        values: RuntimeValues::Live,
        ..RuntimeSettings::default()
    };
    assert!(matches!(
        build_unsigned_package(
            &simulation,
            live_settings,
            UnsignedPackageMeta {
                version: "1.0".into(),
                created_at_ms: 1000,
                created_by: "synthetic".into(),
            },
            Compression::Stored,
        ),
        Err(PackageBuildError::CompileModeMismatch)
    ));
    let signer = Ed25519TestSigner::from_seed("simulation-test", [0x33; 32]).unwrap();
    assert!(matches!(
        build_signed_package(
            &simulation,
            live_settings,
            SignedPackageMeta {
                version: "1.0".into(),
                created_at_ms: 1000,
                created_by: "synthetic".into(),
                sequence: 1,
            },
            &signer,
            Compression::Stored,
        ),
        Err(PackageBuildError::CompileModeMismatch)
    ));

    // ProductionLive without external mappings fails closed.
    assert_eq!(
        compile_project(
            &absolute_project_with_label(),
            &empty,
            CompileOptions {
                mode: CompileMode::ProductionLive,
                ..CompileOptions::default()
            }
        ),
        Err(CompileError::LiveMappingRequired)
    );

    // Alarms refused.
    let mut p = absolute_project_with_label();
    p.tags.push(Tag {
        id: TagId::new("t1").unwrap(),
        name: "T1".into(),
        data_type: DataType::Bool,
        connection: None,
        address: None,
        access: TagAccess::Read,
        unit: None,
        range: None,
        initial_value: Some(json!(false)),
        scan_rate_ms: 0,
        description: None,
        extensions: BTreeMap::new(),
    });
    p.alarms.push(AlarmDefinition {
        id: AlarmId::new("alm-1").unwrap(),
        name: "Trip".into(),
        tag: TagId::new("t1").unwrap(),
        member: None,
        condition: AlarmCondition::Equals { value: json!(true) },
        deadband: None,
        delay_ms: 0,
        severity: AlarmSeverity::Critical,
        priority: 100,
        message: LocalizedText::Literal("Trip".into()),
        group: None,
        acknowledge: true,
        latched: false,
    });
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("alarms"))
    );

    // Recipes refused.
    let mut p = absolute_project_with_label();
    p.tags.push(Tag {
        id: TagId::new("t-sp").unwrap(),
        name: "SP".into(),
        data_type: DataType::Int32,
        connection: None,
        address: None,
        access: TagAccess::ReadWrite,
        unit: None,
        range: None,
        initial_value: Some(json!(10)),
        scan_rate_ms: 0,
        description: None,
        extensions: BTreeMap::new(),
    });
    p.recipes.push(RecipeDefinition {
        id: RecipeId::new("rec-1").unwrap(),
        name: LocalizedText::Literal("Batch".into()),
        description: None,
        fields: vec![RecipeField {
            id: RecipeFieldId::new("rf-1").unwrap(),
            name: LocalizedText::Literal("Setpoint".into()),
            tag: TagId::new("t-sp").unwrap(),
            member: None,
        }],
        sets: Vec::new(),
    });
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("recipes"))
    );

    // Struct data types refused.
    let mut p = absolute_project_with_label();
    p.data_types.push(StructType {
        id: DataTypeId::new("dt-motor").unwrap(),
        name: "MotorUdt".into(),
        members: vec![StructMember {
            name: "run".into(),
            data_type: DataType::Bool,
            description: None,
        }],
    });
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("structDataTypes"))
    );

    // Breakpoints refused.
    let mut p = absolute_project_with_label();
    p.breakpoints.push(Breakpoint {
        id: BreakpointId::new("bp-sm").unwrap(),
        name: "Small".into(),
        min_width: 0.0,
        max_width: Some(600.0),
    });
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("breakpoints"))
    );

    // Device profiles refused.
    let mut p = absolute_project_with_label();
    p.device_profiles.push(DeviceProfile {
        id: DeviceProfileId::new("dp-1").unwrap(),
        name: "Panel".into(),
        platform: Platform::Android,
        width_px: 1280,
        height_px: 800,
        density: 1.0,
        orientation: Orientation::Landscape,
        touch: true,
    });
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("deviceProfiles"))
    );

    // Multi-locale localization refused.
    let mut p = absolute_project_with_label();
    p.localization.locales.push("de".into());
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("multiLocaleLocalization"))
    );

    // Popup screen refused.
    let mut p = absolute_project_with_label();
    p.screens.push(Screen {
        id: ScreenId::new("scr-pop").unwrap(),
        name: "Pop".into(),
        kind: ScreenKind::Popup,
        title: None,
        design_profile: None,
        events: Vec::new(),
        root: Node::new(NodeId::new("n-pop-root").unwrap(), Element::Absolute {}),
        extensions: BTreeMap::new(),
    });
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("popupScreen"))
    );

    // Box layout refused.
    let mut p = minimal_project();
    p.screens[0].root.element = Element::Box {
        content_alignment: Default::default(),
    };
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("boxLayout"))
    );

    // Scroll layout refused.
    let mut p = minimal_project();
    p.screens[0].root.element = Element::Scroll {
        direction: Default::default(),
    };
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("scrollLayout"))
    );

    // Non-default Row arrangement / wrap refused.
    let mut p = minimal_project();
    p.screens[0].root.element = Element::Row {
        arrangement: Arrangement::SpaceBetween,
        alignment: CrossAlignment::Start,
        spacing: 8.0,
        wrap: false,
    };
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("nonDefaultFlowAlignment"))
    );
    p.screens[0].root.element = Element::Row {
        arrangement: Arrangement::Start,
        alignment: CrossAlignment::Start,
        spacing: 8.0,
        wrap: true,
    };
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("wrappedRowLayout"))
    );

    // Responsive rule refused.
    let mut p = minimal_project();
    p.screens[0].root.children[0]
        .responsive
        .push(ResponsiveRule {
            when: ResponsiveCondition {
                min_width: Some(800.0),
                ..Default::default()
            },
            set: hmi_project_model::model::LayoutOverride {
                visibility: Some(hmi_project_model::model::Visibility::Hidden),
                ..Default::default()
            },
        });
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("responsiveLayout"))
    );

    // Unsupported scalar type (uint64) refused.
    let mut p = absolute_project_with_label();
    p.tags.push(Tag {
        id: TagId::new("t-u64").unwrap(),
        name: "Counter".into(),
        data_type: DataType::UInt64,
        connection: None,
        address: None,
        access: TagAccess::Read,
        unit: None,
        range: None,
        initial_value: None,
        scan_rate_ms: 0,
        description: None,
        extensions: BTreeMap::new(),
    });
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("dataType"))
    );

    // Unsupported driver (ModbusTcp) refused.
    let mut p = absolute_project_with_label();
    p.connections.push(Connection {
        id: ConnectionId::new("c-mb").unwrap(),
        name: "Modbus".into(),
        driver: DriverKind::ModbusTcp,
        endpoint: Some("10.0.0.20:502".into()),
        parameters: BTreeMap::new(),
        credential_ref: None,
        extensions: BTreeMap::new(),
    });
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("driverKind"))
    );

    // Unsupported binding transform refused.
    let mut p = absolute_project_with_label();
    p.tags.push(Tag {
        id: TagId::new("t-b").unwrap(),
        name: "Flag".into(),
        data_type: DataType::Bool,
        connection: None,
        address: None,
        access: TagAccess::Read,
        unit: None,
        range: None,
        initial_value: Some(json!(false)),
        scan_rate_ms: 0,
        description: None,
        extensions: BTreeMap::new(),
    });
    p.screens[0].root.children[0]
        .bindings
        .push(PropertyBinding {
            property: "visible".into(),
            source: BindingSource::Tag(TagId::new("t-b").unwrap()),
            member: None,
            mode: BindingMode::Read,
            transform: Some(ValueTransform::Invert {}),
        });
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("bindingTransform"))
    );

    // Unsupported action (SetLocale) refused.
    let mut p = absolute_project_with_label();
    let mut btn = Node::new(
        NodeId::new("n-btn").unwrap(),
        Element::Component {
            kind: ComponentKind::Button,
        },
    );
    btn.layout = LayoutParams {
        position: Some(Position { x: 10.0, y: 10.0 }),
        width: Size::fixed(100.0),
        height: Size::fixed(40.0),
        ..Default::default()
    };
    btn.events.push(EventHandler {
        event: EventKind::Click,
        confirm: None,
        actions: vec![Action::SetLocale {
            locale: "en".into(),
        }],
    });
    p.screens[0].root.children.push(btn);
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("setLocaleAction"))
    );

    // Multiple themes refused.
    let mut p = absolute_project_with_label();
    p.themes.push(Theme {
        id: ThemeId::new("th-1").unwrap(),
        name: "Light".into(),
        dark: false,
        colors: BTreeMap::new(),
        typography: BTreeMap::new(),
        metrics: BTreeMap::new(),
    });
    p.themes.push(Theme {
        id: ThemeId::new("th-2").unwrap(),
        name: "Dark".into(),
        dark: true,
        colors: BTreeMap::new(),
        typography: BTreeMap::new(),
        metrics: BTreeMap::new(),
    });
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("multipleThemes"))
    );

    // Unused TagRef import check (ensure WriteTag compile path is also exercised via refusal on member).
    let mut p = absolute_project_with_label();
    p.tags.push(Tag {
        id: TagId::new("t-w").unwrap(),
        name: "Cmd".into(),
        data_type: DataType::Bool,
        connection: None,
        address: None,
        access: TagAccess::ReadWrite,
        unit: None,
        range: None,
        initial_value: Some(json!(false)),
        scan_rate_ms: 0,
        description: None,
        extensions: BTreeMap::new(),
    });
    let mut btn2 = Node::new(
        NodeId::new("n-btn2").unwrap(),
        Element::Component {
            kind: ComponentKind::Button,
        },
    );
    btn2.layout.position = Some(Position { x: 10.0, y: 80.0 });
    btn2.layout.width = Size::fixed(100.0);
    btn2.layout.height = Size::fixed(40.0);
    btn2.events.push(EventHandler {
        event: EventKind::Click,
        confirm: Some(LocalizedText::Literal("Toggle command?".into())),
        actions: vec![Action::ToggleTag {
            target: TagRef::Tag(TagId::new("t-w").unwrap()),
            member: None,
        }],
    });
    p.screens[0].root.children.push(btn2);
    assert_eq!(
        compile_project(&p, &empty, CompileOptions::default()),
        Err(CompileError::UnsupportedFeature("eventConfirmationPrompt"))
    );
}

#[test]
fn write_command_confirm_matches_kotlin_write_rules() {
    fn project_with_command(
        data_type: DataType,
        initial: serde_json::Value,
        command: CommandKind,
        confirmation_required: bool,
        limits: WriteLimits,
        pulse_ms: Option<u32>,
    ) -> Project {
        let mut p = absolute_project_with_label();
        p.connections.push(Connection {
            id: ConnectionId::new("conn-plc").unwrap(),
            name: "PLC".into(),
            driver: DriverKind::SiemensS7,
            endpoint: Some("10.0.0.10:102".into()),
            parameters: BTreeMap::from([("rack".into(), json!(0)), ("slot".into(), json!(1))]),
            credential_ref: None,
            extensions: BTreeMap::new(),
        });
        p.v2.industrial.connections.push(ConnectionPolicy {
            connection: ConnectionId::new("conn-plc").unwrap(),
            driver: DriverKindV2::SiemensS7,
            commissioning_allowed: false,
        });
        p.tags.push(Tag {
            id: TagId::new("tag-cmd").unwrap(),
            name: "Cmd".into(),
            data_type,
            connection: Some(ConnectionId::new("conn-plc").unwrap()),
            address: Some("DB1.DBX0.0".into()),
            access: TagAccess::ReadWrite,
            unit: None,
            range: None,
            initial_value: Some(initial),
            scan_rate_ms: 250,
            description: None,
            extensions: BTreeMap::new(),
        });
        p.v2.industrial.tags.push(TagWritePolicy {
            tag: TagId::new("tag-cmd").unwrap(),
            command,
            confirmation_required,
            critical: false,
            write_confirmed: true,
            pulse_ms,
            limits,
        });
        p
    }

    let empty = BTreeMap::new();
    let live_opts = CompileOptions {
        mode: CompileMode::ProductionLive,
        ..CompileOptions::default()
    };

    // 1. Setpoint with confirmation_required=true must omit `confirm` (Kotlin WriteRules forbids it).
    let sp_limits = WriteLimits {
        min: Some(serde_json::Number::from(0)),
        max: Some(serde_json::Number::from(100)),
        step: Some(serde_json::Number::from(5)),
    };
    let sp_ok = project_with_command(
        DataType::Int32,
        json!(10),
        CommandKind::Setpoint,
        true,
        sp_limits.clone(),
        None,
    );
    let sp_compiled = compile_project(&sp_ok, &empty, live_opts).unwrap();
    let sp_json: serde_json::Value = serde_json::from_slice(sp_compiled.project_json()).unwrap();
    assert!(sp_json["externalMappings"]["map-tag-cmd"]["command"]
        .get("confirm")
        .is_none());

    // 2. Setpoint with confirmation_required=false fails closed.
    let sp_bad = project_with_command(
        DataType::Int32,
        json!(10),
        CommandKind::Setpoint,
        false,
        sp_limits,
        None,
    );
    assert_eq!(
        compile_project(&sp_bad, &empty, live_opts),
        Err(CompileError::UnsupportedFeature(
            "unconfirmedSetpointOrMaintained"
        ))
    );

    // 3. Maintained with confirmation_required=true omits `confirm`; false fails closed.
    let no_limits = WriteLimits {
        min: None,
        max: None,
        step: None,
    };
    let maint_ok = project_with_command(
        DataType::Bool,
        json!(false),
        CommandKind::Maintained,
        true,
        no_limits.clone(),
        None,
    );
    let maint_compiled = compile_project(&maint_ok, &empty, live_opts).unwrap();
    let maint_json: serde_json::Value =
        serde_json::from_slice(maint_compiled.project_json()).unwrap();
    assert!(maint_json["externalMappings"]["map-tag-cmd"]["command"]
        .get("confirm")
        .is_none());

    let maint_bad = project_with_command(
        DataType::Bool,
        json!(false),
        CommandKind::Maintained,
        false,
        no_limits.clone(),
        None,
    );
    assert_eq!(
        compile_project(&maint_bad, &empty, live_opts),
        Err(CompileError::UnsupportedFeature(
            "unconfirmedSetpointOrMaintained"
        ))
    );

    // 4. Momentary preserves `confirm: true` when requested and omits it when false.
    let mom_confirm = project_with_command(
        DataType::Bool,
        json!(false),
        CommandKind::Momentary,
        true,
        no_limits.clone(),
        Some(500),
    );
    let mom_c = compile_project(&mom_confirm, &empty, live_opts).unwrap();
    let mom_c_json: serde_json::Value = serde_json::from_slice(mom_c.project_json()).unwrap();
    assert_eq!(
        mom_c_json["externalMappings"]["map-tag-cmd"]["command"]["confirm"],
        json!(true)
    );

    let mom_noconfirm = project_with_command(
        DataType::Bool,
        json!(false),
        CommandKind::Momentary,
        false,
        no_limits,
        Some(500),
    );
    let mom_nc = compile_project(&mom_noconfirm, &empty, live_opts).unwrap();
    let mom_nc_json: serde_json::Value = serde_json::from_slice(mom_nc.project_json()).unwrap();
    assert!(mom_nc_json["externalMappings"]["map-tag-cmd"]["command"]
        .get("confirm")
        .is_none());
}

fn project_with_opcua_write_policy(
    data_type: DataType,
    initial: serde_json::Value,
    command: CommandKind,
    confirmation_required: bool,
    limits: WriteLimits,
    pulse_ms: Option<u32>,
) -> Project {
    let mut p = absolute_project_with_label();
    p.connections.push(Connection {
        id: ConnectionId::new("conn-opc").unwrap(),
        name: "OPC UA".into(),
        driver: DriverKind::OpcUa,
        endpoint: Some("opc.tcp://10.0.0.10:4840".into()),
        parameters: BTreeMap::from([
            ("securityPolicy".into(), json!("None")),
            ("securityMode".into(), json!("None")),
        ]),
        credential_ref: None,
        extensions: BTreeMap::new(),
    });
    p.v2.industrial.connections.push(ConnectionPolicy {
        connection: ConnectionId::new("conn-opc").unwrap(),
        driver: DriverKindV2::OpcUa,
        commissioning_allowed: false,
    });
    p.tags.push(Tag {
        id: TagId::new("tag-cmd").unwrap(),
        name: "Cmd".into(),
        data_type,
        connection: Some(ConnectionId::new("conn-opc").unwrap()),
        address: Some("ns=2;s=Cmd".into()),
        access: TagAccess::ReadWrite,
        unit: None,
        range: None,
        initial_value: Some(initial),
        scan_rate_ms: 250,
        description: None,
        extensions: BTreeMap::new(),
    });
    p.v2.industrial.tags.push(TagWritePolicy {
        tag: TagId::new("tag-cmd").unwrap(),
        command,
        confirmation_required,
        critical: false,
        write_confirmed: true,
        pulse_ms,
        limits,
    });
    p
}

#[test]
fn f01_momentary_pulse_duration_boundaries_fail_closed() {
    let empty = BTreeMap::new();
    let live_opts = CompileOptions {
        mode: CompileMode::ProductionLive,
        ..CompileOptions::default()
    };

    // Out-of-range momentary pulses are valid in frozen schema v2, but MUST be
    // refused fail-closed by the format-2 compiler (Kotlin WriteRules: 100..=5000 ms).
    for bad_pulse in [1u32, 99, 5_001, 60_000] {
        let p = project_with_opcua_write_policy(
            DataType::Bool,
            json!(false),
            CommandKind::Momentary,
            false,
            WriteLimits::default(),
            Some(bad_pulse),
        );
        assert!(
            !hmi_project_model::validation::validate(&p).has_errors(),
            "canonical schema v2 must remain unchanged and accept pulse_ms={bad_pulse}"
        );
        assert_eq!(
            compile_project(&p, &empty, live_opts),
            Err(CompileError::UnsupportedFeature("momentaryPulseOutOfRange")),
            "compiler must refuse momentary pulse_ms={bad_pulse}"
        );
    }

    // Exact inclusive boundary values (100 ms and 5000 ms) compile verbatim and round-trip.
    for ok_pulse in [100u32, 5_000] {
        let p = project_with_opcua_write_policy(
            DataType::Bool,
            json!(false),
            CommandKind::Momentary,
            true,
            WriteLimits::default(),
            Some(ok_pulse),
        );
        let compiled = compile_project(&p, &empty, live_opts).unwrap();
        let doc: serde_json::Value = serde_json::from_slice(compiled.project_json()).unwrap();
        assert_eq!(
            doc["externalMappings"]["map-tag-cmd"]["command"]["pulseMs"],
            json!(ok_pulse)
        );
        let imported = import_portable(compiled.project_json());
        assert!(
            imported.report.has_complete_mapping(),
            "round-trip failed for pulseMs={ok_pulse}: {:?}",
            imported.report.findings
        );
    }
}

#[test]
fn f01_setpoint_missing_and_equal_limits_fail_closed() {
    let empty = BTreeMap::new();
    let live_opts = CompileOptions {
        mode: CompileMode::ProductionLive,
        ..CompileOptions::default()
    };

    // Missing min, missing max, or both missing are canonical-valid in schema v2,
    // but MUST be refused fail-closed by the format-2 compiler.
    for missing in [
        WriteLimits {
            min: None,
            max: None,
            step: None,
        },
        WriteLimits {
            min: Some(serde_json::Number::from(0)),
            max: None,
            step: None,
        },
        WriteLimits {
            min: None,
            max: Some(serde_json::Number::from(100)),
            step: None,
        },
        WriteLimits {
            min: None,
            max: None,
            step: Some(serde_json::Number::from(5)),
        },
    ] {
        let p = project_with_opcua_write_policy(
            DataType::Int32,
            json!(10),
            CommandKind::Setpoint,
            true,
            missing,
            None,
        );
        assert!(!hmi_project_model::validation::validate(&p).has_errors());
        assert_eq!(
            compile_project(&p, &empty, live_opts),
            Err(CompileError::UnsupportedFeature("missingSetpointLimits"))
        );
    }

    // Equal min == max across Int32, Int64, Float32, and Float64 are canonical-valid,
    // but Kotlin WriteRules requires strictly min < max -> MUST fail closed.
    let equal_cases = [
        (
            DataType::Int32,
            json!(50),
            serde_json::Number::from(50),
            serde_json::Number::from(50),
        ),
        (
            DataType::Int64,
            json!(9_007_199_254_740_992_i64),
            serde_json::Number::from(9_007_199_254_740_992_i64),
            serde_json::Number::from(9_007_199_254_740_992_i64),
        ),
        (
            DataType::Float32,
            json!(1.5),
            serde_json::Number::from_f64(1.5).unwrap(),
            serde_json::Number::from_f64(1.5).unwrap(),
        ),
        (
            DataType::Float64,
            json!(2.25),
            serde_json::Number::from_f64(2.25).unwrap(),
            serde_json::Number::from_f64(2.25).unwrap(),
        ),
    ];
    for (dt, init, min, max) in equal_cases {
        let p = project_with_opcua_write_policy(
            dt,
            init,
            CommandKind::Setpoint,
            true,
            WriteLimits {
                min: Some(min),
                max: Some(max),
                step: None,
            },
            None,
        );
        assert!(!hmi_project_model::validation::validate(&p).has_errors());
        assert_eq!(
            compile_project(&p, &empty, live_opts),
            Err(CompileError::UnsupportedFeature("invalidSetpointRange"))
        );
    }

    // Strictly ordered limits without step compile cleanly and preserve exact values.
    let ok = project_with_opcua_write_policy(
        DataType::Float64,
        json!(0.0),
        CommandKind::Setpoint,
        true,
        WriteLimits {
            min: Some(serde_json::Number::from_f64(-12.5).unwrap()),
            max: Some(serde_json::Number::from_f64(12.5).unwrap()),
            step: None,
        },
        None,
    );
    let compiled = compile_project(&ok, &empty, live_opts).unwrap();
    let doc: serde_json::Value = serde_json::from_slice(compiled.project_json()).unwrap();
    let cmd = &doc["externalMappings"]["map-tag-cmd"]["command"];
    assert_eq!(cmd["min"], json!(-12.5));
    assert_eq!(cmd["max"], json!(12.5));
    assert!(cmd.get("step").is_none());
}

#[test]
fn f02_int64_setpoint_exact_binary64_representability() {
    let empty = BTreeMap::new();
    let live_opts = CompileOptions {
        mode: CompileMode::ProductionLive,
        ..CompileOptions::default()
    };

    // Unrepresentable INT64 limits (not exact in IEEE 754 binary64 / Kotlin Double)
    // remain valid in frozen schema v2, but MUST be refused fail-closed without silent rounding.
    let lossy_cases: &[(i64, i64, Option<i64>)] = &[
        // 2^53 + 1 would round down to 2^53 and collapse distinct endpoints [2^53, 2^53+1]
        (9_007_199_254_740_992, 9_007_199_254_740_993, None),
        // -(2^53 + 1) in min
        (-9_007_199_254_740_993, 0, None),
        // 2^53 + 3 in max
        (0, 9_007_199_254_740_995, None),
        // i64::MAX (2^63 - 1) rounds up to 2^63 in binary64 (catches saturating-cast trap)
        (0, i64::MAX, None),
        // i64::MIN + 1 (-2^63 + 1) is not exact in binary64
        (i64::MIN + 1, 0, None),
        // Largest exact i64 + 1 (2^63 - 1023) is not exact in binary64
        (0, i64::MAX - 1022, None),
        // Lossy step on otherwise exact endpoints
        (0, 9_007_199_254_740_994, Some(9_007_199_254_740_993)),
    ];
    for &(min, max, step) in lossy_cases {
        let p = project_with_opcua_write_policy(
            DataType::Int64,
            json!(0_i64),
            CommandKind::Setpoint,
            true,
            WriteLimits {
                min: Some(serde_json::Number::from(min)),
                max: Some(serde_json::Number::from(max)),
                step: step.map(serde_json::Number::from),
            },
            None,
        );
        assert!(
            !hmi_project_model::validation::validate(&p).has_errors(),
            "canonical schema v2 must accept exact INT64 limits ({min}, {max}, {step:?})"
        );
        assert_eq!(
            compile_project(&p, &empty, live_opts),
            Err(CompileError::UnsupportedFeature("inexactSetpointLimit")),
            "compiler must refuse lossy binary64 INT64 limits ({min}, {max}, {step:?})"
        );
    }

    // Exact binary64 INT64 limits compile verbatim with zero silent adjustment.
    let exact_cases: &[(i64, i64, Option<i64>)] = &[
        (-9_007_199_254_740_992, 9_007_199_254_740_992, Some(1)),
        (9_007_199_254_740_992, 9_007_199_254_740_994, Some(2)),
        (i64::MIN, i64::MAX - 1023, Some(1024)),
    ];
    for &(min, max, step) in exact_cases {
        let p = project_with_opcua_write_policy(
            DataType::Int64,
            json!(0_i64),
            CommandKind::Setpoint,
            true,
            WriteLimits {
                min: Some(serde_json::Number::from(min)),
                max: Some(serde_json::Number::from(max)),
                step: step.map(serde_json::Number::from),
            },
            None,
        );
        let compiled = compile_project(&p, &empty, live_opts).unwrap();
        let doc: serde_json::Value = serde_json::from_slice(compiled.project_json()).unwrap();
        let cmd = &doc["externalMappings"]["map-tag-cmd"]["command"];
        assert_eq!(cmd["min"].as_i64(), Some(min));
        assert_eq!(cmd["max"].as_i64(), Some(max));
        assert_eq!(cmd["step"].as_i64(), step);
    }
}
