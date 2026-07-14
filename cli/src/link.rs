//! sahou link — the keystone of the zero-IP auto-mesh (design §7; the production version of R&D 013-1).
//! (a) a native peer that actively scouts (a relay; the Z18 solution to bridges not connecting to each other)
//! (b) a WS entrypoint for Node/browser (embedded remote-api plugin; the Task 3 GO shape)
//! Behavior: a duplicate launch shares the existing one / auto-exits after grace with 0 WS clients / startup timeout.

use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use clap::Args;
use sahou_core::diag::Diag;

#[derive(Args)]
pub struct LinkArgs {
    /// remote-api WebSocket port (the entrypoint for Node/browser)
    #[arg(long, default_value_t = 10000)]
    pub port: u16,
    /// Listen port for the native peer (the target for explicit connects from Python etc.)
    #[arg(long, default_value_t = 7448)]
    pub peer_listen: u16,
    /// Pin the NIC for multicast scouting (guards against scouts getting lost on multi-NIC hosts)
    #[arg(long)]
    pub iface: Option<String>,
    /// Explicit connect endpoint (e.g. tcp/192.168.1.10:7448)
    #[arg(long)]
    pub connect: Option<String>,
    /// Auto-exit after this many seconds with 0 WS clients (prevents orphaning)
    #[arg(long, default_value_t = 10)]
    pub grace: u64,
    /// Exit as a misfire if no first client arrives within this many seconds
    #[arg(long, default_value_t = 30)]
    pub startup: u64,
    /// Disable multicast scouting (for tests/CI and explicit-connect operation)
    #[arg(long)]
    pub no_multicast: bool,
}

/// State machine for idle monitoring (pure function, testable). Driven by the WS client count every tick seconds.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct IdleState {
    /// Whether any client has ever arrived
    pub armed: bool,
    /// Seconds of continuous 0 clients after being armed
    pub idle_secs: u64,
    /// Seconds elapsed since startup
    pub up_secs: u64,
}

#[derive(Debug, PartialEq)]
pub enum LinkAction {
    Continue,
    Exit(&'static str),
}

pub fn idle_step(
    s: IdleState,
    clients: usize,
    tick: u64,
    grace: u64,
    startup: u64,
) -> (IdleState, LinkAction) {
    let up = s.up_secs + tick;
    if clients > 0 {
        let st = IdleState {
            armed: true,
            idle_secs: 0,
            up_secs: up,
        };
        return (st, LinkAction::Continue);
    }
    if s.armed {
        let idle = s.idle_secs + tick;
        let st = IdleState {
            armed: true,
            idle_secs: idle,
            up_secs: up,
        };
        if idle >= grace {
            (st, LinkAction::Exit("idle"))
        } else {
            (st, LinkAction::Continue)
        }
    } else {
        let st = IdleState {
            armed: false,
            idle_secs: 0,
            up_secs: up,
        };
        if up >= startup {
            (st, LinkAction::Exit("startup timeout"))
        } else {
            (st, LinkAction::Continue)
        }
    }
}

fn link_err(msg: String) -> Vec<Diag> {
    vec![Diag::new("link_error", "$", msg)]
}

pub fn run(args: LinkArgs) -> Result<(), Vec<Diag>> {
    // Prevent duplicate launches: if the WS port accepts a connection, share the existing link and exit cleanly.
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, args.port));
    if TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok() {
        println!(
            "[link] :{} is already running -> sharing the existing one",
            args.port
        );
        return Ok(());
    }
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| link_err(format!("failed to start the tokio runtime: {e}")))?;
    rt.block_on(run_async(args))
}

/// The list of link's zenoh config (key, json5 value) entries (pure function, testable).
/// websocket_port must be in host:port form (a bare port falls back to an IPv6-only bind,
/// so on Windows a 127.0.0.1 (IPv4) client gets ECONNREFUSED; demonstrated in ②b Task 3).
pub fn config_entries(args: &LinkArgs) -> Vec<(String, String)> {
    let mut v = vec![
        ("mode".to_string(), "\"peer\"".to_string()),
        (
            "listen/endpoints".to_string(),
            format!("[\"tcp/[::]:{}\"]", args.peer_listen),
        ),
        (
            "plugins/remote_api/websocket_port".to_string(),
            format!("\"127.0.0.1:{}\"", args.port),
        ),
    ];
    if args.no_multicast {
        v.push(("scouting/multicast/enabled".into(), "false".into()));
    }
    if let Some(iface) = &args.iface {
        v.push((
            "scouting/multicast/interface".into(),
            format!("\"{iface}\""),
        ));
    }
    if let Some(ep) = &args.connect {
        v.push(("connect/endpoints".into(), format!("[\"{ep}\"]")));
    }
    v
}

/// A cause hint for zenoh runtime startup failure (pure function). Turns a duplicate-launch race
/// (another process grabbed the port after the pre-launch probe) into a rejection "at the right place".
pub fn start_error_hint(msg: &str, port: u16, peer_listen: u16) -> String {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("10048") || lower.contains("in use") || lower.contains("addrinuse") {
        format!(
            "{msg} — port :{port}(WS) / :{peer_listen}(peer) is in use. \
             Either a race where another process grabbed it after the launch probe, or occupancy by a non-link process. \
             Check the existing process, or change --port / --peer-listen"
        )
    } else {
        msg.to_string()
    }
}

fn build_config(args: &LinkArgs) -> Result<zenoh::Config, Vec<Diag>> {
    let mut config = zenoh::Config::default();
    for (k, v) in config_entries(args) {
        config
            .insert_json5(&k, &v)
            .map_err(|e| link_err(format!("failed to set config {k}: {e}")))?;
    }
    Ok(config)
}

async fn start_runtime(
    config: zenoh::Config,
    port: u16,
    peer_listen: u16,
) -> Result<zenoh::internal::runtime::Runtime, Vec<Diag>> {
    // Statically embed the remote-api plugin (the ②b Task 3 GO shape)
    let mut plugins = zenoh::internal::plugins::PluginsManager::static_plugins_only();
    plugins.declare_static_plugin::<zenoh_plugin_remote_api::RemoteApiPlugin, &str>(
        "remote_api",
        true,
    );
    let mut runtime = zenoh::internal::runtime::RuntimeBuilder::new(config)
        .plugins_manager(plugins)
        .build()
        .await
        .map_err(|e| {
            link_err(start_error_hint(
                &format!("failed to build the zenoh runtime: {e}"),
                port,
                peer_listen,
            ))
        })?;
    runtime.start().await.map_err(|e| {
        link_err(start_error_hint(
            &format!("failed to start the zenoh runtime: {e}"),
            port,
            peer_listen,
        ))
    })?;
    Ok(runtime)
}

async fn idle_loop(port: u16, grace: u64, startup: u64) -> &'static str {
    const TICK: u64 = 2;
    let mut st = IdleState::default();
    loop {
        tokio::time::sleep(Duration::from_secs(TICK)).await;
        let clients = count_ws_clients(port);
        let (next, action) = idle_step(st, clients, TICK, grace, startup);
        st = next;
        if let LinkAction::Exit(reason) = action {
            return reason;
        }
    }
}

async fn run_async(args: LinkArgs) -> Result<(), Vec<Diag>> {
    zenoh::init_log_from_env_or("error");
    let config = build_config(&args)?;
    let runtime = start_runtime(config, args.port, args.peer_listen).await?;

    // A native session on the same runtime = a relay that actively scouts (Z18: subscribing to `**` as a safeguard to join the relay path)
    let session = zenoh::session::init(runtime.clone().into())
        .await
        .map_err(|e| link_err(format!("failed to initialize the relay session: {e}")))?;
    session
        .declare_subscriber("**")
        .callback(|_| {})
        .background()
        .await
        .map_err(|e| link_err(format!("failed to declare the relay subscriber: {e}")))?;

    println!(
        "[link] up (ws={} peer=tcp/[::]:{} grace={}s startup={}s multicast={})",
        args.port, args.peer_listen, args.grace, args.startup, !args.no_multicast
    );
    let reason = idle_loop(args.port, args.grace, args.startup).await;
    println!("[link] exit ({reason})");
    Ok(()) // the runtime/session are torn down on drop
}

/// The number of ESTABLISHED connections on the WS port (= connected clients). Cross-platform (netstat2).
fn count_ws_clients(port: u16) -> usize {
    use netstat2::{
        get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState,
    };
    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    match get_sockets_info(af, ProtocolFlags::TCP) {
        Ok(list) => list
            .iter()
            .filter(|si| match &si.protocol_socket_info {
                ProtocolSocketInfo::Tcp(t) => {
                    t.local_port == port && t.state == TcpState::Established
                }
                _ => false,
            })
            .count(),
        Err(_) => 0, // treat a query failure as 0 = the safe side that exits after grace (does not silently keep running)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_default() -> LinkArgs {
        LinkArgs {
            port: 10000,
            peer_listen: 7448,
            iface: None,
            connect: None,
            grace: 10,
            startup: 30,
            no_multicast: false,
        }
    }

    #[test]
    fn config_entries_pins_websocket_port_to_host_port_form() {
        // gotcha (demonstrated in ②b Task 3): a bare port results in an IPv6-only bind,
        // so on Windows a 127.0.0.1 client gets ECONNREFUSED. Pin the host:port form in a test.
        let entries = config_entries(&args_default());
        assert!(entries.contains(&(
            "plugins/remote_api/websocket_port".into(),
            "\"127.0.0.1:10000\"".into()
        )));
        assert!(entries.contains(&("mode".into(), "\"peer\"".into())));
        assert!(entries.contains(&("listen/endpoints".into(), "[\"tcp/[::]:7448\"]".into())));
        // By default, the multicast-disable / iface / connect entries are not emitted
        assert!(entries
            .iter()
            .all(|(k, _)| k != "scouting/multicast/enabled"));
        assert!(entries.iter().all(|(k, _)| k != "connect/endpoints"));
    }

    #[test]
    fn config_entries_reflects_optional_flags() {
        let mut a = args_default();
        a.no_multicast = true;
        a.iface = Some("Ethernet".into());
        a.connect = Some("tcp/192.168.1.10:7448".into());
        let entries = config_entries(&a);
        assert!(entries.contains(&("scouting/multicast/enabled".into(), "false".into())));
        assert!(entries.contains(&("scouting/multicast/interface".into(), "\"Ethernet\"".into())));
        assert!(entries.contains(&(
            "connect/endpoints".into(),
            "[\"tcp/192.168.1.10:7448\"]".into()
        )));
    }

    #[test]
    fn start_error_hint_classifies_port_in_use() {
        // Turn a duplicate-launch race (another process grabbed the port after the probe) into a rejection with a remedy
        for msg in [
            "failed to start the zenoh runtime: os error 10048", // Windows WSAEADDRINUSE
            "failed to start the zenoh runtime: Address already in use", // unix
            "failed to start the zenoh runtime: AddrInUse",      // io::ErrorKind spelling
        ] {
            let hint = start_error_hint(msg, 10000, 7448);
            assert!(hint.contains(":10000"), "{hint}");
            assert!(hint.contains("in use"), "{hint}");
        }
        // An unrelated error is passed through unchanged (does not mislead)
        let other = start_error_hint("failed to build the zenoh runtime: bad config", 10000, 7448);
        assert_eq!(other, "failed to build the zenoh runtime: bad config");
    }

    fn drive(
        mut st: IdleState,
        seq: &[usize],
        grace: u64,
        startup: u64,
    ) -> (IdleState, Option<&'static str>) {
        for &clients in seq {
            let (next, action) = idle_step(st, clients, 2, grace, startup);
            st = next;
            if let LinkAction::Exit(reason) = action {
                return (st, Some(reason));
            }
        }
        (st, None)
    }

    #[test]
    fn no_client_exits_at_startup_timeout() {
        // 30 seconds = tick(2s) × 15 gives a misfire exit
        let (_, reason) = drive(IdleState::default(), &[0; 15], 10, 30);
        assert_eq!(reason, Some("startup timeout"));
        // does not fire at 14
        let (_, reason) = drive(IdleState::default(), &[0; 14], 10, 30);
        assert_eq!(reason, None);
    }

    #[test]
    fn client_arms_then_idle_grace_exits() {
        // A client arrives -> armed. Everyone leaves and grace(10s)=5 ticks gives an idle exit.
        let (_, reason) = drive(IdleState::default(), &[0, 1, 1, 0, 0, 0, 0, 0], 10, 30);
        assert_eq!(reason, Some("idle"));
    }

    #[test]
    fn client_return_resets_idle_countdown() {
        // Leaves, then returns on the 3rd tick -> the countdown is reset and it does not exit
        let (st, reason) = drive(IdleState::default(), &[1, 0, 0, 1, 0, 0], 10, 30);
        assert_eq!(reason, None);
        assert!(st.armed);
        assert_eq!(st.idle_secs, 4); // only the most recent 2 ticks
    }

    #[test]
    fn armed_never_falls_back_to_startup_timeout() {
        // Once armed, even if clients=0 continues past startup, it does not exit via startup timeout
        // (only the idle check applies). Make grace large enough to exclude an idle exit, and
        // directly verify that the armed branch does not fall back to the startup branch.
        let mut seq = vec![1usize]; // arm it first
        seq.extend([0; 20]); // keep 0 clients for 20 ticks = 40s (past the 30s startup)
        let (st, reason) = drive(IdleState::default(), &seq, 1000, 30); // grace=1000 excludes an idle exit
        assert_eq!(
            reason, None,
            "after being armed, it does not exit via startup timeout even when up_secs>startup"
        );
        assert!(st.armed);
        assert!(
            st.up_secs > 30,
            "up_secs is past startup (yet it has not exited)"
        );
    }
}
