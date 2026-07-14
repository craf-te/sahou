use assert_cmd::Command;
use predicates::prelude::*;

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");
const DEV: &str = include_str!("../../examples/demo/endpoints.dev.yaml");

fn write(dir: &tempfile::TempDir, name: &str, content: &str) -> std::path::PathBuf {
    let p = dir.path().join(name);
    std::fs::write(&p, content).unwrap();
    p
}

#[test]
fn validate_ok_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let schema = write(&dir, "schema.sahou.yaml", DEMO);
    Command::cargo_bin("sahou")
        .unwrap()
        .args(["validate", schema.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"));
}

#[test]
fn validate_broken_prints_diags_and_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    let schema = write(
        &dir,
        "bad.sahou.yaml",
        "schema: s\nnodes:\n  a: {}\nconnections:\n  bad:\n    pattern: pub_sub\n    from: a\n    to: [a]\n    payload: { typing: any }\n",
    );
    Command::cargo_bin("sahou")
        .unwrap()
        .args(["validate", schema.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "[self_loop] @connections.bad.to[0]",
        ));
}

#[test]
fn fmt_prints_canonical_yaml() {
    let dir = tempfile::tempdir().unwrap();
    let schema = write(
        &dir,
        "s.yaml",
        "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: b\n    payload: { typing: any }\n",
    );
    Command::cargo_bin("sahou")
        .unwrap()
        .args(["fmt", schema.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("- b")); // to is normalized to a list
}

#[test]
fn fmt_write_warns_about_comments() {
    let dir = tempfile::tempdir().unwrap();
    let schema = write(
        &dir,
        "c.yaml",
        "# an important comment\nschema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: [b]\n    payload: { typing: any }\n",
    );
    Command::cargo_bin("sahou")
        .unwrap()
        .args(["fmt", schema.to_str().unwrap(), "--write"])
        .assert()
        .success()
        .stderr(predicate::str::contains("comments are not preserved"));
    let rewritten = std::fs::read_to_string(&schema).unwrap();
    assert!(!rewritten.contains("an important comment"));
}

#[test]
fn gen_writes_descriptor_json_under_out_dir() {
    let dir = tempfile::tempdir().unwrap();
    let schema = write(&dir, "schema.sahou.yaml", DEMO);
    let endpoints = write(&dir, "endpoints.dev.yaml", DEV);
    let out_dir = dir.path().join("gen");
    Command::cargo_bin("sahou")
        .unwrap()
        .args([
            "gen",
            schema.to_str().unwrap(),
            "--endpoints",
            endpoints.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success();
    // the full IR is <out-dir>/descriptor.json (②d output reorganization; a breaking change)
    let d: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("descriptor.json")).unwrap())
            .unwrap();
    assert_eq!(d["namespace"], "sahou/demo");
    assert_eq!(d["connections"]["touch"]["key"], "sahou/demo/touch");
    assert_eq!(
        d["connections"]["touch"]["hash"].as_str().unwrap().len(),
        16
    );
}

#[test]
fn gen_without_schema_arg_uses_cwd_default() {
    // With no positional schema and no --out-dir, gen reads ./schema.sahou.yaml and writes
    // ./gen/descriptor.json, both resolved against the current directory.
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "schema.sahou.yaml", DEMO);
    Command::cargo_bin("sahou")
        .unwrap()
        .current_dir(dir.path())
        .arg("gen")
        .assert()
        .success();
    assert!(dir.path().join("gen").join("descriptor.json").exists());
}

#[test]
fn gen_explicit_schema_arg_overrides_default() {
    // An explicit positional path is honored over the ./schema.sahou.yaml default.
    let dir = tempfile::tempdir().unwrap();
    let schema = write(&dir, "custom.sahou.yaml", DEMO);
    let out_dir = dir.path().join("gen");
    Command::cargo_bin("sahou")
        .unwrap()
        .args([
            "gen",
            schema.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert!(out_dir.join("descriptor.json").exists());
}

#[test]
fn gen_lang_node_writes_stub_files() {
    let dir = tempfile::tempdir().unwrap();
    let schema = write(&dir, "schema.sahou.yaml", DEMO);
    let out_dir = dir.path().join("gen");
    Command::cargo_bin("sahou")
        .unwrap()
        .args([
            "gen",
            schema.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--lang",
            "python",
            "--node",
            "sensor",
        ])
        .assert()
        .success();
    let py = std::fs::read_to_string(out_dir.join("sensor").join("sahou_stub.py")).unwrap();
    let pyi = std::fs::read_to_string(out_dir.join("sensor").join("sahou_stub.pyi")).unwrap();
    assert!(py.contains("sahou:stub node=sensor"));
    assert!(pyi.contains("class SensorNode(Protocol):"));
    // the IR is emitted at the same time
    assert!(out_dir.join("descriptor.json").exists());
}

#[test]
fn gen_lang_requires_node_and_vice_versa() {
    let dir = tempfile::tempdir().unwrap();
    let schema = write(&dir, "schema.sahou.yaml", DEMO);
    for args in [vec!["--lang", "ts"], vec!["--node", "sensor"]] {
        let mut a = vec!["gen", schema.to_str().unwrap()];
        a.extend(args);
        Command::cargo_bin("sahou")
            .unwrap()
            .args(&a)
            .assert()
            .failure(); // clap's requires
    }
}

#[test]
fn gen_stub_unknown_node_is_core_diag() {
    let dir = tempfile::tempdir().unwrap();
    let schema = write(&dir, "schema.sahou.yaml", DEMO);
    let out_dir = dir.path().join("gen");
    Command::cargo_bin("sahou")
        .unwrap()
        .args([
            "gen",
            schema.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--lang",
            "ts",
            "--node",
            "ghost",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("[unknown_node]"));
}

#[test]
fn gen_refuses_invalid_schema() {
    let dir = tempfile::tempdir().unwrap();
    let schema = write(
        &dir,
        "bad.yaml",
        "schema: s\nnodes:\n  a: {}\nconnections:\n  bad:\n    pattern: pub_sub\n    from: a\n    to: [ghost]\n    payload: { typing: any }\n",
    );
    Command::cargo_bin("sahou")
        .unwrap()
        .args(["gen", schema.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("[unknown_node]"));
}

#[test]
fn gen_stub_header_shows_runtime_usage() {
    // The runtime-facing stub file (.mjs/.py) names the runtime library and shows how to wire it, so an
    // AI/human implementing the app can discover @sahou/runtime / sahou from the stub itself (not only types).
    let dir = tempfile::tempdir().unwrap();
    let schema = write(&dir, "schema.sahou.yaml", DEMO);
    let out_dir = dir.path().join("gen");
    Command::cargo_bin("sahou")
        .unwrap()
        .args([
            "gen",
            schema.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--lang",
            "ts",
            "--node",
            "visuals",
        ])
        .assert()
        .success();
    let mjs = std::fs::read_to_string(out_dir.join("visuals").join("sahou_stub.mjs")).unwrap();
    assert!(
        mjs.contains("@sahou/runtime"),
        "ts stub should name the runtime lib:\n{mjs}"
    );
    assert!(
        mjs.contains("await connect("),
        "ts stub should show connect() usage:\n{mjs}"
    );

    Command::cargo_bin("sahou")
        .unwrap()
        .args([
            "gen",
            schema.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--lang",
            "python",
            "--node",
            "sensor",
        ])
        .assert()
        .success();
    let py = std::fs::read_to_string(out_dir.join("sensor").join("sahou_stub.py")).unwrap();
    assert!(
        py.contains("pip install sahou"),
        "py stub should name the runtime lib:\n{py}"
    );
    assert!(
        py.contains("sahou.connect("),
        "py stub should show connect() usage:\n{py}"
    );
}
