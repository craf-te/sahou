use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

use serde::de::{DeserializeOwned, Deserializer, MapAccess, Visitor};
use serde::Deserialize;

use crate::contract::Contract;
use crate::diag::Diag;

/// YAML → any type. Uses serde_path_to_error to produce a single diagnostic carrying a semantic path.
/// Shared parse entry for contracts and endpoints (the NO on duplicate/unknown keys is enforced by
/// each struct's deny_unknown_fields / de_unique_map).
pub fn parse_yaml<T: DeserializeOwned>(yaml: &str) -> Result<T, Vec<Diag>> {
    let de = serde_norway::Deserializer::from_str(yaml);
    serde_path_to_error::deserialize::<_, T>(de).map_err(|e| {
        let path = e.path().to_string();
        let msg = e.into_inner().to_string();
        vec![Diag::new("parse_error", path, msg)]
    })
}

/// YAML → Contract. Duplicate and unknown keys are a boundary NO (never silently swallowed).
/// The error path is the semantic path from serde_path_to_error (e.g. connections.c.reliability).
pub fn parse_contract(yaml: &str) -> Result<Contract, Vec<Diag>> {
    parse_yaml(yaml)
}

/// MapAccess visitor that promotes serde's default "duplicate key last-wins" into an explicit error.
pub fn de_unique_map<'de, D, V>(deserializer: D) -> Result<BTreeMap<String, V>, D::Error>
where
    D: Deserializer<'de>,
    V: Deserialize<'de>,
{
    struct UniqueMap<V>(PhantomData<V>);
    impl<'de, V: Deserialize<'de>> Visitor<'de> for UniqueMap<V> {
        type Value = BTreeMap<String, V>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "a map with unique keys")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            let mut map = BTreeMap::new();
            while let Some((k, v)) = access.next_entry::<String, V>()? {
                if map.insert(k.clone(), v).is_some() {
                    return Err(serde::de::Error::custom(format!("duplicate key '{k}'")));
                }
            }
            Ok(map)
        }
    }
    deserializer.deserialize_map(UniqueMap(PhantomData))
}

/// Accepts both `to: b` and `to: [b, c]` and normalizes to a Vec (fmt always emits a list).
pub fn de_string_or_seq<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match OneOrMany::deserialize(d)? {
        OneOrMany::One(s) => vec![s],
        OneOrMany::Many(v) => v,
    })
}

/// Normalizes both `version: 1` and `version: "1.2"` into a string.
pub fn de_version<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum V {
        S(String),
        I(i64),
        F(f64),
    }
    Ok(match V::deserialize(d)? {
        V::S(s) => s,
        V::I(i) => i.to_string(),
        V::F(f) => f.to_string(),
    })
}
