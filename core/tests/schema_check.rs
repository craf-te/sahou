use sahou_core::fmt::serialize_contract;
use sahou_core::parse::parse_contract;
use sahou_core::schema_check::validate_schema;

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

fn codes_at(yaml: &str) -> Vec<(String, String)> {
    let c = parse_contract(yaml).unwrap();
    validate_schema(&c)
        .into_iter()
        .map(|d| (d.code, d.path))
        .collect()
}

#[test]
fn valid_demo_has_zero_diags() {
    let c = parse_contract(DEMO).unwrap();
    assert_eq!(validate_schema(&c), vec![]);
}

#[test]
fn detects_self_loop_and_unknown_node() {
    let yaml = "schema: s\nnodes:\n  a: {}\nconnections:\n  bad:\n    pattern: pub_sub\n    from: a\n    to: [a, ghost]\n    payload: { typing: any }\n";
    let codes = codes_at(yaml);
    assert!(
        codes.contains(&("self_loop".into(), "connections.bad.to[0]".into())),
        "{codes:?}"
    );
    assert!(
        codes.contains(&("unknown_node".into(), "connections.bad.to[1]".into())),
        "{codes:?}"
    );
}

#[test]
fn detects_empty_enum_and_duplicate_field() {
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - { name: mode, type: enum, values: [] }\n        - { name: x, type: float }\n        - { name: x, type: int }\n";
    let codes = codes_at(yaml);
    assert!(
        codes.contains(&(
            "empty_enum".into(),
            "connections.c.payload.fields[0].values".into()
        )),
        "{codes:?}"
    );
    assert!(
        codes.contains(&(
            "duplicate_field".into(),
            "connections.c.payload.fields[2].name".into()
        )),
        "{codes:?}"
    );
}

#[test]
fn detects_pattern_slot_mismatch() {
    // pub_sub without payload / query with payload
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  p:\n    pattern: pub_sub\n    from: a\n    to: [b]\n  q:\n    pattern: query\n    from: a\n    to: [b]\n    payload: { typing: any }\n";
    let codes = codes_at(yaml);
    assert!(
        codes.contains(&("missing_slot".into(), "connections.p.payload".into())),
        "{codes:?}"
    );
    assert!(
        codes.contains(&("unexpected_slot".into(), "connections.q.payload".into())),
        "{codes:?}"
    );
    assert!(
        codes.contains(&("missing_slot".into(), "connections.q.request".into())),
        "{codes:?}"
    );
    assert!(
        codes.contains(&("missing_slot".into(), "connections.q.response".into())),
        "{codes:?}"
    );
}

#[test]
fn detects_missing_items_and_invalid_range() {
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - { name: arr, type: array }\n        - { name: v, type: float, min: 2, max: 1 }\n";
    let codes = codes_at(yaml);
    assert!(
        codes.contains(&(
            "missing_items".into(),
            "connections.c.payload.fields[0].items".into()
        )),
        "{codes:?}"
    );
    assert!(
        codes.contains(&(
            "invalid_range".into(),
            "connections.c.payload.fields[1]".into()
        )),
        "{codes:?}"
    );
}

#[test]
fn bare_name_composite_items_are_validated_not_skipped() {
    // items: array / items: enum are shorthand (TypeSpec::Name). Before the fix, the Name arm of
    // check_typespec did nothing, so the inner type went completely unchecked (silent yes).
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - { name: arr, type: array, items: array }\n        - { name: e, type: array, items: enum }\n";
    let codes = codes_at(yaml);
    // the inner side of items: array (shorthand) also requires items
    assert!(
        codes.contains(&(
            "missing_items".into(),
            "connections.c.payload.fields[0].items.items".into()
        )),
        "{codes:?}"
    );
    // an empty values inside items: enum (shorthand) is also detected
    assert!(
        codes.contains(&(
            "empty_enum".into(),
            "connections.c.payload.fields[1].items.values".into()
        )),
        "{codes:?}"
    );
}

#[test]
fn non_finite_bound_is_rejected_finite_bound_is_not() {
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - { name: v, type: float, min: .nan }\n        - { name: w, type: float, min: 0, max: 1 }\n";
    let codes = codes_at(yaml);
    assert!(
        codes.contains(&(
            "non_finite_bound".into(),
            "connections.c.payload.fields[0]".into()
        )),
        "{codes:?}"
    );
    // a normal finite min/max field does not emit non_finite_bound
    assert!(
        !codes
            .iter()
            .any(|(code, path)| code == "non_finite_bound" && path.contains("fields[1]")),
        "{codes:?}"
    );
}

#[test]
fn invalid_default_is_no() {
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - { name: level, type: int, default: \"abc\" }\n";
    let diags = validate_schema(&parse_contract(yaml).unwrap());
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "invalid_default");
    assert_eq!(diags[0].path, "connections.c.payload.fields[0].default");
    assert!(diags[0].message.contains("'level'"), "{}", diags[0].message);
}

#[test]
fn valid_default_passes_but_out_of_range_is_no() {
    let ok = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - { name: level, type: int, default: 3, min: 0, max: 10 }\n";
    assert!(validate_schema(&parse_contract(ok).unwrap()).is_empty());
    let bad = ok.replace("default: 3", "default: 42");
    let diags = validate_schema(&parse_contract(&bad).unwrap());
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "invalid_default"); // out-of-range is also part of the default type-consistency NO
}

#[test]
fn nested_group_default_is_checked() {
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - name: meta\n          type: group\n          fields:\n            - { name: ts, type: timestamp, default: \"not-epoch\" }\n";
    let diags = validate_schema(&parse_contract(yaml).unwrap());
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "invalid_default");
    assert_eq!(
        diags[0].path,
        "connections.c.payload.fields[0].fields[0].default"
    );
}

#[test]
fn selector_roundtrips_and_is_query_only() {
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  q:\n    pattern: query\n    from: a\n    to: [b]\n    selector: \"?level=info\"\n    request: { typing: any }\n    response: { typing: any }\n";
    let c = parse_contract(yaml).unwrap();
    assert_eq!(c.connections["q"].selector.as_deref(), Some("?level=info"));
    assert!(validate_schema(&c).is_empty());
    // serialize roundtrip (constraint ①)
    let back = parse_contract(&serialize_contract(&c)).unwrap();
    assert_eq!(c, back);
}

#[test]
fn selector_on_pub_sub_is_no() {
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    selector: \"?x=1\"\n    payload: { typing: any }\n";
    let diags = validate_schema(&parse_contract(yaml).unwrap());
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "unexpected_selector");
    assert_eq!(diags[0].path, "connections.c.selector");
}

#[test]
fn broken_type_field_skips_default_check() {
    // when the type definition itself is broken (no items), missing_items is the primary diagnostic — don't double-report in the default check
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - { name: pts, type: array, default: [1] }\n";
    let diags = validate_schema(&parse_contract(yaml).unwrap());
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "missing_items");
}

#[test]
fn reserved_sahou_key_prefix_is_rejected() {
    // '@sahou/...' is the sahou admin space (contract + vitals queryables); a user key
    // override colliding with it would corrupt the diagnostics space (spec §3).
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    key: '@sahou/boom'\n    payload: { typing: any }\n";
    let codes = codes_at(yaml);
    assert!(
        codes.contains(&("reserved_key".into(), "connections.c.key".into())),
        "{codes:?}"
    );
}

#[test]
fn reserved_sahou_chunk_mid_key_is_also_rejected() {
    // strict rule: a mid-key '@sahou' chunk has no legitimate use and can still
    // collide with wildcard selectors
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    key: 'stage/@sahou/boom'\n    payload: { typing: any }\n";
    let codes = codes_at(yaml);
    assert!(
        codes.contains(&("reserved_key".into(), "connections.c.key".into())),
        "{codes:?}"
    );
}

#[test]
fn ordinary_key_override_is_not_reserved() {
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    key: 'stage/custom'\n    payload: { typing: any }\n";
    let codes = codes_at(yaml);
    assert!(
        !codes.iter().any(|(code, _)| code == "reserved_key"),
        "{codes:?}"
    );
}

#[test]
fn reserved_sahou_connection_id_is_rejected() {
    // the default key is derived as `<ns>/<conn_id>`, so a connection *named* '@sahou/...'
    // collides with the admin space without any key override (final-review finding)
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  '@sahou/vitals/x':\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload: { typing: any }\n";
    let codes = codes_at(yaml);
    assert!(
        codes.contains(&("reserved_key".into(), "connections.@sahou/vitals/x".into())),
        "{codes:?}"
    );
}
