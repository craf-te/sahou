//! sahou doctor --lan — the mesh roll call (spec: notes/sahou-vitals-spec.md §4).
//! Same split as doctor.rs: pure functions (join / render / classify), unit-tested without a
//! network, plus thin zenoh sweeps. Duplicate instances are detected by counting vitals-queryable
//! replies per node key — NOT liveliness tokens (same-key tokens aggregate to one reply;
//! verified empirically on zenoh 1.9).

use sahou_core::diag::Diag;
use sahou_core::ir::Descriptor;
use sahou_core::vitals::parse_vitals;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::style::{self, Status};
use zenoh::Wait;

/// What the LAN sweeps collected. `vitals` keeps one entry PER reply — duplicates are the signal
/// for double-started instances (tokens cannot carry that signal; see the module doc).
pub struct LanSweep {
    pub token_keys: Vec<String>,
    /// (reply keyexpr, payload json)
    pub vitals: Vec<(String, String)>,
}

/// The node name from a vitals key `<ns>/@sahou/vitals/<node>` (namespace may be multi-chunk).
pub fn vitals_node(key: &str) -> Option<&str> {
    let (_, tail) = key.split_once("/@sahou/vitals/")?;
    (!tail.is_empty() && !tail.contains('/')).then_some(tail)
}

#[derive(Debug)]
pub enum Presence {
    /// liveliness token seen (the normal path)
    Token,
    /// no token, but the vitals queryable answered (fallback B; labeled in the rendering)
    VitalsReply,
}

#[derive(Debug)]
pub enum Generation {
    Match,
    /// which connections' hashes differ from the descriptor this doctor loaded
    Drift(String),
    Unknown,
}

/// Coloring class for per-node roll-call annotations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    /// a blocked handshake — rendered red
    Blocking,
    /// duplicate instances / unreadable vitals — rendered yellow
    Warn,
}

#[derive(Debug)]
pub struct NodeRow {
    pub node: String,
    pub present: Option<Presence>,
    pub sahou_version: Option<String>,
    pub generation: Generation,
    pub notes: Vec<(NoteKind, String)>,
}

/// Join "who should be here" (the descriptor's nodes) against "who is here" (the sweep).
/// Pure: all judgement happens here, all I/O stays in the sweeps.
pub fn roll_call(desc: &Descriptor, sweep: &LanSweep) -> Vec<NodeRow> {
    desc.nodes
        .keys()
        .map(|node| {
            let key_tail = format!("/@sahou/vitals/{node}");
            let has_token = sweep.token_keys.iter().any(|k| k.ends_with(&key_tail));
            let replies: Vec<&(String, String)> = sweep
                .vitals
                .iter()
                .filter(|(k, _)| vitals_node(k) == Some(node.as_str()))
                .collect();
            let present = if has_token {
                Some(Presence::Token)
            } else if !replies.is_empty() {
                Some(Presence::VitalsReply)
            } else {
                None
            };
            let mut notes = Vec::new();
            if replies.len() > 1 {
                notes.push((
                    NoteKind::Warn,
                    format!(
                        "{} instances answered vitals (double-started node?)",
                        replies.len()
                    ),
                ));
            }
            let mut sahou_version = None;
            let mut generation = Generation::Unknown;
            if let Some((_, payload)) = replies.first() {
                match parse_vitals(payload) {
                    Ok(v) => {
                        sahou_version = Some(v.runtime.sahou.clone());
                        let drift: Vec<String> = v
                            .connections
                            .iter()
                            .filter_map(|(conn, vc)| {
                                desc.connections
                                    .get(conn)
                                    .filter(|dc| dc.hash != vc.hash)
                                    .map(|_| conn.clone())
                            })
                            .collect();
                        generation = if drift.is_empty() {
                            Generation::Match
                        } else {
                            Generation::Drift(drift.join(", "))
                        };
                        for (conn, senders) in &v.handshake {
                            for (sender_hash, verdict) in senders {
                                if verdict == "blocked" {
                                    notes.push((
                                        NoteKind::Blocking,
                                        format!("blocking sender {sender_hash} on {conn}"),
                                    ));
                                }
                            }
                        }
                    }
                    Err(diags) => {
                        for d in diags {
                            notes.push((NoteKind::Warn, format!("[{}] {}", d.code, d.message)));
                        }
                    }
                }
            }
            NodeRow {
                node: node.clone(),
                present,
                sahou_version,
                generation,
                notes,
            }
        })
        .collect()
}

/// Render the roll call: `✓/✗ node  sahou <ver>  generation …` with `↳` note
/// lines. Node column width = the longest node name (min 8).
pub fn render_roll_call(rows: &[NodeRow]) -> String {
    let w = rows.iter().map(|r| r.node.len()).max().unwrap_or(0).max(8);
    let mut out = String::new();
    for r in rows {
        match &r.present {
            Some(src) => {
                let gen = match &r.generation {
                    Generation::Match => style::paint(style::META, "generation match"),
                    Generation::Drift(conns) => {
                        style::paint(style::WARN, format!("generation drift: {conns}"))
                    }
                    Generation::Unknown => style::paint(style::WARN.dimmed(), "generation unknown"),
                };
                out.push_str(&format!(
                    "  {} {:<w$}  sahou {:<8} {gen}\n",
                    Status::Ok.glyph(),
                    r.node,
                    r.sahou_version.as_deref().unwrap_or("?"),
                ));
                if matches!(src, Presence::VitalsReply) {
                    out.push_str(&format!(
                        "      {}\n",
                        style::paint(
                            style::META,
                            "↳ presence via vitals reply (no liveliness token)"
                        )
                    ));
                }
            }
            None => out.push_str(&format!(
                "  {} {:<w$}  no vitals — not started / unreachable from here\n",
                Status::Bad.glyph(),
                r.node,
            )),
        }
        for (kind, n) in &r.notes {
            let s = match kind {
                NoteKind::Blocking => style::BAD,
                NoteKind::Warn => style::WARN,
            };
            out.push_str(&format!("      {}\n", style::paint(s, format!("↳ {n}"))));
        }
    }
    out
}

/// Spec §4.5 vantage honesty, part 1: the heading names what the report is
/// judged against and the snapshot window.
pub fn lan_heading(descriptor_desc: &str, lan_secs: u64) -> String {
    style::heading(
        "mesh roll call",
        &format!("· vs {descriptor_desc} · {lan_secs}s snapshot"),
    )
}

/// Spec §4.5 vantage honesty, part 2: the trailing dim disclaimer.
pub fn disclaimer() -> String {
    style::labeled_block(
        "note",
        style::META,
        &["a snapshot from this binary's vantage; absence can be convergence lag; green here does not guarantee green elsewhere".into()],
    )
}

/// The --connect differential probe verdict (spec §4.4). Pure text classification.
/// `local_ok` distinguishes a genuinely healthy-but-lonely binary (the (0, None) arm with
/// `local_ok == true`) from one whose own egress probe already failed — in the latter case
/// declaring the binary "healthy" would contradict the local diagnosis printed just above.
pub fn classify_probe(
    multicast_found: usize,
    direct_found: Option<usize>,
    local_ok: bool,
) -> Vec<String> {
    match (multicast_found, direct_found) {
        (0, Some(d)) if d > 0 => vec![
            format!("multicast discovery found nothing, but the direct connection reached {d} node key(s) — multicast-only filtering confirmed (IGMP snooping/querier class)."),
            "Remedy: fix the switch's IGMP settings, or distribute explicit-connect endpoints.".into(),
        ],
        (0, Some(_)) => vec![
            "neither multicast nor the direct connection reached anyone — full isolation (AP client isolation / VLAN) or the remote is not running.".into(),
            "Next: run doctor on the remote machine (both-side evidence needed).".into(),
        ],
        (0, None) if local_ok => vec![
            "this binary is healthy but no sahou node is visible. Suspicion-ranked:".into(),
            "1. the remote sahou is not started".into(),
            "2. the AP/hub blocks client-to-client traffic".into(),
            "3. multicast-only pruning (IGMP)".into(),
            "4. different VLAN/subnet".into(),
            "To narrow it down: rerun with --connect tcp/<remote-ip>:7447 for a direct-path probe.".into(),
        ],
        (0, None) => vec![
            "no sahou node is visible — and this binary's own egress probe failed (see the local diagnosis above).".into(),
            "Fix the local issue first; the LAN sweep cannot be trusted until this binary can reach the LAN.".into(),
        ],
        (m, _) => vec![format!("{m} node key(s) visible via multicast.")],
    }
}

#[derive(Debug)]
pub enum DescriptorSource {
    Loaded(PathBuf, Descriptor),
    /// no descriptor anywhere -> discovery-only mode
    None,
}

/// Resolve which descriptor to roll-call against: explicit --descriptor, else the
/// `sahou gen` default output (./gen/descriptor.json), else ./descriptor.json, else None.
/// A mistakenly passed schema YAML gets a hint, not a cryptic parse error (spec §4.1).
pub fn resolve_descriptor(
    explicit: Option<&Path>,
    cwd: &Path,
) -> Result<DescriptorSource, Vec<Diag>> {
    if let Some(p) = explicit {
        let text = std::fs::read_to_string(p).map_err(|e| {
            vec![Diag::new(
                "doctor_descriptor_unreadable",
                "$",
                format!("cannot read {}: {e}", p.display()),
            )]
        })?;
        return match sahou_core::runtime::load_descriptor(&text) {
            Ok(desc) => Ok(DescriptorSource::Loaded(p.to_path_buf(), desc)),
            Err(desc_diags) => {
                if sahou_core::parse::parse_contract(&text).is_ok() {
                    Err(vec![Diag::new(
                        "doctor_schema_not_descriptor",
                        "$",
                        format!(
                            "{} is a schema, not the generated descriptor. Run `sahou gen` and pass its descriptor.json (default: gen/descriptor.json)",
                            p.display()
                        ),
                    )])
                } else {
                    Err(vec![Diag::new(
                        "doctor_descriptor_unreadable",
                        "$",
                        format!(
                            "{} is not a loadable descriptor: {}",
                            p.display(),
                            desc_diags
                                .first()
                                .map(|d| d.message.clone())
                                .unwrap_or_default()
                        ),
                    )])
                }
            }
        };
    }
    for cand in ["gen/descriptor.json", "descriptor.json"] {
        let p = cwd.join(cand);
        if let Ok(text) = std::fs::read_to_string(&p) {
            if let Ok(desc) = sahou_core::runtime::load_descriptor(&text) {
                return Ok(DescriptorSource::Loaded(p, desc));
            }
        }
    }
    Ok(DescriptorSource::None)
}

/// Print `classify_probe` guidance under the cyan `next` label (shared by the
/// three guidance sites in `run_lan`).
fn print_next(lines: &[String]) {
    anstream::print!("{}", style::labeled_block("next", style::ACTION, lines));
}

/// Collect liveliness tokens + ALL vitals replies for one selector, polling inside a grace
/// window (a fresh observer's first liveliness get can be empty pre-convergence; verified).
fn sweep(session: &zenoh::Session, selector: &str, grace_secs: u64) -> LanSweep {
    let deadline = Instant::now() + Duration::from_secs(grace_secs);
    let mut out = LanSweep {
        token_keys: vec![],
        vitals: vec![],
    };
    loop {
        out.token_keys.clear();
        if let Ok(replies) = session
            .liveliness()
            .get(selector)
            .timeout(Duration::from_secs(2))
            .wait()
        {
            while let Ok(reply) = replies.recv() {
                if let Ok(sample) = reply.result() {
                    out.token_keys.push(sample.key_expr().as_str().to_string());
                }
            }
        }
        if !out.token_keys.is_empty() || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    // Consolidation::None: duplicate node instances answer with the SAME key expression, and
    // the default Auto consolidation collapses same-key replies down to one (verified empirically
    // on zenoh 1.9 — silently losing the duplicate-instance signal the module doc above promises).
    if let Ok(replies) = session
        .get(selector)
        .consolidation(zenoh::query::ConsolidationMode::None)
        .timeout(Duration::from_secs(2))
        .wait()
    {
        while let Ok(reply) = replies.recv() {
            if let Ok(sample) = reply.result() {
                if let Ok(s) = std::str::from_utf8(&sample.payload().to_bytes()) {
                    out.vitals
                        .push((sample.key_expr().as_str().to_string(), s.to_string()));
                }
            }
        }
    }
    out
}

/// The --lan stage. Roll call with a descriptor; discovery-only without one; a differential
/// second pass when --connect is given.
pub fn run_lan(args: &crate::doctor::DoctorArgs, local_ok: bool) -> Result<(), Vec<Diag>> {
    anstream::println!();
    // heading printed per-branch below (roll call vs discovery)
    let cwd = std::env::current_dir().map_err(|e| {
        vec![Diag::new(
            "doctor_lan_error",
            "$",
            format!("cannot resolve cwd: {e}"),
        )]
    })?;
    let source = resolve_descriptor(args.descriptor.as_deref(), &cwd)?;

    // pass A: the normal path (multicast scouting; plus the explicit endpoint if given)
    let session = crate::tap::open_session(args.connect.as_deref(), false, args.iface.as_deref())?;
    match source {
        DescriptorSource::Loaded(path, desc) => {
            let selector = format!("{}/@sahou/vitals/*", desc.namespace);
            let s = sweep(&session, &selector, args.lan_secs);
            let rows = roll_call(&desc, &s);
            anstream::println!(
                "{}",
                lan_heading(
                    &format!(
                        "{} ({} connections)",
                        path.display(),
                        desc.connections.len()
                    ),
                    args.lan_secs
                )
            );
            anstream::print!("{}", render_roll_call(&rows));
            let missing: Vec<String> = rows
                .iter()
                .filter(|r| r.present.is_none())
                .map(|r| r.node.clone())
                .collect();
            let found = rows.len() - missing.len();
            // Close pass A's own session before any differential probe below: it still has
            // multicast scouting enabled (only --connect was pinned), so if left open it would
            // itself get discovered via multicast by a fresh multicast-only probe session and,
            // being bridged into the mesh via --connect, silently relay queries through —
            // defeating the whole point of a multicast-ONLY health check.
            let _ = session.close().wait();
            if found == 0 {
                // nobody visible: run the differential probe if we have a direct endpoint
                let direct = args.connect.as_ref().map(|ep| {
                    // pass B: direct path only (no multicast) — separates IGMP-class filtering
                    match crate::tap::open_session(Some(ep), true, args.iface.as_deref()) {
                        Ok(s2) => {
                            let sb = sweep(&s2, &selector, args.lan_secs);
                            let n = roll_call(&desc, &sb)
                                .iter()
                                .filter(|r| r.present.is_some())
                                .count();
                            let _ = s2.close().wait();
                            n
                        }
                        Err(_) => 0,
                    }
                });
                print_next(&classify_probe(0, direct, local_ok));
            } else if args.connect.is_some() {
                // multicast-health check: pass A above already includes --connect, so a
                // successful roll call here can be silently dependent on the explicit
                // endpoint. Re-sweep with multicast ONLY (no --connect) — if that finds
                // nobody, warn the user before they walk away thinking the LAN is healthy
                // (the IGMP-class failure §4.4 exists to surface).
                if let Ok(s2) = crate::tap::open_session(None, false, args.iface.as_deref()) {
                    let sb = sweep(&s2, &selector, args.lan_secs);
                    let n = roll_call(&desc, &sb)
                        .iter()
                        .filter(|r| r.present.is_some())
                        .count();
                    if n == 0 {
                        anstream::println!(
                            "{} {}",
                            Status::Warn.glyph(),
                            style::paint(
                                style::WARN,
                                "the roll call succeeded only via the explicit endpoint:"
                            )
                        );
                        print_next(&classify_probe(0, Some(found), local_ok));
                    }
                    let _ = s2.close().wait();
                }
            }
            if missing.is_empty() {
                anstream::print!("{}", disclaimer());
                Ok(())
            } else {
                anstream::println!(
                    "\n{} {}",
                    Status::Bad.glyph(),
                    style::paint(
                        style::HEAD,
                        format!(
                            "{} of {} expected node(s) not visible from here: {}",
                            missing.len(),
                            rows.len(),
                            missing.join(", ")
                        )
                    )
                );
                anstream::print!("{}", disclaimer());
                Err(vec![Diag::new(
                    "doctor_lan_missing_nodes",
                    "nodes",
                    format!(
                        "{} of {} expected node(s) not visible from here: {} (see the roll call above for next steps)",
                        missing.len(),
                        rows.len(),
                        missing.join(", ")
                    ),
                )])
            }
        }
        DescriptorSource::None => {
            // discovery-only: list every sahou node visible, across namespaces (verified selector)
            let s = sweep(&session, "**/@sahou/vitals/**", args.lan_secs);
            anstream::println!(
                "{}",
                style::heading(
                    "mesh discovery",
                    &format!(
                        "· no descriptor (discovery-only) · {}s snapshot",
                        args.lan_secs
                    )
                )
            );
            if s.token_keys.is_empty() && s.vitals.is_empty() {
                print_next(&classify_probe(0, None, local_ok));
            } else {
                let mut keys: Vec<&String> = s.token_keys.iter().collect();
                for (k, _) in &s.vitals {
                    if !s.token_keys.contains(k) {
                        keys.push(k);
                    }
                }
                keys.sort();
                keys.dedup();
                for k in keys {
                    if let Some((ns, _)) = k.split_once("/@sahou/vitals/") {
                        anstream::println!(
                            "  {} {:<12} {}",
                            Status::Ok.glyph(),
                            vitals_node(k).unwrap_or("?"),
                            style::paint(style::META, format!("namespace {ns}"))
                        );
                    }
                }
                anstream::print!(
                    "{}",
                    style::labeled_block(
                        "hint",
                        style::ACTION,
                        &["pass --descriptor <gen/descriptor.json> to see who is MISSING, not just who is here.".into()]
                    )
                );
            }
            anstream::print!("{}", disclaimer());
            let _ = session.close().wait();
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sahou_core::endpoints::Endpoints;
    use sahou_core::ir::descriptor_json;
    use sahou_core::parse::parse_contract;
    use sahou_core::runtime::load_descriptor;
    use sahou_core::vitals::{vitals_key, vitals_payload};

    const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");
    const INFO: &str = r#"{"lang":"rust","sahou":"0.0.2","transport":"native"}"#;

    fn plain(s: &str) -> String {
        anstream::adapter::strip_str(s).to_string()
    }

    fn demo_desc() -> sahou_core::ir::Descriptor {
        let c = parse_contract(&DEMO.replace("\r\n", "\n")).unwrap();
        load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap()
    }

    fn sweep_with(desc: &sahou_core::ir::Descriptor, nodes: &[&str]) -> LanSweep {
        let mut s = LanSweep {
            token_keys: vec![],
            vitals: vec![],
        };
        for n in nodes {
            let k = vitals_key(desc, n);
            s.token_keys.push(k.clone());
            s.vitals.push((k, vitals_payload(desc, n, INFO).unwrap()));
        }
        s
    }

    #[test]
    fn vitals_node_extracts_the_last_chunk() {
        assert_eq!(vitals_node("sahou/@sahou/vitals/sensor"), Some("sensor"));
        assert_eq!(vitals_node("multi/ns/@sahou/vitals/x"), Some("x"));
        assert_eq!(vitals_node("sahou/other/key"), None);
    }

    #[test]
    fn roll_call_marks_present_and_missing_nodes() {
        let desc = demo_desc();
        let rows = roll_call(&desc, &sweep_with(&desc, &["sensor"]));
        let by = |n: &str| rows.iter().find(|r| r.node == n).unwrap();
        assert!(matches!(by("sensor").present, Some(Presence::Token)));
        assert_eq!(by("sensor").sahou_version.as_deref(), Some("0.0.2"));
        assert!(matches!(by("sensor").generation, Generation::Match));
        assert!(by("visuals").present.is_none());
        assert!(matches!(by("visuals").generation, Generation::Unknown));
    }

    #[test]
    fn roll_call_detects_generation_drift() {
        let desc = demo_desc();
        // a sender running a breaking variant: same demo with touch.x float -> string
        let breaking_yaml = DEMO.replace("\r\n", "\n").replace(
            "        - { name: x, type: float, min: 0, max: 1 }",
            "        - { name: x, type: string }",
        );
        let c = parse_contract(&breaking_yaml).unwrap();
        let breaking = load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap();
        let mut sweep = LanSweep {
            token_keys: vec![],
            vitals: vec![],
        };
        let k = vitals_key(&desc, "sensor");
        sweep.token_keys.push(k.clone());
        sweep
            .vitals
            .push((k, vitals_payload(&breaking, "sensor", INFO).unwrap()));
        let rows = roll_call(&desc, &sweep);
        let sensor = rows.iter().find(|r| r.node == "sensor").unwrap();
        match &sensor.generation {
            Generation::Drift(detail) => assert!(detail.contains("touch"), "{detail}"),
            other => panic!("expected Drift: {other:?}"),
        }
    }

    #[test]
    fn roll_call_counts_duplicate_instances_via_vitals_replies() {
        let desc = demo_desc();
        let mut sweep = sweep_with(&desc, &["sensor"]);
        // a second instance answers the same node key (tokens would aggregate; replies do not)
        let k = vitals_key(&desc, "sensor");
        sweep
            .vitals
            .push((k, vitals_payload(&desc, "sensor", INFO).unwrap()));
        let rows = roll_call(&desc, &sweep);
        let sensor = rows.iter().find(|r| r.node == "sensor").unwrap();
        assert!(
            sensor
                .notes
                .iter()
                .any(|(k, n)| *k == NoteKind::Warn && n.contains("2 instances")),
            "{:?}",
            sensor.notes
        );
    }

    #[test]
    fn roll_call_surfaces_blocked_handshakes_and_fallback_presence() {
        let desc = demo_desc();
        let info = r#"{"lang":"rust","sahou":"0.0.2","transport":"native","handshake":{"touch":{"deadbeef00000000":"blocked"}}}"#;
        let k = vitals_key(&desc, "visuals");
        // vitals reply but NO token -> fallback B presence, labeled
        let sweep = LanSweep {
            token_keys: vec![],
            vitals: vec![(k, vitals_payload(&desc, "visuals", info).unwrap())],
        };
        let rows = roll_call(&desc, &sweep);
        let v = rows.iter().find(|r| r.node == "visuals").unwrap();
        assert!(matches!(v.present, Some(Presence::VitalsReply)));
        assert!(
            v.notes.iter().any(|(k, n)| *k == NoteKind::Blocking
                && n.contains("blocking")
                && n.contains("touch")),
            "{:?}",
            v.notes
        );
    }

    #[test]
    fn roll_call_reports_unreadable_vitals_as_a_note_not_a_crash() {
        let desc = demo_desc();
        let k = format!("{}/@sahou/vitals/sensor", desc.namespace);
        let sweep = LanSweep {
            token_keys: vec![k.clone()],
            vitals: vec![(k, r#"{"vitals_format":99}"#.to_string())],
        };
        let rows = roll_call(&desc, &sweep);
        let sensor = rows.iter().find(|r| r.node == "sensor").unwrap();
        assert!(matches!(sensor.generation, Generation::Unknown));
        assert!(
            sensor
                .notes
                .iter()
                .any(|(k, n)| *k == NoteKind::Warn && n.contains("vitals_format_unsupported")),
            "{:?}",
            sensor.notes
        );
    }

    #[test]
    fn render_marks_missing_with_cross_and_present_with_check() {
        let desc = demo_desc();
        let out = plain(&render_roll_call(&roll_call(
            &desc,
            &sweep_with(&desc, &["sensor"]),
        )));
        assert!(out.contains("✓ sensor"), "{out}");
        assert!(out.contains("sahou 0.0.2"), "{out}");
        assert!(out.contains("generation match"), "{out}");
        assert!(out.contains("✗ visuals"), "{out}");
        assert!(
            out.contains("no vitals — not started / unreachable from here"),
            "{out}"
        );
    }

    #[test]
    fn render_puts_fallback_presence_and_notes_on_indented_lines() {
        let desc = demo_desc();
        let info = r#"{"lang":"rust","sahou":"0.0.2","transport":"native","handshake":{"touch":{"deadbeef00000000":"blocked"}}}"#;
        let k = vitals_key(&desc, "visuals");
        let sweep = LanSweep {
            token_keys: vec![],
            vitals: vec![(k, vitals_payload(&desc, "visuals", info).unwrap())],
        };
        let out = plain(&render_roll_call(&roll_call(&desc, &sweep)));
        assert!(
            out.contains("↳ presence via vitals reply (no liveliness token)"),
            "{out}"
        );
        assert!(
            out.contains("↳ blocking sender deadbeef00000000 on touch"),
            "{out}"
        );
    }

    #[test]
    fn lan_heading_and_disclaimer_keep_vantage_honesty() {
        let h = plain(&lan_heading("gen/descriptor.json (4 connections)", 5));
        assert!(h.contains("mesh roll call"), "{h}");
        assert!(h.contains("vs gen/descriptor.json (4 connections)"), "{h}");
        assert!(h.contains("5s snapshot"), "{h}");
        let d = plain(&disclaimer());
        assert!(d.contains("this binary's vantage"), "{d}");
        assert!(
            d.contains("green here does not guarantee green elsewhere"),
            "{d}"
        );
    }

    #[test]
    fn classify_probe_localizes_the_fault() {
        let igmp = classify_probe(0, Some(2), true).join("\n");
        assert!(igmp.contains("multicast-only filtering"), "{igmp}");
        let dark = classify_probe(0, Some(0), true).join("\n");
        assert!(dark.contains("remote machine"), "{dark}");
        let unknown = classify_probe(0, None, true).join("\n");
        assert!(unknown.contains("--connect"), "{unknown}");
        assert!(unknown.contains("1."), "{unknown}"); // suspicion-ranked list
        let fine = classify_probe(3, None, true).join("\n");
        assert!(
            fine.contains("3 node key(s) visible via multicast"),
            "{fine}"
        );
        let blocked = classify_probe(0, None, false).join("\n");
        assert!(!blocked.contains("healthy"), "{blocked}");
        assert!(blocked.contains("Fix the local issue first"), "{blocked}");
    }

    #[test]
    fn resolve_descriptor_prefers_explicit_then_gen_then_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let desc_json = descriptor_json(
            &parse_contract(&DEMO.replace("\r\n", "\n")).unwrap(),
            &Endpoints::default(),
        );
        // nothing anywhere -> discovery-only
        assert!(matches!(
            resolve_descriptor(None, dir.path()).unwrap(),
            DescriptorSource::None
        ));
        // ./descriptor.json is found…
        std::fs::write(dir.path().join("descriptor.json"), &desc_json).unwrap();
        let DescriptorSource::Loaded(p, _) = resolve_descriptor(None, dir.path()).unwrap() else {
            panic!("expected Loaded")
        };
        assert!(p.ends_with("descriptor.json"));
        // …but gen/descriptor.json (the sahou gen default) wins over it
        std::fs::create_dir(dir.path().join("gen")).unwrap();
        std::fs::write(dir.path().join("gen/descriptor.json"), &desc_json).unwrap();
        let DescriptorSource::Loaded(p, _) = resolve_descriptor(None, dir.path()).unwrap() else {
            panic!("expected Loaded")
        };
        assert!(p.ends_with("gen/descriptor.json"), "{p:?}");
    }

    #[test]
    fn resolve_descriptor_hints_when_given_a_schema_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let schema = dir.path().join("schema.sahou.yaml");
        std::fs::write(&schema, DEMO).unwrap();
        let err = resolve_descriptor(Some(&schema), dir.path()).unwrap_err();
        assert_eq!(err[0].code, "doctor_schema_not_descriptor");
        assert!(err[0].message.contains("sahou gen"), "{}", err[0].message);
    }

    #[test]
    fn resolve_descriptor_explicit_missing_or_broken_is_a_hard_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.json");
        assert_eq!(
            resolve_descriptor(Some(&missing), dir.path()).unwrap_err()[0].code,
            "doctor_descriptor_unreadable"
        );
        let broken = dir.path().join("broken.json");
        std::fs::write(&broken, "{not json").unwrap();
        assert_eq!(
            resolve_descriptor(Some(&broken), dir.path()).unwrap_err()[0].code,
            "doctor_descriptor_unreadable"
        );
    }
}
