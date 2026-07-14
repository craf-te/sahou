use sahou_core::endpoints::{parse_endpoints, serialize_endpoints, Endpoints, Mode};

const DEV: &str = include_str!("../../examples/demo/endpoints.dev.yaml");

#[test]
fn default_is_lan_auto() {
    let e = Endpoints::default();
    assert_eq!(e.namespace, "sahou");
    assert!(e.router.is_none());
    assert!(e.nodes.is_empty());
}

#[test]
fn demo_endpoints_parse() {
    let e = parse_endpoints(DEV).unwrap();
    assert_eq!(e.env.as_deref(), Some("dev"));
    assert_eq!(e.namespace, "sahou/demo");
}

#[test]
fn full_form_parses() {
    let yaml = "env: prod\nnamespace: sahou/venue\nrouter: { enabled: true, endpoint: \"tcp/host:7447\" }\nnodes:\n  display: { mode: client, connect: [\"tcp/host:7447\"] }\nplugins: [rest]\n";
    let e = parse_endpoints(yaml).unwrap();
    assert!(e.router.as_ref().unwrap().enabled);
    assert_eq!(e.nodes["display"].mode, Mode::Client);
    assert_eq!(e.plugins, vec!["rest"]);
}

#[test]
fn unknown_key_is_no() {
    // endpoints is also a closed vocabulary (a settled decision of this plan)
    let diags = parse_endpoints("namespace: x\nnamespase: typo\n").unwrap_err();
    assert_eq!(diags[0].code, "parse_error");
    assert!(diags[0].message.contains("unknown field"));
}

#[test]
fn serialize_endpoints_roundtrips() {
    let src = "env: dev\nnamespace: sahou/demo\nnodes:\n  visuals:\n    mode: client\n    connect: [\"ws/127.0.0.1:10000\"]\n";
    let e = parse_endpoints(src).unwrap();
    let yaml = serialize_endpoints(&e);
    let back = parse_endpoints(&yaml).unwrap();
    assert_eq!(
        e, back,
        "endpoints is serialize-roundtrip-stable too (symmetric with the contract; design §1)"
    );
    // deterministic (serializing twice yields identical bytes)
    assert_eq!(yaml, serialize_endpoints(&back));
}
