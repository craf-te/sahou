use sahou_core::contract::{NodeKind, Pattern, Reliability, Typing};
use sahou_core::parse::parse_contract;

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

#[test]
fn demo_contract_parses() {
    let c = parse_contract(DEMO).expect("demo is valid");
    assert_eq!(c.schema, "demo_installation");
    assert_eq!(c.version, "1");
    assert_eq!(c.nodes.len(), 4);
    assert_eq!(c.nodes["osc_light"].kind, NodeKind::External);
    let touch = &c.connections["touch"];
    assert_eq!(touch.pattern, Pattern::PubSub);
    assert_eq!(touch.to, vec!["visuals", "archive"]);
    assert_eq!(touch.reliability, Reliability::Reliable);
    let payload = touch.payload.as_ref().unwrap();
    assert_eq!(payload.typing, Typing::Typed);
    assert_eq!(payload.fields.len(), 3);
    // query has two slots: request/response
    let q = &c.connections["get_state"];
    assert!(q.request.is_some() && q.response.is_some());
}

#[test]
fn to_accepts_single_string() {
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: b\n    payload: { typing: any }\n";
    let c = parse_contract(yaml).unwrap();
    assert_eq!(c.connections["c"].to, vec!["b"]);
}

#[test]
fn duplicate_connection_key_is_no() {
    // serde's default is last-wins (silent) -> promoted to an explicit NO (spec §4)
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload: { typing: any }\n  c:\n    pattern: pub_sub\n    from: b\n    to: [a]\n    payload: { typing: any }\n";
    let diags = parse_contract(yaml).unwrap_err();
    assert_eq!(diags[0].code, "parse_error");
    assert!(
        diags[0].message.contains("duplicate key 'c'"),
        "{}",
        diags[0].message
    );
}

#[test]
fn unknown_field_typo_is_no_with_expected_names() {
    // serde silently ignores unknown keys (typos) by default -> explicit NO + suggests the correct names
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    realiability: reliable\n    payload: { typing: any }\n";
    let diags = parse_contract(yaml).unwrap_err();
    assert_eq!(diags[0].code, "parse_error");
    assert!(diags[0].message.contains("unknown field `realiability`"));
    assert!(diags[0].message.contains("reliability")); // the correct name in the expected-field list
                                                       // semantic path(serde_path_to_error)
    assert!(
        diags[0].path.contains("connections.c"),
        "path={}",
        diags[0].path
    );
}
