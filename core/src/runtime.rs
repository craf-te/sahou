//! Runtime boundary functions (design §4; approach B).
//! Send/receive inspection, envelope assembly, hash checking, the 4 query boundaries, handshake judgement,
//! and retry classification are all placed here as pure functions. Zenoh is never touched (transport is the language glue's responsibility).

use serde::Serialize;

use crate::compat::{classify_pair, ChangeKind};
use crate::contract::{
    Congestion, Connection, Pattern, Priority, Reliability, Slot, Typing, ValidateLevel,
};
use crate::diag::Diag;
use crate::ir::{Descriptor, DescriptorConnection};
use crate::payload::validate_payload;
use crate::typespec::{Field, TypeName};

/// descriptor.json → Descriptor. Structural breakage / unknown keys are a boundary NO.
pub fn load_descriptor(json: &str) -> Result<Descriptor, Vec<Diag>> {
    let de = &mut serde_json::Deserializer::from_str(json);
    serde_path_to_error::deserialize::<_, Descriptor>(de).map_err(|e| {
        let path = e.path().to_string();
        let msg = e.into_inner().to_string();
        vec![Diag::new("descriptor_error", path, msg)]
    })
}

/// The capabilities a node derives from the wiring (capability = derived from wiring; a node is an identifier only).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodePlan {
    pub publishes: Vec<String>,
    pub subscribes: Vec<String>,
    /// the from side of a query (requester)
    pub queries: Vec<String>,
    /// the to side of a query (responder)
    pub answers: Vec<String>,
}

pub fn node_plan(desc: &Descriptor, node: &str) -> Result<NodePlan, Vec<Diag>> {
    if !desc.nodes.contains_key(node) {
        return Err(vec![Diag::new(
            "unknown_node",
            format!("nodes.{node}"),
            format!("undefined node '{node}' (not present in the descriptor)"),
        )]);
    }
    let mut plan = NodePlan {
        publishes: vec![],
        subscribes: vec![],
        queries: vec![],
        answers: vec![],
    };
    for (id, c) in &desc.connections {
        let is_from = c.from == node;
        let is_to = c.to.iter().any(|t| t == node);
        match c.pattern {
            Pattern::PubSub => {
                if is_from {
                    plan.publishes.push(id.clone());
                }
                if is_to {
                    plan.subscribes.push(id.clone());
                }
            }
            Pattern::Query => {
                if is_from {
                    plan.queries.push(id.clone());
                }
                if is_to {
                    plan.answers.push(id.clone());
                }
            }
        }
    }
    Ok(plan)
}

/// Nodes that can send on a pub_sub connection (i.e. are the `from` of one). Sorted, deduped.
/// Used to populate a sender-node selector (design §4). External-kind nodes are not excluded here;
/// the wiring (being a `from`) is what determines "can publish".
pub fn publishing_nodes(desc: &Descriptor) -> Vec<String> {
    let mut nodes: Vec<String> = desc
        .connections
        .values()
        .filter(|c| c.pattern == Pattern::PubSub)
        .map(|c| c.from.clone())
        .collect();
    nodes.sort();
    nodes.dedup();
    nodes
}

/// pub_sub connections whose `from` is `node` (the connections `node` can publish on). Sorted.
/// Empty for an unknown node or a node that publishes nothing (no error — this feeds a selector).
pub fn connections_from(desc: &Descriptor, node: &str) -> Vec<String> {
    // desc.connections is a BTreeMap, so iteration (and thus this list) is already sorted by name.
    desc.connections
        .iter()
        .filter(|(_, c)| c.pattern == Pattern::PubSub && c.from == node)
        .map(|(id, _)| id.clone())
        .collect()
}

/// Nodes that can receive on a pub_sub connection (i.e. are a `to` of one). Sorted, deduped.
/// Used to populate a receiver-node selector (Sahou In CHOP). The mirror of `publishing_nodes`.
pub fn subscribing_nodes(desc: &Descriptor) -> Vec<String> {
    let mut nodes: Vec<String> = desc
        .connections
        .values()
        .filter(|c| c.pattern == Pattern::PubSub)
        .flat_map(|c| c.to.iter().cloned())
        .collect();
    nodes.sort();
    nodes.dedup();
    nodes
}

/// pub_sub connections whose `to` includes `node` (the connections `node` receives). Sorted.
/// Empty for an unknown node or a node that receives nothing (no error — this feeds a selector).
/// The mirror of `connections_from`.
pub fn connections_to(desc: &Descriptor, node: &str) -> Vec<String> {
    // desc.connections is a BTreeMap, so iteration (and thus this list) is already sorted by name.
    desc.connections
        .iter()
        .filter(|(_, c)| c.pattern == Pattern::PubSub && c.to.iter().any(|t| t == node))
        .map(|(id, _)| id.clone())
        .collect()
}

/// The resolved keyexpr of a connection (e.g. "sahou/motion"), or None for an unknown connection.
/// Lets a receiver (Sahou In CHOP) subscribe without knowing the sender node.
pub fn connection_key(desc: &Descriptor, conn: &str) -> Option<String> {
    desc.connections.get(conn).map(|c| c.key.clone())
}

/// The per-connection schema hash of a connection (the 16-hex handshake attachment), or None for
/// an unknown connection. Lets a receiver-side "inject sample" (Sahou In CHOP) attach the same hash
/// a real Sahou sender would, without going through the send boundary — which requires the sender
/// node and so is unavailable to a receiver.
pub fn connection_hash(desc: &Descriptor, conn: &str) -> Option<String> {
    desc.connections.get(conn).map(|c| c.hash.clone())
}

/// The payload schema of a connection, as display rows `[name, type, required, detail]`, for a
/// "what should I send?" panel (design §5/§7). Empty for an unknown or `any`-typed connection.
/// `detail` is a compact human summary (range / enum values / group members) — the rendering lives
/// in the core so every language glue shows it identically.
pub fn connection_fields(desc: &Descriptor, conn: &str) -> Vec<[String; 4]> {
    let Some(c) = desc.connections.get(conn) else {
        return vec![];
    };
    let Some(slot) = c.payload.as_ref() else {
        return vec![];
    };
    if slot.typing == Typing::Any {
        return vec![];
    }
    slot.fields.iter().map(field_row).collect()
}

/// Is this a field type that maps to a numeric CHOP channel?
fn is_numeric(t: TypeName) -> bool {
    matches!(t, TypeName::Int | TypeName::Float | TypeName::Bool)
}

/// Read one JSON value as an f64 channel sample (bool -> 1.0/0.0). None if not numeric-shaped.
fn value_as_f64(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// Format an f64 stably for a channel value (reuse serde_json's Number formatting, which round-trips).
fn fmt_f64(x: f64) -> String {
    serde_json::Value::from(x).to_string()
}

/// One scalar JSON value as a short display string (for array rendering in decode_fields).
fn compact_scalar(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::String(s) => s.clone(),
        _ => "…".to_string(),
    }
}

/// Decode a validated payload's **numeric** fields into flat `name, count, v0, v1, …` groups
/// (scalar => count "1"; numeric array => its element values). Non-numeric fields are omitted.
/// Empty for an unknown/any-typed connection or an unparseable payload. Feeds the In CHOP channels.
pub fn decode_channels(desc: &Descriptor, conn: &str, payload_json: &str) -> Vec<String> {
    let Some(c) = desc.connections.get(conn) else {
        return vec![];
    };
    let Some(slot) = c.payload.as_ref() else {
        return vec![];
    };
    let Ok(serde_json::Value::Object(obj)) =
        serde_json::from_str::<serde_json::Value>(payload_json)
    else {
        return vec![];
    };
    let mut out = Vec::new();
    for f in &slot.fields {
        let Some(v) = obj.get(&f.name) else { continue };
        if is_numeric(f.ty) {
            if let Some(x) = value_as_f64(v) {
                out.push(f.name.clone());
                out.push("1".to_string());
                out.push(fmt_f64(x));
            }
        } else if f.ty == TypeName::Array {
            // A numeric array becomes one channel with N samples.
            if let serde_json::Value::Array(items) = v {
                let nums: Vec<f64> = items.iter().filter_map(value_as_f64).collect();
                if nums.len() == items.len() && !nums.is_empty() {
                    out.push(f.name.clone());
                    out.push(nums.len().to_string());
                    out.extend(nums.iter().map(|x| fmt_f64(*x)));
                }
            }
        }
    }
    out
}

/// Decode **all** payload fields into flat `name, kind, value` triples for a display table
/// (the In CHOP Info DAT — where string fields are visible). Value is stringified; arrays render
/// compact (`0.5, 0.3`). Empty for an unknown/any-typed connection or an unparseable payload.
pub fn decode_fields(desc: &Descriptor, conn: &str, payload_json: &str) -> Vec<String> {
    let Some(c) = desc.connections.get(conn) else {
        return vec![];
    };
    let Some(slot) = c.payload.as_ref() else {
        return vec![];
    };
    let Ok(serde_json::Value::Object(obj)) =
        serde_json::from_str::<serde_json::Value>(payload_json)
    else {
        return vec![];
    };
    let mut out = Vec::new();
    for f in &slot.fields {
        let (kind, value) = match obj.get(&f.name) {
            Some(serde_json::Value::Number(n)) => ("number", n.to_string()),
            Some(serde_json::Value::Bool(b)) => ("bool", b.to_string()),
            Some(serde_json::Value::String(s)) => ("string", s.clone()),
            Some(serde_json::Value::Array(items)) => (
                "array",
                items
                    .iter()
                    .map(compact_scalar)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            Some(serde_json::Value::Object(_)) => ("object", "{…}".to_string()),
            Some(serde_json::Value::Null) | None => ("missing", String::new()),
        };
        out.push(f.name.clone());
        out.push(kind.to_string());
        out.push(value);
    }
    out
}

fn field_row(f: &Field) -> [String; 4] {
    [
        f.name.clone(),
        type_name_str(f.ty).to_string(),
        if f.required { "yes" } else { "no" }.to_string(),
        field_detail(f),
    ]
}

fn type_name_str(t: TypeName) -> &'static str {
    match t {
        TypeName::Int => "int",
        TypeName::Float => "float",
        TypeName::Bool => "bool",
        TypeName::String => "string",
        TypeName::Bytes => "bytes",
        TypeName::Timestamp => "timestamp",
        TypeName::Enum => "enum",
        TypeName::Array => "array",
        TypeName::Map => "map",
        TypeName::Group => "group",
        TypeName::Union => "union",
    }
}

/// Compact, ASCII-only constraint summary for a field (safe to show in any UI).
fn field_detail(f: &Field) -> String {
    match f.ty {
        TypeName::Int | TypeName::Float => match (f.min, f.max) {
            (Some(a), Some(b)) => format!("{a}..{b}"),
            (Some(a), None) => format!(">={a}"),
            (None, Some(b)) => format!("<={b}"),
            (None, None) => String::new(),
        },
        TypeName::Enum => f.values.join("|"),
        TypeName::String => f
            .max_len
            .map(|m| format!("<={m} chars"))
            .unwrap_or_default(),
        TypeName::Group => f
            .fields
            .iter()
            .map(|x| x.name.clone())
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}

/// QoS directive (carries the descriptor's enum values as-is; mapping to zenoh objects is the glue's responsibility; design §4)
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct QosSpec {
    pub reliability: Reliability,
    pub congestion: Congestion,
    pub priority: Priority,
    pub express: bool,
}

/// The product of the send boundary. wire is the canonical JSON text (its UTF-8 becomes the Zenoh payload directly).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WireMsg {
    pub key: String,
    pub wire: String,
    /// per-connection hash (16 hex). Rides on the Zenoh attachment
    pub attachment: String,
    pub qos: QosSpec,
}

/// The result of the receive boundary (returned as tagged JSON over FFI)
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum AcceptOutcome {
    Accept { payload: String },
    Reject { diags: Vec<Diag> },
    HashMismatch { sender_hash: String },
}

#[derive(Clone, Copy, PartialEq)]
enum Role {
    From,
    To,
}

// Payload / Request / Response are used by prepare/accept.
// The SoT is the internal helper spec in the brief, so the enum form is preserved.
#[derive(Clone, Copy, PartialEq)]
enum SlotSel {
    Payload,
    Request,
    Response,
}

/// connection name → connection definition in the descriptor. If undefined, a NO via the `unknown_connection` diagnostic.
/// Both the engine side (prepare/accept) and the glue side (tap, etc.) go through here, structurally guaranteeing that
/// the "undefined connection" diagnostic (code / path / message) is byte-identical.
pub fn conn_of<'a>(
    desc: &'a Descriptor,
    conn: &str,
) -> Result<&'a DescriptorConnection, Vec<Diag>> {
    desc.connections.get(conn).ok_or_else(|| {
        vec![Diag::new(
            "unknown_connection",
            format!("connections.{conn}"),
            format!("undefined connection '{conn}'"),
        )]
    })
}

fn check_role(
    c: &DescriptorConnection,
    conn: &str,
    node: &str,
    pattern: Pattern,
    role: Role,
) -> Result<(), Vec<Diag>> {
    if c.pattern != pattern {
        return Err(vec![Diag::new(
            "pattern_mismatch",
            format!("connections.{conn}.pattern"),
            format!(
                "this operation is for {pattern:?} but the connection is {:?}",
                c.pattern
            ),
        )]);
    }
    let ok = match role {
        Role::From => c.from == node,
        Role::To => c.to.iter().any(|t| t == node),
    };
    if !ok {
        return Err(vec![Diag::new(
            "role_mismatch",
            format!("connections.{conn}"),
            format!("node '{node}' does not participate in this direction of this connection"),
        )]);
    }
    Ok(())
}

fn slot_of<'a>(
    c: &'a DescriptorConnection,
    conn: &str,
    sel: SlotSel,
) -> Result<&'a Slot, Vec<Diag>> {
    let (slot, name) = match sel {
        SlotSel::Payload => (&c.payload, "payload"),
        SlotSel::Request => (&c.request, "request"),
        SlotSel::Response => (&c.response, "response"),
    };
    slot.as_ref().ok_or_else(|| {
        vec![Diag::new(
            "descriptor_error",
            format!("connections.{conn}.{name}"),
            format!("descriptor has no {name} slot (the gen output is broken)"),
        )]
    })
}

/// Sampling rule (design §4): full=every time / sampled=deterministic 1/10 / off=none. Identical across the 3 languages.
fn should_validate(level: ValidateLevel, seq: u64) -> bool {
    match level {
        ValidateLevel::Full => true,
        ValidateLevel::Sampled => seq.is_multiple_of(10),
        ValidateLevel::Off => false,
    }
}

// So that Task 3's query boundary can reuse prepare/accept in the same shape,
// keep the structure that passes Pattern/Role/SlotSel together (do not turn the args into a struct).
#[allow(clippy::too_many_arguments)]
fn prepare(
    desc: &Descriptor,
    node: &str,
    conn: &str,
    pattern: Pattern,
    role: Role,
    sel: SlotSel,
    payload_json: &str,
    seq: u64,
) -> Result<WireMsg, Vec<Diag>> {
    let c = conn_of(desc, conn)?;
    check_role(c, conn, node, pattern, role)?;
    let value: serde_json::Value = serde_json::from_str(payload_json).map_err(|e| {
        vec![Diag::new(
            "decode_error",
            "$",
            format!("cannot be parsed as JSON: {e}"),
        )]
    })?;
    let slot = slot_of(c, conn, sel)?;
    if should_validate(c.validate, seq) {
        let diags = validate_payload(slot, &value);
        if !diags.is_empty() {
            return Err(diags);
        }
    }
    Ok(WireMsg {
        key: c.key.clone(),
        wire: serde_json::to_string(&value).expect("serializing a Value never fails"),
        attachment: c.hash.clone(),
        qos: QosSpec {
            reliability: c.reliability,
            congestion: c.congestion,
            priority: c.priority,
            express: c.express,
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn accept(
    desc: &Descriptor,
    node: &str,
    conn: &str,
    pattern: Pattern,
    role: Role,
    sel: SlotSel,
    wire: &[u8],
    attachment: Option<&str>,
    seq: u64,
    trusted: Option<&str>,
) -> AcceptOutcome {
    let c = match conn_of(desc, conn) {
        Ok(c) => c,
        Err(diags) => return AcceptOutcome::Reject { diags },
    };
    if let Err(diags) = check_role(c, conn, node, pattern, role) {
        return AcceptOutcome::Reject { diags };
    }
    match attachment {
        None => {
            return AcceptOutcome::Reject {
                diags: vec![Diag::new(
                    "missing_schema_hash",
                    "$",
                    "no schema_hash on the attachment (a non-sahou sender or an old client)",
                )],
            }
        }
        Some(h) if h != c.hash && Some(h) != trusted => {
            return AcceptOutcome::HashMismatch {
                sender_hash: h.to_string(),
            }
        }
        _ => {}
    }
    let value: serde_json::Value = match serde_json::from_slice(wire) {
        Ok(v) => v,
        Err(e) => {
            return AcceptOutcome::Reject {
                diags: vec![Diag::new(
                    "decode_error",
                    "$",
                    format!("cannot be parsed as JSON: {e}"),
                )],
            }
        }
    };
    let slot = match slot_of(c, conn, sel) {
        Ok(s) => s,
        Err(diags) => return AcceptOutcome::Reject { diags },
    };
    if should_validate(c.validate, seq) {
        let diags = validate_payload(slot, &value);
        if !diags.is_empty() {
            return AcceptOutcome::Reject { diags };
        }
    }
    AcceptOutcome::Accept {
        payload: serde_json::to_string(&value).expect("serializing a Value never fails"),
    }
}

/// pub_sub send boundary. On NG, do not put (the caller turns the Err into an exception).
pub fn prepare_publish(
    desc: &Descriptor,
    node: &str,
    conn: &str,
    payload_json: &str,
    seq: u64,
) -> Result<WireMsg, Vec<Diag>> {
    prepare(
        desc,
        node,
        conn,
        Pattern::PubSub,
        Role::From,
        SlotSel::Payload,
        payload_json,
        seq,
    )
}

/// pub_sub receive boundary. trusted = a sender_hash the engine's verdict cache has already accepted.
pub fn accept_sample(
    desc: &Descriptor,
    node: &str,
    conn: &str,
    wire: &[u8],
    attachment: Option<&str>,
    seq: u64,
    trusted: Option<&str>,
) -> AcceptOutcome {
    accept(
        desc,
        node,
        conn,
        Pattern::PubSub,
        Role::To,
        SlotSel::Payload,
        wire,
        attachment,
        seq,
        trusted,
    )
}

/// query ① request send boundary (from=requester). On NG, do not get.
pub fn prepare_request(
    desc: &Descriptor,
    node: &str,
    conn: &str,
    payload_json: &str,
    seq: u64,
) -> Result<WireMsg, Vec<Diag>> {
    prepare(
        desc,
        node,
        conn,
        Pattern::Query,
        Role::From,
        SlotSel::Request,
        payload_json,
        seq,
    )
}

/// query ② request receive boundary (to=responder).
pub fn accept_request(
    desc: &Descriptor,
    node: &str,
    conn: &str,
    wire: &[u8],
    attachment: Option<&str>,
    seq: u64,
    trusted: Option<&str>,
) -> AcceptOutcome {
    accept(
        desc,
        node,
        conn,
        Pattern::Query,
        Role::To,
        SlotSel::Request,
        wire,
        attachment,
        seq,
        trusted,
    )
}

/// query ③ response send boundary (responder). A broken response is not replied (the engine falls back to reply_err).
pub fn prepare_reply(
    desc: &Descriptor,
    node: &str,
    conn: &str,
    payload_json: &str,
    seq: u64,
) -> Result<WireMsg, Vec<Diag>> {
    prepare(
        desc,
        node,
        conn,
        Pattern::Query,
        Role::To,
        SlotSel::Response,
        payload_json,
        seq,
    )
}

/// query ④ response receive boundary (requester).
pub fn accept_reply(
    desc: &Descriptor,
    node: &str,
    conn: &str,
    wire: &[u8],
    attachment: Option<&str>,
    seq: u64,
    trusted: Option<&str>,
) -> AcceptOutcome {
    accept(
        desc,
        node,
        conn,
        Pattern::Query,
        Role::From,
        SlotSel::Response,
        wire,
        attachment,
        seq,
        trusted,
    )
}

/// The fragment of one's own connection returned by the contract queryable (`<ns>/@sahou/contract/<conn>/<hash>`).
pub fn contract_fragment(desc: &Descriptor, conn: &str) -> Result<String, Vec<Diag>> {
    let c = conn_of(desc, conn)?;
    Ok(serde_json::to_string(c).expect("serializing a DescriptorConnection never fails"))
}

fn to_connection(c: &DescriptorConnection) -> Connection {
    Connection {
        pattern: c.pattern,
        from: c.from.clone(),
        to: c.to.clone(),
        key: Some(c.key.clone()), // both are resolved keys. A difference = breaking (the destination differs)
        selector: c.selector.clone(),
        reliability: c.reliability,
        congestion: c.congestion,
        priority: c.priority,
        express: c.express,
        encoding: c.encoding,
        validate: c.validate,
        payload: c.payload.clone(),
        request: c.request.clone(),
        response: c.response.clone(),
    }
}

/// The 3-way handshake judgement (②b; spec §5 update).
/// accepted / blocked may be cached by the engine keyed on (conn, sender_hash).
/// unreachable = "cannot judge" = **not cached** (re-fetched on the next drift detection).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum HandshakeOutcome {
    Accepted,
    Blocked { diags: Vec<Diag> },
    Unreachable { diags: Vec<Diag> },
}

/// Delivery-time handshake (design §5; approach A): the receiver (mine) judges the sender's connection fragment.
/// Only pure additive is accepted. promotion / breaking is blocked (schema_incompatible).
/// If the fragment cannot be decoded, or the self-reported hash does not match sender_hash, it is unreachable (not cached).
pub fn handshake_verdict(
    conn_id: &str,
    mine: &DescriptorConnection,
    sender_hash: &str,
    theirs_json: &str,
) -> HandshakeOutcome {
    let de = &mut serde_json::Deserializer::from_str(theirs_json);
    let theirs: DescriptorConnection = match serde_path_to_error::deserialize(de) {
        Ok(t) => t,
        Err(e) => {
            return HandshakeOutcome::Unreachable {
                diags: vec![Diag::new(
                    "contract_unreachable",
                    e.path().to_string(),
                    format!(
                        "the contract fragment cannot be parsed (may recover on re-fetch): {}",
                        e.into_inner()
                    ),
                )],
            }
        }
    };
    if theirs.hash != sender_hash {
        return HandshakeOutcome::Unreachable {
            diags: vec![Diag::new(
                "contract_unreachable",
                format!("connections.{conn_id}.hash"),
                format!(
                    "the fetched fragment's hash '{}' does not match the requested '{sender_hash}' (suspected misdelivery/tampering; not used for judgement)",
                    theirs.hash
                ),
            )],
        };
    }
    let diags: Vec<Diag> = classify_pair(conn_id, &to_connection(mine), &to_connection(&theirs))
        .into_iter()
        .filter(|ch| ch.kind != ChangeKind::Additive)
        .map(|ch| {
            let detail = match ch.kind {
                ChangeKind::Promotion => {
                    format!("{} (conservatively NO at the delivery boundary because direction is undecided; approach A)", ch.detail)
                }
                _ => ch.detail,
            };
            Diag::new("schema_incompatible", ch.path, detail)
        })
        .collect();
    if diags.is_empty() {
        HandshakeOutcome::Accepted
    } else {
        HandshakeOutcome::Blocked { diags }
    }
}

/// The handshake entry exposed to FFI: looks up one's own fragment from the connection id and judges (a single implementation for PyO3/wasm).
/// unknown connection = "cannot judge in this descriptor generation" = unreachable (not cached; retryable).
/// Do not confuse this with the data-path unknown_connection (FATAL).
pub fn handshake_judge(
    desc: &Descriptor,
    conn: &str,
    sender_hash: &str,
    theirs_json: &str,
) -> HandshakeOutcome {
    match desc.connections.get(conn) {
        Some(mine) => handshake_verdict(conn, mine, sender_hash, theirs_json),
        None => HandshakeOutcome::Unreachable {
            diags: vec![Diag::new(
                "contract_unreachable",
                format!("connections.{conn}"),
                format!("undefined connection '{conn}' cannot be handshake-judged (possibly a descriptor-generation gap; not cached)"),
            )],
        },
    }
}

/// Parsing the reply_err envelope (`{"diags":[...]}`). A single implementation in the core = identical across the 3 languages.
/// If it cannot be parsed as an envelope, bad_reply_envelope (**retryable**).
/// Not decode_error (FATAL): the peer may be non-sahou/old, so leave room for the responder to recover on resend
/// (Fable Important-3). This is the wire layer, so unknown keys are dropped (do not confuse with the contract layer).
pub fn parse_reply_err(payload: &[u8]) -> Vec<Diag> {
    #[derive(serde::Deserialize)]
    struct Envelope {
        diags: Vec<Diag>,
    }
    match serde_json::from_slice::<Envelope>(payload) {
        Ok(env) if !env.diags.is_empty() => env.diags,
        _ => vec![Diag::new(
            "bad_reply_envelope",
            "$",
            "the reply_err payload is not a diagnostic envelope ({\"diags\":[...]}) (may recover on resend)",
        )],
    }
}

/// Classification for smart retry (design §4; Z20). Type/contract family = Fatal (pointless unless fixed) / everything else = Retryable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryClass {
    Retryable,
    Fatal,
}

pub fn classify_delivery(timed_out: bool, diags: &[Diag]) -> DeliveryClass {
    const FATAL: &[&str] = &[
        "schema_incompatible",
        "schema_version_mismatch",
        "type_mismatch",
        "required",
        "enum_mismatch",
        "out_of_range",
        "too_long",
        "bad_base64",
        "no_union_match",
        "decode_error",
        "unknown_connection",
        "unknown_node",
        "role_mismatch",
        "pattern_mismatch",
        "missing_schema_hash",
        "descriptor_error",
    ];
    let _ = timed_out; // zero responses (empty diags) fall to Retryable via the default below
    if diags.iter().any(|d| FATAL.contains(&d.code.as_str())) {
        DeliveryClass::Fatal
    } else {
        DeliveryClass::Retryable
    }
}
