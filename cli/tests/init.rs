//! sahou init integration test: scaffolding from an empty directory, existing-file checks, --force, and --name.
use assert_cmd::Command;
use predicates::prelude::*;

fn init(dir: &std::path::Path, extra: &[&str]) -> assert_cmd::assert::Assert {
    let mut args = vec!["init", dir.to_str().unwrap()];
    args.extend_from_slice(extra);
    Command::cargo_bin("sahou").unwrap().args(&args).assert()
}

fn validate(schema: &std::path::Path) -> assert_cmd::assert::Assert {
    Command::cargo_bin("sahou")
        .unwrap()
        .args(["validate", schema.to_str().unwrap()])
        .assert()
}

#[test]
fn init_on_empty_dir_generates_valid_seed() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("proj");
    init(&dir, &[]).success();

    let schema = dir.join("schema.sahou.yaml");
    let endpoints = dir.join("endpoints.dev.yaml");
    let gitignore = dir.join(".gitignore");
    assert!(schema.exists());
    assert!(endpoints.exists());
    assert!(gitignore.exists());

    // The generated schema passes the core parse_contract (sahou validate) = no invalid seed is emitted
    validate(&schema).success();

    let endpoints_text = std::fs::read_to_string(&endpoints).unwrap();
    assert!(endpoints_text.contains("env: dev"));
    let gitignore_text = std::fs::read_to_string(&gitignore).unwrap();
    assert!(gitignore_text.contains("gen/descriptor.json"));
}

#[test]
fn init_default_name_uses_dir_basename() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("my_installation");
    init(&dir, &[]).success();
    let schema_text = std::fs::read_to_string(dir.join("schema.sahou.yaml")).unwrap();
    assert!(
        schema_text.contains("schema: my_installation"),
        "{schema_text}"
    );
    let endpoints_text = std::fs::read_to_string(dir.join("endpoints.dev.yaml")).unwrap();
    assert!(
        endpoints_text.contains("namespace: my_installation"),
        "{endpoints_text}"
    );
}

#[test]
fn init_explicit_name_overrides_basename() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("proj");
    init(&dir, &["--name", "acme_stage"]).success();
    let schema_text = std::fs::read_to_string(dir.join("schema.sahou.yaml")).unwrap();
    assert!(schema_text.contains("schema: acme_stage"), "{schema_text}");
}

#[test]
fn init_invalid_name_falls_back_to_default() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("proj");
    init(&dir, &["--name", "  "]).success();
    let schema_text = std::fs::read_to_string(dir.join("schema.sahou.yaml")).unwrap();
    assert!(
        schema_text.contains("schema: sahou_project"),
        "{schema_text}"
    );
}

#[test]
fn init_without_force_on_existing_schema_is_structured_no() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("proj");
    init(&dir, &[]).success();
    let schema = dir.join("schema.sahou.yaml");
    let before = std::fs::read_to_string(&schema).unwrap();

    // The second run (without --force) stops with a rejection and does not overwrite the existing schema
    init(&dir, &[])
        .failure()
        .stdout(predicate::str::contains("already initialized"));
    let after = std::fs::read_to_string(&schema).unwrap();
    assert_eq!(
        before, after,
        "existing schema.sahou.yaml was silently clobbered"
    );
}

#[test]
fn init_with_force_overwrites_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("proj");
    init(&dir, &["--name", "first"]).success();
    init(&dir, &["--name", "second", "--force"]).success();
    let schema_text = std::fs::read_to_string(dir.join("schema.sahou.yaml")).unwrap();
    assert!(schema_text.contains("schema: second"), "{schema_text}");
}

#[test]
fn init_does_not_touch_existing_gitignore_or_endpoints() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".gitignore"), "my_custom_ignore\n").unwrap();
    std::fs::write(
        dir.join("endpoints.dev.yaml"),
        "env: dev\nnamespace: keep_me\n",
    )
    .unwrap();

    init(&dir, &[]).success();

    let gitignore_text = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert_eq!(gitignore_text, "my_custom_ignore\n");
    let endpoints_text = std::fs::read_to_string(dir.join("endpoints.dev.yaml")).unwrap();
    assert_eq!(endpoints_text, "env: dev\nnamespace: keep_me\n");
    // the schema did not exist, so it is generated
    assert!(dir.join("schema.sahou.yaml").exists());
}

#[test]
fn init_agents_md_points_to_runtime_libraries() {
    // AGENTS.md must guide AI beyond editing the contract: how to implement apps against the runtime
    // libraries (@sahou/runtime for TS, sahou for Python) via connect(...).
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("proj");
    init(&dir, &[]).success();
    let agents = std::fs::read_to_string(dir.join("AGENTS.md")).unwrap();
    assert!(agents.contains("@sahou/runtime"), "{agents}");
    assert!(agents.contains("connect"), "{agents}");
}
