use weave_heart::{Component, DecodeError, FieldKind, ObjectWorld, Value};
use weave_macros::Component as DeriveComponent;

#[derive(Debug, Clone, PartialEq, DeriveComponent)]
struct Placement {
    position: [f32; 3],
    label: String,
    visible: bool,
    layer: i64,
}

mod v1 {
    #[derive(weave_macros::Component)]
    pub struct Shape {
        pub size: i64,
    }
}

mod v2 {
    #[derive(weave_macros::Component)]
    pub struct Shape {
        pub size: f32,
    }
}

fn sample() -> Placement {
    Placement {
        position: [0.1, -2.5, 1.0e-3],
        label: "crate".to_owned(),
        visible: true,
        layer: -7,
    }
}

#[test]
fn schema_lists_name_and_fields_in_declaration_order() {
    assert_eq!(Placement::SCHEMA.name, "Placement");
    let names: Vec<_> = Placement::SCHEMA.fields.iter().map(|f| f.name).collect();
    assert_eq!(names, ["position", "label", "visible", "layer"]);
    let kinds: Vec<_> = Placement::SCHEMA.fields.iter().map(|f| f.kind).collect();
    assert_eq!(
        kinds,
        [
            FieldKind::Vec3,
            FieldKind::Text,
            FieldKind::Bool,
            FieldKind::Int
        ]
    );
}

#[test]
fn schema_hash_is_stable_and_changes_with_field_types() {
    assert_eq!(v1::Shape::SCHEMA.hash, v1::Shape::SCHEMA.hash);
    assert_ne!(v1::Shape::SCHEMA.hash, v2::Shape::SCHEMA.hash);
    assert_ne!(Placement::SCHEMA.hash, v1::Shape::SCHEMA.hash);
}

#[test]
fn values_round_trip_exactly() {
    let original = sample();
    let values = original.to_values();
    let borrowed: Vec<(&str, Value)> = values.iter().map(|(n, v)| (*n, v.clone())).collect();
    let decoded = Placement::from_values(&borrowed).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn a_missing_field_is_reported_by_name() {
    let values = sample().to_values();
    let borrowed: Vec<(&str, Value)> = values
        .iter()
        .filter(|(n, _)| *n != "label")
        .map(|(n, v)| (*n, v.clone()))
        .collect();
    assert_eq!(
        Placement::from_values(&borrowed).unwrap_err(),
        DecodeError::MissingField("label")
    );
}

#[test]
fn a_field_with_the_wrong_type_is_reported_by_name() {
    let mut values = sample().to_values();
    for (name, value) in values.iter_mut() {
        if *name == "visible" {
            *value = Value::Int(1);
        }
    }
    let borrowed: Vec<(&str, Value)> = values.iter().map(|(n, v)| (*n, v.clone())).collect();
    assert_eq!(
        Placement::from_values(&borrowed).unwrap_err(),
        DecodeError::WrongType("visible")
    );
}

#[test]
fn components_attach_read_mutate_and_remove_through_the_world() {
    let mut world = ObjectWorld::new();
    let object = world.spawn(None, "crate").unwrap();

    assert!(!world.has_component::<Placement>(object));
    assert!(world.insert_component(object, sample()));
    assert!(world.has_component::<Placement>(object));

    let layer = world.with_component::<Placement, _>(object, |p| p.layer);
    assert_eq!(layer, Some(-7));

    world.with_component_mut::<Placement, _>(object, |p| p.layer = 3);
    assert_eq!(
        world.with_component::<Placement, _>(object, |p| p.layer),
        Some(3)
    );

    let removed = world.remove_component::<Placement>(object).unwrap();
    assert_eq!(removed.label, "crate");
    assert!(!world.has_component::<Placement>(object));
}

#[test]
fn component_access_on_a_stale_handle_is_none_or_false() {
    let mut world = ObjectWorld::new();
    let object = world.spawn(None, "gone").unwrap();
    world.despawn(object);

    assert!(!world.insert_component(object, sample()));
    assert!(!world.has_component::<Placement>(object));
    assert_eq!(
        world.with_component::<Placement, _>(object, |p| p.layer),
        None
    );
    assert_eq!(world.remove_component::<Placement>(object), None);
}
