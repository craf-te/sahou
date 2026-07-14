use serde::{Deserialize, Serialize};

/// Recursive type system (Z23; spec §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeName {
    Int,
    Float,
    Bool,
    String,
    Bytes,     // base64 string on the JSON wire
    Timestamp, // integer epoch milliseconds (settled in this plan)
    Enum,
    Array,
    Map,
    Group, // anonymous inline hierarchy
    Union,
}

/// Type specification usable in items / any_of. Accepts both the shorthand ("float") and the detailed form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TypeSpec {
    Name(TypeName),
    Detailed(Box<DetailedType>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetailedType {
    #[serde(rename = "type")]
    pub ty: TypeName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<TypeSpec>, // element type of array / value type of map
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>, // enum
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any_of: Vec<TypeSpec>, // union (untagged)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Field>, // group
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_len: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeName,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_len: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>, // enum
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<TypeSpec>, // array / map
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any_of: Vec<TypeSpec>, // union
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Field>, // group
}

pub(crate) fn default_true() -> bool {
    true
}
pub(crate) fn is_true(b: &bool) -> bool {
    *b
}
