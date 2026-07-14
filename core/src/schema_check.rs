use crate::contract::{Connection, Contract, Pattern, Slot, SlotKind, Typing};
use crate::diag::Diag;
use crate::typespec::{Field, TypeName, TypeSpec};

/// Detect contradictions in the schema itself, with positions (spec §4; main battleground ④). Empty for a correct schema.
pub fn validate_schema(c: &Contract) -> Vec<Diag> {
    let mut diags = Vec::new();
    for (id, conn) in &c.connections {
        check_topology(c, id, conn, &mut diags);
        check_slots_for_pattern(id, conn, &mut diags);
        for (slot_name, slot) in present_slots(conn) {
            check_slot(&format!("connections.{id}.{slot_name}"), slot, &mut diags);
        }
    }
    diags
}

fn check_topology(c: &Contract, id: &str, conn: &Connection, diags: &mut Vec<Diag>) {
    if !c.nodes.contains_key(&conn.from) {
        diags.push(Diag::new(
            "unknown_node",
            format!("connections.{id}.from"),
            format!("undefined node '{}' referenced in from", conn.from),
        ));
    }
    for (i, to) in conn.to.iter().enumerate() {
        if to == &conn.from {
            diags.push(Diag::new(
                "self_loop",
                format!("connections.{id}.to[{i}]"),
                format!("from and to are the same node '{to}' (self loop)"),
            ));
        } else if !c.nodes.contains_key(to) {
            diags.push(Diag::new(
                "unknown_node",
                format!("connections.{id}.to[{i}]"),
                format!("undefined node '{to}' referenced in to"),
            ));
        }
    }
}

fn check_slots_for_pattern(id: &str, conn: &Connection, diags: &mut Vec<Diag>) {
    let missing = |slot: &str| {
        Diag::new(
            "missing_slot",
            format!("connections.{id}.{slot}"),
            format!("pattern {:?} requires the {slot} slot", conn.pattern),
        )
    };
    let unexpected = |slot: &str| {
        Diag::new(
            "unexpected_slot",
            format!("connections.{id}.{slot}"),
            format!("pattern {:?} cannot have the {slot} slot", conn.pattern),
        )
    };
    match conn.pattern {
        Pattern::PubSub => {
            if conn.payload.is_none() {
                diags.push(missing("payload"));
            }
            if conn.request.is_some() {
                diags.push(unexpected("request"));
            }
            if conn.response.is_some() {
                diags.push(unexpected("response"));
            }
            if conn.selector.is_some() {
                diags.push(Diag::new(
                    "unexpected_selector",
                    format!("connections.{id}.selector"),
                    "selector is for the query pattern only (not usable with pub_sub)",
                ));
            }
        }
        Pattern::Query => {
            if conn.request.is_none() {
                diags.push(missing("request"));
            }
            if conn.response.is_none() {
                diags.push(missing("response"));
            }
            if conn.payload.is_some() {
                diags.push(unexpected("payload"));
            }
        }
    }
}

pub(crate) fn present_slots(conn: &Connection) -> Vec<(&'static str, &Slot)> {
    let mut v = Vec::new();
    if let Some(s) = &conn.payload {
        v.push(("payload", s));
    }
    if let Some(s) = &conn.request {
        v.push(("request", s));
    }
    if let Some(s) = &conn.response {
        v.push(("response", s));
    }
    v
}

fn check_slot(path: &str, slot: &Slot, diags: &mut Vec<Diag>) {
    if slot.typing == Typing::Any || slot.kind == SlotKind::Opaque {
        return; // any is unvalidated (the GUI shows red) / opaque carries only encoding
    }
    check_fields(path, &slot.fields, diags);
}

fn check_fields(path: &str, fields: &[Field], diags: &mut Vec<Diag>) {
    let mut seen = std::collections::BTreeSet::new();
    for (i, f) in fields.iter().enumerate() {
        let fpath = format!("{path}.fields[{i}]");
        if !seen.insert(f.name.clone()) {
            diags.push(Diag::new(
                "duplicate_field",
                format!("{fpath}.name"),
                format!("duplicate field name '{}'", f.name),
            ));
        }
        check_type_attrs(
            &fpath,
            f.ty,
            f.min,
            f.max,
            &f.values,
            f.items.as_ref(),
            &f.any_of,
            &f.fields,
            diags,
        );
        if let Some(default) = &f.default {
            check_default(&fpath, f, default, diags);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn check_type_attrs(
    path: &str,
    ty: TypeName,
    min: Option<f64>,
    max: Option<f64>,
    values: &[String],
    items: Option<&TypeSpec>,
    any_of: &[TypeSpec],
    fields: &[Field],
    diags: &mut Vec<Diag>,
) {
    for (name, v) in [("min", min), ("max", max)] {
        if let Some(x) = v {
            if !x.is_finite() {
                diags.push(Diag::new(
                    "non_finite_bound",
                    path,
                    format!("range bound {name} cannot be a non-finite value (NaN/Inf)"),
                ));
            }
        }
    }
    if let (Some(lo), Some(hi)) = (min, max) {
        if lo > hi {
            diags.push(Diag::new(
                "invalid_range",
                path,
                format!("min({lo}) > max({hi})"),
            ));
        }
    }
    match ty {
        TypeName::Enum if values.is_empty() => {
            diags.push(Diag::new(
                "empty_enum",
                format!("{path}.values"),
                "enum field has empty values",
            ));
        }
        TypeName::Array | TypeName::Map => match items {
            None => diags.push(Diag::new(
                "missing_items",
                format!("{path}.items"),
                format!("{ty:?} requires items (element/value type)"),
            )),
            Some(spec) => check_typespec(&format!("{path}.items"), spec, diags),
        },
        TypeName::Union if any_of.is_empty() => {
            diags.push(Diag::new(
                "empty_union",
                format!("{path}.any_of"),
                "union has empty any_of",
            ));
        }
        TypeName::Union => {
            for (i, spec) in any_of.iter().enumerate() {
                check_typespec(&format!("{path}.any_of[{i}]"), spec, diags);
            }
        }
        TypeName::Group => check_fields(path, fields, diags),
        _ => {}
    }
}

fn check_typespec(path: &str, spec: &TypeSpec, diags: &mut Vec<Diag>) {
    match spec {
        TypeSpec::Name(ty) => check_type_attrs(path, *ty, None, None, &[], None, &[], &[], diags),
        TypeSpec::Detailed(d) => check_type_attrs(
            path,
            d.ty,
            d.min,
            d.max,
            &d.values,
            d.items.as_ref(),
            &d.any_of,
            &d.fields,
            diags,
        ),
    }
}

/// Type consistency of Field.default (immediate follow-up to spec §10; GUI design §5.1).
/// Wraps the single field into a 1-field Slot and reuses the payload validation engine (validate_json)
/// as-is — no duplicate implementation of the validation logic.
fn check_default(fpath: &str, f: &Field, default: &serde_json::Value, diags: &mut Vec<Diag>) {
    // A field whose type definition itself is broken is left to the earlier diagnostics (missing_items / empty_enum / empty_union)
    let broken_type = match f.ty {
        TypeName::Array | TypeName::Map => f.items.is_none(),
        TypeName::Enum => f.values.is_empty(),
        TypeName::Union => f.any_of.is_empty(),
        _ => false,
    };
    if broken_type {
        return;
    }
    let slot = crate::contract::Slot {
        typing: Typing::Typed,
        kind: SlotKind::Record,
        fields: vec![f.clone()],
        encoding: None,
    };
    let mut obj = serde_json::Map::new();
    obj.insert(f.name.clone(), default.clone());
    let payload = serde_json::Value::Object(obj).to_string();
    for d in crate::payload::validate_json(&slot, &payload) {
        diags.push(Diag::new(
            "invalid_default",
            format!("{fpath}.default"),
            format!(
                "default does not match the type of field '{}': {}",
                f.name, d.message
            ),
        ));
    }
}
