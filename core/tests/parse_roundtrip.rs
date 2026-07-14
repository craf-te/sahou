use proptest::prelude::*;
use sahou_core::contract::*;
use sahou_core::fmt::{fmt, serialize_contract};
use sahou_core::parse::parse_contract;
use sahou_core::typespec::*;
use std::collections::BTreeMap;

const DEMO: &str = include_str!("../../examples/demo/schema.sahou.yaml");

#[test]
fn demo_roundtrip_is_structurally_identical() {
    let c1 = parse_contract(DEMO).unwrap();
    let yaml = serialize_contract(&c1);
    let c2 = parse_contract(&yaml).unwrap();
    assert_eq!(c1, c2);
}

#[test]
fn fmt_is_idempotent_on_demo() {
    let once = fmt(DEMO).unwrap();
    let twice = fmt(&once).unwrap();
    assert_eq!(once, twice);
}

#[test]
fn fmt_normalizes_to_into_list() {
    let yaml = "schema: s\nnodes:\n  a: {}\n  b: {}\nconnections:\n  c:\n    pattern: pub_sub\n    from: a\n    to: b\n    payload: { typing: any }\n";
    let out = fmt(yaml).unwrap();
    assert!(
        out.contains("- b"),
        "to is always emitted in list form: {out}"
    );
}

// ---- proptest: roundtrip stability over safe identifiers + finite floats (spec §13, on par with Z21) ----

fn ident() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,11}".prop_map(|s| s)
}

fn arb_scalar_type() -> impl Strategy<Value = TypeName> {
    prop_oneof![
        Just(TypeName::Int),
        Just(TypeName::Float),
        Just(TypeName::Bool),
        Just(TypeName::String),
        Just(TypeName::Bytes),
        Just(TypeName::Timestamp),
    ]
}

fn arb_field(depth: u32) -> BoxedStrategy<Field> {
    let scalar =
        (ident(), arb_scalar_type(), any::<bool>()).prop_map(|(name, ty, required)| Field {
            name,
            ty,
            required,
            default: None,
            min: None,
            max: None,
            max_len: None,
            values: vec![],
            items: None,
            any_of: vec![],
            fields: vec![],
        });
    if depth == 0 {
        return scalar.boxed();
    }
    let en = (ident(), prop::collection::vec(ident(), 1..4)).prop_map(|(name, values)| Field {
        name,
        ty: TypeName::Enum,
        required: true,
        default: None,
        min: None,
        max: None,
        max_len: None,
        values,
        items: None,
        any_of: vec![],
        fields: vec![],
    });
    let arr = (ident(), arb_scalar_type()).prop_map(|(name, inner)| Field {
        name,
        ty: TypeName::Array,
        required: true,
        default: None,
        min: None,
        max: None,
        max_len: None,
        values: vec![],
        items: Some(TypeSpec::Name(inner)),
        any_of: vec![],
        fields: vec![],
    });
    let group =
        (ident(), prop::collection::vec(arb_field(depth - 1), 1..3)).prop_map(|(name, fields)| {
            Field {
                name,
                ty: TypeName::Group,
                required: true,
                default: None,
                min: None,
                max: None,
                max_len: None,
                values: vec![],
                items: None,
                any_of: vec![],
                fields,
            }
        });
    prop_oneof![scalar, en, arr, group].boxed()
}

fn arb_contract() -> impl Strategy<Value = Contract> {
    (
        ident(),
        prop::collection::btree_map(ident(), Just(Node::default()), 2..5),
        prop::collection::vec(arb_field(2), 1..4),
        any::<bool>(),
    )
        .prop_map(|(schema, nodes, fields, reliable)| {
            let names: Vec<&String> = nodes.keys().collect();
            let mut connections = BTreeMap::new();
            connections.insert(
                "c1".to_string(),
                Connection {
                    pattern: Pattern::PubSub,
                    from: names[0].clone(),
                    to: vec![names[1].clone()],
                    key: None,
                    selector: None,
                    reliability: if reliable {
                        Reliability::Reliable
                    } else {
                        Reliability::BestEffort
                    },
                    congestion: if reliable {
                        Congestion::Block
                    } else {
                        Congestion::Drop
                    },
                    priority: Priority::default(),
                    express: false,
                    encoding: Encoding::default(),
                    validate: ValidateLevel::default(),
                    payload: Some(Slot {
                        typing: Typing::Typed,
                        kind: SlotKind::Record,
                        fields,
                        encoding: None,
                    }),
                    request: None,
                    response: None,
                },
            );
            Contract {
                schema,
                version: "1".to_string(),
                nodes,
                connections,
            }
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn roundtrip_serialize_parse(c in arb_contract()) {
        let yaml = serialize_contract(&c);
        let back = parse_contract(&yaml).unwrap();
        prop_assert_eq!(c, back);
    }

    #[test]
    fn fmt_idempotent(c in arb_contract()) {
        let yaml = serialize_contract(&c);
        let once = fmt(&yaml).unwrap();
        let twice = fmt(&once).unwrap();
        prop_assert_eq!(once, twice);
    }
}
