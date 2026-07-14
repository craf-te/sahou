//! sahou check — detect stub <-> IR drift (design §8/§13; a CLI/CI responsibility).
//! The comparison logic is the pure function core stub::check_drift. This module only walks files + prints.
//! The engine does not read stubs (at engine load it only checks descriptor consistency) — that invariant is fully handled here.

use std::path::PathBuf;

use clap::Args;
use sahou_core::diag::Diag;
use sahou_core::ir::Descriptor;
use sahou_core::stub::{check_drift, parse_stub_hashes, parse_stub_node};

#[derive(Args)]
pub struct CheckArgs {
    /// Full IR (descriptor.json)
    pub descriptor: PathBuf,
    /// gen output directory (scans <gen-dir>/<node>/)
    #[arg(long, default_value = "gen")]
    pub gen_dir: PathBuf,
    /// Check only this node's stub (when omitted, all node directories under gen-dir)
    #[arg(long)]
    pub node: Option<String>,
}

fn check_err(path: impl Into<String>, msg: impl Into<String>) -> Vec<Diag> {
    vec![Diag::new("check_error", path, msg)]
}

/// Pure function that checks one node's set of stub texts. texts = (file name, contents).
/// Ok(node name) = no drift / Err = structured rejection (bad marker, node mismatch, drift).
pub fn check_stub_texts(
    desc: &Descriptor,
    expect_node: Option<&str>,
    texts: &[(String, String)],
) -> Result<String, Vec<Diag>> {
    let names: Vec<&str> = texts.iter().map(|(n, _)| n.as_str()).collect();
    // node marker: must match across all files (do not silently accept a mix of partial regenerations)
    let mut node: Option<String> = None;
    for (name, text) in texts {
        let Some(n) = parse_stub_node(text) else {
            return Err(check_err(
                name.clone(),
                "no sahou:stub node= marker (not a generated stub, or corrupted)",
            ));
        };
        match &node {
            None => node = Some(n),
            Some(prev) if *prev != n => {
                return Err(check_err(
                    name.clone(),
                    format!(
                        "mixed node markers ('{prev}' and '{n}'). Regenerate the full stub set"
                    ),
                ))
            }
            _ => {}
        }
    }
    let node = node.ok_or_else(|| check_err("$", format!("no stub files ({names:?})")))?;
    if let Some(expect) = expect_node {
        if node != expect {
            return Err(check_err(
                "$",
                format!("--node '{expect}' was specified, but the stub is for node '{node}'"),
            ));
        }
    }
    // Parse hash markers in one pass over the concatenation (cross-file conflicts surface as stub_marker_conflict on the parse side)
    let all: String = texts
        .iter()
        .map(|(_, t)| t.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let hashes = parse_stub_hashes(&all)?;
    let diags = check_drift(desc, &node, &hashes);
    if diags.is_empty() {
        Ok(node)
    } else {
        Err(diags)
    }
}

pub fn run(args: CheckArgs) -> Result<(), Vec<Diag>> {
    let json = std::fs::read_to_string(&args.descriptor)
        .map_err(|e| check_err(args.descriptor.display().to_string(), e.to_string()))?;
    let desc = sahou_core::runtime::load_descriptor(&json)?;
    let dirs: Vec<PathBuf> = match &args.node {
        Some(n) => vec![args.gen_dir.join(n)],
        None => {
            let rd = std::fs::read_dir(&args.gen_dir)
                .map_err(|e| check_err(args.gen_dir.display().to_string(), e.to_string()))?;
            let mut v: Vec<PathBuf> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            v.sort(); // deterministic scan order
            v
        }
    };
    let mut checked: Vec<String> = Vec::new();
    let mut all_diags: Vec<Diag> = Vec::new();
    for dir in dirs {
        let mut texts: Vec<(String, String)> = Vec::new();
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue; // when --node is given, this is rejected below via check_no_stubs
        };
        let mut entries: Vec<PathBuf> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        entries.sort();
        for p in entries {
            let text = std::fs::read_to_string(&p)
                .map_err(|e| check_err(p.display().to_string(), e.to_string()))?;
            if text.contains("sahou:stub node=") {
                texts.push((p.display().to_string(), text));
            }
        }
        if texts.is_empty() {
            continue; // dirs without stubs are skipped (zero comparisons is rejected all at once at the end)
        }
        match check_stub_texts(&desc, args.node.as_deref(), &texts) {
            Ok(node) => checked.push(node),
            Err(mut d) => all_diags.append(&mut d),
        }
    }
    if checked.is_empty() && all_diags.is_empty() {
        return Err(vec![Diag::new(
            "check_no_stubs",
            args.gen_dir.display().to_string(),
            "no stubs found to check (generate with `sahou gen --lang ... --node ...`, or check --gen-dir)",
        )]);
    }
    if all_diags.is_empty() {
        println!(
            "[ok] stub matches descriptor (no drift): {}",
            checked.join(", ")
        );
        Ok(())
    } else {
        Err(all_diags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sahou_core::endpoints::Endpoints;
    use sahou_core::ir::descriptor_json;
    use sahou_core::parse::parse_contract;
    use sahou_core::runtime::load_descriptor;
    use sahou_core::stub::{gen_stub, StubLang};

    const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

    fn demo_desc() -> Descriptor {
        let c = parse_contract(DEMO).unwrap();
        load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap()
    }

    fn stub_texts(desc: &Descriptor, node: &str) -> Vec<(String, String)> {
        gen_stub(desc, node, StubLang::Python)
            .unwrap()
            .into_iter()
            .map(|f| (f.rel_path, f.content))
            .collect()
    }

    #[test]
    fn fresh_stub_passes() {
        let desc = demo_desc();
        let texts = stub_texts(&desc, "sensor");
        assert_eq!(check_stub_texts(&desc, None, &texts).unwrap(), "sensor");
        assert_eq!(
            check_stub_texts(&desc, Some("sensor"), &texts).unwrap(),
            "sensor"
        );
    }

    #[test]
    fn changed_contract_is_hash_drift_no() {
        let desc = demo_desc();
        let texts = stub_texts(&desc, "sensor");
        // Simulate a contract change by swapping the descriptor-side hash
        let mut changed = desc.clone();
        changed.connections.get_mut("touch").unwrap().hash = "ffffffffffffffff".into();
        let err = check_stub_texts(&changed, None, &texts).unwrap_err();
        assert_eq!(err[0].code, "stub_hash_drift");
    }

    #[test]
    fn node_marker_mismatch_is_no() {
        let desc = demo_desc();
        let texts = stub_texts(&desc, "sensor");
        let err = check_stub_texts(&desc, Some("visuals"), &texts).unwrap_err();
        assert_eq!(err[0].code, "check_error");
        assert!(err[0].message.contains("sensor"), "{}", err[0].message);
    }

    #[test]
    fn missing_node_marker_is_no() {
        let desc = demo_desc();
        let texts = vec![("sahou_stub.py".to_string(), "# no marker\n".to_string())];
        let err = check_stub_texts(&desc, None, &texts).unwrap_err();
        assert_eq!(err[0].code, "check_error");
    }
}
