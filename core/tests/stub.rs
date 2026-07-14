//! Unit tests for type stub generation (design §8). Verifies that payload types are emitted deterministically from the demo contract.
use std::collections::BTreeMap;

use sahou_core::endpoints::Endpoints;
use sahou_core::ir::descriptor_json;
use sahou_core::parse::parse_contract;
use sahou_core::runtime::load_descriptor;
use sahou_core::stub::{
    check_drift, gen_stub, parse_stub_hashes, parse_stub_node, StubFile, StubLang,
};

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

fn demo_desc() -> sahou_core::ir::Descriptor {
    let c = parse_contract(DEMO).unwrap();
    load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap()
}

fn content_of<'a>(files: &'a [StubFile], rel: &str) -> &'a str {
    &files
        .iter()
        .find(|f| f.rel_path == rel)
        .unwrap_or_else(|| panic!("{rel} not found"))
        .content
}

#[test]
fn python_stub_has_typeddict_payload_types() {
    let files = gen_stub(&demo_desc(), "sensor", StubLang::Python).unwrap();
    let py = content_of(&files, "sahou_stub.py");
    // record → TypedDict / enum → Literal / group → path-concatenated name / required:false → NotRequired
    assert!(py.contains("class Touch(TypedDict):"), "{py}");
    assert!(py.contains("    x: float"), "{py}");
    assert!(
        py.contains(r#"    phase: Literal["down", "move", "up"]"#),
        "{py}"
    );
    assert!(py.contains("    meta: TouchMeta"), "{py}");
    assert!(py.contains("class TouchMeta(TypedDict):"), "{py}");
    assert!(py.contains("    ts: int"), "{py}"); // timestamp = epoch ms integer
    assert!(py.contains("    source: NotRequired[str]"), "{py}");
    // nested array (points: array<array<float>>)
    assert!(py.contains("class Points(TypedDict):"), "{py}");
    assert!(py.contains("    pts: list[list[float]]"), "{py}");
    // the 2 slots of a query
    assert!(py.contains("class GetStateRequest(TypedDict):"), "{py}");
    assert!(py.contains("class GetStateResponse(TypedDict):"), "{py}");
    assert!(py.contains("    level: int"), "{py}");
}

#[test]
fn ts_stub_has_interface_payload_types() {
    let files = gen_stub(&demo_desc(), "visuals", StubLang::Ts).unwrap();
    let dts = content_of(&files, "sahou_stub.d.mts");
    assert!(dts.contains("export interface Touch {"), "{dts}");
    assert!(dts.contains("  x: number;"), "{dts}");
    assert!(dts.contains(r#"  phase: "down" | "move" | "up";"#), "{dts}");
    assert!(dts.contains("  meta: TouchMeta;"), "{dts}");
    assert!(dts.contains("  source?: string;"), "{dts}"); // required:false → optional
    assert!(dts.contains("  pts: Array<Array<number>>;"), "{dts}"); // fixed to Array<> to avoid operator-precedence ambiguity
}

#[test]
fn generation_is_deterministic() {
    let desc = demo_desc();
    for lang in [StubLang::Python, StubLang::Ts] {
        let a = gen_stub(&desc, "sensor", lang).unwrap();
        let b = gen_stub(&desc, "sensor", lang).unwrap();
        assert_eq!(
            a, b,
            "regeneration must not produce a diff (deterministic generation)"
        );
    }
}

#[test]
fn unknown_node_is_core_diag() {
    let err = gen_stub(&demo_desc(), "ghost", StubLang::Python).unwrap_err();
    assert_eq!(err[0].code, "unknown_node"); // pass through node_plan's diagnostic
}

#[test]
fn type_name_collision_is_structured_no() {
    // a contract where connection touch's group meta (TouchMeta) and connection touch_meta's payload type (TouchMeta) collapse to the same name
    let yaml = r#"
schema: s
nodes:
  a: {}
  b: {}
connections:
  touch:
    pattern: pub_sub
    from: a
    to: [b]
    payload:
      typing: typed
      fields:
        - name: meta
          type: group
          fields:
            - { name: ts, type: timestamp }
  touch_meta:
    pattern: pub_sub
    from: a
    to: [b]
    payload:
      typing: typed
      fields:
        - { name: v, type: int }
"#;
    let c = parse_contract(yaml).unwrap();
    let desc = load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap();
    let err = gen_stub(&desc, "a", StubLang::Python).unwrap_err();
    assert_eq!(err[0].code, "stub_name_collision");
    assert!(err[0].message.contains("TouchMeta"), "{}", err[0].message);
}

#[test]
fn python_facade_overloads_match_engine_api() {
    let files = gen_stub(&demo_desc(), "sensor", StubLang::Python).unwrap();
    let pyi = content_of(&files, "sahou_stub.pyi");
    // sensor = publishes {touch, points, debug_tap} + queries {get_state}
    assert!(pyi.contains("class SensorNode(Protocol):"), "{pyi}");
    assert!(
        pyi.contains(
            r#"    def publish(self, conn: Literal["touch"], payload: Touch) -> None: ..."#
        ),
        "{pyi}"
    );
    assert!(
        pyi.contains(
            r#"    def publish(self, conn: Literal["debug_tap"], payload: Any) -> None: ..."#
        ),
        "{pyi}"
    ); // typing:any is Any
    assert!(
        pyi.contains("@overload"),
        "publish over 3 connections must be overloaded: {pyi}"
    );
    assert!(
        pyi.contains(
            r#"    def query_confirmed(self, conn: Literal["get_state"], payload: GetStateRequest, *, timeout: float = ..., retries: int = ..., backoff: float = ...) -> GetStateResponse: ..."#
        ),
        "{pyi}"
    );
    assert!(
        pyi.contains("def typed_node(node: object) -> SensorNode: ..."),
        "{pyi}"
    );
    // subscribe/answer are absent for sensor (directions it does not participate in do not exist in the type)
    assert!(!pyi.contains("def subscribe"), "{pyi}");
    assert!(!pyi.contains("def answer"), "{pyi}");
    // the runtime side is identity + SCHEMA_HASHES only (does not import the engine)
    let py = content_of(&files, "sahou_stub.py");
    assert!(py.contains("def typed_node(node):"), "{py}");
    assert!(py.contains("SCHEMA_HASHES"), "{py}");
    assert!(
        !py.contains("import sahou\n"),
        "the stub is engine-independent: {py}"
    );
}

#[test]
fn python_facade_subscribe_and_answer_for_receiver_nodes() {
    let files = gen_stub(&demo_desc(), "archive", StubLang::Python).unwrap();
    let pyi = content_of(&files, "sahou_stub.pyi");
    // archive = subscribes {touch} + answers {get_state}
    assert!(
        pyi.contains(r#"conn: Literal["touch"], handler: Callable[[Touch], object]"#),
        "{pyi}"
    );
    assert!(
        pyi.contains(
            r#"    def answer(self, conn: Literal["get_state"], fn: Callable[[GetStateRequest], GetStateResponse]) -> Callable[[GetStateRequest], GetStateResponse]: ..."#
        ),
        "{pyi}"
    );
    assert!(!pyi.contains("def publish"), "{pyi}");
}

#[test]
fn ts_facade_overloads_match_engine_api() {
    let files = gen_stub(&demo_desc(), "visuals", StubLang::Ts).unwrap();
    let dts = content_of(&files, "sahou_stub.d.mts");
    // visuals = subscribes {touch, points, debug_tap} only
    assert!(dts.contains("export interface VisualsNode {"), "{dts}");
    assert!(
        dts.contains(
            r#"  subscribe(conn: "touch", handler: (payload: Touch) => void | Promise<void>, opts?: { onReject?: OnReject }): Promise<void>;"#
        ),
        "{dts}"
    );
    assert!(
        dts.contains(
            r#"  subscribe(conn: "debug_tap", handler: (payload: unknown) => void | Promise<void>"#
        ),
        "{dts}"
    );
    assert!(
        dts.contains("export declare function typedNode(node: unknown): VisualsNode;"),
        "{dts}"
    );
    assert!(
        !dts.contains("publish("),
        "directions it does not participate in do not exist in the type: {dts}"
    );
    let mjs = content_of(&files, "sahou_stub.mjs");
    assert!(
        mjs.contains("export const typedNode = (node) => node;"),
        "{mjs}"
    );
    assert!(
        mjs.contains("export const SCHEMA_HASHES = Object.freeze({"),
        "{mjs}"
    );
}

#[test]
fn all_stub_files_carry_hash_markers() {
    let desc = demo_desc();
    let files = gen_stub(&desc, "sensor", StubLang::Python).unwrap();
    let touch_hash = &desc.connections["touch"].hash;
    let gs_hash = &desc.connections["get_state"].hash;
    for f in &files {
        assert!(
            f.content.contains("sahou:stub node=sensor"),
            "{}",
            f.rel_path
        );
        assert!(
            f.content
                .contains(&format!("sahou:hash touch={touch_hash}")),
            "{}",
            f.rel_path
        );
        assert!(
            f.content
                .contains(&format!("sahou:hash get_state={gs_hash}")),
            "{}",
            f.rel_path
        );
    }
}

#[test]
fn roundtrip_markers_and_check_ok() {
    let desc = demo_desc();
    // generate → parse → compare round-trip with zero drift (structurally guarantees the emitter and parser share the format)
    let files = gen_stub(&desc, "sensor", StubLang::Python).unwrap();
    let all: String = files
        .iter()
        .map(|f| f.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(parse_stub_node(&all).as_deref(), Some("sensor"));
    let hashes = parse_stub_hashes(&all).unwrap();
    assert_eq!(hashes["touch"], desc.connections["touch"].hash);
    assert_eq!(hashes.len(), 4); // touch / points / debug_tap / get_state
    assert!(check_drift(&desc, "sensor", &hashes).is_empty());
}

#[test]
fn hash_drift_is_structured_no() {
    let desc = demo_desc();
    let mut hashes: BTreeMap<String, String> = desc
        .connections
        .iter()
        .filter(|(id, _)| ["touch", "points", "debug_tap", "get_state"].contains(&id.as_str()))
        .map(|(id, c)| (id.clone(), c.hash.clone()))
        .collect();
    hashes.insert("touch".into(), "0000000000000000".into()); // assume the contract changed
    let diags = check_drift(&desc, "sensor", &hashes);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "stub_hash_drift");
    assert_eq!(diags[0].path, "connections.touch.hash");
    assert!(
        diags[0].message.contains("Regenerate"),
        "{}",
        diags[0].message
    );
}

#[test]
fn stale_and_missing_connections_are_classified() {
    let desc = demo_desc();
    // the stub has only ghost (absent from the descriptor), and none of sensor's participating connections
    let hashes: BTreeMap<String, String> =
        [("ghost".to_string(), "1111111111111111".to_string())].into();
    let diags = check_drift(&desc, "sensor", &hashes);
    let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
    assert!(codes.contains(&"stub_stale_connection"), "{codes:?}");
    // all 4 connections sensor participates in are missing
    assert_eq!(
        codes
            .iter()
            .filter(|c| **c == "stub_missing_connection")
            .count(),
        4,
        "{codes:?}"
    );
}

#[test]
fn unknown_node_in_check_is_core_diag() {
    let diags = check_drift(&demo_desc(), "ghost", &BTreeMap::new());
    assert_eq!(diags[0].code, "unknown_node");
}

#[test]
fn kebab_case_field_name_is_structured_no_python() {
    // a non-identifier field name (kebab-case) is a structured NO at gen time (not a later SyntaxError).
    let yaml = r#"
schema: s
nodes:
  a: {}
  b: {}
connections:
  touch:
    pattern: pub_sub
    from: a
    to: [b]
    payload:
      typing: typed
      fields:
        - { name: my-field, type: int }
"#;
    let c = parse_contract(yaml).unwrap();
    let desc = load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap();
    let err = gen_stub(&desc, "a", StubLang::Python).unwrap_err();
    assert_eq!(err[0].code, "stub_unrepresentable_name");
    assert!(err[0].message.contains("my-field"), "{}", err[0].message);
    assert!(err[0].path.contains("touch"), "{}", err[0].path);
}

#[test]
fn kebab_case_field_name_is_structured_no_ts() {
    let yaml = r#"
schema: s
nodes:
  a: {}
  b: {}
connections:
  touch:
    pattern: pub_sub
    from: a
    to: [b]
    payload:
      typing: typed
      fields:
        - { name: my-field, type: int }
"#;
    let c = parse_contract(yaml).unwrap();
    let desc = load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap();
    let err = gen_stub(&desc, "a", StubLang::Ts).unwrap_err();
    assert_eq!(err[0].code, "stub_unrepresentable_name");
    assert!(err[0].message.contains("my-field"), "{}", err[0].message);
}

#[test]
fn nested_group_kebab_case_field_name_is_no() {
    // also detects a nested field name under a group
    let yaml = r#"
schema: s
nodes:
  a: {}
  b: {}
connections:
  touch:
    pattern: pub_sub
    from: a
    to: [b]
    payload:
      typing: typed
      fields:
        - name: meta
          type: group
          fields:
            - { name: bad-name, type: int }
"#;
    let c = parse_contract(yaml).unwrap();
    let desc = load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap();
    let err = gen_stub(&desc, "a", StubLang::Python).unwrap_err();
    assert_eq!(err[0].code, "stub_unrepresentable_name");
    assert!(err[0].message.contains("bad-name"), "{}", err[0].message);
}

#[test]
fn enum_value_with_control_char_is_structured_no() {
    // an enum value with a control character (Rust Debug escapes it as \u{1b} → breaks Python/TS string-literal syntax)
    // uses YAML double-quoted-scalar escaping (\x1B) so the contract itself parses validly.
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  touch:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload:\n      typing: typed\n      fields:\n        - name: phase\n          type: enum\n          values: [\"down\", \"bad\\x1Bvalue\"]\n";
    let c = parse_contract(yaml).unwrap();
    let desc = load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap();
    let err = gen_stub(&desc, "a", StubLang::Python).unwrap_err();
    assert_eq!(err[0].code, "stub_unrepresentable_name");
    assert!(
        err[0].message.contains("control character"),
        "{}",
        err[0].message
    );
}

#[test]
fn valid_snake_case_names_generate_successfully() {
    // representable names (snake_case) always pass — the guarantee that expressiveness is not reduced
    let files = gen_stub(&demo_desc(), "sensor", StubLang::Python).unwrap();
    assert!(!files.is_empty());
    let files = gen_stub(&demo_desc(), "visuals", StubLang::Ts).unwrap();
    assert!(!files.is_empty());
}

#[test]
fn conflicting_markers_are_no() {
    let text = "# sahou:hash touch=aaaaaaaaaaaaaaaa\n# sahou:hash touch=bbbbbbbbbbbbbbbb\n";
    let err = parse_stub_hashes(text).unwrap_err();
    assert_eq!(err[0].code, "stub_marker_conflict");
}

#[test]
fn malformed_marker_is_no_not_silently_skipped() {
    let err = parse_stub_hashes("# sahou:hash touch\n").unwrap_err();
    assert_eq!(err[0].code, "stub_marker_invalid");
    let err = parse_stub_hashes("# sahou:hash touch=short\n").unwrap_err();
    assert_eq!(err[0].code, "stub_marker_invalid"); // not 16hex
}
