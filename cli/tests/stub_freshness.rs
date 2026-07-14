//! Freshness guard for the committed demo stubs: FAIL unless they are byte-identical to a regeneration
//! (= when the contract changes, regenerate the stubs with `sahou gen --lang ... --node ...` before committing).
use sahou_core::endpoints::Endpoints;
use sahou_core::ir::descriptor_json;
use sahou_core::parse::parse_contract;
use sahou_core::runtime::load_descriptor;
use sahou_core::stub::{gen_stub, StubLang};

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

fn regenerated(node: &str, lang: StubLang) -> Vec<sahou_core::stub::StubFile> {
    let c = parse_contract(DEMO).unwrap();
    let desc = load_descriptor(&descriptor_json(&c, &Endpoints::default())).unwrap();
    gen_stub(&desc, node, lang).unwrap()
}

#[test]
fn committed_python_sensor_stub_is_fresh() {
    let committed = [
        (
            "sahou_stub.py",
            include_str!("../../examples/demo/runtime/gen/sensor/sahou_stub.py"),
        ),
        (
            "sahou_stub.pyi",
            include_str!("../../examples/demo/runtime/gen/sensor/sahou_stub.pyi"),
        ),
    ];
    let fresh = regenerated("sensor", StubLang::Python);
    for (rel, text) in committed {
        let f = fresh.iter().find(|f| f.rel_path == rel).unwrap();
        assert_eq!(
            f.content, text,
            "{rel} is stale. Regenerate and commit with `sahou gen examples/demo/schema.sahou.yaml --out-dir examples/demo/runtime/gen --lang python --node sensor`"
        );
    }
}

#[test]
fn committed_ts_visuals_stub_is_fresh() {
    let committed = [
        (
            "sahou_stub.mjs",
            include_str!("../../examples/demo/runtime/gen/visuals/sahou_stub.mjs"),
        ),
        (
            "sahou_stub.d.mts",
            include_str!("../../examples/demo/runtime/gen/visuals/sahou_stub.d.mts"),
        ),
    ];
    let fresh = regenerated("visuals", StubLang::Ts);
    for (rel, text) in committed {
        let f = fresh.iter().find(|f| f.rel_path == rel).unwrap();
        assert_eq!(
            f.content, text,
            "{rel} is stale. Regenerate and commit with `sahou gen examples/demo/schema.sahou.yaml --out-dir examples/demo/runtime/gen --lang ts --node visuals`"
        );
    }
}
