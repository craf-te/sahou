use std::collections::BTreeMap;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::contract::{
    Congestion, Connection, Contract, Encoding, Node, Pattern, Priority, Reliability, Slot,
    ValidateLevel,
};
use crate::endpoints::Endpoints;

/// keyexpr = `<namespace>/<connection-id>` auto-derivation + `key:` override (Z16 Fork C).
pub fn resolve_key(namespace: &str, conn_id: &str, conn: &Connection) -> String {
    conn.key
        .clone()
        .unwrap_or_else(|| format!("{namespace}/{conn_id}"))
}

/// Per-connection fingerprint (Z22 A2). First 16 hex of the sha256 of the canonical JSON
/// (struct declaration order + BTreeMap ascending + defaults skipped). Per-connection rather than
/// whole-contract = localizes the blast radius.
pub fn connection_hash(conn_id: &str, conn: &Connection) -> String {
    let bytes = serde_json::to_vec(&(conn_id, conn)).expect("serializing a Connection never fails");
    let digest = Sha256::digest(&bytes);
    hex::encode(digest)[..16].to_string()
}

/// The whole IR (descriptor.json). A read-only derivation; a single artifact shared by every node.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    pub schema: String,
    pub version: String,
    pub namespace: String,
    #[serde(deserialize_with = "crate::parse::de_unique_map")]
    pub nodes: BTreeMap<String, Node>,
    #[serde(deserialize_with = "crate::parse::de_unique_map")]
    pub connections: BTreeMap<String, DescriptorConnection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DescriptorConnection {
    pub pattern: Pattern,
    pub from: String,
    pub to: Vec<String>,
    /// resolved keyexpr
    pub key: String,
    /// per-connection hash (for the handshake attachment)
    pub hash: String,
    /// selector argument for a query (contract attribute; query pattern only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    pub reliability: Reliability,
    pub congestion: Congestion,
    pub priority: Priority,
    pub express: bool,
    pub encoding: Encoding,
    pub validate: ValidateLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Slot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<Slot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<Slot>,
}

pub fn build_descriptor(c: &Contract, e: &Endpoints) -> Descriptor {
    let connections = c
        .connections
        .iter()
        .map(|(id, conn)| {
            (
                id.clone(),
                DescriptorConnection {
                    pattern: conn.pattern,
                    from: conn.from.clone(),
                    to: conn.to.clone(),
                    key: resolve_key(&e.namespace, id, conn),
                    hash: connection_hash(id, conn),
                    selector: conn.selector.clone(),
                    reliability: conn.reliability,
                    congestion: conn.congestion,
                    priority: conn.priority,
                    express: conn.express,
                    encoding: conn.encoding,
                    validate: conn.validate,
                    payload: conn.payload.clone(),
                    request: conn.request.clone(),
                    response: conn.response.clone(),
                },
            )
        })
        .collect();
    Descriptor {
        schema: c.schema.clone(),
        version: c.version.clone(),
        namespace: e.namespace.clone(),
        nodes: c.nodes.clone(),
        connections,
    }
}

/// Pretty JSON for the CLI / wasm.
pub fn descriptor_json(c: &Contract, e: &Endpoints) -> String {
    serde_json::to_string_pretty(&build_descriptor(c, e))
        .expect("serializing a Descriptor never fails")
}
