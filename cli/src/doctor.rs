//! sahou doctor — environment preflight diagnostics (design §7; the Rust production version of R&D 013-4, re-derived rather than copied).
//! Core idea: actively probe something a raw socket probe cannot detect — "can this binary's real zenoh scout reach the LAN?" —
//! and return a human-readable rejection + remedy classified by cause
//! (PermissionBlocked(TCC) / NicError / OtherError / Ok), rejecting at the right place before it becomes a silent "0 messages".

use std::net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

fn permission_remedy(os: &str) -> &'static str {
    match os {
        "macos" => "Most likely cause: no macOS \"Local Network\" permission (TCC), or an unsigned binary. Remedy: (a) use a distribution signed with a Developer ID + the multicast entitlement (b) allow it in System Settings > Privacy & Security > Local Network (c) launch from Terminal (an authorized context) rather than ssh/headless",
        "windows" => "Allow sahou.exe inbound/outbound (UDP 7446 multicast / TCP 7447) in Windows Defender Firewall",
        _ => "Check the firewall (UDP 7446 multicast / TCP 7447) and NIC settings",
    }
}

/// Assemble the diagnosis (pure function). Ok(healthy summary) / Err(cause-specific rejection + remedy).
/// The rejection uses the consistent {code, path, message} shape, so main's print_diags renders it as-is.
pub fn diagnose(r: &ProbeReport) -> Result<String, Vec<Diag>> {
    if !r.loopback {
        return Err(vec![Diag::new(
            "doctor_loopback",
            "probes.loopback",
            "loopback UDP does not work (socket layer is broken; unexpected). Check the network stack / security software",
        )]);
    }
    match &r.egress {
        Egress::Ok => {
            let link = if r.link_ws {
                "link WS running".to_string()
            } else {
                "link WS not started (not required, since the engine spawns it automatically)".to_string()
            };
            Ok(format!(
                "healthy — this binary can speak zenoh over the LAN (scout egress OK / discovered peers={} / {link})",
                r.peers
            ))
        }
        Egress::PermissionBlocked(line) => {
            let net = match r.ping {
                Some(true) => "ping works = the network is healthy, yet only this binary cannot send on the LAN. ",
                _ => "",
            };
            Err(vec![Diag::new(
                "doctor_permission_blocked",
                "probes.egress",
                format!(
                    "this binary cannot send a zenoh scout to the LAN. {net}{}. [captured] {line}",
                    permission_remedy(r.os)
                ),
            )])
        }
        Egress::NicError(line) => Err(vec![Diag::new(
            "doctor_nic_error",
            "probes.egress",
            format!(
                "the specified NIC ({}) cannot be used (the scout interface was not found). Point --iface at a real LAN NIC. [captured] {line}",
                r.iface_desc
            ),
        )]),
        Egress::OtherError(line) => {
            let net = match r.ping {
                Some(false) => "ping to the gateway also failed = suspected network/NIC-level outage. ",
                _ => "",
            };
            Err(vec![Diag::new(
                "doctor_egress_error",
                "probes.egress",
                format!("error during zenoh scout egress. {net}Check the causing line. [captured] {line}"),
            )])
        }
    }
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

fn report_line(name: &str, ok: Option<bool>, note: &str) {
    let mark = match ok {
        Some(true) => "OK",
        Some(false) => "NG",
        None => "--",
    };
    println!("  [{mark}] {name:<20} {note}");
}

pub fn run(args: DoctorArgs) -> Result<(), Vec<Diag>> {
    println!(
        "sahou doctor — environment preflight diagnostics (os={} iface={})",
        std::env::consts::OS,
        args.iface.as_deref().unwrap_or("auto")
    );
    let loopback = probe_loopback();
    report_line("loopback UDP", Some(loopback), "socket layer sanity");
    let gw = default_gateway();
    let ping = match &gw {
        Some(g) => {
            let ok = system_ping(g);
            report_line("LAN reachability (ping)", Some(ok), &format!("gateway={g}"));
            Some(ok)
        }
        None => {
            report_line(
                "LAN reachability (ping)",
                None,
                "default gateway unknown (skipped)",
            );
            None
        }
    };
    let (egress, peers) = zenoh_scout_egress(args.iface.as_deref(), args.scout_secs);
    report_line(
        "zenoh scout egress",
        Some(matches!(egress, Egress::Ok)),
        &format!("this binary's LAN egress / discovered peers={peers}"),
    );
    let link_ws = probe_link_ws(args.link_port);
    report_line(
        "link WS",
        Some(link_ws),
        &format!(
            ":{} (informational; the engine spawns it automatically even if not started)",
            args.link_port
        ),
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
    if let Ok(summary) = &local {
        println!("\n{summary}");
    }
    if args.lan {
        let local_ok = local.is_ok();
        let lan = crate::doctor_lan::run_lan(&args, local_ok);
        // the local diagnosis stays authoritative for its own failures; LAN failures also exit 1
        return match (local, lan) {
            (Err(d), _) => Err(d),
            (Ok(_), r) => r,
        };
    }
    local.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;
    use std::time::Instant;

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
        let summary = diagnose(&report(Egress::Ok, "windows")).unwrap();
        assert!(summary.contains("healthy"));
    }

    #[test]
    fn loopback_failure_is_terminal_no() {
        let mut r = report(Egress::Ok, "windows");
        r.loopback = false;
        let diags = diagnose(&r).unwrap_err();
        assert_eq!(diags[0].code, "doctor_loopback");
    }

    #[test]
    fn permission_blocked_has_os_specific_remedy() {
        let mac = diagnose(&report(Egress::PermissionBlocked("line".into()), "macos")).unwrap_err();
        assert_eq!(mac[0].code, "doctor_permission_blocked");
        assert!(
            mac[0].message.contains("Local Network"),
            "{}",
            mac[0].message
        );
        assert!(
            mac[0].message.contains("the network is healthy"),
            "ping-OK triangulation wording: {}",
            mac[0].message
        );
        let win =
            diagnose(&report(Egress::PermissionBlocked("line".into()), "windows")).unwrap_err();
        assert!(win[0].message.contains("Firewall"), "{}", win[0].message);
    }

    #[test]
    fn nic_error_points_to_iface_fix() {
        let diags = diagnose(&report(Egress::NicError("line".into()), "windows")).unwrap_err();
        assert_eq!(diags[0].code, "doctor_nic_error");
        assert!(diags[0].message.contains("--iface"), "{}", diags[0].message);
    }

    #[test]
    fn other_error_mentions_network_break_when_ping_fails() {
        let mut r = report(Egress::OtherError("line".into()), "windows");
        r.ping = Some(false);
        let diags = diagnose(&r).unwrap_err();
        assert_eq!(diags[0].code, "doctor_egress_error");
        assert!(
            diags[0].message.contains("network/NIC"),
            "{}",
            diags[0].message
        );
    }
}
