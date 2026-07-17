//! Native tests for the wasm ABI (run with --features wasm).
//! wasm-bindgen structs can be tested as ordinary Rust natively.
//! Here we pin "wasm ABI = same envelope as the PyO3 ABI (via ffi)" down to the envelope bytes.
#![cfg(feature = "wasm")]

use sahou_core::endpoints::Endpoints;
use sahou_core::ir::descriptor_json;
use sahou_core::parse::parse_contract;
use sahou_core::wasm::{wasm_classify_delivery, wasm_parse_reply_err, WasmRuntime};

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

fn demo_descriptor_json() -> String {
    let c = parse_contract(DEMO).unwrap();
    descriptor_json(&c, &Endpoints::default())
}

#[test]
fn wasm_runtime_mirrors_core_boundaries() {
    // note: in this version of wasm-bindgen JsError derives Debug, so we call
    // .expect() directly to avoid clippy::ok_expect (differs from the brief's assumption but behaves identically).
    let rt = WasmRuntime::new(&demo_descriptor_json()).expect("gen output can be loaded");
    assert_eq!(rt.namespace(), "sahou");
    // send boundary -> recv boundary roundtrip
    let res: serde_json::Value = serde_json::from_str(&rt.prepare_publish(
        "sensor",
        "touch",
        r#"{"x":0.5,"phase":"move","meta":{"ts":1752105600000}}"#,
        0,
    ))
    .unwrap();
    assert_eq!(res["ok"], true);
    let wire = res["msg"]["wire"].as_str().unwrap();
    let att = res["msg"]["attachment"].as_str().unwrap().to_string();
    let out: serde_json::Value = serde_json::from_str(&rt.accept_sample(
        "visuals",
        "touch",
        wire.as_bytes(),
        Some(att),
        0,
        None,
    ))
    .unwrap();
    assert_eq!(out["result"], "accept");
    // send boundary NO (envelope form)
    let ng: serde_json::Value = serde_json::from_str(&rt.prepare_publish(
        "sensor",
        "touch",
        r#"{"x":"bad","phase":"move","meta":{"ts":1}}"#,
        0,
    ))
    .unwrap();
    assert_eq!(ng["ok"], false);
    assert_eq!(ng["diags"][0]["code"], "type_mismatch");
    // no attachment -> missing_schema_hash (never silently passes)
    let out: serde_json::Value =
        serde_json::from_str(&rt.accept_sample("visuals", "touch", b"{}", None, 0, None)).unwrap();
    assert_eq!(out["result"], "reject");
    assert_eq!(out["diags"][0]["code"], "missing_schema_hash");
}

#[test]
fn wasm_handshake_is_three_valued_envelope() {
    let rt = WasmRuntime::new(&demo_descriptor_json()).ok().unwrap();
    // undecodable fragment -> unreachable (not an exception)
    let out: serde_json::Value =
        serde_json::from_str(&rt.handshake("touch", "deadbeef00000000", "{not json")).unwrap();
    assert_eq!(out["verdict"], "unreachable");
    // an unknown connection is also an unreachable envelope (②c: unified to contract_unreachable, no throw)
    let out: serde_json::Value =
        serde_json::from_str(&rt.handshake("ghost", "deadbeef00000000", "{}")).unwrap();
    assert_eq!(out["verdict"], "unreachable");
    assert_eq!(out["diags"][0]["code"], "contract_unreachable");
    // identical fragment -> accepted
    let frag = rt.contract_fragment("touch").ok().unwrap();
    let hash: serde_json::Value = serde_json::from_str(&frag).unwrap();
    let out: serde_json::Value =
        serde_json::from_str(&rt.handshake("touch", hash["hash"].as_str().unwrap(), &frag))
            .unwrap();
    assert_eq!(out["verdict"], "accepted");
}

#[test]
fn wasm_free_functions_mirror_pyo3() {
    assert_eq!(
        wasm_classify_delivery(
            false,
            r#"[{"code":"type_mismatch","path":"$.x","message":"m"}]"#
        )
        .ok()
        .unwrap(),
        "fatal"
    );
    assert_eq!(wasm_classify_delivery(true, "").ok().unwrap(), "retryable");
    let diags: serde_json::Value = serde_json::from_str(&wasm_parse_reply_err(b"garbage")).unwrap();
    assert_eq!(diags[0]["code"], "bad_reply_envelope");
}

#[test]
fn legacy_gui_abi_still_works() {
    // plan ① carryover (spec §10 follow-up (b)): minimal native coverage of the 6 GUI functions
    use sahou_core::wasm::{wasm_descriptor, wasm_parse, wasm_validate_schema};
    let parsed: serde_json::Value = serde_json::from_str(&wasm_parse(DEMO)).unwrap();
    assert_eq!(parsed["ok"], true);
    let v: serde_json::Value = serde_json::from_str(&wasm_validate_schema(DEMO)).unwrap();
    assert_eq!(v["ok"], true);
    let d: serde_json::Value = serde_json::from_str(&wasm_descriptor(DEMO, "")).unwrap();
    assert_eq!(d["ok"], true);
}

#[test]
fn wasm_vitals_payload_mirrors_core() {
    let rt = WasmRuntime::new(&demo_descriptor_json()).expect("gen output can be loaded");
    let info = r#"{"lang":"typescript","sahou":"0.0.2","transport":"ws-link"}"#;
    let json = rt
        .vitals_payload("sensor", info)
        .expect("a known node reports vitals");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["vitals_format"], 1);
    assert_eq!(v["node"], "sensor");
    assert_eq!(v["runtime"]["transport"], "ws-link");
    assert_eq!(rt.vitals_key("sensor"), "sahou/@sahou/vitals/sensor");
    // the Err branch (unknown node) is covered natively in core/tests/vitals.rs;
    // JsError cannot be constructed off-wasm, so no Result<_, JsError> Err path is testable here (same as contract_fragment).
}
