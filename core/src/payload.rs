use base64::Engine;
use serde_json::Value;

use crate::contract::{Slot, SlotKind, Typing};
use crate::diag::Diag;
use crate::typespec::{Field, TypeName, TypeSpec};

/// The "type view" being validated. Walks Field / DetailedType / TypeName in the same shape.
struct TypeView<'a> {
    ty: TypeName,
    min: Option<f64>,
    max: Option<f64>,
    max_len: Option<u64>,
    values: &'a [String],
    items: Option<&'a TypeSpec>,
    any_of: &'a [TypeSpec],
    fields: &'a [Field],
}

impl<'a> TypeView<'a> {
    fn of_field(f: &'a Field) -> Self {
        Self {
            ty: f.ty,
            min: f.min,
            max: f.max,
            max_len: f.max_len,
            values: &f.values,
            items: f.items.as_ref(),
            any_of: &f.any_of,
            fields: &f.fields,
        }
    }
    fn of_spec(s: &'a TypeSpec) -> Self {
        match s {
            TypeSpec::Name(ty) => Self {
                ty: *ty,
                min: None,
                max: None,
                max_len: None,
                values: &[],
                items: None,
                any_of: &[],
                fields: &[],
            },
            TypeSpec::Detailed(d) => Self {
                ty: d.ty,
                min: d.min,
                max: d.max,
                max_len: d.max_len,
                values: &d.values,
                items: d.items.as_ref(),
                any_of: &d.any_of,
                fields: &d.fields,
            },
        }
    }
}

/// Path segment. The string is assembled **only on error** (spec §8 2-A).
#[derive(Clone, Copy)]
enum Seg<'a> {
    Key(&'a str),
    Idx(usize),
}

struct Ctx<'a> {
    stack: Vec<Seg<'a>>,
    diags: Vec<Diag>,
}

impl<'a> Ctx<'a> {
    fn path(&self) -> String {
        let mut s = String::from("$");
        for seg in &self.stack {
            match seg {
                Seg::Key(k) => {
                    s.push('.');
                    s.push_str(k);
                }
                Seg::Idx(i) => s.push_str(&format!("[{i}]")),
            }
        }
        s
    }
    fn err(&mut self, code: &str, message: String) {
        let path = self.path();
        self.diags.push(Diag::new(code, path, message));
    }
}

/// Validate a slot against a JSON value. typing=any / kind=opaque is skipped (returns []).
pub fn validate_payload(slot: &Slot, value: &Value) -> Vec<Diag> {
    if slot.typing == Typing::Any || slot.kind == SlotKind::Opaque {
        return vec![];
    }
    let mut ctx = Ctx {
        stack: Vec::with_capacity(8),
        diags: Vec::new(),
    };
    validate_record(&slot.fields, value, &mut ctx);
    ctx.diags
}

/// JSON-string variant (for FFI/wasm). A parse failure is a decode_error.
pub fn validate_json(slot: &Slot, json: &str) -> Vec<Diag> {
    match serde_json::from_str::<Value>(json) {
        Ok(v) => validate_payload(slot, &v),
        Err(e) => vec![Diag::new(
            "decode_error",
            "$",
            format!("cannot be parsed as JSON: {e}"),
        )],
    }
}

fn validate_record<'a>(fields: &'a [Field], value: &'a Value, ctx: &mut Ctx<'a>) {
    let Some(obj) = value.as_object() else {
        ctx.err(
            "type_mismatch",
            format!("expected record but got {}", kind_name(value)),
        );
        return;
    };
    // Unknown fields are dropped (forward compat). Only defined fields are checked.
    for f in fields {
        match obj.get(&f.name) {
            None | Some(Value::Null) => {
                if f.required {
                    ctx.stack.push(Seg::Key(&f.name));
                    ctx.err("required", format!("missing required field '{}'", f.name));
                    ctx.stack.pop();
                }
            }
            Some(v) => {
                ctx.stack.push(Seg::Key(&f.name));
                validate_value(&TypeView::of_field(f), v, ctx);
                ctx.stack.pop();
            }
        }
    }
}

fn validate_value<'a>(view: &TypeView<'a>, value: &'a Value, ctx: &mut Ctx<'a>) {
    match view.ty {
        TypeName::Int => match value.as_i64() {
            Some(n) => check_range(view, n as f64, ctx),
            None => ctx.err(
                "type_mismatch",
                format!("expected int but got {}", kind_name(value)),
            ),
        },
        TypeName::Float => match value.as_f64() {
            Some(n) => check_range(view, n, ctx),
            None => ctx.err(
                "type_mismatch",
                format!("expected float but got {}", kind_name(value)),
            ),
        },
        TypeName::Bool => {
            if !value.is_boolean() {
                ctx.err(
                    "type_mismatch",
                    format!("expected bool but got {}", kind_name(value)),
                );
            }
        }
        TypeName::String => match value.as_str() {
            Some(s) => {
                if let Some(max) = view.max_len {
                    if s.chars().count() as u64 > max {
                        ctx.err(
                            "too_long",
                            format!("string length {} exceeds max_len {max}", s.chars().count()),
                        );
                    }
                }
            }
            None => ctx.err(
                "type_mismatch",
                format!("expected string but got {}", kind_name(value)),
            ),
        },
        TypeName::Bytes => match value.as_str() {
            Some(s) => {
                if base64::engine::general_purpose::STANDARD.decode(s).is_err() {
                    ctx.err("bad_base64", "bytes must be a base64 string".to_string());
                }
            }
            None => ctx.err(
                "type_mismatch",
                format!(
                    "expected bytes (base64 string) but got {}",
                    kind_name(value)
                ),
            ),
        },
        TypeName::Timestamp => {
            // integer epoch milliseconds (settled in this plan)
            if value.as_i64().is_none() {
                ctx.err(
                    "type_mismatch",
                    format!(
                        "expected timestamp (epoch ms integer) but got {}",
                        kind_name(value)
                    ),
                );
            }
        }
        TypeName::Enum => match value.as_str() {
            Some(s) if view.values.iter().any(|v| v == s) => {}
            Some(s) => ctx.err(
                "enum_mismatch",
                format!("'{s}' is not among the allowed values {:?}", view.values),
            ),
            None => ctx.err(
                "type_mismatch",
                format!("expected enum (string) but got {}", kind_name(value)),
            ),
        },
        TypeName::Array => match value.as_array() {
            Some(arr) => {
                if let Some(items) = view.items {
                    let iv = TypeView::of_spec(items);
                    for (i, item) in arr.iter().enumerate() {
                        ctx.stack.push(Seg::Idx(i));
                        validate_value(&iv, item, ctx);
                        ctx.stack.pop();
                    }
                }
            }
            None => ctx.err(
                "type_mismatch",
                format!("expected array but got {}", kind_name(value)),
            ),
        },
        TypeName::Map => match value.as_object() {
            Some(obj) => {
                if let Some(items) = view.items {
                    let iv = TypeView::of_spec(items);
                    for (k, v) in obj {
                        ctx.stack.push(Seg::Key(k));
                        validate_value(&iv, v, ctx);
                        ctx.stack.pop();
                    }
                }
            }
            None => ctx.err(
                "type_mismatch",
                format!("expected map but got {}", kind_name(value)),
            ),
        },
        TypeName::Group => validate_record(view.fields, value, ctx),
        TypeName::Union => {
            // untagged: OK if any candidate produces zero diagnostics
            let ok = view.any_of.iter().any(|spec| {
                let mut sub = Ctx {
                    stack: Vec::new(),
                    diags: Vec::new(),
                };
                validate_value(&TypeView::of_spec(spec), value, &mut sub);
                sub.diags.is_empty()
            });
            if !ok {
                ctx.err(
                    "no_union_match",
                    format!("{} matches none of the candidate types", kind_name(value)),
                );
            }
        }
    }
}

fn check_range(view: &TypeView, n: f64, ctx: &mut Ctx) {
    if let Some(min) = view.min {
        if n < min {
            ctx.err("out_of_range", format!("{n} < min({min})"));
            return;
        }
    }
    if let Some(max) = view.max {
        if n > max {
            ctx.err("out_of_range", format!("{n} > max({max})"));
        }
    }
}

fn kind_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
