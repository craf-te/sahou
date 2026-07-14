use serde::Serialize;

use crate::contract::{Connection, Contract};
use crate::diag::Diag;
use crate::ir::connection_hash;
use crate::typespec::{Field, TypeName, TypeSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Additive,
    Promotion,
    Breaking,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Change {
    pub connection: String,
    pub path: String,
    pub kind: ChangeKind,
    pub detail: String,
}

/// Automatically classifies the old→new contract diff into additive / promotion / breaking (Z22 B3).
pub fn classify(old: &Contract, new: &Contract) -> Vec<Change> {
    let mut changes = Vec::new();
    for (id, new_conn) in &new.connections {
        match old.connections.get(id) {
            None => changes.push(Change {
                connection: id.clone(),
                path: format!("connections.{id}"),
                kind: ChangeKind::Additive,
                detail: "new connection added".to_string(),
            }),
            Some(old_conn) => classify_connection(id, old_conn, new_conn, &mut changes),
        }
    }
    for id in old.connections.keys() {
        if !new.connections.contains_key(id) {
            changes.push(Change {
                connection: id.clone(),
                path: format!("connections.{id}"),
                kind: ChangeKind::Breaking,
                detail: "connection removed".to_string(),
            });
        }
    }
    changes
}

fn classify_connection(id: &str, old: &Connection, new: &Connection, changes: &mut Vec<Change>) {
    let mut push = |path: String, kind: ChangeKind, detail: String| {
        changes.push(Change {
            connection: id.to_string(),
            path,
            kind,
            detail,
        });
    };
    // topology changes are breaking
    if old.pattern != new.pattern {
        push(
            format!("connections.{id}.pattern"),
            ChangeKind::Breaking,
            "pattern changed (topology)".into(),
        );
    }
    if old.from != new.from {
        push(
            format!("connections.{id}.from"),
            ChangeKind::Breaking,
            "from changed (topology)".into(),
        );
    }
    if old.to != new.to {
        push(
            format!("connections.{id}.to"),
            ChangeKind::Breaking,
            "to changed (topology)".into(),
        );
    }
    if old.key != new.key {
        push(
            format!("connections.{id}.key"),
            ChangeKind::Breaking,
            "keyexpr override changed (the delivery keyexpr changes)".into(),
        );
    }
    if old.selector != new.selector {
        push(
            format!("connections.{id}.selector"),
            ChangeKind::Breaking,
            "selector changed (the query's request condition changes)".into(),
        );
    }
    for (slot_name, old_slot, new_slot) in [
        ("payload", &old.payload, &new.payload),
        ("request", &old.request, &new.request),
        ("response", &old.response, &new.response),
    ] {
        if let (Some(o), Some(n)) = (old_slot, new_slot) {
            diff_fields(
                id,
                &format!("connections.{id}.{slot_name}.fields"),
                &o.fields,
                &n.fields,
                changes,
            );
        }
    }
}

fn diff_fields(id: &str, base: &str, old: &[Field], new: &[Field], changes: &mut Vec<Change>) {
    // path grammar (unified in spec §4): index form of the structural position. Add/change = index on
    // the new side / removal = index on the old side (absent in the new contract). The field name
    // (identity) is always included in detail.
    for (i, nf) in new.iter().enumerate() {
        let path = format!("{base}[{i}]");
        match old.iter().find(|of| of.name == nf.name) {
            None => {
                let (kind, detail) = if nf.required {
                    (
                        ChangeKind::Breaking,
                        format!(
                            "required field '{}' added (old senders cannot send it)",
                            nf.name
                        ),
                    )
                } else {
                    (
                        ChangeKind::Additive,
                        format!("optional field '{}' added", nf.name),
                    )
                };
                changes.push(Change {
                    connection: id.to_string(),
                    path,
                    kind,
                    detail,
                });
            }
            Some(of) => diff_field(id, &path, of, nf, changes),
        }
    }
    for (i, of) in old.iter().enumerate() {
        if !new.iter().any(|nf| nf.name == of.name) {
            changes.push(Change {
                connection: id.to_string(),
                path: format!("{base}[{i}]"),
                kind: ChangeKind::Breaking,
                detail: format!(
                    "field '{}' removed (a receiver may require it; conservative call)",
                    of.name
                ),
            });
        }
    }
}

fn diff_field(id: &str, path: &str, old: &Field, new: &Field, changes: &mut Vec<Change>) {
    let fname = &new.name;
    let mut push = |p: String, kind: ChangeKind, detail: String| {
        changes.push(Change {
            connection: id.to_string(),
            path: p,
            kind,
            detail,
        });
    };
    if old.ty != new.ty {
        if old.ty == TypeName::Int && new.ty == TypeName::Float {
            push(
                format!("{path}.type"),
                ChangeKind::Promotion,
                format!("field '{fname}' type promotion int→float (compatible)"),
            );
        } else {
            push(
                format!("{path}.type"),
                ChangeKind::Breaking,
                format!("field '{fname}' type changed {:?}→{:?}", old.ty, new.ty),
            );
        }
        return; // once the type changes, comparing the attributes below is meaningless
    }
    if !old.required && new.required {
        push(
            format!("{path}.required"),
            ChangeKind::Breaking,
            format!("field '{fname}': optional→required"),
        );
    } else if old.required && !new.required {
        push(
            format!("{path}.required"),
            ChangeKind::Additive,
            format!("field '{fname}': required→optional"),
        );
    }
    // enum values
    for v in &new.values {
        if !old.values.contains(v) {
            push(
                format!("{path}.values"),
                ChangeKind::Additive,
                format!("field '{fname}': enum value added '{v}'"),
            );
        }
    }
    for v in &old.values {
        if !new.values.contains(v) {
            push(
                format!("{path}.values"),
                ChangeKind::Breaking,
                format!("field '{fname}': enum value removed '{v}'"),
            );
        }
    }
    // range: relaxation = additive / tightening = breaking
    diff_bound(id, path, fname, "min", old.min, new.min, false, changes);
    diff_bound(id, path, fname, "max", old.max, new.max, true, changes);
    diff_bound(
        id,
        path,
        fname,
        "max_len",
        old.max_len.map(|v| v as f64),
        new.max_len.map(|v| v as f64),
        true,
        changes,
    );
    // items (element type of array/map): if not identical, judge conservatively
    match (&old.items, &new.items) {
        (Some(o), Some(n)) if o != n => {
            let kind = if is_int_to_float(o, n) {
                ChangeKind::Promotion
            } else {
                ChangeKind::Breaking
            };
            changes.push(Change {
                connection: id.to_string(),
                path: format!("{path}.items"),
                kind,
                detail: format!("field '{fname}': element type changed"),
            });
        }
        _ => {}
    }
    // group recurses (nesting is also index form: fields[i].fields[j]…)
    if old.ty == TypeName::Group {
        diff_fields(
            id,
            &format!("{path}.fields"),
            &old.fields,
            &new.fields,
            changes,
        );
    }
}

fn is_int_to_float(old: &TypeSpec, new: &TypeSpec) -> bool {
    matches!(
        (old, new),
        (
            TypeSpec::Name(TypeName::Int),
            TypeSpec::Name(TypeName::Float)
        )
    )
}

/// "For upper bounds (upper=true) an increase is a relaxation / for lower bounds a decrease is a relaxation."
#[allow(clippy::too_many_arguments)]
fn diff_bound(
    id: &str,
    path: &str,
    fname: &str,
    name: &str,
    old: Option<f64>,
    new: Option<f64>,
    upper: bool,
    changes: &mut Vec<Change>,
) {
    let (kind, detail) = match (old, new) {
        (Some(o), Some(n)) if o == n => return,
        (None, None) => return,
        // dropping a constraint = relaxation
        (Some(_), None) => (
            ChangeKind::Additive,
            format!("field '{fname}': {name} constraint dropped (relaxation)"),
        ),
        // introducing a constraint = tightening
        (None, Some(_)) => (
            ChangeKind::Breaking,
            format!("field '{fname}': {name} constraint introduced (tightening)"),
        ),
        (Some(o), Some(n)) => {
            let relaxed = if upper { n > o } else { n < o };
            if relaxed {
                (
                    ChangeKind::Additive,
                    format!("field '{fname}': {name} relaxed {o}→{n}"),
                )
            } else {
                (
                    ChangeKind::Breaking,
                    format!("field '{fname}': {name} tightened {o}→{n}"),
                )
            }
        }
    };
    changes.push(Change {
        connection: id.to_string(),
        path: format!("{path}.{name}"),
        kind,
        detail,
    });
}

/// Structural classification of a single connection pair (the public entry the runtime handshake reuses).
pub fn classify_pair(id: &str, old: &Connection, new: &Connection) -> Vec<Change> {
    let mut changes = Vec::new();
    classify_connection(id, old, new, &mut changes);
    changes
}

pub fn is_compatible(changes: &[Change]) -> bool {
    !changes.iter().any(|c| c.kind == ChangeKind::Breaking)
}

/// List of connections whose hash changed (to check the blast radius; Z22 A2).
pub fn changed_connections(old: &Contract, new: &Contract) -> Vec<String> {
    let mut ids: Vec<String> = new
        .connections
        .iter()
        .filter(|(id, conn)| {
            old.connections
                .get(*id)
                .map(|o| connection_hash(id, o) != connection_hash(id, conn))
                .unwrap_or(true)
        })
        .map(|(id, _)| id.clone())
        .collect();
    ids.sort();
    ids
}

/// Delivery-time handshake (Z26): not a strict hash match but a compat judgement.
/// The receiver (its own contract) judges the sender's contract for the connection in question and
/// **passes only pure additive (or no change); breaking and promotion are a NO** (= live-rollout works).
///
/// Approach A (conservative; spec §10 fork 5): `classify` is undirected (structural diff old→new) and
/// treats int→float as promotion=compatible, but delivery is a **directed** problem where the writer
/// (sender) and reader (receiver) hold different versions. Passing an undirected promotion as-is would
/// falsely YES the dangerous direction (receiver=int / sender=float → type_mismatch on every message).
/// Directional judgement is not implemented, so at the delivery boundary promotion is also
/// conservatively turned into a NO (at the cost of falsely NO-ing the safe direction, it seals off the
/// dangerous false YES). Directional support (approach B) will be settled in fork 5 when a numeric-type
/// live extension is actually needed.
///
/// Note that `is_compatible` / `classify` stay undirected for tooling judgement
/// ("should I worry about this diff?" and "can these two actually deliver to each other?" are different questions).
pub fn handshake(receiver: &Contract, sender: &Contract, conn_id: &str) -> Result<(), Vec<Diag>> {
    let diags: Vec<Diag> = classify(receiver, sender)
        .into_iter()
        .filter(|c| c.connection == conn_id && c.kind != ChangeKind::Additive)
        .map(|c| {
            let detail = match c.kind {
                ChangeKind::Promotion => format!(
                    "{} (conservatively NO at the delivery boundary because direction is undecided; spec §10 fork 5)",
                    c.detail
                ),
                _ => c.detail,
            };
            Diag::new("schema_incompatible", c.path, detail)
        })
        .collect();
    if diags.is_empty() {
        Ok(())
    } else {
        Err(diags)
    }
}
