//! Native tests for the GUI wasm ABI (free functions) (run with --features wasm).
//! Pins the envelope contract {ok:true,<key>:…} / {ok:false,diags:[…]} that the GUI (core-bridge.ts) depends on,
//! covering both the OK and NG paths (design §4, §9-1). The runtime ABI (WasmRuntime) is covered by runtime_wasm_abi.rs.
#![cfg(feature = "wasm")]

use sahou_core::wasm::{
    wasm_descriptor, wasm_parse, wasm_parse_endpoints, wasm_sample, wasm_serialize,
    wasm_serialize_endpoints, wasm_validate_payload, wasm_validate_schema,
};

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}

#[test]
fn parse_ok_envelope_carries_contract() {
    let v = json(&wasm_parse(DEMO));
    assert_eq!(v["ok"], true);
    assert_eq!(v["contract"]["schema"], "demo_installation");
    assert_eq!(v["contract"]["connections"]["touch"]["pattern"], "pub_sub");
}

#[test]
fn parse_no_is_structured_with_semantic_path() {
    // unknown key (typo) = the contract is never silently folded
    let v = json(&wasm_parse(
        "schema: s\nnodes:\n  a: {}\nconnections:\n  c:\n    pattern: pub_sub\n    frm: a\n    to: [a]\n",
    ));
    assert_eq!(v["ok"], false);
    assert_eq!(v["diags"][0]["code"], "parse_error");
    assert!(
        v["diags"][0]["path"]
            .as_str()
            .unwrap()
            .contains("connections.c"),
        "{v}"
    );
    // duplicate key (defeats serde's default silent last-wins)
    let v = json(&wasm_parse(
        "schema: s\nnodes:\n  a: {}\n  a: {}\nconnections: {}\n",
    ));
    assert_eq!(v["ok"], false);
    assert_eq!(v["diags"][0]["code"], "parse_error");
}

#[test]
fn serialize_roundtrips_parse_output() {
    // the GUI's save path itself: parse -> contract JSON -> serialize -> parse yields structural equality (constraint ①)
    let parsed = json(&wasm_parse(DEMO));
    let ser = json(&wasm_serialize(&parsed["contract"].to_string()));
    assert_eq!(ser["ok"], true);
    let back = json(&wasm_parse(ser["yaml"].as_str().unwrap()));
    assert_eq!(back["contract"], parsed["contract"]);
}

#[test]
fn serialize_no_on_broken_contract_json() {
    let v = json(&wasm_serialize(r#"{"schema": 1}"#));
    assert_eq!(v["ok"], false);
    assert_eq!(v["diags"][0]["code"], "decode_error");
}

#[test]
fn validate_schema_reports_positioned_diags() {
    let ok = json(&wasm_validate_schema(DEMO));
    assert_eq!(ok["ok"], true);
    assert_eq!(ok["diags"].as_array().unwrap().len(), 0);
    let ng = json(&wasm_validate_schema(
        "schema: s\nnodes:\n  a: {}\nconnections:\n  bad:\n    pattern: pub_sub\n    from: a\n    to: [a]\n    payload: { typing: any }\n",
    ));
    assert_eq!(ng["ok"], false);
    assert_eq!(ng["diags"][0]["code"], "self_loop");
    assert_eq!(ng["diags"][0]["path"], "connections.bad.to[0]");
}

#[test]
fn validate_payload_and_sample_agree() {
    // the GUI slices the slot out of the contract JSON and passes it through as-is (ShapeEditor default suggestion §5.1)
    let parsed = json(&wasm_parse(DEMO));
    let slot = parsed["contract"]["connections"]["touch"]["payload"].to_string();
    // sample is always valid (generated via proptest)
    let s = json(&wasm_sample(&slot));
    assert_eq!(s["ok"], true);
    let v = json(&wasm_validate_payload(&slot, &s["sample"].to_string()));
    assert_eq!(v["ok"], true);
    // a type mismatch is a positioned NO
    let ng = json(&wasm_validate_payload(
        &slot,
        r#"{"x":"bad","phase":"move","meta":{"ts":1}}"#,
    ));
    assert_eq!(ng["ok"], false);
    assert_eq!(ng["diags"][0]["code"], "type_mismatch");
    // the slot JSON itself is broken -> decode_error
    let bad = json(&wasm_validate_payload("{not json", "{}"));
    assert_eq!(bad["ok"], false);
    assert_eq!(bad["diags"][0]["code"], "decode_error");
    // sample also returns the same NG envelope when the slot JSON is broken (pins the NG path of all 6 GUI functions)
    let bad_sample = json(&wasm_sample("{not json"));
    assert_eq!(bad_sample["ok"], false);
    assert_eq!(bad_sample["diags"][0]["code"], "decode_error");
}

#[test]
fn descriptor_resolves_namespace_and_hash() {
    let ok = json(&wasm_descriptor(DEMO, "env: dev\nnamespace: sahou/demo\n"));
    assert_eq!(ok["ok"], true);
    assert_eq!(
        ok["descriptor"]["connections"]["touch"]["key"],
        "sahou/demo/touch"
    );
    assert_eq!(
        ok["descriptor"]["connections"]["touch"]["hash"]
            .as_str()
            .unwrap()
            .len(),
        16
    );
    // empty endpoints string = default (namespace "sahou")
    let d = json(&wasm_descriptor(DEMO, ""));
    assert_eq!(d["descriptor"]["namespace"], "sahou");
    // while there are schema diagnostics it's NG (the GUI shows the last good value as stale §7)
    let ng = json(&wasm_descriptor(
        "schema: s\nnodes:\n  a: {}\nconnections:\n  bad:\n    pattern: pub_sub\n    from: a\n    to: [a]\n    payload: { typing: any }\n",
        "",
    ));
    assert_eq!(ng["ok"], false);
    assert_eq!(ng["diags"][0]["code"], "self_loop");
    // broken endpoints -> its diagnostic
    let ng2 = json(&wasm_descriptor(DEMO, "namespace: [broken\n"));
    assert_eq!(ng2["ok"], false);
    assert_eq!(ng2["diags"][0]["code"], "parse_error");
}

#[test]
fn endpoints_abi_is_symmetric_with_schema() {
    let ok = json(&wasm_parse_endpoints("env: dev\nnamespace: sahou/demo\n"));
    assert_eq!(ok["ok"], true);
    assert_eq!(ok["endpoints"]["namespace"], "sahou/demo");
    // empty input = default (same convention as wasm_descriptor; the initial state with no endpoints)
    let d = json(&wasm_parse_endpoints("  \n"));
    assert_eq!(d["ok"], true);
    assert_eq!(d["endpoints"]["namespace"], "sahou");
    // unknown keys are NO for endpoints too (deny_unknown_fields applies via core; design §1)
    let ng = json(&wasm_parse_endpoints("namespace: x\nrouterr: {}\n"));
    assert_eq!(ng["ok"], false);
    assert_eq!(ng["diags"][0]["code"], "parse_error");
    // serialize: JSON -> YAML -> parse roundtrips
    let ser = json(&wasm_serialize_endpoints(&ok["endpoints"].to_string()));
    assert_eq!(ser["ok"], true);
    let back = json(&wasm_parse_endpoints(ser["yaml"].as_str().unwrap()));
    assert_eq!(back["endpoints"], ok["endpoints"]);
    // broken JSON -> decode_error
    let bad = json(&wasm_serialize_endpoints("{"));
    assert_eq!(bad["ok"], false);
    assert_eq!(bad["diags"][0]["code"], "decode_error");
}
