//! Vitals format tests: build (vitals_payload) and read (parse_vitals) both live in the core
//! so every language reports and reads byte-identically (spec: notes/sahou-vitals-spec.md §2).
use sahou_core::endpoints::Endpoints;
use sahou_core::ir::descriptor_json;
use sahou_core::parse::parse_contract;
use sahou_core::runtime::load_descriptor;
use sahou_core::vitals::{parse_vitals, vitals_key, vitals_payload, VITALS_FORMAT};

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

fn demo_desc() -> sahou_core::ir::Descriptor {
    let c = parse_contract(&DEMO.replace("\r\n", "\n")).unwrap();
    load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap()
}

const INFO: &str = r#"{"lang":"python","sahou":"0.0.2","zenoh":"1.9.0","transport":"native","uptime_secs":312,"handshake":{"touch":{"cd34":"accepted"}}}"#;

#[test]
fn payload_roundtrips_and_reports_descriptor_generation() {
    let desc = demo_desc();
    let json = vitals_payload(&desc, "sensor", INFO).unwrap();
    let v = parse_vitals(&json).unwrap();
    assert_eq!(v.vitals_format, VITALS_FORMAT);
    assert_eq!(v.node, "sensor");
    assert_eq!(v.namespace, desc.namespace);
    assert_eq!(v.schema, desc.schema);
    assert_eq!(v.schema_version, desc.version);
    // generation = the per-connection hashes this node runs (spec §1.2)
    assert_eq!(v.connections["touch"].role, "from");
    assert_eq!(v.connections["touch"].hash, desc.connections["touch"].hash);
    assert_eq!(v.runtime.lang, "python");
    assert_eq!(v.runtime.zenoh.as_deref(), Some("1.9.0"));
    assert_eq!(v.uptime_secs, 312);
    assert_eq!(v.handshake["touch"]["cd34"], "accepted");
}

#[test]
fn receiver_role_is_to_and_uninvolved_connections_are_omitted() {
    let desc = demo_desc();
    let v = parse_vitals(&vitals_payload(&desc, "visuals", INFO).unwrap()).unwrap();
    assert_eq!(v.connections["touch"].role, "to");
    // visuals is not a party to the sensor->archive query
    assert!(
        !v.connections.contains_key("get_state"),
        "{:?}",
        v.connections
    );
}

#[test]
fn payload_is_deterministic() {
    let desc = demo_desc();
    assert_eq!(
        vitals_payload(&desc, "sensor", INFO).unwrap(),
        vitals_payload(&desc, "sensor", INFO).unwrap()
    );
}

#[test]
fn unknowable_fields_are_omitted_not_faked() {
    // a runtime that cannot learn its zenoh version omits it (spec §1.2)
    let desc = demo_desc();
    let info = r#"{"lang":"c++","sahou":"0.0.2","transport":"native"}"#;
    let json = vitals_payload(&desc, "sensor", info).unwrap();
    assert!(!json.contains("\"zenoh\""), "{json}");
    let v = parse_vitals(&json).unwrap();
    assert_eq!(v.runtime.zenoh, None);
    assert_eq!(v.uptime_secs, 0); // defaulted, not an error
}

#[test]
fn unknown_node_is_a_structured_no() {
    let desc = demo_desc();
    let err = vitals_payload(&desc, "ghost", INFO).unwrap_err();
    assert_eq!(err[0].code, "unknown_node");
}

#[test]
fn broken_runtime_info_is_a_structured_no() {
    let desc = demo_desc();
    let err = vitals_payload(&desc, "sensor", "{not json").unwrap_err();
    assert_eq!(err[0].code, "vitals_bad_runtime_info");
}

#[test]
fn parse_rejects_newer_format_with_upgrade_hint() {
    let err = parse_vitals(r#"{"vitals_format":2,"node":"x"}"#).unwrap_err();
    assert_eq!(err[0].code, "vitals_format_unsupported");
    assert!(err[0].message.contains("upgrade"), "{}", err[0].message);
}

#[test]
fn parse_rejects_non_vitals_payloads() {
    assert_eq!(
        parse_vitals("{not json").unwrap_err()[0].code,
        "vitals_unreadable"
    );
    assert_eq!(
        parse_vitals(r#"{"hello":"world"}"#).unwrap_err()[0].code,
        "vitals_unreadable"
    );
}

#[test]
fn parse_ignores_unknown_fields_within_a_known_format() {
    // wire-layer tolerance: a same-format payload with extra fields still parses
    let desc = demo_desc();
    let json = vitals_payload(&desc, "sensor", INFO).unwrap();
    let with_extra = json.replacen('{', r#"{"future_field":42,"#, 1);
    assert!(parse_vitals(&with_extra).is_ok());

    let with_nested_extra = json.replacen(r#""runtime":{"#, r#""runtime":{"future_field":42,"#, 1);
    assert!(parse_vitals(&with_nested_extra).is_ok());
}

#[test]
fn parse_rejects_non_integer_format_as_unreadable() {
    let err = parse_vitals(r#"{"vitals_format":"1","node":"x"}"#).unwrap_err();
    assert_eq!(err[0].code, "vitals_unreadable");
    assert!(
        err[0].message.contains("unsigned integer"),
        "{}",
        err[0].message
    );
}

#[test]
fn vitals_key_is_namespaced_admin_space() {
    let desc = demo_desc();
    assert_eq!(
        vitals_key(&desc, "sensor"),
        format!("{}/@sahou/vitals/sensor", desc.namespace)
    );
}
