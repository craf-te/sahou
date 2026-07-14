use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::parse::{de_string_or_seq, de_unique_map, de_version};
use crate::typespec::Field;

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}
fn is_false(b: &bool) -> bool {
    !*b
}
fn default_version() -> String {
    "1".to_string()
}

/// The whole contract (schema.sahou.yaml). The single editable source (spec §3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub schema: String,
    /// semver label (states intent; structural compat is the actual check. spec §5.3)
    #[serde(default = "default_version", deserialize_with = "de_version")]
    pub version: String,
    #[serde(deserialize_with = "de_unique_map")]
    pub nodes: BTreeMap<String, Node>,
    #[serde(deserialize_with = "de_unique_map")]
    pub connections: BTreeMap<String, Connection>,
}

/// node = identifier only (no role/language; capabilities are derived from the wiring).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    #[serde(default, skip_serializing_if = "is_default")]
    pub kind: NodeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    #[default]
    Sahou,
    External,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub pattern: Pattern,
    pub from: String,
    #[serde(deserialize_with = "de_string_or_seq")]
    pub to: Vec<String>,
    /// keyexpr override (when omitted, derived automatically from namespace + connection id)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// selector argument for a query (query pattern only; GUI design §5.2).
    /// A contract attribute = serialized and included in the connection hash (skip keeps existing
    /// hashes unchanged when it is unspecified).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub reliability: Reliability,
    #[serde(default, skip_serializing_if = "is_default")]
    pub congestion: Congestion,
    #[serde(default, skip_serializing_if = "is_default")]
    pub priority: Priority,
    #[serde(default, skip_serializing_if = "is_false")]
    pub express: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub encoding: Encoding,
    #[serde(default, skip_serializing_if = "is_default")]
    pub validate: ValidateLevel,
    // shape slots: pub_sub = 1 payload / query = request + response (2 slots)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Slot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<Slot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<Slot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    PubSub,
    Query,
}

/// QoS default = streaming (best_effort + drop). "Reliable" is an explicit opt-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reliability {
    #[default]
    BestEffort,
    Reliable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Congestion {
    #[default]
    Drop,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    #[serde(rename = "realtime")]
    RealTime,
    InteractiveHigh,
    InteractiveLow,
    DataHigh,
    #[default]
    Data,
    DataLow,
    Background,
}

/// encoding is a contract attribute of the connection (spec §8 1-B). The first version supports json only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    #[default]
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidateLevel {
    #[default]
    Full,
    Sampled,
    Off,
}

/// shape slot (typing: any = unvalidated / typed = validated. spec §3)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub typing: Typing,
    #[serde(default, skip_serializing_if = "is_default")]
    pub kind: SlotKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Field>,
    /// encoding tag for opaque slots (e.g. "video/raw")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Typing {
    Any,
    Typed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotKind {
    #[default]
    Record,
    Opaque,
}
