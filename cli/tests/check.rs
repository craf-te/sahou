//! sahou check integration test: check compares the real files that gen wrote (exactly how it is used in CI).
use assert_cmd::Command;
use predicates::prelude::*;

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

fn gen(dir: &std::path::Path, schema: &std::path::Path, extra: &[&str]) {
    let mut args = vec![
        "gen",
        schema.to_str().unwrap(),
        "--out-dir",
        dir.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    Command::cargo_bin("sahou")
        .unwrap()
        .args(&args)
        .assert()
        .success();
}

#[test]
fn check_passes_after_gen_and_fails_after_contract_change() {
    let tmp = tempfile::tempdir().unwrap();
    let schema = tmp.path().join("schema.sahou.yaml");
    std::fs::write(&schema, DEMO).unwrap();
    let out_dir = tmp.path().join("gen");
    gen(&out_dir, &schema, &["--lang", "python", "--node", "sensor"]);
    let desc = out_dir.join("descriptor.json");

    // 1) check right after gen: no drift
    Command::cargo_bin("sahou")
        .unwrap()
        .args([
            "check",
            desc.to_str().unwrap(),
            "--gen-dir",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("no drift"));

    // 2) contract change (add a required field to touch) -> regenerate only the descriptor -> stub is stale = drift rejection
    let changed = DEMO.replace(
        "        - { name: x, type: float, min: 0, max: 1 }",
        "        - { name: x, type: float, min: 0, max: 1 }\n        - { name: pressure, type: float }",
    );
    assert_ne!(
        changed, DEMO,
        "target line for replacement not found (update this test if the demo schema changes)"
    );
    std::fs::write(&schema, changed).unwrap();
    gen(&out_dir, &schema, &[]); // update IR only (leave the stub as-is)
    Command::cargo_bin("sahou")
        .unwrap()
        .args([
            "check",
            desc.to_str().unwrap(),
            "--gen-dir",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("[stub_hash_drift]"));
}

#[test]
fn check_with_no_stubs_is_structured_no() {
    let tmp = tempfile::tempdir().unwrap();
    let schema = tmp.path().join("schema.sahou.yaml");
    std::fs::write(&schema, DEMO).unwrap();
    let out_dir = tmp.path().join("gen");
    gen(&out_dir, &schema, &[]); // IR only, no stub
    Command::cargo_bin("sahou")
        .unwrap()
        .args([
            "check",
            out_dir.join("descriptor.json").to_str().unwrap(),
            "--gen-dir",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("[check_no_stubs]"));
}
