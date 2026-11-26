use weave_heart::{ObjectWorld, Runtime, RuntimeStats};

#[test]
fn update_runs_a_frame_without_a_listener() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "solo").unwrap();
    rt.commands().load(object);
    let report = rt.update(0.016);
    assert!(report.is_clean());
    assert!(rt.is_ready(object));
    assert_eq!(rt.elapsed(), f64::from(0.016_f32));
}

#[test]
fn describe_shows_each_object_state_and_script_count() {
    let mut rt = Runtime::new();
    rt.register_script("Noop", || NoopScript).unwrap();
    let world = rt.spawn(None, "world").unwrap();
    let player = rt.spawn(Some(world), "player").unwrap();
    let arm = rt.spawn(Some(player), "arm").unwrap();
    rt.attach_modular(arm, "Noop").unwrap();
    rt.commands().load(world);
    rt.commands().load(player);
    rt.update(0.0);
    rt.commands().set_enabled(player, false);
    rt.update(0.0);

    let expected = "\
world [ready, 0 scripts]
  player [disabled, 0 scripts]
    arm [ready, 1 scripts]
";
    assert_eq!(rt.describe(), expected);
}

#[test]
fn stats_track_objects_loaded_active_and_scripts() {
    let mut rt = Runtime::new();
    rt.register_script("Noop", || NoopScript).unwrap();
    let a = rt.spawn(None, "a").unwrap();
    let b = rt.spawn(None, "b").unwrap();
    rt.spawn(None, "c").unwrap();
    rt.attach_modular(a, "Noop").unwrap();
    rt.attach_modular(b, "Noop").unwrap();
    rt.commands().load(a);
    rt.commands().load(b);
    rt.commands().spawn(None, "queued");

    assert_eq!(
        rt.stats(),
        RuntimeStats {
            objects: 3,
            loaded: 0,
            active: 0,
            scripts: 2,
            queued_commands: 3,
        }
    );

    rt.update(0.0);
    assert_eq!(
        rt.stats(),
        RuntimeStats {
            objects: 4,
            loaded: 2,
            active: 2,
            scripts: 2,
            queued_commands: 0,
        }
    );
}

#[test]
fn a_clean_frame_is_clean_and_bad_commands_are_counted() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "gone").unwrap();
    rt.commands().despawn(object);
    rt.update(0.0);

    rt.commands().load(object);
    let report = rt.update(0.0);

    assert!(!report.is_clean());
    assert_eq!(report.issue_count(), 1);
    assert!(rt.update(0.0).is_clean());
}

#[test]
fn objects_iterate_in_stable_id_order_and_skip_despawned_ones() {
    let mut world = ObjectWorld::new();
    let first = world.spawn(None, "first").unwrap();
    let second = world.spawn(None, "second").unwrap();
    let third = world.spawn(Some(first), "third").unwrap();
    assert_eq!(
        world.objects().collect::<Vec<_>>(),
        vec![first, second, third]
    );

    world.despawn(second);
    assert_eq!(world.objects().collect::<Vec<_>>(), vec![first, third]);
}

#[derive(Default)]
struct NoopScript;

impl weave_heart::Script for NoopScript {
    type Services = ();
}
