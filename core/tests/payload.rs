use sahou_core::contract::{Slot, SlotKind, Typing};
use sahou_core::payload::{validate_json, validate_payload};
use sahou_core::typespec::{Field, TypeName, TypeSpec};
use serde_json::json;

fn field(name: &str, ty: TypeName) -> Field {
    Field {
        name: name.into(),
        ty,
        required: true,
        default: None,
        min: None,
        max: None,
        max_len: None,
        values: vec![],
        items: None,
        any_of: vec![],
        fields: vec![],
    }
}

fn slot(fields: Vec<Field>) -> Slot {
    Slot {
        typing: Typing::Typed,
        kind: SlotKind::Record,
        fields,
        encoding: None,
    }
}

#[test]
fn point_cloud_nested_array_path() {
    // point cloud = array<array<float>> (representative Z23 payload)
    let mut pts = field("pts", TypeName::Array);
    pts.items = Some(TypeSpec::Detailed(Box::new(
        sahou_core::typespec::DetailedType {
            ty: TypeName::Array,
            items: Some(TypeSpec::Name(TypeName::Float)),
            values: vec![],
            any_of: vec![],
            fields: vec![],
            min: None,
            max: None,
            max_len: None,
        },
    )));
    let s = slot(vec![pts]);
    assert_eq!(
        validate_payload(&s, &json!({"pts": [[0.1, 0.2], [0.3, 0.4]]})),
        vec![]
    );
    let diags = validate_payload(&s, &json!({"pts": [[0.1, 0.2], [0.3, "x"]]}));
    assert_eq!(diags[0].code, "type_mismatch");
    assert_eq!(diags[0].path, "$.pts[1][1]");
}

#[test]
fn required_enum_range_group() {
    let mut phase = field("phase", TypeName::Enum);
    phase.values = vec!["down".into(), "move".into(), "up".into()];
    let mut x = field("x", TypeName::Float);
    x.min = Some(0.0);
    x.max = Some(1.0);
    let mut meta = field("meta", TypeName::Group);
    meta.fields = vec![field("ts", TypeName::Timestamp)];
    let s = slot(vec![x, phase, meta]);

    assert_eq!(
        validate_payload(
            &s,
            &json!({"x": 0.5, "phase": "move", "meta": {"ts": 1752105600000_i64}})
        ),
        vec![]
    );
    let diags = validate_payload(&s, &json!({"x": 1.5, "phase": "hover", "meta": {}}));
    let codes: Vec<(&str, &str)> = diags
        .iter()
        .map(|d| (d.code.as_str(), d.path.as_str()))
        .collect();
    assert!(codes.contains(&("out_of_range", "$.x")), "{codes:?}");
    assert!(codes.contains(&("enum_mismatch", "$.phase")), "{codes:?}");
    assert!(codes.contains(&("required", "$.meta.ts")), "{codes:?}");
}

#[test]
fn bytes_string_union_map() {
    let mut img = field("img", TypeName::Bytes);
    img.required = true;
    let mut label = field("label", TypeName::String);
    label.max_len = Some(5);
    let mut id = field("id", TypeName::Union);
    id.any_of = vec![
        TypeSpec::Name(TypeName::Int),
        TypeSpec::Name(TypeName::String),
    ];
    let mut params = field("params", TypeName::Map);
    params.items = Some(TypeSpec::Name(TypeName::Float));
    let s = slot(vec![img, label, id, params]);

    assert_eq!(
        validate_payload(
            &s,
            &json!({"img": "aGVsbG8=", "label": "abc", "id": 7, "params": {"gain": 0.5}})
        ),
        vec![]
    );
    let diags = validate_payload(
        &s,
        &json!({"img": "@@not-base64@@", "label": "toolong", "id": true, "params": {"gain": "x"}}),
    );
    let codes: Vec<(&str, &str)> = diags
        .iter()
        .map(|d| (d.code.as_str(), d.path.as_str()))
        .collect();
    assert!(codes.contains(&("bad_base64", "$.img")), "{codes:?}");
    assert!(codes.contains(&("too_long", "$.label")), "{codes:?}");
    assert!(codes.contains(&("no_union_match", "$.id")), "{codes:?}");
    assert!(
        codes.contains(&("type_mismatch", "$.params.gain")),
        "{codes:?}"
    );
}

#[test]
fn unknown_payload_fields_are_dropped_not_rejected() {
    // the wire layer drops unknown fields (forward compat). A separate layer from the contract-vocabulary deny (spec §4).
    let s = slot(vec![field("x", TypeName::Float)]);
    assert_eq!(
        validate_payload(&s, &json!({"x": 0.1, "extra": 999})),
        vec![]
    );
}

#[test]
fn any_and_opaque_skip_validation() {
    let any = Slot {
        typing: Typing::Any,
        kind: SlotKind::Record,
        fields: vec![],
        encoding: None,
    };
    assert_eq!(validate_payload(&any, &json!({"whatever": 1})), vec![]);
    // opaque (opaque data such as video) is not validated even when typed
    let opaque = Slot {
        typing: Typing::Typed,
        kind: SlotKind::Opaque,
        fields: vec![],
        encoding: Some("video/raw".into()),
    };
    assert_eq!(validate_payload(&opaque, &json!({"whatever": 1})), vec![]);
}

#[test]
fn validate_json_reports_decode_error() {
    let s = slot(vec![field("x", TypeName::Float)]);
    let diags = validate_json(&s, "{not json");
    assert_eq!(diags[0].code, "decode_error");
    assert_eq!(diags[0].path, "$");
}
