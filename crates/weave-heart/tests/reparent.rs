use weave_heart::{
    Component, Listener, Name, ObjectHandle, ObjectId, ObjectWorld, ReparentError, Runtime, Stage,
    StageContext, Transform,
};

struct Silent;

impl Listener for Silent {
    fn on_stage(&mut self, _stage: Stage, _object: ObjectHandle, _cx: &mut StageContext<'_>) {}
}

fn tree() -> (
    ObjectWorld,
    ObjectHandle,
    ObjectHandle,
    ObjectHandle,
    ObjectHandle,
) {
    let mut world = ObjectWorld::new();
    let a = world.spawn(None, "a").unwrap();
    let b = world.spawn(Some(a), "b").unwrap();
    let c = world.spawn(Some(b), "c").unwrap();
    let d = world.spawn(None, "d").unwrap();
    (world, a, b, c, d)
}

#[test]
fn reparenting_moves_the_subtree_and_keeps_ids() {
    let (mut world, _a, b, c, d) = tree();
    let id_before = world.id(b);

    world.reparent(b, Some(d)).unwrap();

    assert_eq!(world.path(b).as_deref(), Some("d/b"));
    assert_eq!(world.path(c).as_deref(), Some("d/b/c"));
    assert_eq!(world.lookup("a/b"), None);
    assert_eq!(world.lookup("d/b/c"), Some(c));
    assert_eq!(world.id(b), id_before);
    assert_eq!(world.parent(b), Some(Some(d)));
}

#[test]
fn reparenting_to_the_root_works() {
    let (mut world, _a, b, _c, _d) = tree();
    world.reparent(b, None).unwrap();
    assert_eq!(world.path(b).as_deref(), Some("b"));
    assert_eq!(world.parent(b), Some(None));
    assert!(world.roots().any(|r| r == b));
}

#[test]
fn reparenting_under_a_sibling_name_clash_is_refused_and_changes_nothing() {
    let mut world = ObjectWorld::new();
    let root = world.spawn(None, "root").unwrap();
    let clash = world.spawn(Some(root), "item").unwrap();
    let other = world.spawn(None, "item").unwrap();

    let err = world.reparent(other, Some(root)).unwrap_err();

    assert!(matches!(err, ReparentError::DuplicateName { .. }));
    assert_eq!(world.path(other).as_deref(), Some("item"));
    assert_eq!(world.path(clash).as_deref(), Some("root/item"));
}

#[test]
fn moving_an_object_under_itself_or_a_descendant_is_a_cycle() {
    let (mut world, a, b, c, _d) = tree();
    assert_eq!(world.reparent(a, Some(a)), Err(ReparentError::Cycle));
    assert_eq!(world.reparent(a, Some(c)), Err(ReparentError::Cycle));
    assert_eq!(world.path(a).as_deref(), Some("a"));
    assert_eq!(world.parent(b), Some(Some(a)));
}

#[test]
fn stale_handles_are_refused_and_same_parent_is_a_no_op() {
    let (mut world, a, b, _c, d) = tree();
    world.despawn(d);
    assert_eq!(
        world.reparent(b, Some(d)),
        Err(ReparentError::StaleParent(d))
    );
    assert_eq!(
        world.reparent(d, Some(a)),
        Err(ReparentError::StaleObject(d))
    );
    assert_eq!(world.reparent(b, Some(a)), Ok(()));
}

#[test]
fn a_reparented_object_keeps_its_place_in_id_order() {
    let mut world = ObjectWorld::new();
    let root = world.spawn(None, "root").unwrap();
    let late = world
        .spawn_with_id(ObjectId::new(50), Some(root), "late")
        .unwrap();
    let early = world
        .spawn_with_id(ObjectId::new(10), None, "early")
        .unwrap();

    world.reparent(early, Some(root)).unwrap();

    let order: Vec<_> = world.children(root).unwrap().collect();
    assert_eq!(order, vec![early, late]);
}

#[test]
fn the_runtime_applies_reparent_commands_at_the_boundary() {
    let mut rt = Runtime::new();
    let a = rt.spawn(None, "a").unwrap();
    let b = rt.spawn(None, "b").unwrap();
    rt.commands().load(b);
    rt.commands().reparent(b, Some(a));

    let report = rt.tick(0.0, &mut Silent);

    assert!(report.reparent_errors.is_empty());
    assert_eq!(rt.world().path(b).as_deref(), Some("a/b"));
    assert!(rt.is_loaded(b), "reparenting does not unload an object");
}

#[test]
fn the_runtime_reports_failed_reparent_commands() {
    let mut rt = Runtime::new();
    let a = rt.spawn(None, "a").unwrap();
    rt.commands().reparent(a, Some(a));
    let report = rt.tick(0.0, &mut Silent);
    assert_eq!(report.reparent_errors, vec![ReparentError::Cycle]);
}

#[test]
fn name_and_transform_are_components_with_schemas() {
    assert_eq!(Name::SCHEMA.name, "Name");
    assert_eq!(Name::SCHEMA.fields[0].name, "value");
    assert_eq!(Transform::SCHEMA.name, "Transform");
    let names: Vec<_> = Transform::SCHEMA.fields.iter().map(|f| f.name).collect();
    assert_eq!(names, ["translation", "rotation", "scale"]);

    let original = Transform {
        translation: [1.0, 2.0, 3.0],
        rotation: [0.0, 0.7, 0.0, 0.7],
        scale: [2.0; 3],
    };
    let values = original.to_values();
    let borrowed: Vec<_> = values.iter().map(|(n, v)| (*n, v.clone())).collect();
    assert_eq!(Transform::from_values(&borrowed).unwrap(), original);
}

#[test]
fn reserved_components_cannot_be_replaced_or_removed_through_the_generic_api() {
    let mut world = ObjectWorld::new();
    let object = world.spawn(None, "guarded").unwrap();

    assert!(!world.insert_component(
        object,
        Name {
            value: "renamed".into()
        }
    ));
    assert_eq!(world.name(object).as_deref(), Some("guarded"));
    assert!(
        world
            .with_component_mut::<Name, _>(object, |n| n.value = "".into())
            .is_none()
    );
    assert_eq!(world.name(object).as_deref(), Some("guarded"));

    assert_eq!(world.remove_component::<Transform>(object), None);
    assert_eq!(world.transform(object), Some(Transform::default()));

    assert_eq!(
        world
            .with_component::<Name, _>(object, |n| n.value.clone())
            .as_deref(),
        Some("guarded")
    );
}
