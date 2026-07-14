use sahou_core::compat::{changed_connections, classify, handshake, is_compatible, ChangeKind};
use sahou_core::parse::parse_contract;

const BASE: &str = "schema: s\nversion: 1\nnodes:\n  a: {}\n  b: {}\nconnections:\n  touch:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - { name: x, type: float, min: 0, max: 1 }\n        - { name: phase, type: enum, values: [down, up] }\n  cue:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - { name: id, type: int }\n";

fn kinds(old: &str, new: &str) -> Vec<(ChangeKind, String)> {
    let o = parse_contract(old).unwrap();
    let n = parse_contract(new).unwrap();
    classify(&o, &n)
        .into_iter()
        .map(|c| (c.kind, c.path))
        .collect()
}

#[test]
fn additive_changes_are_compatible() {
    // add optional + add enum value + relax range
    let new = BASE
        .replace(
            "        - { name: phase, type: enum, values: [down, up] }",
            "        - { name: phase, type: enum, values: [down, up, move] }\n        - { name: pressure, type: float, required: false }",
        )
        .replace("min: 0, max: 1", "min: 0, max: 2");
    let o = parse_contract(BASE).unwrap();
    let n = parse_contract(&new).unwrap();
    let changes = classify(&o, &n);
    assert!(!changes.is_empty());
    assert!(is_compatible(&changes), "{changes:?}");
    // blast radius: cue is unchanged
    assert_eq!(changed_connections(&o, &n), vec!["touch"]);
}

#[test]
fn breaking_changes_are_detected_with_path() {
    // add required + change type
    let new = BASE.replace(
        "        - { name: x, type: float, min: 0, max: 1 }",
        "        - { name: x, type: string }\n        - { name: device_id, type: int }",
    );
    let ks = kinds(BASE, &new);
    assert!(
        ks.contains(&(
            ChangeKind::Breaking,
            "connections.touch.payload.fields[0].type".into()
        )),
        "{ks:?}"
    );
    assert!(
        ks.contains(&(
            ChangeKind::Breaking,
            // new contract fields = [x(0), device_id(1), phase(2)]
            "connections.touch.payload.fields[1]".into()
        )),
        "{ks:?}"
    );
}

#[test]
fn int_to_float_is_promotion() {
    let new = BASE.replace("- { name: id, type: int }", "- { name: id, type: float }");
    let ks = kinds(BASE, &new);
    assert!(
        ks.contains(&(
            ChangeKind::Promotion,
            "connections.cue.payload.fields[0].type".into()
        )),
        "{ks:?}"
    );
    let o = parse_contract(BASE).unwrap();
    let n = parse_contract(&new).unwrap();
    assert!(is_compatible(&classify(&o, &n)));
}

#[test]
fn field_removal_and_new_connection() {
    let new = BASE.replace(
        "        - { name: phase, type: enum, values: [down, up] }\n",
        "",
    );
    let ks = kinds(BASE, &new);
    assert!(
        ks.contains(&(
            ChangeKind::Breaking,
            // removals use the old-contract index. old fields = [x(0), phase(1)]
            "connections.touch.payload.fields[1]".into()
        )),
        "{ks:?}"
    );
    // a new connection is additive
    let with_new = format!("{BASE}  extra:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload: {{ typing: any }}\n");
    let ks2 = kinds(BASE, &with_new);
    assert!(
        ks2.contains(&(ChangeKind::Additive, "connections.extra".into())),
        "{ks2:?}"
    );
}

#[test]
fn key_change_is_breaking() {
    // changing key (keyexpr override) changes the delivery keyexpr itself = breaking.
    // topology (pattern/from/to) is unchanged, but before the classify fix key was not inspected at all,
    // so is_compatible wrongly returned true (silent yes).
    let old_with_key = BASE.replace(
        "  touch:\n    pattern: pub_sub\n    from: a\n    to: [b]\n",
        "  touch:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    key: custom/a\n",
    );
    let new_with_key = BASE.replace(
        "  touch:\n    pattern: pub_sub\n    from: a\n    to: [b]\n",
        "  touch:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    key: custom/b\n",
    );
    let ks = kinds(&old_with_key, &new_with_key);
    assert!(
        ks.contains(&(ChangeKind::Breaking, "connections.touch.key".into())),
        "{ks:?}"
    );
    let o = parse_contract(&old_with_key).unwrap();
    let n = parse_contract(&new_with_key).unwrap();
    assert!(!is_compatible(&classify(&o, &n)));
}

#[test]
fn handshake_accepts_additive_rejects_breaking() {
    // user's live-rollout scenario (Z26): only the sender has the new contract
    let added = BASE.replace(
        "        - { name: phase, type: enum, values: [down, up] }",
        "        - { name: phase, type: enum, values: [down, up] }\n        - { name: pressure, type: float, required: false }",
    );
    let receiver = parse_contract(BASE).unwrap();
    let sender = parse_contract(&added).unwrap();
    assert!(
        handshake(&receiver, &sender, "touch").is_ok(),
        "additive is allowed"
    );
    assert!(
        handshake(&receiver, &sender, "cue").is_ok(),
        "an unrelated connection is unaffected"
    );

    let broken = BASE.replace(
        "- { name: x, type: float, min: 0, max: 1 }",
        "- { name: x, type: string }",
    );
    let sender2 = parse_contract(&broken).unwrap();
    let err = handshake(&receiver, &sender2, "touch").unwrap_err();
    assert_eq!(err[0].code, "schema_incompatible");
    assert!(err[0].path.contains("connections.touch"), "{}", err[0].path);
}

#[test]
fn handshake_rejects_promotion_in_both_directions() {
    // policy A (conservative): at the delivery boundary, an int<->float mismatch is NO in either direction.
    // undirected classify treats int->float as promotion=compatible, but delivery is a directional problem,
    // and allowing undirected promotion would let the dangerous direction (recv=int/send=float -> type_mismatch on every message)
    // pass as a false YES.
    let int_c = parse_contract(BASE).unwrap();
    let float_src = BASE.replace("- { name: id, type: int }", "- { name: id, type: float }");
    let float_c = parse_contract(&float_src).unwrap();

    // dangerous direction (recv=int / send=float): the old handshake wrongly passed it as promotion -> NO under A
    let err1 = handshake(&int_c, &float_c, "cue").unwrap_err();
    assert_eq!(err1[0].code, "schema_incompatible");
    assert!(err1[0].path.contains("connections.cue"), "{}", err1[0].path);

    // safe direction (recv=float / send=int): classify says breaking -> conservatively NO (a safe false NO)
    let err2 = handshake(&float_c, &int_c, "cue").unwrap_err();
    assert_eq!(err2[0].code, "schema_incompatible");

    // however is_compatible/classify stay undirected for tooling decisions (an intentional discrepancy)
    assert!(
        is_compatible(&classify(&int_c, &float_c)),
        "tooling: int->float is still compatible under undirected classify"
    );
}

#[test]
fn paths_are_index_form_with_name_in_detail() {
    // unified grammar (spec §4): path = structural position (index form); the field name is always included in detail
    let new = BASE.replace(
        "        - { name: x, type: float, min: 0, max: 1 }",
        "        - { name: x, type: string }",
    );
    let o = parse_contract(BASE).unwrap();
    let n = parse_contract(&new).unwrap();
    let changes = classify(&o, &n);
    let ch = changes
        .iter()
        .find(|c| c.path == "connections.touch.payload.fields[0].type")
        .expect("an index-form path is produced");
    assert!(
        ch.detail.contains("'x'"),
        "field name goes in detail: {}",
        ch.detail
    );
}

#[test]
fn selector_change_is_breaking() {
    let base_q = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  q:\n    pattern: query\n    from: a\n    to: [b]\n    request: { typing: any }\n    response: { typing: any }\n";
    let with_sel = base_q.replace(
        "    request: { typing: any }",
        "    selector: \"?level=info\"\n    request: { typing: any }",
    );
    let ks = kinds(base_q, &with_sel);
    assert!(
        ks.contains(&(ChangeKind::Breaking, "connections.q.selector".into())),
        "{ks:?}"
    );
}

#[test]
fn nested_group_paths_are_index_form() {
    let old = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  t:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - { name: x, type: float }\n        - name: meta\n          type: group\n          fields:\n            - { name: ts, type: timestamp }\n";
    let new = old.replace(
        "- { name: ts, type: timestamp }",
        "- { name: ts, type: string }",
    );
    let o = parse_contract(old).unwrap();
    let n = parse_contract(&new).unwrap();
    let changes = classify(&o, &n);
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(
        changes[0].path,
        "connections.t.payload.fields[1].fields[0].type"
    );
    assert!(changes[0].detail.contains("'ts'"), "{}", changes[0].detail);
}
