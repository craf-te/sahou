//! doctor --lan integration: a fake node (token + vitals queryable) on a real loopback session;
//! the roll call must mark it OK, mark the others NG, flag duplicates, and exit 1 on missing nodes.
use std::sync::Mutex;
use zenoh::Wait;

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");
const INFO: &str = r#"{"lang":"rust","sahou":"0.0.2","transport":"native"}"#;

/// `sahou doctor --lan` always opens its own zenoh peer session with multicast scouting
/// ENABLED (only --connect is pinned), independent of any test fixture's settings. cargo test
/// runs the `#[test]` fns in this file concurrently by default, and concurrently-running
/// `doctor` subprocesses share the same host's multicast group — so a test expecting silence
/// (e.g. the dark-mesh test) can spuriously discover an unrelated test's fake node and ride
/// through its --connect bridge. Serialize the tests in this file so their real zenoh sessions
/// never overlap.
static NETWORK_TEST_LOCK: Mutex<()> = Mutex::new(());

fn serial_guard() -> std::sync::MutexGuard<'static, ()> {
    NETWORK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn gen_descriptor(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let c = sahou_core::parse::parse_contract(DEMO).unwrap();
    let json = sahou_core::ir::descriptor_json(&c, &sahou_core::endpoints::Endpoints::default());
    let p = dir.path().join("descriptor.json");
    std::fs::write(&p, json).unwrap();
    p
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn listen_session(port: u16) -> zenoh::Session {
    let mut conf = zenoh::Config::default();
    conf.insert_json5("listen/endpoints", &format!("[\"tcp/127.0.0.1:{port}\"]"))
        .unwrap();
    conf.insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    zenoh::open(conf).wait().unwrap()
}

#[test]
fn doctor_lan_roll_call_reports_present_missing_and_duplicates() {
    let _guard = serial_guard();
    let dir = tempfile::tempdir().unwrap();
    let desc_path = gen_descriptor(&dir);
    let desc = sahou_core::runtime::load_descriptor(&std::fs::read_to_string(&desc_path).unwrap())
        .unwrap();
    let port = free_port();
    let session = listen_session(port);

    // fake node "sensor": liveliness token + TWO vitals queryables (double-start simulation)
    let vkey = sahou_core::vitals::vitals_key(&desc, "sensor");
    let payload = sahou_core::vitals::vitals_payload(&desc, "sensor", INFO).unwrap();
    let _token = session.liveliness().declare_token(&vkey).wait().unwrap();
    let mk_q = |payload: String, vkey: String| {
        move |query: zenoh::query::Query| {
            let _ = query.reply(vkey.clone(), payload.clone()).wait();
        }
    };
    let _q1 = session
        .declare_queryable(&vkey)
        .callback(mk_q(payload.clone(), vkey.clone()))
        .wait()
        .unwrap();
    // second instance joins the same mesh via the first port
    let mut conf = zenoh::Config::default();
    conf.insert_json5("connect/endpoints", &format!("[\"tcp/127.0.0.1:{port}\"]"))
        .unwrap();
    conf.insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    let session3 = zenoh::open(conf).wait().unwrap();
    let _q2 = session3
        .declare_queryable(&vkey)
        .callback(mk_q(payload.clone(), vkey.clone()))
        .wait()
        .unwrap();

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_sahou"))
        .args([
            "doctor",
            "--lan",
            "--descriptor",
            desc_path.to_str().unwrap(),
            "--connect",
            &format!("tcp/127.0.0.1:{port}"),
            "--lan-secs",
            "8",
            "--scout-secs",
            "1",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("[OK] sensor"),
        "sensor should be present:\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("generation=match"),
        "generation should match:\n{stdout}"
    );
    assert!(
        stdout.contains("instances answered vitals"),
        "duplicate note missing:\n{stdout}"
    );
    assert!(
        stdout.contains("[NG] visuals") && stdout.contains("[NG] archive"),
        "missing nodes should be NG:\n{stdout}"
    );
    assert!(
        stdout.contains("this binary's vantage"),
        "vantage line missing:\n{stdout}"
    );
    assert!(
        !out.status.success(),
        "missing nodes must exit 1:\n{stdout}"
    );
    // the fake node is multicast-invisible (scouting disabled), so the multicast-health
    // pass must warn that the roll call depended on the explicit endpoint (IGMP class)
    assert!(
        stdout.contains("only via the explicit endpoint"),
        "multicast-health warning missing:\n{stdout}"
    );
    assert!(
        stdout.contains("multicast-only filtering"),
        "IGMP classification missing:\n{stdout}"
    );
    session.close().wait().unwrap();
    session3.close().wait().unwrap();
}

#[test]
fn doctor_lan_without_descriptor_lists_discovered_nodes() {
    let _guard = serial_guard();
    let dir = tempfile::tempdir().unwrap(); // empty cwd -> no descriptor -> discovery-only
    let desc_dir = tempfile::tempdir().unwrap();
    let desc_path = gen_descriptor(&desc_dir);
    let desc = sahou_core::runtime::load_descriptor(&std::fs::read_to_string(&desc_path).unwrap())
        .unwrap();
    let port = free_port();
    let session = listen_session(port);
    let vkey = sahou_core::vitals::vitals_key(&desc, "sensor");
    let payload = sahou_core::vitals::vitals_payload(&desc, "sensor", INFO).unwrap();
    let _token = session.liveliness().declare_token(&vkey).wait().unwrap();
    let vk2 = vkey.clone();
    let _q = session
        .declare_queryable(&vkey)
        .callback(move |query: zenoh::query::Query| {
            let _ = query.reply(vk2.clone(), payload.clone()).wait();
        })
        .wait()
        .unwrap();

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_sahou"))
        .current_dir(dir.path())
        .args([
            "doctor",
            "--lan",
            "--connect",
            &format!("tcp/127.0.0.1:{port}"),
            "--lan-secs",
            "8",
            "--scout-secs",
            "1",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("[OK] sensor"),
        "discovered node missing:\n{stdout}"
    );
    assert!(
        stdout.contains("namespace sahou"),
        "namespace grouping missing:\n{stdout}"
    );
    assert!(
        stdout.contains("--descriptor"),
        "the missing-descriptor hint should be printed:\n{stdout}"
    );
    assert!(
        out.status.success(),
        "discovery-only mode always exits 0:\n{stdout}"
    );
    session.close().wait().unwrap();
}

#[test]
fn doctor_lan_dark_mesh_prints_suspicion_ranked_probe_guidance() {
    let _guard = serial_guard();
    let dir = tempfile::tempdir().unwrap();
    let desc_path = gen_descriptor(&dir);
    // a --connect endpoint that accepts TCP but serves no sahou (a plain listen session)
    let port = free_port();
    let session = listen_session(port);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_sahou"))
        .args([
            "doctor",
            "--lan",
            "--descriptor",
            desc_path.to_str().unwrap(),
            "--connect",
            &format!("tcp/127.0.0.1:{port}"),
            "--lan-secs",
            "2",
            "--scout-secs",
            "1",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    // nobody answered on either path -> the "both dark" classification, and exit 1 (missing nodes)
    assert!(
        stdout.contains("remote machine"),
        "both-dark classification missing:\n{stdout}"
    );
    assert!(!out.status.success());
    session.close().wait().unwrap();
}
