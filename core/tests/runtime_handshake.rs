use sahou_core::diag::Diag;
use sahou_core::endpoints::Endpoints;
use sahou_core::ir::descriptor_json;
use sahou_core::parse::parse_contract;
use sahou_core::runtime::{
    classify_delivery, contract_fragment, handshake_judge, handshake_verdict, load_descriptor,
    parse_reply_err, DeliveryClass, HandshakeOutcome,
};

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

fn desc_from(yaml: &str) -> sahou_core::ir::Descriptor {
    let c = parse_contract(yaml).unwrap();
    load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap()
}

fn norm(s: &str) -> String {
    s.replace("\r\n", "\n") // guard against CRLF checkout (lesson from ②a Task 2)
}

#[test]
fn additive_sender_is_accepted_breaking_is_blocked() {
    let demo = norm(DEMO);
    let base = desc_from(&demo);
    let additive_yaml = demo.replace(
        "        - { name: phase, type: enum, values: [down, move, up] }",
        "        - { name: phase, type: enum, values: [down, move, up] }\n        - { name: pressure, type: float, required: false }",
    );
    let additive = desc_from(&additive_yaml);
    let frag = contract_fragment(&additive, "touch").unwrap();
    let sender = additive.connections["touch"].hash.clone();
    assert_eq!(
        handshake_verdict("touch", &base.connections["touch"], &sender, &frag),
        HandshakeOutcome::Accepted,
        "additive should pass"
    );

    let breaking_yaml = demo.replace(
        "        - { name: x, type: float, min: 0, max: 1 }",
        "        - { name: x, type: string }",
    );
    let breaking = desc_from(&breaking_yaml);
    let frag = contract_fragment(&breaking, "touch").unwrap();
    let sender = breaking.connections["touch"].hash.clone();
    match handshake_verdict("touch", &base.connections["touch"], &sender, &frag) {
        HandshakeOutcome::Blocked { diags } => {
            assert_eq!(diags[0].code, "schema_incompatible");
            assert!(
                diags[0].path.contains("touch"),
                "the diagnostic path should carry the real connection name: {}",
                diags[0].path
            );
        }
        other => panic!("expected Blocked: {other:?}"),
    }
}

#[test]
fn promotion_is_blocked_at_delivery_boundary() {
    // approach A: promotion is also a NO at the delivery boundary
    let demo = norm(DEMO);
    let base = desc_from(&demo);
    let promoted_yaml = demo.replace(
        "- { name: level, type: int }",
        "- { name: level, type: float }",
    );
    let promoted = desc_from(&promoted_yaml);
    let frag = contract_fragment(&promoted, "get_state").unwrap();
    let sender = promoted.connections["get_state"].hash.clone();
    match handshake_verdict("get_state", &base.connections["get_state"], &sender, &frag) {
        HandshakeOutcome::Blocked { diags } => assert_eq!(diags[0].code, "schema_incompatible"),
        other => panic!("expected Blocked: {other:?}"),
    }
}

#[test]
fn undecodable_fragment_is_unreachable_not_blocked() {
    // the core of the 3-way split: "cannot judge" goes to the not-cached side (unreachable). Do not confuse with blocked.
    let base = desc_from(&norm(DEMO));
    match handshake_verdict(
        "touch",
        &base.connections["touch"],
        "deadbeef00000000",
        "{not json",
    ) {
        HandshakeOutcome::Unreachable { diags } => {
            assert_eq!(diags[0].code, "contract_unreachable");
            // unreachable is retryable (re-fetched on the next drift detection)
            assert_eq!(classify_delivery(false, &diags), DeliveryClass::Retryable);
        }
        other => panic!("expected Unreachable: {other:?}"),
    }
}

#[test]
fn fragment_hash_mismatch_is_unreachable() {
    // content-addressed sanity: the fetched fragment's self-reported hash ≠ requested hash → not used for judgement
    let demo = norm(DEMO);
    let base = desc_from(&demo);
    let frag = contract_fragment(&base, "touch").unwrap(); // a correct fragment, but…
    match handshake_verdict(
        "touch",
        &base.connections["touch"],
        "0000000000000000",
        &frag,
    ) {
        HandshakeOutcome::Unreachable { diags } => {
            assert_eq!(diags[0].code, "contract_unreachable");
            assert!(
                diags[0].message.contains("does not match"),
                "{}",
                diags[0].message
            );
        }
        other => panic!("expected Unreachable: {other:?}"),
    }
}

#[test]
fn handshake_outcome_serializes_as_tagged_verdict() {
    let base = desc_from(&norm(DEMO));
    let frag = contract_fragment(&base, "touch").unwrap();
    let sender = base.connections["touch"].hash.clone();
    let out = handshake_verdict("touch", &base.connections["touch"], &sender, &frag);
    assert_eq!(
        serde_json::to_string(&out).unwrap(),
        r#"{"verdict":"accepted"}"#
    );
}

#[test]
fn reply_err_envelope_roundtrips_diags() {
    let diags = vec![Diag::new("handler_error", "$", "boom")];
    let wire = serde_json::json!({ "diags": diags }).to_string();
    assert_eq!(parse_reply_err(wire.as_bytes()), diags);
}

#[test]
fn bad_reply_envelope_is_retryable_not_fatal() {
    // Fable Important-3: prevents the regression where a bad envelope became decode_error (FATAL) and was not resent
    for garbage in [&b"not json"[..], br#"{"diags":[]}"#, br#"{"other":1}"#, b""] {
        let diags = parse_reply_err(garbage);
        assert_eq!(diags[0].code, "bad_reply_envelope", "input={garbage:?}");
        assert_eq!(classify_delivery(false, &diags), DeliveryClass::Retryable);
    }
}

#[test]
fn delivery_classification_matches_smart_retry_policy() {
    let fatal = vec![Diag::new("type_mismatch", "$.x", "…")];
    assert_eq!(classify_delivery(false, &fatal), DeliveryClass::Fatal);
    let fatal2 = vec![Diag::new("schema_incompatible", "connections.touch", "…")];
    assert_eq!(classify_delivery(false, &fatal2), DeliveryClass::Fatal);
    assert_eq!(classify_delivery(true, &[]), DeliveryClass::Retryable);
    let pending = vec![Diag::new("handshake_pending", "$", "…")];
    assert_eq!(classify_delivery(false, &pending), DeliveryClass::Retryable);
    let server = vec![Diag::new("handler_error", "$", "…")];
    assert_eq!(classify_delivery(false, &server), DeliveryClass::Retryable);
}

#[test]
fn handshake_judge_unknown_connection_is_unreachable_retryable() {
    // an unknown connection in the handshake context = "cannot judge" = unreachable (not cached; retryable).
    // Do not confuse with the data-path unknown_connection (FATAL) (code alignment from the ②c backlog).
    let base = desc_from(&norm(DEMO));
    match handshake_judge(&base, "ghost", "deadbeef00000000", "{}") {
        HandshakeOutcome::Unreachable { diags } => {
            assert_eq!(diags[0].code, "contract_unreachable");
            assert!(diags[0].path.contains("ghost"), "{}", diags[0].path);
            assert_eq!(classify_delivery(false, &diags), DeliveryClass::Retryable);
        }
        other => panic!("expected Unreachable: {other:?}"),
    }
}

#[test]
fn handshake_judge_known_connection_delegates_to_verdict() {
    let base = desc_from(&norm(DEMO));
    let frag = contract_fragment(&base, "touch").unwrap();
    let sender = base.connections["touch"].hash.clone();
    assert_eq!(
        handshake_judge(&base, "touch", &sender, &frag),
        HandshakeOutcome::Accepted
    );
}
