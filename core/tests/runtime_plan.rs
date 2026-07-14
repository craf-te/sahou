use sahou_core::endpoints::Endpoints;
use sahou_core::ir::descriptor_json;
use sahou_core::parse::parse_contract;
use sahou_core::runtime::{
    connection_fields, connections_from, load_descriptor, node_plan, publishing_nodes,
};

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

fn demo_descriptor_json() -> String {
    let c = parse_contract(DEMO).unwrap();
    descriptor_json(&c, &Endpoints::default())
}

#[test]
fn descriptor_roundtrips_through_load() {
    let json = demo_descriptor_json();
    let d = load_descriptor(&json).expect("gen output can be loaded");
    assert_eq!(d.schema, "demo_installation");
    assert_eq!(d.connections["touch"].key, "sahou/touch");
    assert_eq!(d.connections["touch"].hash.len(), 16);
}

#[test]
fn broken_descriptor_is_no() {
    // unknown keys (typos) are NO at the boundary (the contract layer is a closed vocabulary)
    let json = demo_descriptor_json().replace("\"namespace\"", "\"namespase\"");
    let diags = load_descriptor(&json).unwrap_err();
    assert_eq!(diags[0].code, "descriptor_error");
    assert!(
        diags[0].message.contains("unknown field"),
        "{}",
        diags[0].message
    );
}

#[test]
fn node_plan_derives_capabilities_from_wiring() {
    let d = load_descriptor(&demo_descriptor_json()).unwrap();
    // demo: sensor is the `from` of touch/points/debug_tap + the `from` of get_state (requester)
    let sensor = node_plan(&d, "sensor").unwrap();
    assert_eq!(sensor.publishes, vec!["debug_tap", "points", "touch"]);
    assert_eq!(sensor.subscribes, Vec::<String>::new());
    assert_eq!(sensor.queries, vec!["get_state"]);
    assert_eq!(sensor.answers, Vec::<String>::new());
    // archive is the `to` of touch + the `to` of get_state (responder)
    let archive = node_plan(&d, "archive").unwrap();
    assert_eq!(archive.subscribes, vec!["touch"]);
    assert_eq!(archive.answers, vec!["get_state"]);
}

#[test]
fn unknown_node_is_no() {
    let d = load_descriptor(&demo_descriptor_json()).unwrap();
    let diags = node_plan(&d, "ghost").unwrap_err();
    assert_eq!(diags[0].code, "unknown_node");
    assert_eq!(diags[0].path, "nodes.ghost");
}

#[test]
fn publishing_nodes_are_the_pub_sub_senders() {
    let d = load_descriptor(&demo_descriptor_json()).unwrap();
    // demo: only `sensor` is the `from` of any pub_sub connection (touch/points/debug_tap).
    // get_state is a query, so its `from` (sensor again) does not add a new node.
    assert_eq!(publishing_nodes(&d), vec!["sensor"]);
}

#[test]
fn connections_from_lists_pub_sub_only_sorted() {
    let d = load_descriptor(&demo_descriptor_json()).unwrap();
    // sensor publishes on these pub_sub connections; get_state (query) is excluded.
    assert_eq!(
        connections_from(&d, "sensor"),
        vec!["debug_tap", "points", "touch"]
    );
    // a node that only receives publishes nothing.
    assert_eq!(connections_from(&d, "visuals"), Vec::<String>::new());
    // an unknown node is empty, not an error (it feeds a selector).
    assert_eq!(connections_from(&d, "ghost"), Vec::<String>::new());
}

#[test]
fn connection_fields_render_the_payload_schema() {
    let d = load_descriptor(&demo_descriptor_json()).unwrap();
    // touch: x float 0..1, phase enum, meta group{ts, source}
    assert_eq!(
        connection_fields(&d, "touch"),
        vec![
            ["x".to_string(), "float".into(), "yes".into(), "0..1".into()],
            [
                "phase".into(),
                "enum".into(),
                "yes".into(),
                "down|move|up".into()
            ],
            [
                "meta".into(),
                "group".into(),
                "yes".into(),
                "ts, source".into()
            ],
        ]
    );
    // an any-typed payload declares no fields
    assert!(connection_fields(&d, "debug_tap").is_empty());
    // an unknown connection is empty, not an error (it feeds a panel)
    assert!(connection_fields(&d, "ghost").is_empty());
}
