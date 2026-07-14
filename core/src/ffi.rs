//! Shared FFI envelopes. A single implementation so PyO3 (python.rs) and wasm (wasm.rs) return **identical strings**.
//! Building an envelope without going through here is forbidden (structural guarantee of byte-identical diagnostics).

use crate::diag::Diag;
use crate::runtime::{AcceptOutcome, DeliveryClass, HandshakeOutcome, NodePlan, WireMsg};

/// prepare_* envelope: `{"ok":true,"msg":{...}}` | `{"ok":false,"diags":[...]}`
pub fn publish_envelope(result: Result<WireMsg, Vec<Diag>>) -> String {
    match result {
        Ok(msg) => serde_json::json!({ "ok": true, "msg": msg }).to_string(),
        Err(diags) => serde_json::json!({ "ok": false, "diags": diags }).to_string(),
    }
}

/// accept_* envelope (tagged JSON of AcceptOutcome)
pub fn outcome_json(out: AcceptOutcome) -> String {
    serde_json::to_string(&out).expect("serializing an AcceptOutcome never fails")
}

/// handshake envelope (tagged JSON of HandshakeOutcome)
pub fn handshake_json(out: &HandshakeOutcome) -> String {
    serde_json::to_string(out).expect("serializing a HandshakeOutcome never fails")
}

pub fn diags_json(diags: &[Diag]) -> String {
    serde_json::to_string(diags).expect("serializing a Diag never fails")
}

pub fn plan_json(plan: &NodePlan) -> String {
    serde_json::to_string(plan).expect("serializing a NodePlan never fails")
}

pub fn delivery_str(class: DeliveryClass) -> &'static str {
    match class {
        DeliveryClass::Retryable => "retryable",
        DeliveryClass::Fatal => "fatal",
    }
}
