use sahou_core::endpoints::Endpoints;
use sahou_core::ir::descriptor_json;
use sahou_core::parse::parse_contract;
use sahou_core::runtime::{
    accept_reply, accept_request, load_descriptor, prepare_reply, prepare_request, AcceptOutcome,
};

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

fn desc() -> sahou_core::ir::Descriptor {
    let c = parse_contract(DEMO).unwrap();
    load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap()
}

// demo's get_state: from=sensor (requester) / to=[archive] (responder)
// request = {sel: string(max_len 64)} / response = {level: int}

#[test]
fn four_boundaries_roundtrip() {
    let d = desc();
    // ① send request (requester)
    let req = prepare_request(&d, "sensor", "get_state", r#"{"sel":"levels"}"#, 0).unwrap();
    assert_eq!(req.key, "sahou/get_state");
    // ② receive request (responder)
    let out = accept_request(
        &d,
        "archive",
        "get_state",
        req.wire.as_bytes(),
        Some(&req.attachment),
        0,
        None,
    );
    assert!(matches!(out, AcceptOutcome::Accept { .. }));
    // ③ send response (responder)
    let rep = prepare_reply(&d, "archive", "get_state", r#"{"level":3}"#, 0).unwrap();
    // ④ receive response (requester)
    let out = accept_reply(
        &d,
        "sensor",
        "get_state",
        rep.wire.as_bytes(),
        Some(&rep.attachment),
        0,
        None,
    );
    match out {
        AcceptOutcome::Accept { payload } => {
            let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
            assert_eq!(v["level"], 3);
        }
        other => panic!("expected Accept: {other:?}"),
    }
}

#[test]
fn each_boundary_rejects_its_own_violation() {
    let d = desc();
    // ① a broken request is NO before get (send boundary)
    let diags = prepare_request(&d, "sensor", "get_state", r#"{"sel":123}"#, 0).unwrap_err();
    assert_eq!(diags[0].code, "type_mismatch");
    // ② responder recv boundary (raw that bypasses validation)
    let hash = d.connections["get_state"].hash.clone();
    let out = accept_request(
        &d,
        "archive",
        "get_state",
        br#"{"sel":123}"#,
        Some(&hash),
        0,
        None,
    );
    assert!(matches!(out, AcceptOutcome::Reject { .. }));
    // ③ a broken response is NO before reply (responder send boundary)
    let diags = prepare_reply(&d, "archive", "get_state", r#"{"level":"high"}"#, 0).unwrap_err();
    assert_eq!(diags[0].code, "type_mismatch");
    // ④ requester reply-recv boundary
    let out = accept_reply(
        &d,
        "sensor",
        "get_state",
        br#"{"level":"high"}"#,
        Some(&hash),
        0,
        None,
    );
    assert!(matches!(out, AcceptOutcome::Reject { .. }));
}

#[test]
fn roles_are_directional() {
    let d = desc();
    // responder tries to send a request -> role_mismatch
    let diags = prepare_request(&d, "archive", "get_state", r#"{"sel":"x"}"#, 0).unwrap_err();
    assert_eq!(diags[0].code, "role_mismatch");
    // requester tries to reply -> role_mismatch
    let diags = prepare_reply(&d, "sensor", "get_state", r#"{"level":1}"#, 0).unwrap_err();
    assert_eq!(diags[0].code, "role_mismatch");
    // a query operation on a pub_sub connection -> pattern_mismatch
    let diags = prepare_request(&d, "sensor", "touch", r#"{}"#, 0).unwrap_err();
    assert_eq!(diags[0].code, "pattern_mismatch");
}
