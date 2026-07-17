//! Vitals — self-reported node state (spec: notes/sahou-vitals-spec.md).
//! Build (vitals_payload) and read (parse_vitals) both live here so every language runtime
//! reports and reads byte-identically. Pure: runtime-only facts arrive via `info_json`
//! (string-based like the handshake FFI), zenoh never appears in this module.
//! Reading is wire-layer tolerant — unknown fields in a known format are ignored
//! (do not confuse with the contract layer's deny_unknown_fields).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::diag::Diag;
use crate::ir::Descriptor;

/// Version stamp of the vitals payload. Bump only with a documented migration story:
/// fields added in a newer format must be `#[serde(default)]`/`Option` so older payloads
/// stay readable, and the bump must decide the read policy for older formats explicitly
/// (the current gate accepts the exact supported format only).
pub const VITALS_FORMAT: u32 = 1;

/// Runtime-only facts, passed IN by each language runtime (keeps the core pure).
#[derive(Debug, Clone, Deserialize)]
pub struct VitalsRuntimeInfo {
    pub lang: String,
    /// sahou runtime library version
    pub sahou: String,
    /// zenoh library version when the runtime can learn it (omitted, not faked, otherwise)
    #[serde(default)]
    pub zenoh: Option<String>,
    /// "native" | "ws-link" | "browser"
    pub transport: String,
    #[serde(default)]
    pub uptime_secs: u64,
    /// cached delivery verdicts: conn -> sender_hash -> "accepted" | "blocked"
    #[serde(default)]
    pub handshake: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VitalsConn {
    /// this node's side of the connection: "from" | "to"
    pub role: String,
    /// the per-connection hash this node runs = its descriptor generation, per connection
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VitalsRuntime {
    pub lang: String,
    pub sahou: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zenoh: Option<String>,
    pub transport: String,
}

/// The vitals payload (format 1). BTreeMap = deterministic serialization
/// (same canonical-order rationale as connection_hash).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vitals {
    pub vitals_format: u32,
    pub node: String,
    pub namespace: String,
    pub schema: String,
    pub schema_version: String,
    pub connections: BTreeMap<String, VitalsConn>,
    pub runtime: VitalsRuntime,
    #[serde(default)]
    pub uptime_secs: u64,
    #[serde(default)]
    pub handshake: BTreeMap<String, BTreeMap<String, String>>,
}

/// The key both the liveliness token and the vitals queryable are declared at.
/// One implementation in the core so no runtime drifts on the key shape.
pub fn vitals_key(desc: &Descriptor, node: &str) -> String {
    format!("{}/@sahou/vitals/{node}", desc.namespace)
}

/// Build this node's vitals payload. Everything reported is state the engine already holds —
/// zero instrumentation, zero periodic traffic (spec §1.2).
pub fn vitals_payload(desc: &Descriptor, node: &str, info_json: &str) -> Result<String, Vec<Diag>> {
    if !desc.nodes.contains_key(node) {
        return Err(vec![Diag::new(
            "unknown_node",
            format!("nodes.{node}"),
            format!("undefined node '{node}' cannot report vitals"),
        )]);
    }
    let info: VitalsRuntimeInfo = serde_json::from_str(info_json).map_err(|e| {
        vec![Diag::new(
            "vitals_bad_runtime_info",
            "$",
            format!("the runtime info is not valid: {e}"),
        )]
    })?;
    let connections: BTreeMap<String, VitalsConn> = desc
        .connections
        .iter()
        .filter_map(|(id, c)| {
            // a self-loop node would be both; "from" wins (it is the rarer, more specific role)
            let role = if c.from == node {
                "from"
            } else if c.to.iter().any(|t| t == node) {
                "to"
            } else {
                return None;
            };
            Some((
                id.clone(),
                VitalsConn {
                    role: role.to_string(),
                    hash: c.hash.clone(),
                },
            ))
        })
        .collect();
    let v = Vitals {
        vitals_format: VITALS_FORMAT,
        node: node.to_string(),
        namespace: desc.namespace.clone(),
        schema: desc.schema.clone(),
        schema_version: desc.version.clone(),
        connections,
        runtime: VitalsRuntime {
            lang: info.lang,
            sahou: info.sahou,
            zenoh: info.zenoh,
            transport: info.transport,
        },
        uptime_secs: info.uptime_secs,
        handshake: info.handshake,
    };
    Ok(serde_json::to_string(&v).expect("serializing Vitals never fails"))
}

/// The reading side (doctor). Format check first: an unknown vitals_format is a structured
/// diagnostic ("upgrade to read it"), never a cryptic parse explosion.
pub fn parse_vitals(json: &str) -> Result<Vitals, Vec<Diag>> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| {
        vec![Diag::new(
            "vitals_unreadable",
            "$",
            format!("not valid JSON: {e}"),
        )]
    })?;
    match value.get("vitals_format") {
        None => {
            return Err(vec![Diag::new(
                "vitals_unreadable",
                "$.vitals_format",
                "missing vitals_format (not a vitals payload)",
            )])
        }
        Some(f) => match f.as_u64() {
            Some(n) if n == u64::from(VITALS_FORMAT) => {}
            Some(n) if n > u64::from(VITALS_FORMAT) => {
                return Err(vec![Diag::new(
                    "vitals_format_unsupported",
                    "$.vitals_format",
                    format!(
                        "vitals format {n} is newer than this sahou understands (supported: {VITALS_FORMAT}); upgrade sahou to read it"
                    ),
                )])
            }
            Some(n) => {
                return Err(vec![Diag::new(
                    "vitals_format_unsupported",
                    "$.vitals_format",
                    format!("unsupported vitals format {n} (this sahou reads format {VITALS_FORMAT})"),
                )])
            }
            None => {
                return Err(vec![Diag::new(
                    "vitals_unreadable",
                    "$.vitals_format",
                    format!("vitals_format must be an unsigned integer (got {f})"),
                )])
            }
        },
    }
    serde_json::from_value(value).map_err(|e| {
        vec![Diag::new(
            "vitals_unreadable",
            "$",
            format!("cannot parse vitals: {e}"),
        )]
    })
}
