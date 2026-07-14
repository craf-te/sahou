use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::diag::Diag;
use crate::parse::{de_unique_map, parse_yaml};

fn default_namespace() -> String {
    "sahou".to_string()
}

/// Deployment layer (endpoints.<env>.yaml). "Empty is the default = auto-discovery on the same LAN" (spec §3; Z16/Z18).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
    /// Default prefix for the keyexpr (the derivation source of Fork C)
    #[serde(default = "default_namespace")]
    pub namespace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router: Option<Router>,
    #[serde(default, deserialize_with = "de_unique_map")]
    pub nodes: BTreeMap<String, NodeEndpoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<String>,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            env: None,
            namespace: default_namespace(),
            router: None,
            nodes: BTreeMap::new(),
            plugins: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Router {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeEndpoint {
    #[serde(default)]
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connect: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Auto,
    Peer,
    Client,
}

pub fn parse_endpoints(yaml: &str) -> Result<Endpoints, Vec<Diag>> {
    parse_yaml(yaml)
}

/// Endpoints → deterministic canonical YAML (symmetric with serialize_contract; the save path of the
/// GUI deploy tab). Determinism rationale: nodes is a BTreeMap (keys ascending) / structs follow
/// declaration order / defaults are skipped.
pub fn serialize_endpoints(e: &Endpoints) -> String {
    serde_norway::to_string(e)
        .expect("serializing Endpoints never fails (every field is Serialize)")
}
