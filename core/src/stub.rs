//! Type stub generation + drift check (design §8; main battleground ②). All pure functions (no IO; wasm-capable = D11).
//! The stub is the static layer only: the engine never reads it; the runtime behaves identically without it.
//! `check` is a CLI/CI responsibility (design §13) — this module provides only the comparison logic; scanning/IO lives on the cli side.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::contract::{NodeKind, Pattern, Slot, SlotKind, Typing};
use crate::diag::Diag;
use crate::ir::{Descriptor, DescriptorConnection};
use crate::runtime::{node_plan, NodePlan};
use crate::typespec::{Field, TypeName, TypeSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StubLang {
    Python,
    Ts,
}

/// Transport a whole-descriptor TS stub binds to. Selects the `connect` re-export source in the
/// runtime `.mjs` and the node-only fields in the typed opts. Ignored for Python (single transport).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TsTarget {
    Node,
    Browser,
}

/// One generated file. rel_path is relative to `gen/<node>/` (deciding the write destination is a CLI responsibility).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StubFile {
    pub rel_path: String,
    pub content: String,
}

/// Whether a name emitted as an "identifier" in the output satisfies the target language's identifier rules.
/// Conservatively allows only `^[A-Za-z_][A-Za-z0-9_]*$` (a subset safe in both Python and TS).
/// Every representable name (snake_case, etc.) always passes — the minimal regex that does not reduce expressiveness.
fn is_valid_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// Whether a value emitted as a "string literal" in the output does not break the target language's string-literal syntax.
/// Rust's `{:?}` (Debug) escapes control characters in `\u{XX}` form, which is incompatible with Python's
/// string-literal syntax (Python allows only `\uXXXX` (4 digits) / `\xXX`; `\u{}` is unsupported).
/// Conservatively rejects any value containing control characters (to return a NO at the right place = at gen time).
fn is_safe_literal(s: &str) -> bool {
    !s.chars().any(|c| c.is_control())
}

/// Structured NO for a string-literal value that cannot be represented in the stub (a Python/TS-common problem, hence language-independent).
fn diag_bad_literal(path: &str, what: &str, value: &str) -> Diag {
    Diag::new(
        "stub_unrepresentable_name",
        path.to_string(),
        format!(
            "{what} '{value}' contains a control character and cannot be emitted safely as a string literal (it would break the target language's string-literal syntax). Cannot generate the stub"
        ),
    )
}

fn pascal(s: &str) -> String {
    s.replace('-', "_")
        .split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// A view that sees the composite attributes of Field and TypeSpec in the same shape (same pattern as payload.rs).
struct View<'a> {
    ty: TypeName,
    items: Option<&'a TypeSpec>,
    values: &'a [String],
    any_of: &'a [TypeSpec],
    fields: &'a [Field],
}

fn view_field(f: &Field) -> View<'_> {
    View {
        ty: f.ty,
        items: f.items.as_ref(),
        values: &f.values,
        any_of: &f.any_of,
        fields: &f.fields,
    }
}

fn view_spec(s: &TypeSpec) -> View<'_> {
    match s {
        TypeSpec::Name(n) => View {
            ty: *n,
            items: None,
            values: &[],
            any_of: &[],
            fields: &[],
        },
        TypeSpec::Detailed(d) => View {
            ty: d.ty,
            items: d.items.as_ref(),
            values: &d.values,
            any_of: &d.any_of,
            fields: &d.fields,
        },
    }
}

/// Accumulator of type definitions. defs is a BTreeMap (name-ordered output = deterministic).
struct Gen {
    lang: StubLang,
    defs: BTreeMap<String, String>,
}

impl Gen {
    fn lang_name(&self) -> &'static str {
        match self.lang {
            StubLang::Python => "Python",
            StubLang::Ts => "TypeScript",
        }
    }

    /// Structured NO for an identifier that cannot be represented in the stub (returned at gen time, at the right place).
    fn diag_bad_ident(&self, path: &str, what: &str, name: &str) -> Diag {
        Diag::new(
            "stub_unrepresentable_name",
            path.to_string(),
            format!(
                "{what} '{name}' is not usable as a {} identifier (safe only: first char is a letter/`_`, the rest are alphanumeric/`_`). Cannot generate the stub",
                self.lang_name()
            ),
        )
    }

    fn scalar(&self, ty: TypeName) -> &'static str {
        match (self.lang, ty) {
            (StubLang::Python, TypeName::Int | TypeName::Timestamp) => "int",
            (StubLang::Python, TypeName::Float) => "float",
            (StubLang::Python, TypeName::Bool) => "bool",
            // bytes is a base64 string on the JSON wire (spec §3.1)
            (StubLang::Python, TypeName::String | TypeName::Bytes) => "str",
            (StubLang::Ts, TypeName::Int | TypeName::Float | TypeName::Timestamp) => "number",
            (StubLang::Ts, TypeName::Bool) => "boolean",
            (StubLang::Ts, TypeName::String | TypeName::Bytes) => "string",
            _ => unreachable!("composite types are handled by type_text"),
        }
    }

    /// View → type text. owner = basename of the deterministic name given to an anonymous composite type.
    /// path = structured path for diagnostics (same `connections.<id>.<slot>...` family as schema_check).
    fn type_text(&mut self, owner: &str, path: &str, v: View<'_>) -> Result<String, Vec<Diag>> {
        Ok(match v.ty {
            TypeName::Int
            | TypeName::Float
            | TypeName::Bool
            | TypeName::String
            | TypeName::Bytes
            | TypeName::Timestamp => self.scalar(v.ty).to_string(),
            TypeName::Enum => {
                for (i, val) in v.values.iter().enumerate() {
                    if !is_safe_literal(val) {
                        return Err(vec![diag_bad_literal(
                            &format!("{path}.values[{i}]"),
                            "enum value",
                            val,
                        )]);
                    }
                }
                let vals: Vec<String> = v.values.iter().map(|x| format!("{x:?}")).collect();
                match self.lang {
                    StubLang::Python => format!("Literal[{}]", vals.join(", ")),
                    StubLang::Ts => vals.join(" | "),
                }
            }
            TypeName::Array => {
                let inner =
                    self.items_text(&format!("{owner}Item"), &format!("{path}.items"), v.items)?;
                match self.lang {
                    StubLang::Python => format!("list[{inner}]"),
                    StubLang::Ts => format!("Array<{inner}>"),
                }
            }
            TypeName::Map => {
                let inner =
                    self.items_text(&format!("{owner}Value"), &format!("{path}.items"), v.items)?;
                match self.lang {
                    StubLang::Python => format!("dict[str, {inner}]"),
                    StubLang::Ts => format!("Record<string, {inner}>"),
                }
            }
            TypeName::Group => {
                self.record(owner, path, v.fields)?;
                owner.to_string()
            }
            TypeName::Union => {
                let mut parts = Vec::new();
                for (i, s) in v.any_of.iter().enumerate() {
                    parts.push(self.type_text(
                        &format!("{owner}V{i}"),
                        &format!("{path}.any_of[{i}]"),
                        view_spec(s),
                    )?);
                }
                parts.join(" | ")
            }
        })
    }

    fn items_text(
        &mut self,
        owner: &str,
        path: &str,
        items: Option<&TypeSpec>,
    ) -> Result<String, Vec<Diag>> {
        let spec = items.ok_or_else(|| {
            vec![Diag::new(
                "stub_error",
                path.to_string(),
                "items missing (cannot happen for a validated contract; the descriptor is broken)",
            )]
        })?;
        self.type_text(owner, path, view_spec(spec))
    }

    /// record/group → named type (TypedDict / interface). A name collision or unrepresentable name is a structured NO
    /// (never silently overwrite; never silently emit a broken stub).
    fn record(&mut self, name: &str, path: &str, fields: &[Field]) -> Result<(), Vec<Diag>> {
        if self.defs.contains_key(name) {
            return Err(vec![Diag::new(
                "stub_name_collision",
                name.to_string(),
                format!("generated type name '{name}' collides (review the combination of connection/field names)"),
            )]);
        }
        if !is_valid_ident(name) {
            return Err(vec![self.diag_bad_ident(path, "generated type name", name)]);
        }
        self.defs.insert(name.to_string(), String::new()); // reserve first (for collision detection)
        let mut lines = Vec::new();
        for (i, f) in fields.iter().enumerate() {
            if !is_valid_ident(&f.name) {
                return Err(vec![self.diag_bad_ident(
                    &format!("{path}.fields[{i}].name"),
                    "field name",
                    &f.name,
                )]);
            }
            let t = self.type_text(
                &format!("{name}{}", pascal(&f.name)),
                &format!("{path}.fields[{i}]"),
                view_field(f),
            )?;
            lines.push(match (self.lang, f.required) {
                (StubLang::Python, true) => format!("    {}: {t}", f.name),
                (StubLang::Python, false) => format!("    {}: NotRequired[{t}]", f.name),
                (StubLang::Ts, true) => format!("  {}: {t};", f.name),
                (StubLang::Ts, false) => format!("  {}?: {t};", f.name),
            });
        }
        let body = match self.lang {
            StubLang::Python => {
                let b = if lines.is_empty() {
                    "    pass".to_string()
                } else {
                    lines.join("\n")
                };
                format!("class {name}(TypedDict):\n{b}")
            }
            StubLang::Ts => format!("export interface {name} {{\n{}\n}}", lines.join("\n")),
        };
        self.defs.insert(name.to_string(), body);
        Ok(())
    }
}

/// slot → type name (registers the record into defs and returns the name). any / opaque stay dynamically typed (Any / unknown).
/// path = structured path for diagnostics (the caller passes `connections.<conn>.<slot>`).
fn slot_type(
    g: &mut Gen,
    conn: &str,
    suffix: &str,
    slot: &Slot,
    path: &str,
) -> Result<String, Vec<Diag>> {
    if slot.typing == Typing::Any || slot.kind == SlotKind::Opaque {
        return Ok(match g.lang {
            StubLang::Python => "Any".to_string(),
            StubLang::Ts => "unknown".to_string(),
        });
    }
    let name = format!("{}{suffix}", pascal(conn));
    g.record(&name, path, &slot.fields)?;
    Ok(name)
}

/// Type names resolved per connection (input to facade generation).
struct Sigs {
    publishes: Vec<(String, String)>, // (conn, payload type)
    subscribes: Vec<(String, String)>,
    queries: Vec<(String, String, String)>, // (conn, request type, response type)
    answers: Vec<(String, String, String)>,
}

fn missing_slot(conn: &str, name: &str) -> Vec<Diag> {
    vec![Diag::new(
        "descriptor_error",
        format!("connections.{conn}.{name}"),
        format!("descriptor has no {name} slot (the gen output is broken)"),
    )]
}

/// Marker at the top of each generated file (read by check; only the comment syntax differs by language).
/// `kind` is `node` for a per-node stub, `schema` for a whole-descriptor stub.
fn markers(comment: &str, kind: &str, name: &str, hashes: &BTreeMap<String, String>) -> String {
    let mut s = format!("{comment} sahou:stub {kind}={name}\n");
    for (c, h) in hashes {
        s.push_str(&format!("{comment} sahou:hash {c}={h}\n"));
    }
    s
}

/// Python: @overload if there are >= 2 variants, plain def if 1 (mypy errors on a lone @overload).
fn py_method(sigs: &[String]) -> String {
    if sigs.len() == 1 {
        sigs[0].clone()
    } else {
        sigs.iter()
            .map(|s| format!("    @overload\n{s}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The Protocol method lines for one node facade (.pyi). Shared by per-node and whole-descriptor output.
fn py_methods(s: &Sigs) -> Vec<String> {
    let mut methods: Vec<String> = Vec::new();
    let mut pub_sigs = Vec::new();
    for (conn, t) in &s.publishes {
        pub_sigs.push(format!(
            "    def publish(self, conn: Literal[{conn:?}], payload: {t}) -> None: ..."
        ));
    }
    if !pub_sigs.is_empty() {
        methods.push(py_method(&pub_sigs));
    }
    let mut sub_sigs = Vec::new();
    for (conn, t) in &s.subscribes {
        // both decorator forms (with / without handler) = 2 variants per connection
        sub_sigs.push(format!(
            "    def subscribe(self, conn: Literal[{conn:?}], handler: Callable[[{t}], object], *, on_reject: _OnReject | None = ...) -> Callable[[{t}], object]: ..."
        ));
        sub_sigs.push(format!(
            "    def subscribe(self, conn: Literal[{conn:?}], handler: None = ..., *, on_reject: _OnReject | None = ...) -> Callable[[Callable[[{t}], object]], Callable[[{t}], object]]: ..."
        ));
    }
    if !sub_sigs.is_empty() {
        methods.push(py_method(&sub_sigs));
    }
    let mut q_sigs = Vec::new();
    let mut qc_sigs = Vec::new();
    for (conn, req, resp) in &s.queries {
        q_sigs.push(format!(
            "    def query(self, conn: Literal[{conn:?}], payload: {req}, timeout: float = ...) -> dict[str, object]: ..."
        ));
        qc_sigs.push(format!(
            "    def query_confirmed(self, conn: Literal[{conn:?}], payload: {req}, *, timeout: float = ..., retries: int = ..., backoff: float = ...) -> {resp}: ..."
        ));
    }
    if !q_sigs.is_empty() {
        methods.push(py_method(&q_sigs));
        methods.push(py_method(&qc_sigs));
    }
    let mut a_sigs = Vec::new();
    for (conn, req, resp) in &s.answers {
        a_sigs.push(format!(
            "    def answer(self, conn: Literal[{conn:?}], fn: Callable[[{req}], {resp}]) -> Callable[[{req}], {resp}]: ..."
        ));
    }
    if !a_sigs.is_empty() {
        methods.push(py_method(&a_sigs));
    }
    // connection-independent common API (consistent with the real engine's signatures)
    methods.push("    def on_reject(self, cb: _OnReject) -> None: ...".to_string());
    methods.push("    def connection_info(self, conn: str) -> dict[str, str]: ...".to_string());
    methods.push("    def close(self) -> None: ...".to_string());
    methods
}

/// One `class <Name>(Protocol): ...` facade block (.pyi).
fn py_facade(name: &str, s: &Sigs) -> String {
    format!("class {name}(Protocol):\n{}", py_methods(s).join("\n"))
}

/// Python output: `.py` (runtime: identity + SCHEMA_HASHES only; engine-independent) and
/// `.pyi` (type-check time: Protocol + Literal overloads; consistent with the real engine API).
fn py_files(
    node: &str,
    hashes: &BTreeMap<String, String>,
    defs: &BTreeMap<String, String>,
    s: &Sigs,
) -> Vec<StubFile> {
    let facade = format!("{}Node", pascal(node));
    let hash_entries = hashes
        .iter()
        .map(|(c, h)| format!("    {c:?}: {h:?},"))
        .collect::<Vec<_>>()
        .join("\n");
    let defs_text = defs.values().cloned().collect::<Vec<_>>().join("\n\n");

    // --- .py (runtime: declarations + identity only; engine-independent) ---
    let py = format!(
        "{m}\"\"\"AUTO-GENERATED by `sahou gen --lang python --node {node}`. Do not edit by hand (it will be regenerated).\n\nStatic layer only: the engine does not read this file. The runtime behaves identically without the stub.\ndrift detection is `sahou check` (CLI/CI responsibility; design §8/§13).\n\nUsage (app code): pip install sahou, then use its connect():\n    node = typed_node(sahou.connect(\"gen/descriptor.json\", node=\"{node}\"))\n    node.publish(...) / node.subscribe(...) / node.query_confirmed(...) / node.answer(...)\n\"\"\"\nfrom __future__ import annotations\n\nfrom typing import Any, Literal, NotRequired, TypedDict\n\nSCHEMA_HASHES: dict[str, str] = {{\n{hash_entries}\n}}\n\n\n{defs_text}\n\n\ndef typed_node(node):\n    \"\"\"Identity (zero runtime cost). The .pyi declares it returns {facade}, so IDE completion works.\"\"\"\n    return node\n",
        m = markers("#", "node", node, hashes),
    );

    // --- .pyi (type-check time: Protocol + Literal overloads) ---
    let facade_block = py_facade(&facade, s);
    let pyi = format!(
        "{m}# AUTO-GENERATED by `sahou gen --lang python --node {node}`. Do not edit by hand (it will be regenerated).\nfrom typing import Any, Callable, Final, Literal, NotRequired, Protocol, TypedDict, overload\n\nSCHEMA_HASHES: Final[dict[str, str]]\n\n_OnReject = Callable[[str, list[dict[str, str]]], object]\n\n{defs_text}\n\n{facade_block}\n\ndef typed_node(node: object) -> {facade}: ...\n",
        m = markers("#", "node", node, hashes),
    );

    vec![
        StubFile {
            rel_path: "sahou_stub.py".into(),
            content: py,
        },
        StubFile {
            rel_path: "sahou_stub.pyi".into(),
            content: pyi,
        },
    ]
}

/// The interface method lines for one node facade (.d.mts). Shared by per-node and whole-descriptor output.
fn ts_methods(s: &Sigs) -> Vec<String> {
    let mut methods: Vec<String> = Vec::new();
    for (conn, t) in &s.publishes {
        methods.push(format!(
            "  publish(conn: {conn:?}, payload: {t}): Promise<void>;"
        ));
    }
    for (conn, t) in &s.subscribes {
        methods.push(format!(
            "  subscribe(conn: {conn:?}, handler: (payload: {t}) => void | Promise<void>, opts?: {{ onReject?: OnReject }}): Promise<void>;"
        ));
    }
    for (conn, req, resp) in &s.queries {
        methods.push(format!(
            "  query(conn: {conn:?}, payload: {req}, opts?: {{ timeoutMs?: number }}): Promise<{{ delivered: boolean; response: {resp} | null; diags: SahouDiag[]; timedOut: boolean }}>;"
        ));
        methods.push(format!(
            "  queryConfirmed(conn: {conn:?}, payload: {req}, opts?: {{ timeoutMs?: number; retries?: number; backoffMs?: number }}): Promise<{resp}>;"
        ));
    }
    for (conn, req, resp) in &s.answers {
        methods.push(format!(
            "  answer(conn: {conn:?}, fn: (req: {req}) => {resp} | Promise<{resp}>): Promise<void>;"
        ));
    }
    methods.push("  onReject(cb: OnReject): void;".to_string());
    methods.push("  connectionInfo(conn: string): { key: string; hash: string };".to_string());
    methods.push("  close(): Promise<void>;".to_string());
    methods
}

/// One `export interface <Name> { ... }` facade block (.d.mts).
fn ts_facade(name: &str, s: &Sigs) -> String {
    format!(
        "export interface {name} {{\n{}\n}}",
        ts_methods(s).join("\n")
    )
}

/// TS output: `.mjs` (runtime: identity + SCHEMA_HASHES only; engine-independent) and
/// `.d.mts` (type-check time: interface overloads; consistent with the real engine API. `.d.mts` is correct under tsc's nodenext resolution).
fn ts_files(
    node: &str,
    hashes: &BTreeMap<String, String>,
    defs: &BTreeMap<String, String>,
    s: &Sigs,
) -> Vec<StubFile> {
    let facade = format!("{}Node", pascal(node));
    let hash_entries = hashes
        .iter()
        .map(|(c, h)| format!("  {c:?}: {h:?},"))
        .collect::<Vec<_>>()
        .join("\n");
    let defs_text = defs.values().cloned().collect::<Vec<_>>().join("\n\n");

    let mjs = format!(
        "{m}// AUTO-GENERATED by `sahou gen --lang ts --node {node}`. Do not edit by hand (it will be regenerated).\n// Static layer only: the engine does not read this file. The runtime behaves identically without the stub.\n//\n// Usage (app code): npm i sahou, then use its connect():\n//   const node = typedNode(await connect(\"gen/descriptor.json\", {{ node: \"{node}\" }}));\n//   await node.publish(...) / node.subscribe(...) / node.queryConfirmed(...) / node.answer(...)\nexport const SCHEMA_HASHES = Object.freeze({{\n{hash_entries}\n}});\n\n/** Identity (zero runtime cost). The .d.mts declares it returns {facade}, so IDE completion works. */\nexport const typedNode = (node) => node;\n",
        m = markers("//", "node", node, hashes),
    );

    let facade_block = ts_facade(&facade, s);

    let dts = format!(
        "{m}// AUTO-GENERATED by `sahou gen --lang ts --node {node}`. Do not edit by hand (it will be regenerated).\nexport interface SahouDiag {{\n  code: string;\n  path: string;\n  message: string;\n}}\n\nexport type OnReject = (conn: string, diags: SahouDiag[]) => void | Promise<void>;\n\n{defs_text}\n\n{facade_block}\n\nexport declare const SCHEMA_HASHES: Readonly<Record<string, string>>;\n\nexport declare function typedNode(node: unknown): {facade};\n",
        m = markers("//", "node", node, hashes),
    );

    vec![
        StubFile {
            rel_path: "sahou_stub.mjs".to_string(),
            content: mjs,
        },
        StubFile {
            rel_path: "sahou_stub.d.mts".to_string(),
            content: dts,
        },
    ]
}

/// Collect hash markers from the stub text (co-located with the emitter `markers` = structurally prevents format drift).
/// You may pass the concatenated text of multiple files (identical duplicates are OK; contradictions are a NO).
/// Broken markers are not silently skipped (so a partially edited stub is not mistaken for a "match").
pub fn parse_stub_hashes(text: &str) -> Result<BTreeMap<String, String>, Vec<Diag>> {
    let mut out = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        let Some(rest) = line.split("sahou:hash ").nth(1) else {
            continue;
        };
        let mut it = rest.trim().splitn(2, '=');
        match (it.next(), it.next()) {
            (Some(c), Some(h))
                if !c.is_empty() && h.len() == 16 && h.chars().all(|ch| ch.is_ascii_hexdigit()) =>
            {
                if let Some(prev) = out.insert(c.to_string(), h.to_string()) {
                    if prev != h {
                        return Err(vec![Diag::new(
                            "stub_marker_conflict",
                            format!("line {}", i + 1),
                            format!(
                                "hash markers for connection '{c}' contradict ({prev} / {h}). Regenerate the whole stub set"
                            ),
                        )]);
                    }
                }
            }
            _ => {
                return Err(vec![Diag::new(
                    "stub_marker_invalid",
                    format!("line {}", i + 1),
                    "malformed sahou:hash marker (correct: `sahou:hash <conn>=<16hex>`). Regenerate the stub"
                        .to_string(),
                )])
            }
        }
    }
    Ok(out)
}

/// Get the node marker from the stub text (the first one).
pub fn parse_stub_node(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        l.split("sahou:stub node=")
            .nth(1)
            .map(|s| s.trim().to_string())
    })
}

/// Get the schema marker from a whole-descriptor stub text (the first one).
pub fn parse_stub_schema(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        l.split("sahou:stub schema=")
            .nth(1)
            .map(|s| s.trim().to_string())
    })
}

/// Drift check of the stub-embedded hashes × descriptor (design §8; the guts of check). An empty Vec = no drift.
/// All classified as NO (the stub generates every participating connection, so any mismatch means "regeneration needed").
pub fn check_drift(
    desc: &Descriptor,
    node: &str,
    stub_hashes: &BTreeMap<String, String>,
) -> Vec<Diag> {
    let plan = match node_plan(desc, node) {
        Ok(p) => p,
        Err(diags) => return diags, // unknown node: pass through the core's unknown_node
    };
    let participating: BTreeSet<&String> = plan
        .publishes
        .iter()
        .chain(plan.subscribes.iter())
        .chain(plan.queries.iter())
        .chain(plan.answers.iter())
        .collect();
    let mut diags = Vec::new();
    for (conn, stub_hash) in stub_hashes {
        match desc.connections.get(conn) {
            None => diags.push(Diag::new(
                "stub_stale_connection",
                format!("connections.{conn}"),
                format!(
                    "connection '{conn}' present in the stub is absent from the descriptor (removed from the contract). Regenerate with `sahou gen --lang ... --node {node}`"
                ),
            )),
            Some(c) if &c.hash != stub_hash => diags.push(Diag::new(
                "stub_hash_drift",
                format!("connections.{conn}.hash"),
                format!(
                    "stub hash '{stub_hash}' and descriptor hash '{}' do not match (the contract changed). Regenerate with `sahou gen --lang ... --node {node}`",
                    c.hash
                ),
            )),
            _ => {}
        }
    }
    for conn in participating {
        if !stub_hashes.contains_key(conn.as_str()) {
            diags.push(Diag::new(
                "stub_missing_connection",
                format!("connections.{conn}"),
                format!(
                    "connection '{conn}' in which node '{node}' participates is absent from the stub (added to the contract later). Regenerate with `sahou gen --lang ... --node {node}`"
                ),
            ));
        }
    }
    diags
}

/// Whether `node` is a `sahou`-kind node in the descriptor (a Sahou runtime cannot be an `external` node).
fn is_sahou(desc: &Descriptor, node: &str) -> bool {
    desc.nodes
        .get(node)
        .is_some_and(|n| n.kind == NodeKind::Sahou)
}

/// Connections a whole-descriptor stub covers: those touched by at least one `sahou`-kind node
/// (its `from` is sahou, or any of its `to` is sahou). BTreeSet = deterministic, sorted.
fn participating_conns(desc: &Descriptor) -> BTreeSet<String> {
    desc.connections
        .iter()
        .filter(|(_, c)| is_sahou(desc, &c.from) || c.to.iter().any(|t| is_sahou(desc, t)))
        .map(|(id, _)| id.clone())
        .collect()
}

/// Whole-descriptor drift check (design §8): compare every participating connection's hash against
/// the descriptor. Same diagnostic codes as `check_drift`, scoped to the whole descriptor. Empty = no drift.
pub fn check_drift_all(desc: &Descriptor, stub_hashes: &BTreeMap<String, String>) -> Vec<Diag> {
    let participating = participating_conns(desc);
    let mut diags = Vec::new();
    for (conn, stub_hash) in stub_hashes {
        match desc.connections.get(conn) {
            None => diags.push(Diag::new(
                "stub_stale_connection",
                format!("connections.{conn}"),
                format!(
                    "connection '{conn}' present in the stub is absent from the descriptor (removed from the contract). Regenerate with `sahou gen --lang ...`"
                ),
            )),
            Some(c) if &c.hash != stub_hash => diags.push(Diag::new(
                "stub_hash_drift",
                format!("connections.{conn}.hash"),
                format!(
                    "stub hash '{stub_hash}' and descriptor hash '{}' do not match (the contract changed). Regenerate with `sahou gen --lang ...`",
                    c.hash
                ),
            )),
            _ => {}
        }
    }
    for conn in &participating {
        if !stub_hashes.contains_key(conn.as_str()) {
            diags.push(Diag::new(
                "stub_missing_connection",
                format!("connections.{conn}"),
                format!(
                    "connection '{conn}' (a sahou node participates in it) is absent from the stub (added to the contract later). Regenerate with `sahou gen --lang ...`"
                ),
            ));
        }
    }
    diags
}

pub fn gen_stub(desc: &Descriptor, node: &str, lang: StubLang) -> Result<Vec<StubFile>, Vec<Diag>> {
    let plan = node_plan(desc, node)?;
    // per-connection hashes of the participating connections (embedded in the marker / SCHEMA_HASHES; BTree = deterministic order).
    // The same set is reused for the unrepresentable-name check.
    let participating: BTreeSet<&String> = plan
        .publishes
        .iter()
        .chain(plan.subscribes.iter())
        .chain(plan.queries.iter())
        .chain(plan.answers.iter())
        .collect();
    let mut g = Gen {
        lang,
        defs: BTreeMap::new(),
    };
    // Connection names are emitted as string literals for Literal[...] / SCHEMA_HASHES keys (not identifiers).
    // A control character would break the target language's literal syntax, so stop with a structured NO at gen time.
    for conn in &participating {
        if !is_safe_literal(conn) {
            return Err(vec![diag_bad_literal(
                &format!("connections.{conn}"),
                "connection name",
                conn,
            )]);
        }
    }
    // The facade class name (Pascal(node)Node) is emitted as an identifier.
    let facade = format!("{}Node", pascal(node));
    if !is_valid_ident(&facade) {
        return Err(vec![g.diag_bad_ident(
            &format!("nodes.{node}"),
            "facade class name",
            &facade,
        )]);
    }
    let mut sigs = Sigs {
        publishes: vec![],
        subscribes: vec![],
        queries: vec![],
        answers: vec![],
    };
    for conn in &plan.publishes {
        let c = &desc.connections[conn];
        let slot = c
            .payload
            .as_ref()
            .ok_or_else(|| missing_slot(conn, "payload"))?;
        let path = format!("connections.{conn}.payload");
        sigs.publishes
            .push((conn.clone(), slot_type(&mut g, conn, "", slot, &path)?));
    }
    for conn in &plan.subscribes {
        let c = &desc.connections[conn];
        let slot = c
            .payload
            .as_ref()
            .ok_or_else(|| missing_slot(conn, "payload"))?;
        let path = format!("connections.{conn}.payload");
        sigs.subscribes
            .push((conn.clone(), slot_type(&mut g, conn, "", slot, &path)?));
    }
    for conn in &plan.queries {
        let c = &desc.connections[conn];
        let req = c
            .request
            .as_ref()
            .ok_or_else(|| missing_slot(conn, "request"))?;
        let resp = c
            .response
            .as_ref()
            .ok_or_else(|| missing_slot(conn, "response"))?;
        sigs.queries.push((
            conn.clone(),
            slot_type(
                &mut g,
                conn,
                "Request",
                req,
                &format!("connections.{conn}.request"),
            )?,
            slot_type(
                &mut g,
                conn,
                "Response",
                resp,
                &format!("connections.{conn}.response"),
            )?,
        ));
    }
    for conn in &plan.answers {
        let c = &desc.connections[conn];
        let req = c
            .request
            .as_ref()
            .ok_or_else(|| missing_slot(conn, "request"))?;
        let resp = c
            .response
            .as_ref()
            .ok_or_else(|| missing_slot(conn, "response"))?;
        sigs.answers.push((
            conn.clone(),
            slot_type(
                &mut g,
                conn,
                "Request",
                req,
                &format!("connections.{conn}.request"),
            )?,
            slot_type(
                &mut g,
                conn,
                "Response",
                resp,
                &format!("connections.{conn}.response"),
            )?,
        ));
    }
    let hashes: BTreeMap<String, String> = participating
        .into_iter()
        .map(|c| (c.clone(), desc.connections[c].hash.clone()))
        .collect();
    Ok(match lang {
        StubLang::Python => py_files(node, &hashes, &g.defs, &sigs),
        StubLang::Ts => ts_files(node, &hashes, &g.defs, &sigs),
    })
}

// ---- Whole-descriptor generation (gen_stub_all) -----------------------------
// One typed module covering the whole descriptor: a `connect` overload per sahou node (node-name
// completion) returning that node's facade. Reuses the per-connection type resolution and facade
// builders; shared payload types are resolved once and deduplicated.

/// Resolved type names for one connection (pub_sub → payload, query → request + response).
struct ConnTypes {
    payload: Option<String>,
    request: Option<String>,
    response: Option<String>,
}

/// Resolve a connection's slot types into the shared `Gen` exactly once. pub_sub uses the `""`
/// suffix (type name = Pascal(conn)); query uses `Request` / `Response` (identical to per-node gen).
fn resolve_conn_types(
    g: &mut Gen,
    conn: &str,
    c: &DescriptorConnection,
) -> Result<ConnTypes, Vec<Diag>> {
    match c.pattern {
        Pattern::PubSub => {
            let slot = c
                .payload
                .as_ref()
                .ok_or_else(|| missing_slot(conn, "payload"))?;
            let t = slot_type(g, conn, "", slot, &format!("connections.{conn}.payload"))?;
            Ok(ConnTypes {
                payload: Some(t),
                request: None,
                response: None,
            })
        }
        Pattern::Query => {
            let req = c
                .request
                .as_ref()
                .ok_or_else(|| missing_slot(conn, "request"))?;
            let resp = c
                .response
                .as_ref()
                .ok_or_else(|| missing_slot(conn, "response"))?;
            let rt = slot_type(
                g,
                conn,
                "Request",
                req,
                &format!("connections.{conn}.request"),
            )?;
            let rp = slot_type(
                g,
                conn,
                "Response",
                resp,
                &format!("connections.{conn}.response"),
            )?;
            Ok(ConnTypes {
                payload: None,
                request: Some(rt),
                response: Some(rp),
            })
        }
    }
}

/// Build one node's Sigs from already-resolved connection types (no re-resolution → no double record()).
fn node_sigs(plan: &NodePlan, types: &BTreeMap<String, ConnTypes>) -> Sigs {
    let mut sigs = Sigs {
        publishes: vec![],
        subscribes: vec![],
        queries: vec![],
        answers: vec![],
    };
    for conn in &plan.publishes {
        if let Some(t) = types.get(conn).and_then(|c| c.payload.clone()) {
            sigs.publishes.push((conn.clone(), t));
        }
    }
    for conn in &plan.subscribes {
        if let Some(t) = types.get(conn).and_then(|c| c.payload.clone()) {
            sigs.subscribes.push((conn.clone(), t));
        }
    }
    for conn in &plan.queries {
        if let Some(ct) = types.get(conn) {
            if let (Some(req), Some(resp)) = (ct.request.clone(), ct.response.clone()) {
                sigs.queries.push((conn.clone(), req, resp));
            }
        }
    }
    for conn in &plan.answers {
        if let Some(ct) = types.get(conn) {
            if let (Some(req), Some(resp)) = (ct.request.clone(), ct.response.clone()) {
                sigs.answers.push((conn.clone(), req, resp));
            }
        }
    }
    sigs
}

/// TS whole-descriptor output: `sahou.gen.mjs` (re-exports the real connect + SCHEMA_HASHES) and
/// `sahou.gen.d.mts` (all payload types + one facade per node + one typed `connect` overload per node).
/// Self-contained (no import from "sahou") so tsc needs no package resolution to consume the types.
fn ts_files_all(
    schema: &str,
    hashes: &BTreeMap<String, String>,
    defs: &BTreeMap<String, String>,
    nodes: &[(String, String, Sigs)],
    target: TsTarget,
) -> Vec<StubFile> {
    let import_src = match target {
        TsTarget::Node => "sahou",
        TsTarget::Browser => "sahou/browser",
    };
    let hash_entries = hashes
        .iter()
        .map(|(c, h)| format!("  {c:?}: {h:?},"))
        .collect::<Vec<_>>()
        .join("\n");
    let defs_text = defs.values().cloned().collect::<Vec<_>>().join("\n\n");
    let facades = nodes
        .iter()
        .map(|(_, facade, sigs)| ts_facade(facade, sigs))
        .collect::<Vec<_>>()
        .join("\n\n");
    let overloads = nodes
        .iter()
        .map(|(node, facade, _)| {
            // opts mirror the target's ConnectOptions (browser has no port / spawnLink)
            let opts = match target {
                TsTarget::Node => format!(
                    "{{ node: {node:?}; locator?: string; port?: number; spawnLink?: boolean }}"
                ),
                TsTarget::Browser => format!("{{ node: {node:?}; locator?: string }}"),
            };
            format!(
                "export declare function connect(descriptor: string | object, opts: {opts}): Promise<{facade}>;"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mjs = format!(
        "{m}// AUTO-GENERATED by `sahou gen --lang ts`. Do not edit by hand (it will be regenerated).\n// Static layer only: the engine does not read this file. The runtime behaves identically without it.\n//\n// Usage (app code): npm i sahou, then import the typed connect from here:\n//   import {{ connect }} from \"./sahou.gen.mjs\";\n//   const node = await connect(\"gen/descriptor.json\", {{ node: \"<node>\" }});\nexport {{ connect }} from \"{import_src}\";\nexport const SCHEMA_HASHES = Object.freeze({{\n{hash_entries}\n}});\n",
        m = markers("//", "schema", schema, hashes),
    );
    let dts = format!(
        "{m}// AUTO-GENERATED by `sahou gen --lang ts`. Do not edit by hand (it will be regenerated).\nexport interface SahouDiag {{\n  code: string;\n  path: string;\n  message: string;\n}}\n\nexport type OnReject = (conn: string, diags: SahouDiag[]) => void | Promise<void>;\n\n{defs_text}\n\n{facades}\n\n{overloads}\n\nexport declare const SCHEMA_HASHES: Readonly<Record<string, string>>;\n",
        m = markers("//", "schema", schema, hashes),
    );
    vec![
        StubFile {
            rel_path: "sahou.gen.mjs".to_string(),
            content: mjs,
        },
        StubFile {
            rel_path: "sahou.gen.d.mts".to_string(),
            content: dts,
        },
    ]
}

/// Python whole-descriptor output: `sahou_gen.py` (re-exports the real connect + SCHEMA_HASHES) and
/// `sahou_gen.pyi` (all payload types + one Protocol facade per node + one `connect` overload per node).
fn py_files_all(
    schema: &str,
    hashes: &BTreeMap<String, String>,
    defs: &BTreeMap<String, String>,
    nodes: &[(String, String, Sigs)],
) -> Vec<StubFile> {
    let hash_entries = hashes
        .iter()
        .map(|(c, h)| format!("    {c:?}: {h:?},"))
        .collect::<Vec<_>>()
        .join("\n");
    let defs_text = defs.values().cloned().collect::<Vec<_>>().join("\n\n");
    let facades = nodes
        .iter()
        .map(|(_, facade, sigs)| py_facade(facade, sigs))
        .collect::<Vec<_>>()
        .join("\n\n\n");
    let overload_sigs: Vec<String> = nodes
        .iter()
        .map(|(node, facade, _)| {
            format!(
                "def connect(descriptor, node: Literal[{node:?}], *, connect: list[str] | None = ..., listen: list[str] | None = ..., multicast: bool = ...) -> {facade}: ..."
            )
        })
        .collect();
    // mypy errors on a lone @overload; a single node gets a plain def (matches py_method).
    let overloads = py_method(&overload_sigs);

    let py = format!(
        "{m}\"\"\"AUTO-GENERATED by `sahou gen --lang python`. Do not edit by hand (it will be regenerated).\n\nStatic layer: the .pyi drives type checking; this module only re-exports the real connect.\ndrift detection is `sahou check` (CLI/CI responsibility; design §8/§13).\n\nUsage (app code): pip install sahou, then import the typed connect from here:\n    from sahou_gen import connect\n    node = connect(\"gen/descriptor.json\", \"<node>\")\n\"\"\"\nfrom __future__ import annotations\n\nfrom sahou import connect\n\nSCHEMA_HASHES: dict[str, str] = {{\n{hash_entries}\n}}\n",
        m = markers("#", "schema", schema, hashes),
    );
    let pyi = format!(
        "{m}# AUTO-GENERATED by `sahou gen --lang python`. Do not edit by hand (it will be regenerated).\nfrom typing import Any, Callable, Final, Literal, NotRequired, Protocol, TypedDict, overload\n\nSCHEMA_HASHES: Final[dict[str, str]]\n\n_OnReject = Callable[[str, list[dict[str, str]]], object]\n\n{defs_text}\n\n{facades}\n\n{overloads}\n",
        m = markers("#", "schema", schema, hashes),
    );
    vec![
        StubFile {
            rel_path: "sahou_gen.py".to_string(),
            content: py,
        },
        StubFile {
            rel_path: "sahou_gen.pyi".to_string(),
            content: pyi,
        },
    ]
}

/// Whole-descriptor stub generation (design §8): one typed module per descriptor. `target` selects the
/// TS transport (ignored for Python). Errors are structured NOs (unrepresentable name / collision).
pub fn gen_stub_all(
    desc: &Descriptor,
    lang: StubLang,
    target: TsTarget,
) -> Result<Vec<StubFile>, Vec<Diag>> {
    let participating = participating_conns(desc);
    let mut g = Gen {
        lang,
        defs: BTreeMap::new(),
    };
    // connection names are emitted as string literals → reject control chars at gen time (right place).
    for conn in &participating {
        if !is_safe_literal(conn) {
            return Err(vec![diag_bad_literal(
                &format!("connections.{conn}"),
                "connection name",
                conn,
            )]);
        }
    }
    // resolve each participating connection's types once (shared types are deduped in g.defs).
    let mut types: BTreeMap<String, ConnTypes> = BTreeMap::new();
    for conn in &participating {
        let c = &desc.connections[conn];
        types.insert(conn.clone(), resolve_conn_types(&mut g, conn, c)?);
    }
    // one facade per sahou node (BTreeMap iteration = sorted → deterministic output).
    let mut nodes: Vec<(String, String, Sigs)> = Vec::new();
    for (node, n) in &desc.nodes {
        if n.kind != NodeKind::Sahou {
            continue; // external nodes cannot host a Sahou runtime
        }
        let facade = format!("{}Node", pascal(node));
        if !is_valid_ident(&facade) {
            return Err(vec![g.diag_bad_ident(
                &format!("nodes.{node}"),
                "facade class name",
                &facade,
            )]);
        }
        let plan = node_plan(desc, node)?;
        let sigs = node_sigs(&plan, &types);
        nodes.push((node.clone(), facade, sigs));
    }
    let hashes: BTreeMap<String, String> = participating
        .iter()
        .map(|c| (c.clone(), desc.connections[c].hash.clone()))
        .collect();
    Ok(match lang {
        StubLang::Python => py_files_all(&desc.schema, &hashes, &g.defs, &nodes),
        StubLang::Ts => ts_files_all(&desc.schema, &hashes, &g.defs, &nodes, target),
    })
}
