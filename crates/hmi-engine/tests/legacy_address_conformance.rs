//! Differential admission against the production parsers, without device I/O.
//! The importer module is compiled here only for tests; no engine dependency
//! is added to the portable importer.
#[path = "../../legacy-import/src/address.rs"]
mod admission;

use hmi_engine::driver::s7_address::S7Address;
use hmi_engine::protocol::ValueType;
use opcua::types::NodeId;

#[test]
fn admitted_s7_addresses_match_production_parser() {
    use ValueType::*;
    for (address, canonical_type, engine_type, writable) in [
        ("DB1.X0.7", "bool", Bool, true),
        ("DB1.DBX0.7", "bool", Bool, false),
        ("M10.3", "bool", Bool, true),
        ("E0.0", "bool", Bool, false),
        ("A4.1", "bool", Bool, true),
        ("db01.sint2", "int32", Int, true),
        ("DB1.BYTE0", "int32", Int, true),
        ("DB1.USINT0", "int64", Long, true),
        ("DB1.WORD0", "int32", Int, false),
        ("DB1.UINT0", "int64", Long, true),
        ("DB1.INT0", "int32", Int, true),
        ("DB1.DINT4", "int64", Long, true),
        ("DB1.DWORD0", "int64", Long, true),
        ("DB1.UDINT0", "int64", Long, true),
        ("DB1.REAL4", "float32", Float, true),
        ("DB1.REAL4", "float64", Double, false),
        ("DB1.LREAL8", "float64", Double, true),
        ("DB1.STRING20.32", "string", String, false),
        ("DB1.INT65534", "int32", Int, false),
        ("DB1.X8191.7", "bool", Bool, false),
    ] {
        let span = admission::s7(address, canonical_type, writable)
            .unwrap_or_else(|| panic!("importer rejected {address}"));
        let actual =
            S7Address::parse(address).unwrap_or_else(|e| panic!("engine rejected {address}: {e}"));
        assert_eq!((span.start, span.end), actual.bytes(), "{address}");
        assert!(actual.kind.readable_as(engine_type), "{address}");
        assert!(!span.area.is_empty());
        if let Some((min, max)) = span.integer_range {
            assert!(min < max);
        }
    }
    for address in ["DB1.DBW0", "DB0.INT0", "DB1.INT65535", "DB1.X8192.0"] {
        assert!(admission::s7(address, "int32", false).is_none());
        assert!(S7Address::parse(address).is_err());
    }
    // Intentional stricter rules: no width/type ambiguity, physical write
    // narrowing, input writes or string writes during migration.
    for (address, ty) in [
        ("I.X0.0", "bool"),
        ("DB1.REAL0", "float64"),
        ("DB1.STRING0.20", "string"),
        ("DB1.UDINT0", "int32"),
    ] {
        assert!(S7Address::parse(address).is_ok());
        assert!(admission::s7(address, ty, true).is_none());
    }
}

#[test]
fn admitted_fixed_opcua_nodes_match_production_node_parser() {
    // The engine's fixed-target branch calls this same NodeId parser.
    for address in [
        "ns=2;i=100",
        "ns=02;i=0100",
        "ns=2;s=Demo.Counter",
        "ns=0;i=2255",
    ] {
        let normalized = admission::opcua(address).expect("admitted address");
        let original = address.parse::<NodeId>().expect("engine node parser");
        let canonical = normalized.parse::<NodeId>().expect("normalized node");
        assert_eq!(original, canonical);
    }
    for address in ["ns=65536;i=1", "ns=2;i=4294967296", "invalid"] {
        assert!(admission::opcua(address).is_none());
        assert!(address.parse::<NodeId>().is_err());
    }
    // URI namespaces, GUID and opaque identifiers are deliberately outside
    // the current importer subset; their aliases require separate review.
    assert!(admission::opcua("nsu=urn:synthetic;s=Counter").is_none());
}
