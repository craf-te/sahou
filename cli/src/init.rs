//! sahou init — scaffold a new project (a minimal valid seed to start from an empty directory).
//! Creates schema.sahou.yaml / endpoints.dev.yaml / .gitignore. The generated schema is self-checked
//! by always running it through the core parse_contract (guaranteeing init never emits an invalid seed).
//! An existing schema.sahou.yaml is not overwritten without --force (the "never silently break the contract" invariant).

use std::path::{Path, PathBuf};

use clap::Args;
use sahou_core::contract::Contract;
use sahou_core::diag::Diag;
use sahou_core::endpoints::{serialize_endpoints, Endpoints};
use sahou_core::fmt::serialize_contract;
use sahou_core::parse::parse_contract;

const FALLBACK_NAME: &str = "sahou_project";
const GITIGNORE_CONTENT: &str = "gen/descriptor.json\n";
/// A short pointer for AI (coding agents). It does not spell out the full grammar; it directs them to `sahou reference`.
const AGENTS_MD_CONTENT: &str = "\
# AGENTS.md — instructions for AI working on this Sahou project

- The contract (the blueprint of the communication) = `schema.sahou.yaml` (the single source to edit).
- **Run `sahou reference` before editing the contract** to review the format, types, invariants, and diagnostic codes.
- **After editing, run `sahou validate <file>`** and fix each returned rejection (`{code, path, message}`) \
at the location given by path. Repeat until there are zero rejections.
- `endpoints.<env>.yaml` = deployment (separate from the contract; per environment). `layout.sahou.json` = GUI coordinates (separate from the contract). \
`gen/` = generated artifacts (do not edit by hand).
- If `sahou gui` is running, file edits are reflected in the GUI immediately.
- **Implementing an app** (not editing the contract): use the runtime library — TS `@sahou/runtime` \
(`npm i @sahou/runtime`; browser entry `@sahou/runtime/browser`) / Python `sahou` (`pip install sahou`). \
Generate a typed stub with `sahou gen --lang <ts|python> --node <name>` (written under `gen/<node>/`), then wire it:
  - TS: `import { connect } from \"@sahou/runtime\";` + `import { typedNode } from \"./gen/<name>/sahou_stub.mjs\";` \
then `const node = typedNode(await connect(\"gen/descriptor.json\", { node: \"<name>\" }));`
  - Python: `import sahou` + `from gen.<name>.sahou_stub import typed_node;` \
then `node = typed_node(sahou.connect(\"gen/descriptor.json\", node=\"<name>\"))`
  - API: `publish` / `subscribe` / `queryConfirmed`(TS) or `query_confirmed`(py) / `answer` / `close`. A boundary NO throws `SahouRejected`.
";

#[derive(Args)]
pub struct InitArgs {
    /// Target directory (when omitted = current). Created if it does not exist (including parents)
    pub dir: Option<PathBuf>,
    /// Schema name, also used as the namespace (when omitted = DIR's basename; falls back to the default if empty/invalid)
    #[arg(long)]
    pub name: Option<String>,
    /// Overwrite even if a schema.sahou.yaml already exists
    #[arg(long)]
    pub force: bool,
}

fn init_err(path: impl Into<String>, msg: impl Into<String>) -> Vec<Diag> {
    vec![Diag::new("init_error", path, msg)]
}

fn is_valid_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

/// Validate a name (reject empty / characters that could break YAML). Some(trimmed) when valid.
fn sanitize_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !trimmed.is_empty() && trimmed.chars().all(is_valid_name_char) {
        Some(trimmed.to_string())
    } else {
        None
    }
}

/// Determine the schema name from --name / DIR's basename. Falls back to the default if both are invalid.
fn resolve_name(dir: &Path, name_arg: Option<&str>) -> String {
    if let Some(n) = name_arg {
        return sanitize_name(n).unwrap_or_else(|| FALLBACK_NAME.to_string());
    }
    dir.canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .and_then(|b| sanitize_name(&b))
        .unwrap_or_else(|| FALLBACK_NAME.to_string())
}

fn schema_yaml(name: &str) -> String {
    let contract = Contract {
        schema: name.to_string(),
        version: "1".to_string(),
        nodes: Default::default(),
        connections: Default::default(),
    };
    format!(
        "# Sahou contract (schema.sahou.yaml) — the single source to edit. Can also be edited via sahou gui.\n\
         # Add devices/processes under nodes, and connections (pub_sub / query) under connections.\n\
         {}",
        serialize_contract(&contract)
    )
}

fn endpoints_yaml(name: &str) -> String {
    let endpoints = Endpoints {
        env: Some("dev".to_string()),
        namespace: name.to_string(),
        ..Endpoints::default()
    };
    format!(
        "# Deployment settings (separate from the contract). Auto-connect on the same LAN is the default.\n{}",
        serialize_endpoints(&endpoints)
    )
}

pub fn run(args: InitArgs) -> Result<(), Vec<Diag>> {
    let dir = args.dir.unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&dir)
        .map_err(|e| init_err(dir.display().to_string(), e.to_string()))?;
    let name = resolve_name(&dir, args.name.as_deref());

    let schema_path = dir.join("schema.sahou.yaml");
    if schema_path.exists() && !args.force {
        return Err(init_err(
            schema_path.display().to_string(),
            "already initialized (schema.sahou.yaml exists). Pass --force to overwrite",
        ));
    }

    let mut created: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    // schema.sahou.yaml
    let schema_text = schema_yaml(&name);
    std::fs::write(&schema_path, &schema_text)
        .map_err(|e| init_err(schema_path.display().to_string(), e.to_string()))?;
    created.push(schema_path.display().to_string());

    // Self-check: guarantee the written file always passes the core parse_contract (never emit an invalid seed)
    if let Err(diags) = parse_contract(&schema_text) {
        let mut out = vec![Diag::new(
            "init_self_check_failed",
            schema_path.display().to_string(),
            "the generated schema.sahou.yaml does not pass self-check (parse_contract) — this is a bug in sahou init itself",
        )];
        out.extend(diags);
        return Err(out);
    }

    // endpoints.dev.yaml — skip individually if it already exists (do not overwrite; do not halt the whole thing based on the schema)
    let endpoints_path = dir.join("endpoints.dev.yaml");
    if endpoints_path.exists() {
        skipped.push(endpoints_path.display().to_string());
    } else {
        std::fs::write(&endpoints_path, endpoints_yaml(&name))
            .map_err(|e| init_err(endpoints_path.display().to_string(), e.to_string()))?;
        created.push(endpoints_path.display().to_string());
    }

    // .gitignore — leave it alone if it exists (do not append either)
    let gitignore_path = dir.join(".gitignore");
    if gitignore_path.exists() {
        skipped.push(gitignore_path.display().to_string());
    } else {
        std::fs::write(&gitignore_path, GITIGNORE_CONTENT)
            .map_err(|e| init_err(gitignore_path.display().to_string(), e.to_string()))?;
        created.push(gitignore_path.display().to_string());
    }

    // AGENTS.md — leave it alone if it exists (guidance for AI; no-clobber, in the same spirit as the "never silently break the contract" invariant)
    let agents_path = dir.join("AGENTS.md");
    if agents_path.exists() {
        skipped.push(agents_path.display().to_string());
    } else {
        std::fs::write(&agents_path, AGENTS_MD_CONTENT)
            .map_err(|e| init_err(agents_path.display().to_string(), e.to_string()))?;
        created.push(format!("{} (for AI)", agents_path.display()));
    }

    println!("[ok] initialized: {}", dir.display());
    for f in &created {
        println!("  created: {f}");
    }
    for f in &skipped {
        println!("  skipped (exists, unchanged): {f}");
    }
    println!("Next steps:");
    println!("  sahou gui {}       # open the editor", dir.display());
    println!("  sahou validate {}  # validate", schema_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_name_rejects_empty_and_whitespace() {
        assert_eq!(sanitize_name(""), None);
        assert_eq!(sanitize_name("   "), None);
    }

    #[test]
    fn sanitize_name_rejects_yaml_unsafe_chars() {
        assert_eq!(sanitize_name("foo: bar"), None);
        assert_eq!(sanitize_name("foo/bar"), None);
        assert_eq!(sanitize_name("\"quoted\""), None);
    }

    #[test]
    fn sanitize_name_accepts_word_like_names() {
        assert_eq!(
            sanitize_name("my_project-1.0"),
            Some("my_project-1.0".into())
        );
        assert_eq!(sanitize_name("  trimmed  "), Some("trimmed".into()));
    }

    #[test]
    fn resolve_name_prefers_explicit_name_over_basename() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(resolve_name(tmp.path(), Some("explicit")), "explicit");
    }

    #[test]
    fn resolve_name_falls_back_when_explicit_name_invalid() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(resolve_name(tmp.path(), Some("  ")), FALLBACK_NAME);
    }

    #[test]
    fn resolve_name_uses_dir_basename_when_omitted() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("my_install");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(resolve_name(&dir, None), "my_install");
    }

    #[test]
    fn schema_yaml_round_trips_through_parse_contract() {
        let text = schema_yaml("demo_ns");
        let contract = parse_contract(&text).unwrap();
        assert_eq!(contract.schema, "demo_ns");
        assert!(contract.nodes.is_empty());
        assert!(contract.connections.is_empty());
    }

    #[test]
    fn run_generates_four_files_on_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("proj");
        let args = InitArgs {
            dir: Some(dir.clone()),
            name: None,
            force: false,
        };
        run(args).unwrap();
        assert!(dir.join("schema.sahou.yaml").exists());
        assert!(dir.join("endpoints.dev.yaml").exists());
        assert!(dir.join(".gitignore").exists());
        assert!(dir.join("AGENTS.md").exists());
    }

    #[test]
    fn run_generates_agents_md_pointing_to_reference_and_validate() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("proj");
        run(InitArgs {
            dir: Some(dir.clone()),
            name: None,
            force: false,
        })
        .unwrap();
        let text = std::fs::read_to_string(dir.join("AGENTS.md")).unwrap();
        assert!(text.contains("sahou reference"));
        assert!(text.contains("sahou validate"));
        assert!(text.contains("schema.sahou.yaml"));
    }

    #[test]
    fn run_does_not_clobber_existing_agents_md() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("proj");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("AGENTS.md"), "existing content").unwrap();
        run(InitArgs {
            dir: Some(dir.clone()),
            name: None,
            force: false,
        })
        .unwrap();
        let text = std::fs::read_to_string(dir.join("AGENTS.md")).unwrap();
        assert_eq!(text, "existing content");
    }

    #[test]
    fn run_with_force_does_not_clobber_existing_agents_md_either() {
        // --force only overwrites schema.sahou.yaml (AGENTS.md is out of scope; its existing content is always left untouched)
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("proj");
        run(InitArgs {
            dir: Some(dir.clone()),
            name: None,
            force: false,
        })
        .unwrap();
        std::fs::write(dir.join("AGENTS.md"), "hand-edited content").unwrap();
        run(InitArgs {
            dir: Some(dir.clone()),
            name: None,
            force: true,
        })
        .unwrap();
        let text = std::fs::read_to_string(dir.join("AGENTS.md")).unwrap();
        assert_eq!(text, "hand-edited content");
    }

    #[test]
    fn run_without_force_on_existing_schema_is_no() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("proj");
        run(InitArgs {
            dir: Some(dir.clone()),
            name: None,
            force: false,
        })
        .unwrap();
        let err = run(InitArgs {
            dir: Some(dir.clone()),
            name: None,
            force: false,
        })
        .unwrap_err();
        assert_eq!(err[0].code, "init_error");
    }

    #[test]
    fn run_with_force_overwrites_schema() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("proj");
        run(InitArgs {
            dir: Some(dir.clone()),
            name: Some("first".into()),
            force: false,
        })
        .unwrap();
        run(InitArgs {
            dir: Some(dir.clone()),
            name: Some("second".into()),
            force: true,
        })
        .unwrap();
        let text = std::fs::read_to_string(dir.join("schema.sahou.yaml")).unwrap();
        assert!(text.contains("schema: second"));
    }
}
