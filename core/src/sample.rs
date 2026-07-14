use serde_json::{json, Map, Value};

use crate::contract::{Slot, SlotKind, Typing};
use crate::typespec::{Field, TypeName, TypeSpec};

/// Generate a valid sample payload from a type (deterministic; no randomness).
/// Guarantee: `validate_payload(slot, &sample_slot(slot))` is always empty (ensured by tests).
pub fn sample_slot(slot: &Slot) -> Value {
    if slot.typing == Typing::Any || slot.kind == SlotKind::Opaque {
        return json!({});
    }
    sample_record(&slot.fields)
}

fn sample_record(fields: &[Field]) -> Value {
    let mut obj = Map::new();
    for f in fields {
        obj.insert(f.name.clone(), sample_field(f));
    }
    Value::Object(obj)
}

fn sample_field(f: &Field) -> Value {
    sample_type(
        f.ty,
        f.min,
        f.max,
        f.max_len,
        &f.values,
        f.items.as_ref(),
        &f.any_of,
        &f.fields,
    )
}

fn sample_spec(spec: &TypeSpec) -> Value {
    match spec {
        TypeSpec::Name(ty) => sample_type(*ty, None, None, None, &[], None, &[], &[]),
        TypeSpec::Detailed(d) => sample_type(
            d.ty,
            d.min,
            d.max,
            d.max_len,
            &d.values,
            d.items.as_ref(),
            &d.any_of,
            &d.fields,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn sample_type(
    ty: TypeName,
    min: Option<f64>,
    max: Option<f64>,
    max_len: Option<u64>,
    values: &[String],
    items: Option<&TypeSpec>,
    any_of: &[TypeSpec],
    fields: &[Field],
) -> Value {
    match ty {
        TypeName::Int => {
            let v: i64 = match (min, max) {
                (Some(lo), _) => lo.ceil() as i64,
                (None, Some(hi)) => hi.min(0.0).floor() as i64,
                (None, None) => 0,
            };
            json!(v)
        }
        TypeName::Float => {
            let v: f64 = match (min, max) {
                (Some(lo), _) => lo,
                (None, Some(hi)) => hi.min(0.0),
                (None, None) => 0.0,
            };
            json!(v)
        }
        TypeName::Bool => json!(false),
        TypeName::String => {
            let s = "sample";
            json!(match max_len {
                Some(n) => s.chars().take(n as usize).collect::<String>(),
                None => s.to_string(),
            })
        }
        TypeName::Bytes => json!(""), // the empty string is valid base64
        TypeName::Timestamp => json!(0),
        TypeName::Enum => json!(values.first().cloned().unwrap_or_default()),
        TypeName::Array => json!([items.map(sample_spec).unwrap_or(Value::Null)]),
        TypeName::Map => json!({ "key": items.map(sample_spec).unwrap_or(Value::Null) }),
        TypeName::Group => sample_record(fields),
        TypeName::Union => any_of.first().map(sample_spec).unwrap_or(Value::Null),
    }
}
