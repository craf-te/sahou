//! doctor integration smoke test: the real probes run to completion, print 3+1 probe lines,
//! and exit with either healthy(0) or NO(1) (does not depend on environment-specific results).
#[test]
fn doctor_runs_all_probes_and_exits_zero_or_one() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_sahou"))
        .args(["doctor", "--scout-secs", "1"])
        .output()
        .unwrap();
    let code = out.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "exit other than healthy(0)/NO(1): {code}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    for label in [
        "loopback UDP",
        "LAN reachability",
        "zenoh scout egress",
        "link WS",
    ] {
        assert!(
            stdout.contains(label),
            "probe line '{label}' is missing:\n{stdout}"
        );
    }
}
