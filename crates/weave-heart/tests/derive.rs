use weave_heart::{Component, DecodeError, Field, FieldKind, Value};
use weave_macros::{Component as DeriveComponent, Field as DeriveField};

fn round_trip<C: Component + PartialEq + std::fmt::Debug>(original: &C) -> C {
    let values = original.to_values();
    let borrowed: Vec<(&str, Value)> = values.iter().map(|(n, v)| (*n, v.clone())).collect();
    let decoded = C::from_values(&borrowed).unwrap();
    assert_eq!(&decoded, original);
    decoded
}

#[derive(Debug, PartialEq, DeriveComponent)]
struct Marker;

#[test]
fn a_unit_struct_is_a_component_with_no_fields() {
    assert_eq!(Marker::SCHEMA.name, "Marker");
    assert!(Marker::SCHEMA.fields.is_empty());
    round_trip(&Marker);
}

#[derive(Debug, PartialEq, DeriveComponent)]
struct Speed(f32);

#[test]
fn a_tuple_struct_uses_positional_field_names() {
    assert_eq!(Speed::SCHEMA.fields[0].name, "0");
    assert_eq!(Speed::SCHEMA.fields[0].kind, FieldKind::Float);
    round_trip(&Speed(3.5));
}

#[derive(Debug, PartialEq, DeriveComponent)]
#[component(name = "Hitpoints")]
struct Health {
    #[component(rename = "cur")]
    current: i64,
    max: i64,
}

#[test]
fn struct_and_field_names_can_be_renamed_for_the_file() {
    assert_eq!(Health::SCHEMA.name, "Hitpoints");
    let names: Vec<_> = Health::SCHEMA.fields.iter().map(|f| f.name).collect();
    assert_eq!(names, ["cur", "max"]);
    round_trip(&Health { current: 4, max: 9 });
}

#[derive(Debug, PartialEq, DeriveComponent)]
struct Cached {
    value: u32,
    #[component(skip)]
    scratch: Vec<String>,
}

#[test]
fn skipped_fields_are_left_out_of_the_schema_and_come_back_as_default() {
    let names: Vec<_> = Cached::SCHEMA.fields.iter().map(|f| f.name).collect();
    assert_eq!(names, ["value"]);

    let original = Cached {
        value: 7,
        scratch: Vec::new(),
    };
    let values = original.to_values();
    assert_eq!(values.len(), 1);
    let borrowed: Vec<(&str, Value)> = values.iter().map(|(n, v)| (*n, v.clone())).collect();
    let decoded = Cached::from_values(&borrowed).unwrap();
    assert_eq!(decoded.value, 7);
    assert!(decoded.scratch.is_empty());
}

#[derive(Debug, PartialEq, DeriveComponent)]
struct Tag {
    label: Option<String>,
    rank: Option<i32>,
}

#[test]
fn an_optional_field_round_trips_when_present_and_absent() {
    round_trip(&Tag {
        label: Some("boss".into()),
        rank: None,
    });
    round_trip(&Tag {
        label: None,
        rank: Some(-3),
    });
}

#[test]
fn a_missing_optional_field_decodes_as_none_for_older_files() {
    let values: Vec<(&str, Value)> = vec![("label", Value::Text("old".into()))];
    let decoded = Tag::from_values(&values).unwrap();
    assert_eq!(decoded.label.as_deref(), Some("old"));
    assert_eq!(decoded.rank, None);
}

#[test]
fn a_missing_required_field_is_still_an_error() {
    let values: Vec<(&str, Value)> = vec![("label", Value::Nothing)];
    assert_eq!(
        Health::from_values(&values).unwrap_err(),
        DecodeError::MissingField("cur")
    );
}

#[derive(Debug, Clone, Copy, PartialEq, DeriveField)]
enum Mode {
    Idle,
    Run,
}

#[derive(Debug, PartialEq, DeriveComponent)]
struct Actor {
    mode: Mode,
}

#[test]
fn a_unit_enum_derives_field_and_is_stored_as_text() {
    assert_eq!(Mode::KIND, FieldKind::Text);
    assert_eq!(Mode::Run.to_value(), Value::Text("Run".into()));
    round_trip(&Actor { mode: Mode::Run });
}

#[test]
fn an_unknown_enum_label_is_a_wrong_type_error_not_a_panic() {
    let values: Vec<(&str, Value)> = vec![("mode", Value::Text("Fly".into()))];
    assert_eq!(
        Actor::from_values(&values).unwrap_err(),
        DecodeError::WrongType("mode")
    );
}

#[derive(Debug, Clone, Copy, PartialEq, DeriveField)]
struct Meters(f32);

#[derive(Debug, Clone, Copy, PartialEq, DeriveField)]
struct Named {
    inner: i64,
}

#[derive(Debug, PartialEq, DeriveComponent)]
struct Distances {
    walk: Meters,
    jump: Named,
}

#[test]
fn a_newtype_derives_field_from_its_inner_type() {
    assert_eq!(<Meters as Field>::KIND, FieldKind::Float);
    assert_eq!(<Named as Field>::KIND, FieldKind::Int);
    round_trip(&Distances {
        walk: Meters(2.5),
        jump: Named { inner: 9 },
    });
}

#[derive(Debug, PartialEq, DeriveComponent)]
struct Numbers {
    small: i32,
    count: u32,
    precise: f64,
}

#[test]
fn i32_u32_and_f64_fields_round_trip_exactly() {
    round_trip(&Numbers {
        small: -42,
        count: 4_000_000_000,
        precise: 0.1 + 0.2,
    });
}

#[test]
fn an_out_of_range_integer_is_a_wrong_type_error() {
    let values: Vec<(&str, Value)> = vec![
        ("small", Value::Int(i64::from(i32::MAX) + 1)),
        ("count", Value::Int(1)),
        ("precise", Value::Double(1.0)),
    ];
    assert_eq!(
        Numbers::from_values(&values).unwrap_err(),
        DecodeError::WrongType("small")
    );
}
