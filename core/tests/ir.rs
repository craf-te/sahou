use sahou_core::endpoints::{parse_endpoints, Endpoints};
use sahou_core::ir::{build_descriptor, connection_hash, resolve_key};
use sahou_core::parse::parse_contract;

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");
const DEV: &str = include_str!("../../examples/demo/endpoints.dev.yaml");

#[test]
fn keyexpr_is_namespace_plus_id_with_override() {
    let c = parse_contract(DEMO).unwrap();
    let touch = &c.connections["touch"];
    assert_eq!(
        resolve_key("sahou/demo", "touch", touch),
        "sahou/demo/touch"
    );
    let mut with_key = touch.clone();
    with_key.key = Some("custom/expr".into());
    assert_eq!(resolve_key("sahou/demo", "touch", &with_key), "custom/expr");
}

#[test]
fn connection_hash_is_stable_and_local() {
    let c = parse_contract(DEMO).unwrap();
    let h1 = connection_hash("touch", &c.connections["touch"]);
    let h2 = connection_hash("touch", &c.connections["touch"]);
    assert_eq!(h1, h2, "deterministic");
    assert_eq!(h1.len(), 16, "16 hex");
    // blast radius locality (Z22 A2): changing touch leaves the points hash unchanged
    let mut changed = c.clone();
    let payload = changed
        .connections
        .get_mut("touch")
        .unwrap()
        .payload
        .as_mut()
        .unwrap();
    payload.fields[0].max = Some(2.0);
    assert_ne!(connection_hash("touch", &changed.connections["touch"]), h1);
    assert_eq!(
        connection_hash("points", &changed.connections["points"]),
        connection_hash("points", &c.connections["points"])
    );
}

#[test]
fn descriptor_carries_resolved_keys_and_hashes() {
    let c = parse_contract(DEMO).unwrap();
    let e = parse_endpoints(DEV).unwrap();
    let d = build_descriptor(&c, &e);
    assert_eq!(d.schema, "demo_installation");
    assert_eq!(d.namespace, "sahou/demo");
    let touch = &d.connections["touch"];
    assert_eq!(touch.key, "sahou/demo/touch");
    assert_eq!(touch.hash.len(), 16);
    assert_eq!(touch.to, vec!["visuals", "archive"]);
}

#[test]
fn descriptor_with_default_endpoints_uses_sahou_namespace() {
    let c = parse_contract(DEMO).unwrap();
    let d = build_descriptor(&c, &Endpoints::default());
    assert_eq!(d.connections["touch"].key, "sahou/touch");
}
