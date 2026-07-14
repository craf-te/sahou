//! The C ABI (feature = "capi") must return the same JSON envelopes as the wasm/PyO3 layers, so
//! C / C++ / Go / TouchDesigner get byte-identical diagnostics from the one core.
#![cfg(feature = "capi")]

use std::ffi::{c_char, CStr, CString};

use sahou_core::cffi::{
    sahou_accept_sample, sahou_free, sahou_prepare_publish, sahou_runtime_free, sahou_runtime_new,
    sahou_validate_schema,
};
use sahou_core::endpoints::Endpoints;
use sahou_core::ir::descriptor_json;
use sahou_core::parse::parse_contract;

/// Read a returned C string and hand the pointer back to sahou_free (how a C caller must behave).
///
/// # Safety
/// `ptr` must be a non-null string returned by a `sahou_*` function and not yet freed.
unsafe fn take(ptr: *mut c_char) -> String {
    let s = CStr::from_ptr(ptr).to_str().unwrap().to_owned();
    sahou_free(ptr);
    s
}

fn validate(yaml: &str) -> String {
    let input = CString::new(yaml).unwrap();
    // SAFETY: `input` is a valid NUL-terminated C string kept alive across the call.
    unsafe { take(sahou_validate_schema(input.as_ptr())) }
}

#[test]
fn c_validate_schema_accepts_valid_contract() {
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload: { typing: any }\n";
    let out = validate(yaml);
    assert!(out.contains("\"ok\":true"), "{out}");
}

#[test]
fn c_validate_schema_rejects_broken_contract_with_diags() {
    let yaml = "schema: s\nnodes:\n  a: {}\nconnections:\n  bad:\n    pattern: pub_sub\n    from: a\n    to: [ghost]\n    payload: { typing: any }\n";
    let out = validate(yaml);
    assert!(out.contains("\"ok\":false"), "{out}");
    assert!(out.contains("unknown_node"), "{out}");
}

/// A descriptor (gen/descriptor.json equivalent) for a `touch` pub_sub with a bounded float field.
fn demo_descriptor() -> CString {
    let yaml = "schema: demo\nnodes:\n  sensor: {}\n  display: {}\nconnections:\n  touch:\n    pattern: pub_sub\n    from: sensor\n    to: [display]\n    payload:\n      typing: typed\n      fields:\n        - { name: x, type: float, min: 0, max: 1 }\n";
    let contract = parse_contract(yaml).unwrap();
    CString::new(descriptor_json(&contract, &Endpoints::default())).unwrap()
}

#[test]
fn c_runtime_out_boundary_then_in_boundary_roundtrip() {
    let desc = demo_descriptor();
    let node = CString::new("sensor").unwrap();
    let display = CString::new("display").unwrap();
    let conn = CString::new("touch").unwrap();
    // SAFETY: all pointers are valid C strings; the handle is freed at the end.
    unsafe {
        let rt = sahou_runtime_new(desc.as_ptr());
        assert!(!rt.is_null(), "a valid descriptor must build a runtime");

        // OUT (send boundary): a valid payload passes.
        let good = CString::new(r#"{"x":0.5}"#).unwrap();
        let env = take(sahou_prepare_publish(
            rt,
            node.as_ptr(),
            conn.as_ptr(),
            good.as_ptr(),
            1,
        ));
        assert!(env.contains("\"ok\":true"), "{env}");

        // OUT: an out-of-range value is rejected at the send boundary.
        let bad = CString::new(r#"{"x":5}"#).unwrap();
        let env_bad = take(sahou_prepare_publish(
            rt,
            node.as_ptr(),
            conn.as_ptr(),
            bad.as_ptr(),
            1,
        ));
        assert!(env_bad.contains("\"ok\":false"), "{env_bad}");

        // IN (receive boundary): feed the canonical wire + attachment back → accepted.
        let v: serde_json::Value = serde_json::from_str(&env).unwrap();
        let wire = v["msg"]["wire"].as_str().unwrap().to_owned();
        let attachment = CString::new(v["msg"]["attachment"].as_str().unwrap()).unwrap();
        let outcome = take(sahou_accept_sample(
            rt,
            display.as_ptr(),
            conn.as_ptr(),
            wire.as_ptr(),
            wire.len(),
            attachment.as_ptr(),
            1,
            std::ptr::null(),
        ));
        assert!(outcome.contains("\"result\":\"accept\""), "{outcome}");

        sahou_runtime_free(rt);
    }
}
