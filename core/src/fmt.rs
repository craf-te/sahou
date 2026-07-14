use crate::contract::Contract;
use crate::diag::Diag;
use crate::parse::parse_contract;

/// Contract → deterministic canonical YAML.
/// Determinism rationale: maps are BTreeMap (keys ascending) / structs follow declaration order /
/// defaults are skipped / `to` is always a list.
pub fn serialize_contract(c: &Contract) -> String {
    serde_norway::to_string(c)
        .expect("serializing a Contract never fails (every field is Serialize)")
}

/// The body of `sahou fmt` (a pure function).
/// Known limitation: comments are not preserved (fork settled 2026-07-10; the CLI layer warns about it).
pub fn fmt(yaml: &str) -> Result<String, Vec<Diag>> {
    Ok(serialize_contract(&parse_contract(yaml)?))
}
