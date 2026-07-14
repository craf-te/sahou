use sahou_core::parse::parse_contract;
use sahou_core::payload::validate_payload;
use sahou_core::sample::sample_slot;

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

#[test]
fn samples_of_all_demo_slots_validate_against_their_own_type() {
    // core property: sample(slot) always passes validate
    let c = parse_contract(DEMO).unwrap();
    for (id, conn) in &c.connections {
        for slot in [&conn.payload, &conn.request, &conn.response]
            .into_iter()
            .flatten()
        {
            let sample = sample_slot(slot);
            let diags = validate_payload(slot, &sample);
            assert_eq!(
                diags,
                vec![],
                "sample for connection '{id}' is invalid: {sample}"
            );
        }
    }
}

#[test]
fn sample_respects_min_and_enum() {
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - { name: gain, type: float, min: 0.5, max: 2 }\n        - { name: mode, type: enum, values: [alpha, beta] }\n";
    let c = parse_contract(yaml).unwrap();
    let slot = c.connections["c"].payload.as_ref().unwrap();
    let v = sample_slot(slot);
    assert_eq!(v["gain"], serde_json::json!(0.5)); // respects min
    assert_eq!(v["mode"], serde_json::json!("alpha")); // first candidate
}

#[test]
fn sample_truncates_string_to_max_len() {
    // even when max_len < "sample".len() (6 chars), returns a valid sample
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - { name: code, type: string, max_len: 3 }\n";
    let c = parse_contract(yaml).unwrap();
    let slot = c.connections["c"].payload.as_ref().unwrap();
    let v = sample_slot(slot);
    let diags = validate_payload(slot, &v);
    assert_eq!(diags, vec![], "sample for max_len:3 is invalid: {v}");
    assert_eq!(v["code"], serde_json::json!("sam"));
}

#[test]
fn sample_respects_max_without_min() {
    // when max is present but min is absent, verify the sample stays within range
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - { name: f_neg, type: float, max: -5 }\n        - { name: f_pos, type: float, max: 10 }\n        - { name: i_neg, type: int, max: -5 }\n        - { name: i_pos, type: int, max: 10 }\n";
    let c = parse_contract(yaml).unwrap();
    let slot = c.connections["c"].payload.as_ref().unwrap();
    let v = sample_slot(slot);
    let diags = validate_payload(slot, &v);
    assert_eq!(diags, vec![], "sample with only max is invalid: {v}");
    // f_neg: max=-5, no min -> should pick -5.0
    assert_eq!(v["f_neg"], serde_json::json!(-5.0));
    // f_pos: max=10, no min -> should pick 0.0 (0 <= 10)
    assert_eq!(v["f_pos"], serde_json::json!(0.0));
    // i_neg: max=-5, no min -> should pick -5
    assert_eq!(v["i_neg"], serde_json::json!(-5));
    // i_pos: max=10, no min -> should pick 0 (0 <= 10)
    assert_eq!(v["i_pos"], serde_json::json!(0));
}
