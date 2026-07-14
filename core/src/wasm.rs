//! wasm bindings for the GUI (plan ②). The ABI is unified as "string in / JSON string out"
//! (avoids JsValue conversion; the JS side just does JSON.parse).
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsError;

use crate::contract::{Contract, Slot};
use crate::diag::Diag;
use crate::endpoints::{parse_endpoints, serialize_endpoints, Endpoints};
use crate::ffi;
use crate::fmt::serialize_contract;
use crate::ir::build_descriptor;
use crate::ir::Descriptor;
use crate::parse::parse_contract;
use crate::payload::validate_json;
use crate::runtime as rt;
use crate::sample::sample_slot;
use crate::schema_check::validate_schema;

fn ok(key: &str, value: serde_json::Value) -> String {
    serde_json::json!({ "ok": true, key: value }).to_string()
}

fn ng(diags: Vec<Diag>) -> String {
    serde_json::json!({ "ok": false, "diags": diags }).to_string()
}

fn slot_from_json(slot_json: &str) -> Result<Slot, Vec<Diag>> {
    serde_json::from_str::<Slot>(slot_json).map_err(|e| {
        vec![Diag::new(
            "decode_error",
            "$",
            format!("invalid slot JSON: {e}"),
        )]
    })
}

#[wasm_bindgen]
pub fn wasm_parse(yaml: &str) -> String {
    match parse_contract(yaml) {
        Ok(c) => ok("contract", serde_json::to_value(&c).unwrap()),
        Err(diags) => ng(diags),
    }
}

#[wasm_bindgen]
pub fn wasm_serialize(contract_json: &str) -> String {
    match serde_json::from_str::<Contract>(contract_json) {
        Ok(c) => ok("yaml", serde_json::Value::String(serialize_contract(&c))),
        Err(e) => ng(vec![Diag::new(
            "decode_error",
            "$",
            format!("invalid contract JSON: {e}"),
        )]),
    }
}

#[wasm_bindgen]
pub fn wasm_validate_schema(yaml: &str) -> String {
    let diags = match parse_contract(yaml) {
        Ok(c) => validate_schema(&c),
        Err(diags) => diags,
    };
    serde_json::json!({ "ok": diags.is_empty(), "diags": diags }).to_string()
}

#[wasm_bindgen]
pub fn wasm_validate_payload(slot_json: &str, payload_json: &str) -> String {
    match slot_from_json(slot_json) {
        Ok(slot) => {
            let diags = validate_json(&slot, payload_json);
            serde_json::json!({ "ok": diags.is_empty(), "diags": diags }).to_string()
        }
        Err(diags) => ng(diags),
    }
}

#[wasm_bindgen]
pub fn wasm_sample(slot_json: &str) -> String {
    match slot_from_json(slot_json) {
        Ok(slot) => ok("sample", sample_slot(&slot)),
        Err(diags) => ng(diags),
    }
}

#[wasm_bindgen]
pub fn wasm_descriptor(yaml: &str, endpoints_yaml: &str) -> String {
    let contract = match parse_contract(yaml) {
        Ok(c) => c,
        Err(diags) => return ng(diags),
    };
    let schema_diags = validate_schema(&contract);
    if !schema_diags.is_empty() {
        return ng(schema_diags);
    }
    let eps = if endpoints_yaml.trim().is_empty() {
        Endpoints::default()
    } else {
        match parse_endpoints(endpoints_yaml) {
            Ok(e) => e,
            Err(diags) => return ng(diags),
        }
    };
    ok(
        "descriptor",
        serde_json::to_value(build_descriptor(&contract, &eps)).unwrap(),
    )
}

/// ABI for treating endpoints symmetrically with schema (design §1; §9-3).
/// Whitespace-only input yields the default Endpoints (same convention as wasm_descriptor = the initial no-file state).
#[wasm_bindgen]
pub fn wasm_parse_endpoints(yaml: &str) -> String {
    let result = if yaml.trim().is_empty() {
        Ok(Endpoints::default())
    } else {
        parse_endpoints(yaml)
    };
    match result {
        Ok(e) => ok("endpoints", serde_json::to_value(&e).unwrap()),
        Err(diags) => ng(diags),
    }
}

#[wasm_bindgen]
pub fn wasm_serialize_endpoints(endpoints_json: &str) -> String {
    match serde_json::from_str::<Endpoints>(endpoints_json) {
        Ok(e) => ok("yaml", serde_json::Value::String(serialize_endpoints(&e))),
        Err(e) => ng(vec![Diag::new(
            "decode_error",
            "$",
            format!("invalid endpoints JSON: {e}"),
        )]),
    }
}

/// Runtime ABI for the TS runtime (sahou).
/// Same method names and same JSON envelopes as PyO3 `SahouRuntime` (all via ffi = structural guarantee of byte-identical output).
#[wasm_bindgen]
pub struct WasmRuntime {
    desc: Descriptor,
}

#[wasm_bindgen]
impl WasmRuntime {
    /// An invalid descriptor throws (message = JSON array of Diag).
    #[wasm_bindgen(constructor)]
    pub fn new(descriptor_json: &str) -> Result<WasmRuntime, JsError> {
        rt::load_descriptor(descriptor_json)
            .map(|desc| WasmRuntime { desc })
            .map_err(|diags| JsError::new(&ffi::diags_json(&diags)))
    }

    pub fn namespace(&self) -> String {
        self.desc.namespace.clone()
    }

    pub fn node_plan(&self, node: &str) -> Result<String, JsError> {
        rt::node_plan(&self.desc, node)
            .map(|p| ffi::plan_json(&p))
            .map_err(|diags| JsError::new(&ffi::diags_json(&diags)))
    }

    pub fn prepare_publish(&self, node: &str, conn: &str, payload_json: &str, seq: u64) -> String {
        ffi::publish_envelope(rt::prepare_publish(
            &self.desc,
            node,
            conn,
            payload_json,
            seq,
        ))
    }

    pub fn accept_sample(
        &self,
        node: &str,
        conn: &str,
        wire: &[u8],
        attachment: Option<String>,
        seq: u64,
        trusted: Option<String>,
    ) -> String {
        ffi::outcome_json(rt::accept_sample(
            &self.desc,
            node,
            conn,
            wire,
            attachment.as_deref(),
            seq,
            trusted.as_deref(),
        ))
    }

    pub fn prepare_request(&self, node: &str, conn: &str, payload_json: &str, seq: u64) -> String {
        ffi::publish_envelope(rt::prepare_request(
            &self.desc,
            node,
            conn,
            payload_json,
            seq,
        ))
    }

    pub fn accept_request(
        &self,
        node: &str,
        conn: &str,
        wire: &[u8],
        attachment: Option<String>,
        seq: u64,
        trusted: Option<String>,
    ) -> String {
        ffi::outcome_json(rt::accept_request(
            &self.desc,
            node,
            conn,
            wire,
            attachment.as_deref(),
            seq,
            trusted.as_deref(),
        ))
    }

    pub fn prepare_reply(&self, node: &str, conn: &str, payload_json: &str, seq: u64) -> String {
        ffi::publish_envelope(rt::prepare_reply(&self.desc, node, conn, payload_json, seq))
    }

    pub fn accept_reply(
        &self,
        node: &str,
        conn: &str,
        wire: &[u8],
        attachment: Option<String>,
        seq: u64,
        trusted: Option<String>,
    ) -> String {
        ffi::outcome_json(rt::accept_reply(
            &self.desc,
            node,
            conn,
            wire,
            attachment.as_deref(),
            seq,
            trusted.as_deref(),
        ))
    }

    pub fn contract_fragment(&self, conn: &str) -> Result<String, JsError> {
        rt::contract_fragment(&self.desc, conn)
            .map_err(|diags| JsError::new(&ffi::diags_json(&diags)))
    }

    /// The 3-way handshake judgement. Always returns a verdict envelope (an unknown connection is an unreachable envelope; never throws).
    pub fn handshake(&self, conn: &str, sender_hash: &str, theirs_json: &str) -> String {
        ffi::handshake_json(&rt::handshake_judge(
            &self.desc,
            conn,
            sender_hash,
            theirs_json,
        ))
    }
}

/// Classification for smart retry (identical to PyO3 classify_delivery).
#[wasm_bindgen]
pub fn wasm_classify_delivery(timed_out: bool, diags_json: &str) -> Result<String, JsError> {
    let diags: Vec<Diag> = if diags_json.trim().is_empty() {
        vec![]
    } else {
        serde_json::from_str(diags_json)
            .map_err(|e| JsError::new(&format!("invalid diags JSON: {e}")))?
    };
    Ok(ffi::delivery_str(rt::classify_delivery(timed_out, &diags)).to_string())
}

/// Parsing the reply_err envelope (a single core implementation). Returns = JSON array of Diag.
#[wasm_bindgen]
pub fn wasm_parse_reply_err(payload: &[u8]) -> String {
    ffi::diags_json(&rt::parse_reply_err(payload))
}
