use weave_heart::{ObjectId, ObjectWorld, SpawnError, Transform};

#[test]
fn spawn_builds_stable_paths_and_lookup_round_trips() {
    let mut world = ObjectWorld::new();
    let root = world.spawn(None, "world").unwrap();
    let player = world.spawn(Some(root), "player").unwrap();
    let arm = world.spawn(Some(player), "arm_left").unwrap();

    assert_eq!(world.path(arm).as_deref(), Some("world/player/arm_left"));
    assert_eq!(world.lookup("world/player/arm_left"), Some(arm));
    assert_eq!(world.lookup("world/player"), Some(player));
    assert_eq!(world.lookup("world/missing"), None);
    assert_eq!(world.lookup(""), None);
    assert_eq!(world.lookup("world//player"), None);
    assert_eq!(world.len(), 3);
}

#[test]
fn sibling_names_must_be_unique_and_failed_spawns_change_nothing() {
    let mut world = ObjectWorld::new();
    let root = world.spawn(None, "world").unwrap();
    world.spawn(Some(root), "tree").unwrap();

    let err = world.spawn(Some(root), "tree").unwrap_err();
    assert!(matches!(err, SpawnError::DuplicateName { .. }));
    assert_eq!(world.len(), 2);

    let other = world.spawn(None, "other").unwrap();
    world.spawn(Some(other), "tree").unwrap();
    assert_eq!(world.len(), 4);

    assert!(matches!(
        world.spawn(None, "world").unwrap_err(),
        SpawnError::DuplicateName { parent: None, .. }
    ));
}

#[test]
fn invalid_names_are_rejected() {
    let mut world = ObjectWorld::new();
    assert!(matches!(
        world.spawn(None, "").unwrap_err(),
        SpawnError::InvalidName(_)
    ));
    assert!(matches!(
        world.spawn(None, "a/b").unwrap_err(),
        SpawnError::InvalidName(_)
    ));
    assert!(world.is_empty());
}

#[test]
fn spawning_under_a_stale_parent_fails() {
    let mut world = ObjectWorld::new();
    let parent = world.spawn(None, "parent").unwrap();
    assert!(world.despawn(parent));
    assert_eq!(
        world.spawn(Some(parent), "child").unwrap_err(),
        SpawnError::StaleParent(parent)
    );
    assert!(world.is_empty());
}

#[test]
fn despawn_removes_the_subtree_and_every_handle_in_it_goes_stale() {
    let mut world = ObjectWorld::new();
    let root = world.spawn(None, "root").unwrap();
    let a = world.spawn(Some(root), "a").unwrap();
    let b = world.spawn(Some(root), "b").unwrap();
    let a_child = world.spawn(Some(a), "leaf").unwrap();
    let keep = world.spawn(None, "keep").unwrap();

    assert!(world.despawn(root));
    assert_eq!(world.len(), 1);

    for stale in [root, a, b, a_child] {
        assert!(!world.contains(stale));
        assert_eq!(world.id(stale), None);
        assert_eq!(world.name(stale), None);
        assert_eq!(world.path(stale), None);
        assert_eq!(world.transform(stale), None);
        assert!(world.children(stale).is_none());
        assert!(!world.set_transform(stale, Transform::default()));
        assert!(!world.despawn(stale));
    }

    assert!(world.contains(keep));
    assert_eq!(world.lookup("root"), None);
    assert_eq!(world.roots().collect::<Vec<_>>(), vec![keep]);
}

#[test]
fn a_reused_slot_does_not_revive_a_stale_handle() {
    let mut world = ObjectWorld::new();
    let old = world.spawn(None, "first").unwrap();
    world.despawn(old);
    let new = world.spawn(None, "second").unwrap();

    assert_ne!(old, new);
    assert!(!world.contains(old));
    assert_eq!(world.name(old), None);
    assert_eq!(world.name(new).as_deref(), Some("second"));
    assert!(!world.despawn(old));
    assert!(world.contains(new));
}

#[test]
fn a_name_is_free_again_after_its_object_is_despawned() {
    let mut world = ObjectWorld::new();
    let first = world.spawn(None, "name").unwrap();
    world.despawn(first);
    let second = world.spawn(None, "name").unwrap();
    assert_eq!(world.lookup("name"), Some(second));
}

#[test]
fn explicit_ids_are_stable_unique_and_fresh_ids_skip_them() {
    let mut world = ObjectWorld::new();
    let seven = world
        .spawn_with_id(ObjectId::new(7), None, "seven")
        .unwrap();
    assert_eq!(world.id(seven), Some(ObjectId::new(7)));
    assert_eq!(world.by_id(ObjectId::new(7)), Some(seven));

    assert_eq!(
        world
            .spawn_with_id(ObjectId::new(7), None, "again")
            .unwrap_err(),
        SpawnError::DuplicateId(ObjectId::new(7))
    );

    let fresh = world.spawn(None, "fresh").unwrap();
    assert!(world.id(fresh).unwrap().get() > 7);
}

#[test]
fn children_iterate_in_id_order_not_insertion_order() {
    let mut world = ObjectWorld::new();
    let root = world.spawn(None, "root").unwrap();
    let c = world
        .spawn_with_id(ObjectId::new(30), Some(root), "c")
        .unwrap();
    let a = world
        .spawn_with_id(ObjectId::new(10), Some(root), "a")
        .unwrap();
    let b = world
        .spawn_with_id(ObjectId::new(20), Some(root), "b")
        .unwrap();

    let order: Vec<_> = world.children(root).unwrap().collect();
    assert_eq!(order, vec![a, b, c]);
}

#[test]
fn transforms_default_to_identity_and_can_be_set() {
    let mut world = ObjectWorld::new();
    let object = world.spawn(None, "object").unwrap();
    assert_eq!(world.transform(object), Some(Transform::default()));

    let moved = Transform {
        translation: [1.0, 2.0, 3.0],
        ..Transform::default()
    };
    assert!(world.set_transform(object, moved));
    assert_eq!(world.transform(object), Some(moved));
}

#[test]
fn a_hundred_thousand_objects_keep_their_invariants() {
    let mut world = ObjectWorld::new();
    let mut roots = Vec::new();
    for r in 0..1_000 {
        let root = world.spawn(None, &format!("root{r}")).unwrap();
        roots.push(root);
        for c in 0..99 {
            world.spawn(Some(root), &format!("child{c}")).unwrap();
        }
    }
    assert_eq!(world.len(), 100_000);
    assert_eq!(world.roots().count(), 1_000);

    let target = world.lookup("root500/child42").unwrap();
    assert_eq!(world.path(target).as_deref(), Some("root500/child42"));

    assert!(world.despawn(roots[500]));
    assert_eq!(world.len(), 99_900);
    assert_eq!(world.lookup("root500/child42"), None);
    assert!(!world.contains(target));
    assert_eq!(world.path(target), None);
}
