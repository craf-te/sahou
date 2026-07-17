//! sahou tap integration test. Over a real zenoh loopback (multicast disabled, explicit connect),
//! verifies that "inject" really arrives and that attachment = connection hash / payload passes the core receive boundary.
use std::time::Duration;

use zenoh::Wait;

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

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
fn tap_send_sample_reaches_subscriber_with_hash_attachment() {
    let dir = tempfile::tempdir().unwrap();
    let desc_path = gen_descriptor(&dir);
    let desc = sahou_core::runtime::load_descriptor(&std::fs::read_to_string(&desc_path).unwrap())
        .unwrap();
    let key = desc.connections["touch"].key.clone(); // the resolved keyexpr (not hardcoded)
    let port = free_port();
    let session = listen_session(port);
    let (tx, rx) = std::sync::mpsc::channel::<(Vec<u8>, Option<Vec<u8>>)>();
    let _sub = session
        .declare_subscriber(&key)
        .callback(move |s: zenoh::sample::Sample| {
            let wire = s.payload().to_bytes().to_vec();
            let att = s.attachment().map(|a| a.to_bytes().to_vec());
            let _ = tx.send((wire, att));
        })
        .wait()
        .unwrap();

    // Retry tap --send a few times until it arrives (absorbs route-convergence jitter right after the peer connects)
    let mut received = None;
    for _ in 0..10 {
        assert_cmd::Command::cargo_bin("sahou")
            .unwrap()
            .args([
                "tap",
                desc_path.to_str().unwrap(),
                "--send",
                "touch",
                "--sample",
                "--connect",
                &format!("tcp/127.0.0.1:{port}"),
                "--no-multicast",
            ])
            .assert()
            .success();
        if let Ok(got) = rx.recv_timeout(Duration::from_secs(2)) {
            received = Some(got);
            break;
        }
    }
    let (wire, att) = received.expect("tap --send did not arrive");
    // attachment = the per-connection hash (16 hex)
    assert_eq!(
        String::from_utf8(att.expect("no attachment")).unwrap(),
        desc.connections["touch"].hash
    );
    // the payload passes the core receive boundary (= the same valid sample as the engine; sample_slot's guarantee)
    let out = sahou_core::runtime::accept_sample(
        &desc,
        "visuals",
        "touch",
        &wire,
        Some(&desc.connections["touch"].hash),
        0,
        None,
    );
    assert!(
        matches!(out, sahou_core::runtime::AcceptOutcome::Accept { .. }),
        "a payload from sample_slot should pass the receive boundary: {out:?}"
    );
    session.close().wait().unwrap();
}

#[test]
fn tap_watch_prints_accept_and_core_reject_then_exits_by_count() {
    let dir = tempfile::tempdir().unwrap();
    let desc_path = gen_descriptor(&dir);
    let desc = sahou_core::runtime::load_descriptor(&std::fs::read_to_string(&desc_path).unwrap())
        .unwrap();
    let key = desc.connections["touch"].key.clone();
    let hash = desc.connections["touch"].hash.clone();
    let port = free_port();
    let session = listen_session(port);

    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_sahou"))
        .args([
            "tap",
            desc_path.to_str().unwrap(),
            "--node",
            "visuals",
            "--count",
            "2",
            "--connect",
            &format!("tcp/127.0.0.1:{port}"),
            "--no-multicast",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    // Keep sending valid+broken pairs without waiting for the subscription to establish (tap exits at --count 2).
    // put on the same session and same key preserves order -> sending in pairs yields 2 events, one OK and one NO.
    let valid = &br#"{"x":0.5,"phase":"move","meta":{"ts":0}}"#[..];
    let broken = &br#"{"x":"oops","phase":"move","meta":{"ts":0}}"#[..];
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        session
            .put(&key, valid)
            .attachment(hash.as_bytes())
            .wait()
            .unwrap();
        session
            .put(&key, broken)
            .attachment(hash.as_bytes())
            .wait()
            .unwrap();
        if child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "tap --count 2 does not terminate"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "exit != 0:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains(&format!("OK {key}")),
        "accept line is missing:\n{stdout}"
    );
    // The core payload.rs diagnostic path convention uses a "$." prefix (the brief uses shorthand, so match the actual)
    assert!(
        stdout.contains("[type_mismatch] @$.x"),
        "the core receive-boundary rejection is missing:\n{stdout}"
    );
    session.close().wait().unwrap();
}

#[test]
fn tap_watch_explains_hash_mismatch_via_contract_queryable() {
    let dir = tempfile::tempdir().unwrap();
    let desc_path = gen_descriptor(&dir);
    let desc = sahou_core::runtime::load_descriptor(&std::fs::read_to_string(&desc_path).unwrap())
        .unwrap();
    let key = desc.connections["touch"].key.clone();
    let ns = desc.namespace.clone();

    // A "breaking sender": same demo schema with touch.x float -> string
    // (same replace as core/tests/runtime_handshake.rs; must match the YAML byte-for-byte).
    let breaking_yaml = DEMO.replace("\r\n", "\n").replace(
        "        - { name: x, type: float, min: 0, max: 1 }",
        "        - { name: x, type: string }",
    );
    let c = sahou_core::parse::parse_contract(&breaking_yaml).unwrap();
    let breaking = sahou_core::runtime::load_descriptor(&sahou_core::ir::descriptor_json(
        &c,
        &sahou_core::endpoints::Endpoints::default(),
    ))
    .unwrap();
    let frag = sahou_core::runtime::contract_fragment(&breaking, "touch").unwrap();
    let bhash = breaking.connections["touch"].hash.clone();

    let port = free_port();
    let session = listen_session(port);
    // Serve the sender's contract fragment exactly where a real engine declares it.
    let contract_key = format!("{ns}/@sahou/contract/touch/{bhash}");
    let ckey2 = contract_key.clone();
    let _q = session
        .declare_queryable(&contract_key)
        .callback(move |query: zenoh::query::Query| {
            let _ = query.reply(ckey2.clone(), frag.clone()).wait();
        })
        .wait()
        .unwrap();

    // --count 3 (not 1): the first fetch may race route convergence and print
    // "unreachable" (uncached by design), so later mismatch events retry it.
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_sahou"))
        .args([
            "tap",
            desc_path.to_str().unwrap(),
            "--node",
            "visuals",
            "--count",
            "3",
            "--connect",
            &format!("tcp/127.0.0.1:{port}"),
            "--no-multicast",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    // Keep publishing a sample that is valid under the breaking contract, with the
    // breaking sender's hash attached -> the base-descriptor tap sees HashMismatch.
    let wire = &br#"{"x":"oops","phase":"move","meta":{"ts":0}}"#[..];
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        session
            .put(&key, wire)
            .attachment(bhash.as_bytes())
            .wait()
            .unwrap();
        if child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "tap --count 3 does not terminate"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "exit != 0:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("[hash_mismatch]"),
        "the mismatch line is missing:\n{stdout}"
    );
    assert!(
        stdout.contains("[handshake:blocked]"),
        "the why-NO explanation is missing:\n{stdout}"
    );
    assert!(
        stdout.contains("schema_incompatible"),
        "the structured diag is missing:\n{stdout}"
    );
    assert!(
        stdout.contains("judged vs the descriptor tap loaded"),
        "the vantage note is missing:\n{stdout}"
    );
    session.close().wait().unwrap();
}
