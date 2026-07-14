use sahou_core::endpoints::Endpoints;
use sahou_core::ir::descriptor_json;
use sahou_core::parse::parse_contract;
use sahou_core::runtime::{accept_sample, load_descriptor, prepare_publish, AcceptOutcome};

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

fn desc() -> sahou_core::ir::Descriptor {
    let c = parse_contract(DEMO).unwrap();
    load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap()
}

#[test]
fn valid_payload_roundtrips_prepare_to_accept() {
    let d = desc();
    let msg = prepare_publish(
        &d,
        "sensor",
        "touch",
        r#"{"x":0.5,"phase":"move","meta":{"ts":1752105600000}}"#,
        0,
    )
    .unwrap();
    assert_eq!(msg.key, "sahou/touch");
    assert_eq!(msg.attachment, d.connections["touch"].hash);
    assert_eq!(
        msg.qos.reliability,
        sahou_core::contract::Reliability::Reliable
    );
    // receiver side (visuals is a `to` of touch)
    let out = accept_sample(
        &d,
        "visuals",
        "touch",
        msg.wire.as_bytes(),
        Some(&msg.attachment),
        0,
        None,
    );
    match out {
        AcceptOutcome::Accept { payload } => {
            let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
            assert_eq!(v["x"], 0.5);
        }
        other => panic!("expected Accept: {other:?}"),
    }
}

#[test]
fn send_boundary_rejects_and_role_pattern_checked() {
    let d = desc();
    // type NG (send boundary)
    let diags = prepare_publish(
        &d,
        "sensor",
        "touch",
        r#"{"x":"bad","phase":"move","meta":{"ts":1}}"#,
        0,
    )
    .unwrap_err();
    assert_eq!(diags[0].code, "type_mismatch");
    assert_eq!(diags[0].path, "$.x");
    // a non-`from` node publishes -> role_mismatch
    let diags = prepare_publish(&d, "visuals", "touch", r#"{}"#, 0).unwrap_err();
    assert_eq!(diags[0].code, "role_mismatch");
    // publishing on a query connection -> pattern_mismatch
    let diags = prepare_publish(&d, "sensor", "get_state", r#"{}"#, 0).unwrap_err();
    assert_eq!(diags[0].code, "pattern_mismatch");
    // nonexistent connection
    let diags = prepare_publish(&d, "sensor", "ghost", r#"{}"#, 0).unwrap_err();
    assert_eq!(diags[0].code, "unknown_connection");
    // broken JSON
    let diags = prepare_publish(&d, "sensor", "touch", "{not json", 0).unwrap_err();
    assert_eq!(diags[0].code, "decode_error");
}

#[test]
fn recv_boundary_rejects_bad_payload_and_missing_hash() {
    let d = desc();
    let hash = d.connections["touch"].hash.clone();
    // broken raw that bypasses validation -> recv boundary NO
    let out = accept_sample(
        &d,
        "visuals",
        "touch",
        br#"{"x":"bad","phase":"move","meta":{"ts":1}}"#,
        Some(&hash),
        0,
        None,
    );
    match out {
        AcceptOutcome::Reject { diags } => assert_eq!(diags[0].code, "type_mismatch"),
        other => panic!("expected Reject: {other:?}"),
    }
    // no attachment -> missing_schema_hash (conservative NO; never silently passes)
    let out = accept_sample(&d, "visuals", "touch", br#"{}"#, None, 0, None);
    match out {
        AcceptOutcome::Reject { diags } => assert_eq!(diags[0].code, "missing_schema_hash"),
        other => panic!("expected Reject: {other:?}"),
    }
}

#[test]
fn hash_mismatch_and_trusted_path() {
    let d = desc();
    let wire = br#"{"x":0.5,"phase":"move","meta":{"ts":1}}"#;
    // unknown hash -> HashMismatch (engine routes to the handshake path)
    let out = accept_sample(
        &d,
        "visuals",
        "touch",
        wire,
        Some("deadbeef00000000"),
        0,
        None,
    );
    assert!(
        matches!(out, AcceptOutcome::HashMismatch { ref sender_hash } if sender_hash == "deadbeef00000000")
    );
    // verdict already cached (trusted) -> skip hash matching, validate, and accept
    let out = accept_sample(
        &d,
        "visuals",
        "touch",
        wire,
        Some("deadbeef00000000"),
        0,
        Some("deadbeef00000000"),
    );
    assert!(matches!(out, AcceptOutcome::Accept { .. }));
    // even when trusted, a broken type is NO (validation is never skipped)
    let out = accept_sample(
        &d,
        "visuals",
        "touch",
        br#"{"x":"bad","phase":"move","meta":{"ts":1}}"#,
        Some("deadbeef00000000"),
        0,
        Some("deadbeef00000000"),
    );
    assert!(matches!(out, AcceptOutcome::Reject { .. }));
}

#[test]
fn sampled_level_validates_deterministically_1_in_10() {
    // demo's debug_tap is typing:any, so verify sampled validation with a contract that rewrites points (typed) to validate:sampled
    // note: the repo uses core.autocrlf=true (CRLF line endings), so normalize to LF before replacing
    // (the replacement pattern itself matches the brief; only the line ending is made environment-independent).
    let yaml = DEMO.replace("\r\n", "\n").replace(
        "  points:\n    pattern: pub_sub",
        "  points:\n    validate: sampled\n    pattern: pub_sub",
    );
    let c = parse_contract(&yaml).unwrap();
    let d = load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap();
    let bad = r#"{"pts":[["bad"]]}"#;
    // seq=0 is a multiple of 10 -> validated and NO
    assert!(prepare_publish(&d, "sensor", "points", bad, 0).is_err());
    // seq=1..9 skips validation -> passes (this is what sampled means)
    assert!(prepare_publish(&d, "sensor", "points", bad, 1).is_ok());
    assert!(prepare_publish(&d, "sensor", "points", bad, 9).is_ok());
    // seq=10 -> validated again
    assert!(prepare_publish(&d, "sensor", "points", bad, 10).is_err());
}
