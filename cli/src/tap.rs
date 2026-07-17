//! sahou tap — observe/inject without an app (design §7, ②c).
//! Validation and sample generation go only through the same core functions the engine uses
//! (prepare_publish / prepare_request / accept_sample / accept_reply / sample_slot)
//! = no tap-specific validation logic is written (a structural guarantee of byte-identical diagnostics).
//! watch additionally explains hash mismatches by fetching the sender's contract fragment
//! (`<ns>/@sahou/contract/<conn>/<hash>`) and judging it with the core handshake_judge.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use clap::Args;
use sahou_core::contract::{Congestion, Pattern, Priority, Reliability};
use sahou_core::diag::Diag;
use sahou_core::ir::Descriptor;
use sahou_core::runtime as rt;
use sahou_core::sample::sample_slot;
use zenoh::Wait;

#[derive(Args)]
pub struct TapArgs {
    /// Full IR (descriptor.json)
    pub descriptor: PathBuf,
    /// Observe from this node's vantage point (when omitted, all pub_sub connections from the to[0] vantage)
    #[arg(long)]
    pub node: Option<String>,
    /// Inject: send one message to this connection and exit (when omitted, the observe/watch mode)
    #[arg(long)]
    pub send: Option<String>,
    /// The --send payload (a JSON string). Mutually exclusive with --sample
    #[arg(long)]
    pub payload: Option<String>,
    /// Auto-generate the --send payload with the core sample_slot (a valid sample). Mutually exclusive with --payload
    #[arg(long)]
    pub sample: bool,
    /// watch: exit after displaying this many events (for tests/scripts; when omitted, until Ctrl+C)
    #[arg(long)]
    pub count: Option<u64>,
    /// Explicit connect endpoint (e.g. tcp/[::1]:7448 = link's peer port)
    #[arg(long)]
    pub connect: Option<String>,
    /// Disable multicast scouting (for explicit-connect operation and tests/CI)
    #[arg(long)]
    pub no_multicast: bool,
    /// Pin the NIC for multicast scouting (guards against scouts getting lost on multi-NIC hosts)
    #[arg(long)]
    pub iface: Option<String>,
}

fn tap_err(path: impl Into<String>, msg: impl Into<String>) -> Vec<Diag> {
    vec![Diag::new("tap_error", path, msg)]
}

/// The assembled message to send. from = the sending vantage node (carried in the return value so run_send does not look it up twice).
#[derive(Debug)]
pub struct SendPlan {
    pub pattern: Pattern,
    pub from: String,
    pub msg: rt::WireMsg,
}

/// Assemble the message to send (pure function). Vantage = the connection's from (satisfies the core's send-boundary role check).
pub fn build_send(
    desc: &Descriptor,
    conn: &str,
    payload: Option<&str>,
    sample: bool,
) -> Result<SendPlan, Vec<Diag>> {
    // Validation of an undefined connection calls the core conn_of directly (structurally guaranteeing byte-identical diagnostics; option B).
    let c = rt::conn_of(desc, conn)?;
    let payload_json = match (payload, sample) {
        (Some(p), false) => p.to_string(),
        (None, true) => {
            let slot = match c.pattern {
                Pattern::PubSub => &c.payload,
                Pattern::Query => &c.request,
            };
            let slot = slot.as_ref().ok_or_else(|| {
                tap_err(
                    format!("connections.{conn}"),
                    "the descriptor has no slot (the gen output is broken)",
                )
            })?;
            sample_slot(slot).to_string()
        }
        _ => {
            return Err(tap_err(
                "$",
                "specify exactly one of --payload <json> or --sample",
            ))
        }
    };
    let from = c.from.clone();
    let msg = match c.pattern {
        Pattern::PubSub => rt::prepare_publish(desc, &from, conn, &payload_json, 0)?,
        Pattern::Query => rt::prepare_request(desc, &from, conn, &payload_json, 0)?,
    };
    Ok(SendPlan {
        pattern: c.pattern,
        from,
        msg,
    })
}

fn fmt_diags(diags: &[Diag]) -> String {
    diags
        .iter()
        .map(|d| format!("[{}] @{}: {}", d.code, d.path, d.message))
        .collect::<Vec<_>>()
        .join("; ")
}

/// descriptor QoS enum -> zenoh objects (a glue responsibility; the mapping table in design §4).
fn map_qos(
    q: &rt::QosSpec,
) -> (
    zenoh::qos::Reliability,
    zenoh::qos::CongestionControl,
    zenoh::qos::Priority,
    bool,
) {
    use zenoh::qos as z;
    let rel = match q.reliability {
        Reliability::Reliable => z::Reliability::Reliable,
        Reliability::BestEffort => z::Reliability::BestEffort,
    };
    let cc = match q.congestion {
        Congestion::Block => z::CongestionControl::Block,
        Congestion::Drop => z::CongestionControl::Drop,
    };
    let prio = match q.priority {
        Priority::RealTime => z::Priority::RealTime,
        Priority::InteractiveHigh => z::Priority::InteractiveHigh,
        Priority::InteractiveLow => z::Priority::InteractiveLow,
        Priority::DataHigh => z::Priority::DataHigh,
        Priority::Data => z::Priority::Data,
        Priority::DataLow => z::Priority::DataLow,
        Priority::Background => z::Priority::Background,
    };
    (rel, cc, prio, q.express)
}

/// A zenoh peer session for tap (the same config pattern as link.rs; synchronous API).
pub(crate) fn open_session(
    connect: Option<&str>,
    no_multicast: bool,
    iface: Option<&str>,
) -> Result<zenoh::Session, Vec<Diag>> {
    let mut config = zenoh::Config::default();
    {
        let mut ins = |k: &str, v: String| {
            config
                .insert_json5(k, &v)
                .map_err(|e| tap_err("$", format!("failed to set config {k}: {e}")))
        };
        ins("mode", "\"peer\"".into())?;
        if no_multicast {
            ins("scouting/multicast/enabled", "false".into())?;
        }
        if let Some(i) = iface {
            ins("scouting/multicast/interface", format!("\"{i}\""))?;
        }
        if let Some(ep) = connect {
            ins("connect/endpoints", format!("[\"{ep}\"]"))?;
        }
    }
    zenoh::open(config)
        .wait()
        .map_err(|e| tap_err("$", format!("failed to open the zenoh session: {e}")))
}

pub fn run(args: TapArgs) -> Result<(), Vec<Diag>> {
    let json = std::fs::read_to_string(&args.descriptor)
        .map_err(|e| tap_err(args.descriptor.display().to_string(), e.to_string()))?;
    let desc = rt::load_descriptor(&json)?;
    match &args.send {
        Some(conn) => run_send(&desc, conn, &args),
        None => run_watch(Arc::new(desc), &args),
    }
}

/// Observation targets: (conn_id, vantage node). Picks receivers that satisfy the core accept_sample role check (Role::To).
pub fn watch_targets(
    desc: &Descriptor,
    node: Option<&str>,
) -> Result<Vec<(String, String)>, Vec<Diag>> {
    match node {
        Some(n) => {
            let plan = rt::node_plan(desc, n)?; // an unknown node yields the core's unknown_node
            if plan.subscribes.is_empty() {
                return Err(tap_err(
                    format!("nodes.{n}"),
                    format!("node '{n}' has no subscribing connections (nothing to observe)"),
                ));
            }
            Ok(plan
                .subscribes
                .into_iter()
                .map(|c| (c, n.to_string()))
                .collect())
        }
        None => {
            let mut targets: Vec<(String, String)> = Vec::new();
            for (id, c) in desc
                .connections
                .iter()
                .filter(|(_, c)| c.pattern == Pattern::PubSub)
            {
                let vantage = c.to.first().ok_or_else(|| {
                    vec![Diag::new(
                        "no_receiver",
                        format!("connections.{id}"),
                        format!("connection '{id}' has no receiver (to) defined"),
                    )]
                })?;
                targets.push((id.clone(), vantage.clone()));
            }
            if targets.is_empty() {
                return Err(tap_err(
                    "connections",
                    "no pub_sub connections (nothing to observe)",
                ));
            }
            Ok(targets)
        }
    }
}

/// One observed sample, reported from the subscriber callback to the main loop.
/// Exactly one event per received sample, so --count keeps counting samples
/// regardless of any explanation fetch that follows.
enum WatchEvent {
    /// already printed inline; nothing further to do
    Seen,
    /// printed inline as NO [hash_mismatch]; the main loop fetches the sender's contract and explains
    Mismatch { conn: String, sender_hash: String },
}

/// Fetch the sender's contract fragment from `<ns>/@sahou/contract/<conn>/<sender_hash>`
/// (the queryable every engine declares). None = no usable reply within the timeout
/// (sender absent / pre-contract peer / route not converged) — reported as
/// handshake:unreachable by the caller and NOT cached (retryable on the next mismatch).
/// Timeout matches the engine's contract fetch (2s).
fn fetch_fragment(
    session: &zenoh::Session,
    ns: &str,
    conn: &str,
    sender_hash: &str,
) -> Option<String> {
    let sel = format!("{ns}/@sahou/contract/{conn}/{sender_hash}");
    let replies = session
        .get(&sel)
        .timeout(Duration::from_secs(2))
        .wait()
        .ok()?;
    while let Ok(reply) = replies.recv() {
        if let Ok(sample) = reply.result() {
            if let Ok(s) = std::str::from_utf8(&sample.payload().to_bytes()) {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn run_watch(desc: Arc<Descriptor>, args: &TapArgs) -> Result<(), Vec<Diag>> {
    let targets = watch_targets(&desc, args.node.as_deref())?;
    let session = open_session(
        args.connect.as_deref(),
        args.no_multicast,
        args.iface.as_deref(),
    )?;
    let (tx, rx) = mpsc::channel::<WatchEvent>();
    let mut subs = Vec::new();
    for (conn, vantage) in targets {
        let key = desc.connections[&conn].key.clone();
        println!("[tap] watching {conn} ({key}) as {vantage}");
        let desc2 = Arc::clone(&desc);
        let tx2 = tx.clone();
        let seq = AtomicU64::new(0);
        // key stays borrowed by the declare_subscriber(&key) builder until .wait(), so a clone is needed
        // (conn is unused afterward -> moved directly into the closure without cloning).
        let key2 = key.clone();
        let sub = session
            .declare_subscriber(&key)
            .callback(move |sample: zenoh::sample::Sample| {
                let s = seq.fetch_add(1, Ordering::Relaxed);
                let wire = sample.payload().to_bytes();
                // a non-UTF-8 attachment becomes None -> the core returns missing_schema_hash (same handling as the engine)
                let att_bytes = sample.attachment().map(|a| a.to_bytes().to_vec());
                let att = att_bytes
                    .as_deref()
                    .and_then(|b| std::str::from_utf8(b).ok());
                let out = rt::accept_sample(&desc2, &vantage, &conn, &wire, att, s, None);
                println!("{}", format_event(&conn, s, &key2, &out));
                let ev = match &out {
                    rt::AcceptOutcome::HashMismatch { sender_hash } => WatchEvent::Mismatch {
                        conn: conn.clone(),
                        sender_hash: sender_hash.clone(),
                    },
                    _ => WatchEvent::Seen,
                };
                let _ = tx2.send(ev);
            })
            .wait()
            .map_err(|e| tap_err("$", format!("failed to declare the subscriber: {e}")))?;
        subs.push(sub);
    }
    drop(tx); // all senders live only inside the callback -> rx closes after all subs are dropped
              // Explanations are resolved once per (conn, sender_hash) and only a delivered verdict
              // is cached; a failed fetch stays retryable (spec: unreachable is not a final verdict).
    let mut explained: HashSet<(String, String)> = HashSet::new();
    let mut handle_event = |ev: WatchEvent| {
        let WatchEvent::Mismatch { conn, sender_hash } = ev else {
            return;
        };
        let pair = (conn.clone(), sender_hash.clone());
        if explained.contains(&pair) {
            return;
        }
        match fetch_fragment(&session, &desc.namespace, &conn, &sender_hash) {
            Some(frag) => {
                println!("{}", explain_mismatch(&desc, &conn, &sender_hash, &frag));
                explained.insert(pair);
            }
            None => println!(
                "[{conn}] [handshake:unreachable] sender {sender_hash}: cannot fetch the sender's contract (absent / pre-contract peer / route not converged; retried on the next mismatch)"
            ),
        }
    };
    match args.count {
        Some(n) => {
            for _ in 0..n {
                match rx.recv() {
                    Ok(ev) => handle_event(ev),
                    Err(_) => break,
                }
            }
        }
        None => {
            // keep receiving until Ctrl+C (process termination)
            while let Ok(ev) = rx.recv() {
                handle_event(ev);
            }
        }
    }
    drop(subs);
    if let Err(e) = session.close().wait() {
        eprintln!("[tap] session close failed (teardown; processing already complete): {e}");
    }
    Ok(())
}

fn run_send(desc: &Descriptor, conn: &str, args: &TapArgs) -> Result<(), Vec<Diag>> {
    let plan = build_send(desc, conn, args.payload.as_deref(), args.sample)?;
    let session = open_session(
        args.connect.as_deref(),
        args.no_multicast,
        args.iface.as_deref(),
    )?;
    // Practical grace for a one-shot CLI: wait a little for the declaration exchange (route convergence) before sending
    std::thread::sleep(Duration::from_millis(300));
    match plan.pattern {
        Pattern::PubSub => {
            let (rel, cc, prio, express) = map_qos(&plan.msg.qos);
            let publisher = session
                .declare_publisher(plan.msg.key.clone())
                .reliability(rel)
                .congestion_control(cc)
                .priority(prio)
                .express(express)
                .wait()
                .map_err(|e| tap_err("$", format!("failed to declare the publisher: {e}")))?;
            publisher
                .put(plan.msg.wire.as_bytes())
                .attachment(plan.msg.attachment.as_bytes())
                .wait()
                .map_err(|e| tap_err("$", format!("put failed: {e}")))?;
            println!("[tap] sent {conn} -> {} {}", plan.msg.key, plan.msg.wire);
            std::thread::sleep(Duration::from_millis(500)); // grace for delivery (do not drop it by closing immediately)
        }
        Pattern::Query => {
            println!("[tap] query {conn} -> {} {}", plan.msg.key, plan.msg.wire);
            let replies = session
                .get(&plan.msg.key)
                .payload(plan.msg.wire.as_bytes())
                .attachment(plan.msg.attachment.as_bytes())
                .timeout(Duration::from_secs(2))
                .wait()
                .map_err(|e| tap_err("$", format!("get failed: {e}")))?;
            let mut seq: u64 = 0;
            let mut got_any = false;
            while let Ok(reply) = replies.recv() {
                got_any = true;
                match reply.result() {
                    Ok(sample) => {
                        let att_bytes = sample.attachment().map(|a| a.to_bytes().to_vec());
                        let att = att_bytes
                            .as_deref()
                            .and_then(|b| std::str::from_utf8(b).ok());
                        // ④ the reply-receive boundary is the core too (same as the engine; byte-identical diagnostics)
                        let out = rt::accept_reply(
                            desc,
                            &plan.from,
                            conn,
                            &sample.payload().to_bytes(),
                            att,
                            seq,
                            None,
                        );
                        println!("{}", format_event(conn, seq, &plan.msg.key, &out));
                        if matches!(out, rt::AcceptOutcome::HashMismatch { .. }) {
                            // send-path honesty: only watch fetches the sender's contract and explains (Phase 1 scope)
                            println!("[{conn}] tap --send does not handshake; run tap watch on this connection to fetch the sender's contract and explain the mismatch");
                        }
                        seq += 1;
                    }
                    Err(err) => {
                        let diags = rt::parse_reply_err(&err.payload().to_bytes());
                        println!("[{conn}] reply_err {}", fmt_diags(&diags));
                    }
                }
            }
            if !got_any {
                println!("[{conn}] no response (timeout 2s; the responder is absent or the route has not converged)");
            }
        }
    }
    if let Err(e) = session.close().wait() {
        eprintln!("[tap] session close failed (teardown; processing already complete): {e}");
    }
    Ok(())
}

/// Explain a delivery-time hash mismatch by judging the sender's fetched contract fragment
/// with the same core judge the engine uses (rt::handshake_judge = byte-identical diagnostics).
/// Pure (no I/O) so judgement + formatting are unit-testable without a network.
/// Every line names its vantage: the judgement is relative to the descriptor tap loaded
/// (the running apps may hold a different generation).
pub fn explain_mismatch(
    desc: &Descriptor,
    conn: &str,
    sender_hash: &str,
    fragment_json: &str,
) -> String {
    const VANTAGE: &str = "judged vs the descriptor tap loaded";
    match rt::handshake_judge(desc, conn, sender_hash, fragment_json) {
        rt::HandshakeOutcome::Accepted => format!(
            "[{conn}] [handshake:accepted] sender {sender_hash}: contracts differ but are additive-compatible; delivery would proceed after the handshake ({VANTAGE})"
        ),
        rt::HandshakeOutcome::Blocked { diags } => format!(
            "[{conn}] [handshake:blocked] sender {sender_hash}: {} ({VANTAGE})",
            fmt_diags(&diags)
        ),
        rt::HandshakeOutcome::Unreachable { diags } => format!(
            "[{conn}] [handshake:unreachable] sender {sender_hash}: {} ({VANTAGE})",
            fmt_diags(&diags)
        ),
    }
}

/// A human-readable one-liner for a received event. The validation result is formatted directly from the core AcceptOutcome (no custom judgment is inserted).
/// Both the query reply display (Task 1) and the watch subscription display (Task 2) use this formatting.
pub fn format_event(conn: &str, seq: u64, key: &str, outcome: &rt::AcceptOutcome) -> String {
    match outcome {
        rt::AcceptOutcome::Accept { payload } => format!("[{conn}] #{seq} OK {key} {payload}"),
        rt::AcceptOutcome::Reject { diags } => {
            format!("[{conn}] #{seq} NO {key} {}", fmt_diags(diags))
        }
        rt::AcceptOutcome::HashMismatch { sender_hash } => format!(
            "[{conn}] #{seq} NO {key} [hash_mismatch] @connections.{conn}: contract version mismatch (sender_hash={sender_hash})"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sahou_core::endpoints::Endpoints;
    use sahou_core::ir::descriptor_json;
    use sahou_core::parse::parse_contract;

    const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

    fn demo_desc() -> Descriptor {
        let c = parse_contract(DEMO).unwrap();
        rt::load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap()
    }

    #[test]
    fn build_send_sample_passes_core_receive_boundary() {
        let desc = demo_desc();
        let plan = build_send(&desc, "touch", None, true).unwrap();
        assert_eq!(plan.pattern, Pattern::PubSub);
        assert_eq!(plan.msg.attachment, desc.connections["touch"].hash);
        // wire produced by sample_slot passes the core receive boundary (= the same valid sample as the engine)
        let out = rt::accept_sample(
            &desc,
            "visuals",
            "touch",
            plan.msg.wire.as_bytes(),
            Some(&plan.msg.attachment),
            0,
            None,
        );
        assert!(matches!(out, rt::AcceptOutcome::Accept { .. }), "{out:?}");
    }

    #[test]
    fn build_send_query_uses_request_boundary() {
        let desc = demo_desc();
        let plan = build_send(&desc, "get_state", None, true).unwrap();
        assert_eq!(plan.pattern, Pattern::Query);
        let out = rt::accept_request(
            &desc,
            "archive",
            "get_state",
            plan.msg.wire.as_bytes(),
            Some(&plan.msg.attachment),
            0,
            None,
        );
        assert!(matches!(out, rt::AcceptOutcome::Accept { .. }), "{out:?}");
    }

    #[test]
    fn build_send_returns_sender_vantage_from_descriptor() {
        let desc = demo_desc();
        let plan = build_send(&desc, "get_state", None, true).unwrap();
        assert_eq!(plan.pattern, Pattern::Query);
        assert_eq!(plan.from, "sensor"); // the query requester = the connection's from (carried in the return value, not looked up twice)
        let plan2 = build_send(&desc, "touch", None, true).unwrap();
        assert_eq!(plan2.from, "sensor");
    }

    #[test]
    fn build_send_requires_exactly_one_payload_source() {
        let desc = demo_desc();
        assert_eq!(
            build_send(&desc, "touch", None, false).unwrap_err()[0].code,
            "tap_error"
        );
        assert_eq!(
            build_send(&desc, "touch", Some("{}"), true).unwrap_err()[0].code,
            "tap_error"
        );
    }

    #[test]
    fn build_send_broken_payload_is_core_send_boundary_no() {
        let desc = demo_desc();
        let err = build_send(
            &desc,
            "touch",
            Some(r#"{"x":"oops","phase":"move","meta":{"ts":0}}"#),
            false,
        )
        .unwrap_err();
        assert_eq!(err[0].code, "type_mismatch"); // the send boundary = the core prepare_publish diagnostic itself
    }

    #[test]
    fn build_send_unknown_connection_is_core_diag() {
        let desc = demo_desc();
        let err = build_send(&desc, "ghost", None, true).unwrap_err();
        assert_eq!(err[0].code, "unknown_connection");
    }

    #[test]
    fn watch_targets_for_node_uses_its_subscriptions() {
        let desc = demo_desc();
        let t = watch_targets(&desc, Some("visuals")).unwrap();
        // visuals is a receiver of touch / points / debug_tap (from the descriptor; query cannot be observed via subscription)
        assert!(t.contains(&("touch".into(), "visuals".into())));
        assert!(t.contains(&("points".into(), "visuals".into())));
        assert!(t.iter().all(|(c, _)| c != "get_state"));
    }

    #[test]
    fn watch_targets_without_node_covers_all_pubsub_from_first_receiver() {
        let desc = demo_desc();
        let t = watch_targets(&desc, None).unwrap();
        assert!(t.contains(&("touch".into(), "visuals".into()))); // the to[0] vantage
        assert!(t.iter().all(|(c, _)| c != "get_state"));
        assert_eq!(t.len(), 3); // touch / points / debug_tap
    }

    #[test]
    fn watch_targets_empty_to_is_structured_no_not_panic() {
        // to: [] can legitimately deserialize even when empty (it may pass through validate/gen).
        // The original bug is that directly indexing to[0] when --node is omitted panics;
        // this is a regression test guaranteeing it returns a structured rejection (no_receiver).
        let mut desc = demo_desc();
        desc.connections.get_mut("touch").unwrap().to = vec![];
        let err = watch_targets(&desc, None).unwrap_err();
        assert_eq!(err[0].code, "no_receiver");
        assert!(err[0].path.contains("touch"));
    }

    #[test]
    fn watch_targets_unknown_node_is_core_diag() {
        let desc = demo_desc();
        let err = watch_targets(&desc, Some("ghost")).unwrap_err();
        assert_eq!(err[0].code, "unknown_node"); // the core node_plan diagnostic, as-is
    }

    #[test]
    fn format_event_renders_core_outcomes() {
        let ok = rt::AcceptOutcome::Accept {
            payload: r#"{"x":0.5}"#.into(),
        };
        assert_eq!(
            format_event("touch", 3, "sahou/touch", &ok),
            r#"[touch] #3 OK sahou/touch {"x":0.5}"#
        );
        let ng = rt::AcceptOutcome::Reject {
            diags: vec![Diag::new("type_mismatch", "x", "expected float")],
        };
        assert!(format_event("touch", 4, "sahou/touch", &ng)
            .contains("[type_mismatch] @x: expected float"));
        let hm = rt::AcceptOutcome::HashMismatch {
            sender_hash: "deadbeef00000000".into(),
        };
        assert!(
            format_event("touch", 5, "sahou/touch", &hm).contains("sender_hash=deadbeef00000000")
        );
    }

    // ---- explain_mismatch (Phase 1: surface "why NO") ----
    // Fragment fixtures are built with rt::contract_fragment on demo-schema variants,
    // the same pattern as core/tests/runtime_handshake.rs (byte-identical judge inputs).

    fn norm(s: &str) -> String {
        s.replace("\r\n", "\n") // guard against CRLF checkout (same lesson as the core handshake tests)
    }

    fn desc_from_yaml(yaml: &str) -> Descriptor {
        let c = parse_contract(yaml).unwrap();
        rt::load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap()
    }

    #[test]
    fn explain_mismatch_additive_is_accepted_not_an_anomaly() {
        let demo = norm(DEMO);
        let base = desc_from_yaml(&demo);
        let additive_yaml = demo.replace(
            "        - { name: phase, type: enum, values: [down, move, up] }",
            "        - { name: phase, type: enum, values: [down, move, up] }\n        - { name: pressure, type: float, required: false }",
        );
        let additive = desc_from_yaml(&additive_yaml);
        let frag = rt::contract_fragment(&additive, "touch").unwrap();
        let sender = additive.connections["touch"].hash.clone();
        let line = explain_mismatch(&base, "touch", &sender, &frag);
        assert!(line.contains("[handshake:accepted]"), "{line}");
        assert!(line.contains("additive-compatible"), "{line}");
        assert!(
            line.contains("judged vs the descriptor tap loaded"),
            "{line}"
        );
    }

    #[test]
    fn explain_mismatch_breaking_is_blocked_with_structured_diag() {
        let demo = norm(DEMO);
        let base = desc_from_yaml(&demo);
        let breaking_yaml = demo.replace(
            "        - { name: x, type: float, min: 0, max: 1 }",
            "        - { name: x, type: string }",
        );
        let breaking = desc_from_yaml(&breaking_yaml);
        let frag = rt::contract_fragment(&breaking, "touch").unwrap();
        let sender = breaking.connections["touch"].hash.clone();
        let line = explain_mismatch(&base, "touch", &sender, &frag);
        assert!(line.contains("[handshake:blocked]"), "{line}");
        assert!(line.contains("schema_incompatible"), "{line}");
        assert!(line.contains("touch"), "{line}"); // the diag path names the real connection
        assert!(
            line.contains("judged vs the descriptor tap loaded"),
            "{line}"
        );
    }

    #[test]
    fn explain_mismatch_promotion_is_blocked_conservatively() {
        let demo = norm(DEMO);
        let base = desc_from_yaml(&demo);
        let promoted_yaml = demo.replace(
            "- { name: level, type: int }",
            "- { name: level, type: float }",
        );
        let promoted = desc_from_yaml(&promoted_yaml);
        let frag = rt::contract_fragment(&promoted, "get_state").unwrap();
        let sender = promoted.connections["get_state"].hash.clone();
        let line = explain_mismatch(&base, "get_state", &sender, &frag);
        assert!(line.contains("[handshake:blocked]"), "{line}");
        assert!(line.contains("conservatively NO"), "{line}"); // core wording for promotion (approach A)
    }

    #[test]
    fn explain_mismatch_malformed_fragment_is_unreachable() {
        let base = desc_from_yaml(&norm(DEMO));
        let line = explain_mismatch(&base, "touch", "deadbeef00000000", "{not json");
        assert!(line.contains("[handshake:unreachable]"), "{line}");
        assert!(line.contains("contract_unreachable"), "{line}");
    }

    #[test]
    fn explain_mismatch_self_reported_hash_mismatch_is_unreachable() {
        let demo = norm(DEMO);
        let base = desc_from_yaml(&demo);
        let frag = rt::contract_fragment(&base, "touch").unwrap(); // a correct fragment, but…
        let line = explain_mismatch(&base, "touch", "0000000000000000", &frag);
        assert!(line.contains("[handshake:unreachable]"), "{line}");
        assert!(line.contains("does not match"), "{line}"); // core's misdelivery/tampering message
    }
}
