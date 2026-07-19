//! sahou doctor — environment preflight diagnostics (design §7; the Rust production version of R&D 013-4, re-derived rather than copied).
//! Core idea: actively probe something a raw socket probe cannot detect — "can this binary's real zenoh scout reach the LAN?" —
//! and return a human-readable rejection + remedy classified by cause
//! (PermissionBlocked(TCC) / NicError / OtherError / Ok), rejecting at the right place before it becomes a silent "0 messages".

use std::net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::style::{self, Status};
use clap::Args;
use sahou_core::diag::Diag;

#[derive(Args)]
pub struct DoctorArgs {
    /// Pin the NIC for multicast scouting (when omitted, the zenoh default = auto)
    #[arg(long)]
    pub iface: Option<String>,
    /// Port to check link WS reachability on
    #[arg(long, default_value_t = 10000)]
    pub link_port: u16,
    /// Window (seconds) over which to observe scout egress errors
    #[arg(long, default_value_t = 4)]
    pub scout_secs: u64,
    /// Also check the LAN: roll call against a descriptor (auto-found in ./gen/descriptor.json
    /// or ./descriptor.json), or discovery-only without one
    #[arg(long)]
    pub lan: bool,
    /// Descriptor to roll-call against (--lan; overrides auto-discovery)
    #[arg(long)]
    pub descriptor: Option<std::path::PathBuf>,
    /// Explicit endpoint for the LAN stage and the direct-path differential probe (--lan)
    #[arg(long)]
    pub connect: Option<String>,
    /// Grace window (seconds) for the LAN sweep to converge (--lan)
    #[arg(long, default_value_t = 5)]
    pub lan_secs: u64,
}

/// Classification of scout egress (four values hardened by measurements in 013-4).
#[derive(Debug, Clone, PartialEq)]
pub enum Egress {
    Ok,
    /// LAN egress was rejected by errno 65 etc. (suspected macOS TCC / unsigned binary)
    PermissionBlocked(String),
    /// Configuration error such as the specified NIC not being found
    NicError(String),
    /// Other zenoh error (keeps the causing line)
    OtherError(String),
}

/// Classify captured zenoh WARN/ERROR logs (pure function).
pub fn classify_egress(log: &str) -> Egress {
    let err_line = |needle: &str| {
        log.lines()
            .find(|l| l.contains(needle))
            .unwrap_or("")
            .trim()
            .to_string()
    };
    // Signatures of blocked LAN egress observed in 013-1 (the verbatim unit test is the ground truth)
    for sig in [
        "No route to host",
        "os error 65",
        "EHOSTUNREACH",
        "Unable to send Scout",
    ] {
        if log.contains(sig) {
            return Egress::PermissionBlocked(err_line(sig));
        }
    }
    if log.contains("Unable to find interface") {
        return Egress::NicError(err_line("Unable to find interface"));
    }
    if log.contains("ERROR") {
        return Egress::OtherError(err_line("ERROR"));
    }
    Egress::Ok
}

/// Bundle of probe results (input to the pure function diagnose).
pub struct ProbeReport {
    pub loopback: bool,
    /// None = skipped because the default gateway is unknown
    pub ping: Option<bool>,
    pub egress: Egress,
    pub peers: usize,
    pub link_ws: bool,
    /// Pass std::env::consts::OS (as an argument, to keep this pure)
    pub os: &'static str,
    pub iface_desc: String,
}

/// The healthy summary (rendered as one green line + dim detail).
#[derive(Debug)]
pub struct Healthy {
    pub title: &'static str,
    pub detail: String,
}

/// A structured failure report: rendered as verdict — title, then
/// cause / fix / captured blocks. `fix` entries are unnumbered; the renderer
/// numbers them. The machine-readable Diag uses (code, path, title) only.
#[derive(Debug)]
pub struct Failure {
    pub code: &'static str,
    pub path: &'static str,
    pub verdict: &'static str,
    pub title: String,
    pub cause: Vec<String>,
    pub fix: Vec<String>,
    pub captured: Option<String>,
}

/// Remedy steps ordered cheapest-first (settings toggle before launch context
/// before code signing).
fn fix_steps(os: &str) -> Vec<String> {
    match os {
        "macos" => vec![
            "allow it in System Settings › Privacy & Security › Local Network".into(),
            "launch from Terminal (an authorized context) rather than ssh/headless".into(),
            "use a distribution signed with a Developer ID + the multicast entitlement".into(),
        ],
        "windows" => vec![
            "allow sahou.exe inbound/outbound (UDP 7446 multicast / TCP 7447) in Windows Defender Firewall".into(),
        ],
        _ => vec![
            "check the firewall (UDP 7446 multicast / TCP 7447) and NIC settings".into(),
        ],
    }
}

/// Assemble the diagnosis (pure function). Ok(healthy summary) / Err(boxed
/// structured failure). The caller renders the failure AND converts it to the
/// compact {code, path, title} Diag for the exit path.
pub fn diagnose(r: &ProbeReport) -> Result<Healthy, Box<Failure>> {
    if !r.loopback {
        return Err(Box::new(Failure {
            code: "doctor_loopback",
            path: "probes.loopback",
            verdict: "broken",
            title: "loopback UDP does not work (socket layer is broken; unexpected)".into(),
            cause: vec!["the most basic local send/recv failed before any LAN probing".into()],
            fix: vec!["check the network stack / security software".into()],
            captured: None,
        }));
    }
    match &r.egress {
        Egress::Ok => Ok(Healthy {
            title: "healthy — this binary can speak zenoh over the LAN",
            detail: format!(
                "scout egress OK · peers discovered: {} · link WS {}",
                r.peers,
                if r.link_ws {
                    "running"
                } else {
                    "not started (the engine spawns it)"
                }
            ),
        }),
        Egress::PermissionBlocked(line) => {
            let mut cause = Vec::new();
            if r.ping == Some(true) {
                cause.push(
                    "ping works = the network is healthy, yet only this binary cannot send on the LAN"
                        .into(),
                );
            }
            cause.push(match r.os {
                "macos" => {
                    "most likely: no macOS \"Local Network\" permission (TCC), or an unsigned binary"
                        .into()
                }
                "windows" => "most likely: Windows Defender Firewall is blocking this binary".into(),
                _ => "most likely: a firewall or NIC configuration is blocking multicast".into(),
            });
            Err(Box::new(Failure {
                code: "doctor_permission_blocked",
                path: "probes.egress",
                verdict: "blocked",
                title: "this binary cannot send a zenoh scout to the LAN".into(),
                cause,
                fix: fix_steps(r.os),
                captured: Some(line.clone()),
            }))
        }
        Egress::NicError(line) => Err(Box::new(Failure {
            code: "doctor_nic_error",
            path: "probes.egress",
            verdict: "misconfigured",
            title: format!(
                "the specified NIC ({}) cannot be used (the scout interface was not found)",
                r.iface_desc
            ),
            cause: vec![],
            fix: vec!["point --iface at a real LAN NIC".into()],
            captured: Some(line.clone()),
        })),
        Egress::OtherError(line) => {
            let mut cause = Vec::new();
            if r.ping == Some(false) {
                cause.push(
                    "ping to the gateway also failed = suspected network/NIC-level outage".into(),
                );
            }
            Err(Box::new(Failure {
                code: "doctor_egress_error",
                path: "probes.egress",
                verdict: "error",
                title: "error during zenoh scout egress".into(),
                cause,
                fix: vec!["check the captured line below".into()],
                captured: Some(line.clone()),
            }))
        }
    }
}

pub fn render_healthy(h: &Healthy) -> String {
    format!(
        "{} {} {}",
        Status::Ok.glyph(),
        style::paint(style::OK, h.title),
        style::paint(style::META, format!("({})", h.detail))
    )
}

pub fn render_failure(f: &Failure) -> String {
    let mut out = format!(
        "{} {} — {}\n",
        Status::Bad.glyph(),
        style::paint(style::BAD.bold(), f.verdict),
        style::paint(style::HEAD, &f.title)
    );
    if !f.cause.is_empty() {
        out.push_str(&style::labeled_block("cause", style::META, &f.cause));
    }
    if !f.fix.is_empty() {
        let numbered: Vec<String> = f
            .fix
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}. {s}", i + 1))
            .collect();
        out.push_str(&style::labeled_block("fix", style::ACTION, &numbered));
    }
    if let Some(c) = &f.captured {
        out.push_str(&style::labeled_block(
            "captured",
            style::META,
            &[style::paint(style::META, c)],
        ));
    }
    out
}

/// Capture writer that accumulates zenoh WARN/ERROR logs. Rather than relying on specific strings,
/// it collects "every zenoh error emitted during the scout window" and classifies them via classify_egress.
#[derive(Clone)]
struct Capture {
    buf: Arc<Mutex<String>>,
}
struct CaptureW {
    buf: Arc<Mutex<String>>,
}
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = CaptureW;
    fn make_writer(&'a self) -> CaptureW {
        CaptureW {
            buf: self.buf.clone(),
        }
    }
}
impl std::io::Write for CaptureW {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut b) = self.buf.lock() {
            b.push_str(&String::from_utf8_lossy(buf));
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// loopback UDP send/recv reachability (socket layer sanity check).
fn probe_loopback() -> bool {
    (|| -> std::io::Result<bool> {
        let rx = UdpSocket::bind("127.0.0.1:0")?;
        rx.set_read_timeout(Some(Duration::from_millis(500)))?;
        let addr = rx.local_addr()?;
        let tx = UdpSocket::bind("127.0.0.1:0")?;
        tx.send_to(b"sahou-doctor", addr)?;
        let mut buf = [0u8; 32];
        Ok(rx.recv(&mut buf).map(|n| n > 0).unwrap_or(false))
    })()
    .unwrap_or(false)
}

/// WebSocket-handshake reachability of the link WS port (informational; not started is not a
/// rejection = the engine spawns it automatically).
///
/// This completes a real WS handshake (and closes cleanly) rather than opening a raw TCP
/// connection and dropping it mid-handshake. That matters for two reasons: (a) a dropped raw TCP
/// connection makes zenoh_plugin_remote_api log an ERROR ("Handshake not finished") on an
/// otherwise-healthy link — a diagnostic tool has no business causing that; (b) it makes the
/// check honest — a non-WS process squatting the port now fails the handshake and correctly
/// reports as not running, instead of a bare TCP connect falsely reporting "link WS running".
fn probe_link_ws(port: u16) -> bool {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(300)) else {
        return false;
    };
    // Bound the handshake and the shutdown drain below so this probe can never hang.
    let timeout = Some(Duration::from_millis(700));
    if stream.set_read_timeout(timeout).is_err() || stream.set_write_timeout(timeout).is_err() {
        return false;
    }
    let Ok((mut ws, _response)) = tungstenite::client(format!("ws://127.0.0.1:{port}/"), stream)
    else {
        return false;
    };
    // Best-effort clean shutdown: the probe already succeeded (the handshake completed), so
    // ignore every error from here on and just try not to leave the peer hanging mid-close.
    let _ = ws.close(None);
    for _ in 0..16 {
        if ws.read().is_err() {
            break;
        }
    }
    true
}

/// The default gateway IP (Windows = PowerShell Get-NetRoute / macOS = route / Linux = ip route).
fn default_gateway() -> Option<String> {
    let out = if cfg!(target_os = "windows") {
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-NetRoute -DestinationPrefix 0.0.0.0/0 | Sort-Object RouteMetric | Select-Object -First 1).NextHop",
            ])
            .output()
            .ok()?
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("route")
            .args(["-n", "get", "default"])
            .output()
            .ok()?
    } else {
        std::process::Command::new("ip")
            .args(["route", "show", "default"])
            .output()
            .ok()?
    };
    let text = String::from_utf8_lossy(&out.stdout);
    if cfg!(target_os = "windows") {
        let s = text.trim().to_string();
        (!s.is_empty() && s != "0.0.0.0").then_some(s)
    } else if cfg!(target_os = "macos") {
        text.lines().find_map(|l| {
            l.trim()
                .strip_prefix("gateway:")
                .map(|g| g.trim().to_string())
        })
    } else {
        text.split_whitespace()
            .skip_while(|w| *w != "via")
            .nth(1)
            .map(str::to_string)
    }
}

/// Confirm LAN reachability via system ping (an authorized tool). Separates network outages unrelated to TCC.
fn system_ping(target: &str) -> bool {
    let args: &[&str] = if cfg!(target_os = "windows") {
        &["-n", "1", "-w", "2000", target]
    } else if cfg!(target_os = "macos") {
        &["-c1", "-t", "2", target]
    } else {
        &["-c1", "-W", "2", target]
    };
    matches!(std::process::Command::new("ping").args(args).output(), Ok(o) if o.status.success())
}

/// Actively probe this binary's zenoh scout egress (open a multicast-enabled peer and capture logs during the window).
fn zenoh_scout_egress(iface: Option<&str>, window_secs: u64) -> (Egress, usize) {
    use zenoh::Wait;
    let buf = Arc::new(Mutex::new(String::new()));
    // Capture zenoh logs with our own subscriber (do not call init_log_from_env_or; install before open)
    let _ = tracing_subscriber::fmt()
        .with_writer(Capture { buf: buf.clone() })
        .with_max_level(tracing::Level::WARN)
        .with_ansi(false)
        .try_init();
    let mut config = zenoh::Config::default();
    let mut ins = |k: &str, v: String| {
        config
            .insert_json5(k, &v)
            .map_err(|e| format!("failed to set config {k}: {e}"))
    };
    if let Err(e) = ins("mode", "\"peer\"".into())
        .and_then(|_| ins("scouting/multicast/enabled", "true".into()))
    {
        return (Egress::OtherError(e), 0);
    }
    if let Some(i) = iface {
        if let Err(e) = ins("scouting/multicast/interface", format!("\"{i}\"")) {
            return (Egress::OtherError(e), 0);
        }
    }
    let session = match zenoh::open(config).wait() {
        Ok(s) => s,
        Err(e) => return (Egress::OtherError(format!("zenoh open failed: {e}")), 0),
    };
    std::thread::sleep(Duration::from_secs(window_secs)); // window for periodic scout emission
    let peers = session.info().peers_zid().wait().count();
    let _ = session.close().wait();
    let log = buf.lock().map(|b| b.clone()).unwrap_or_default();
    (classify_egress(&log), peers)
}

fn report_line(name: &str, status: Status, note: &str) {
    anstream::println!("  {} {:<12} {}", status.glyph(), name, note);
}

pub fn run(args: DoctorArgs) -> Result<(), Vec<Diag>> {
    anstream::println!(
        "{}",
        style::heading(
            "sahou doctor · environment preflight",
            &format!(
                "· os {} · iface {}",
                std::env::consts::OS,
                args.iface.as_deref().unwrap_or("auto")
            ),
        )
    );
    anstream::println!();
    let loopback = probe_loopback();
    report_line(
        "loopback UDP",
        if loopback { Status::Ok } else { Status::Bad },
        "socket layer sanity",
    );
    let gw = default_gateway();
    let ping = match &gw {
        Some(g) => {
            let ok = system_ping(g);
            report_line(
                "LAN ping",
                if ok { Status::Ok } else { Status::Bad },
                &format!("gateway {g}"),
            );
            Some(ok)
        }
        None => {
            report_line(
                "LAN ping",
                Status::Skip,
                "default gateway unknown (skipped)",
            );
            None
        }
    };
    let (egress, peers) = zenoh_scout_egress(args.iface.as_deref(), args.scout_secs);
    let scout_ok = matches!(egress, Egress::Ok);
    report_line(
        "zenoh scout",
        if scout_ok { Status::Ok } else { Status::Bad },
        &format!(
            "LAN egress {} · peers discovered: {peers}",
            if scout_ok { "OK" } else { "failed" }
        ),
    );
    let link_ws = probe_link_ws(args.link_port);
    report_line(
        "link WS",
        if link_ws { Status::Ok } else { Status::Skip },
        &if link_ws {
            format!(":{} running", args.link_port)
        } else {
            format!(
                ":{} not running {}",
                args.link_port,
                style::paint(style::META, "(informational — the engine spawns it)")
            )
        },
    );
    let r = ProbeReport {
        loopback,
        ping,
        egress,
        peers,
        link_ws,
        os: std::env::consts::OS,
        iface_desc: args.iface.clone().unwrap_or_else(|| "auto".into()),
    };
    let local = diagnose(&r);
    anstream::println!();
    match &local {
        Ok(h) => anstream::println!("{}", render_healthy(h)),
        Err(f) => anstream::print!("{}", render_failure(f)),
    }
    let to_diags = |f: Box<Failure>| vec![Diag::new(f.code, f.path, f.title)];
    if args.lan {
        let local_ok = local.is_ok();
        let lan = crate::doctor_lan::run_lan(&args, local_ok);
        // the local diagnosis stays authoritative for its own failures; LAN failures also exit 1
        return match (local, lan) {
            (Err(f), _) => Err(to_diags(f)),
            (Ok(_), r) => r,
        };
    }
    local.map(|_| ()).map_err(to_diags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;
    use std::time::Instant;

    fn plain(s: &str) -> String {
        anstream::adapter::strip_str(s).to_string()
    }

    #[test]
    fn probe_link_ws_true_against_a_real_ws_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                if let Ok(mut ws) = tungstenite::accept(stream) {
                    // Drain until the client's close completes (or the connection drops).
                    while ws.read().is_ok() {}
                }
            }
        });
        assert!(probe_link_ws(port));
        server.join().unwrap();
    }

    #[test]
    fn probe_link_ws_false_against_a_silent_tcp_listener_and_returns_within_timeouts() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            // Accept the connection but never speak — the probe must give up on its own
            // via its read/write timeouts rather than hang.
            if let Ok((stream, _)) = listener.accept() {
                thread::sleep(Duration::from_secs(2));
                drop(stream);
            }
        });
        let start = Instant::now();
        assert!(!probe_link_ws(port));
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "probe must return before the silent peer gives up, took {:?}",
            start.elapsed()
        );
        server.join().unwrap();
    }

    #[test]
    fn probe_link_ws_false_on_closed_port() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener); // free the port; nothing is listening on it anymore
        assert!(!probe_link_ws(port));
    }

    // Verbatim log captured on real hardware in 013-1 (scout egress failure when launching unsigned Rust over ssh; ground truth).
    const TCC_LOG: &str = "2026-07-08T00:00:00Z ERROR zenoh::net::runtime::orchestrator: \
        Unable to send Scout ZScout on interface 192.168.1.10: No route to host (os error 65)";

    fn report(egress: Egress, os: &'static str) -> ProbeReport {
        ProbeReport {
            loopback: true,
            ping: Some(true),
            egress,
            peers: 0,
            link_ws: false,
            os,
            iface_desc: "auto".into(),
        }
    }

    #[test]
    fn tcc_signature_is_permission_blocked() {
        assert!(matches!(
            classify_egress(TCC_LOG),
            Egress::PermissionBlocked(_)
        ));
        assert!(matches!(
            classify_egress("ERROR foo: connect failed (os error 65)"),
            Egress::PermissionBlocked(_)
        ));
    }

    #[test]
    fn missing_interface_is_nic_error() {
        let log = "ERROR zenoh::net::runtime::orchestrator: Unable to find interface nonexist99";
        assert!(matches!(classify_egress(log), Egress::NicError(_)));
    }

    #[test]
    fn clean_log_is_ok_and_generic_error_is_other() {
        assert!(matches!(classify_egress(""), Egress::Ok));
        assert!(matches!(
            classify_egress("INFO zenoh: session opened"),
            Egress::Ok
        ));
        assert!(matches!(
            classify_egress("ERROR zenoh::foo: something unexpected"),
            Egress::OtherError(_)
        ));
    }

    #[test]
    fn healthy_when_egress_ok() {
        let h = diagnose(&report(Egress::Ok, "windows")).unwrap();
        assert!(h.title.contains("healthy"), "{}", h.title);
        assert!(h.detail.contains("peers discovered: 0"), "{}", h.detail);
    }

    #[test]
    fn loopback_failure_is_terminal_no() {
        let mut r = report(Egress::Ok, "windows");
        r.loopback = false;
        let f = diagnose(&r).unwrap_err();
        assert_eq!(f.code, "doctor_loopback");
        assert_eq!(f.path, "probes.loopback");
    }

    #[test]
    fn permission_blocked_fixes_are_cheapest_first_and_os_specific() {
        let mac = diagnose(&report(Egress::PermissionBlocked("line".into()), "macos")).unwrap_err();
        assert_eq!(mac.code, "doctor_permission_blocked");
        assert_eq!(mac.verdict, "blocked");
        // cheapest remedy first: settings toggle, then launch context, then signing
        assert!(mac.fix[0].contains("System Settings"), "{:?}", mac.fix);
        assert!(mac.fix[1].contains("Terminal"), "{:?}", mac.fix);
        assert!(mac.fix[2].contains("Developer ID"), "{:?}", mac.fix);
        assert!(
            mac.fix.iter().any(|s| s.contains("Local Network")),
            "{:?}",
            mac.fix
        );
        // ping-OK triangulation lands in cause
        assert!(
            mac.cause
                .iter()
                .any(|s| s.contains("the network is healthy")),
            "{:?}",
            mac.cause
        );
        assert_eq!(mac.captured.as_deref(), Some("line"));
        let win =
            diagnose(&report(Egress::PermissionBlocked("line".into()), "windows")).unwrap_err();
        assert!(
            win.fix.iter().any(|s| s.contains("Firewall")),
            "{:?}",
            win.fix
        );
    }

    #[test]
    fn nic_error_points_to_iface_fix() {
        let f = diagnose(&report(Egress::NicError("line".into()), "windows")).unwrap_err();
        assert_eq!(f.code, "doctor_nic_error");
        assert!(f.fix.iter().any(|s| s.contains("--iface")), "{:?}", f.fix);
    }

    #[test]
    fn other_error_mentions_network_break_when_ping_fails() {
        let mut r = report(Egress::OtherError("line".into()), "windows");
        r.ping = Some(false);
        let f = diagnose(&r).unwrap_err();
        assert_eq!(f.code, "doctor_egress_error");
        assert!(
            f.cause.iter().any(|s| s.contains("network/NIC")),
            "{:?}",
            f.cause
        );
    }

    #[test]
    fn render_failure_orders_cause_fix_captured() {
        let f = diagnose(&report(
            Egress::PermissionBlocked("ERR line".into()),
            "macos",
        ))
        .unwrap_err();
        let p = plain(&render_failure(&f));
        assert!(p.starts_with("✗ blocked — "), "{p}");
        assert!(p.contains("  fix       1. "), "{p}");
        let (c, x, cap) = (
            p.find("cause").unwrap(),
            p.find("fix").unwrap(),
            p.find("captured").unwrap(),
        );
        assert!(c < x && x < cap, "{p}");
        assert!(p.contains("ERR line"), "{p}");
    }

    #[test]
    fn render_healthy_is_one_green_line_with_dim_detail() {
        let h = diagnose(&report(Egress::Ok, "macos")).unwrap();
        let p = plain(&render_healthy(&h));
        assert!(p.starts_with("✓ healthy — "), "{p}");
        assert!(p.contains("peers discovered: 0"), "{p}");
    }
}
