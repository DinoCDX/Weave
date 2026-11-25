use weave_heart::{Listener, ObjectHandle, Runtime, SkipReason, Stage, StageContext, Transform};

#[derive(Default)]
struct Recorder {
    log: Vec<String>,
}

impl Recorder {
    fn take(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }
}

impl Listener for Recorder {
    fn on_stage(&mut self, stage: Stage, object: ObjectHandle, cx: &mut StageContext<'_>) {
        let path = cx.world.path(object).unwrap_or_default();
        self.log.push(format!("{} {}", stage.name(), path));
    }
}

fn tree(rt: &mut Runtime) -> (ObjectHandle, ObjectHandle, ObjectHandle) {
    let world = rt.spawn(None, "world").unwrap();
    let player = rt.spawn(Some(world), "player").unwrap();
    let arm = rt.spawn(Some(player), "arm").unwrap();
    (world, player, arm)
}

fn lines(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn load_runs_parents_first_then_ready_then_update_in_id_order() {
    let mut rt = Runtime::new();
    let (world, _, _) = tree(&mut rt);
    rt.commands().load(world);
    let mut rec = Recorder::default();

    rt.tick(0.016, &mut rec);

    assert_eq!(
        rec.take(),
        lines(&[
            "on_load world",
            "on_enable world",
            "on_load world/player",
            "on_enable world/player",
            "on_load world/player/arm",
            "on_enable world/player/arm",
            "on_ready world",
            "on_ready world/player",
            "on_ready world/player/arm",
            "on_update world",
            "on_update world/player",
            "on_update world/player/arm",
            "on_late_update world",
            "on_late_update world/player",
            "on_late_update world/player/arm",
        ])
    );
}

#[test]
fn ready_runs_once_and_updates_repeat_every_frame() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "solo").unwrap();
    rt.commands().load(object);
    let mut rec = Recorder::default();
    rt.tick(0.0, &mut rec);
    rec.take();

    rt.tick(0.0, &mut rec);

    assert_eq!(
        rec.take(),
        lines(&["on_update solo", "on_late_update solo"])
    );
}

#[test]
fn unload_runs_children_before_parents_with_disable_before_unload() {
    let mut rt = Runtime::new();
    let (world, _, _) = tree(&mut rt);
    rt.commands().load(world);
    let mut rec = Recorder::default();
    rt.tick(0.0, &mut rec);
    rec.take();

    rt.commands().unload(world);
    rt.tick(0.0, &mut rec);

    let log = rec.take();
    let tail = &log[log.len() - 6..];
    assert_eq!(
        tail,
        lines(&[
            "on_disable world/player/arm",
            "on_unload world/player/arm",
            "on_disable world/player",
            "on_unload world/player",
            "on_disable world",
            "on_unload world",
        ])
    );
    assert_eq!(rt.loaded_count(), 0);
    assert!(rt.world().contains(world));
}

#[test]
fn despawning_a_loaded_subtree_unloads_it_then_removes_it() {
    let mut rt = Runtime::new();
    let (world, player, arm) = tree(&mut rt);
    rt.commands().load(world);
    let mut rec = Recorder::default();
    rt.tick(0.0, &mut rec);
    rec.take();

    rt.commands().despawn(world);
    let report = rt.tick(0.0, &mut rec);

    let log = rec.take();
    assert_eq!(
        &log[log.len() - 6..],
        lines(&[
            "on_disable world/player/arm",
            "on_unload world/player/arm",
            "on_disable world/player",
            "on_unload world/player",
            "on_disable world",
            "on_unload world",
        ])
    );
    assert!(report.skipped.is_empty());
    assert!(rt.world().is_empty());
    for stale in [world, player, arm] {
        assert!(!rt.world().contains(stale));
        assert!(!rt.is_loaded(stale));
    }
}

#[test]
fn disabling_stops_updates_and_re_enabling_does_not_repeat_ready() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "lamp").unwrap();
    rt.commands().load(object);
    let mut rec = Recorder::default();
    rt.tick(0.0, &mut rec);
    rec.take();

    rt.commands().set_enabled(object, false);
    rt.tick(0.0, &mut rec);
    assert_eq!(
        rec.take(),
        lines(&["on_update lamp", "on_late_update lamp", "on_disable lamp"])
    );
    assert!(!rt.is_enabled(object));
    assert!(rt.is_ready(object));

    rt.tick(0.0, &mut rec);
    assert!(rec.take().is_empty());

    rt.commands().set_enabled(object, true);
    rt.tick(0.0, &mut rec);
    assert_eq!(
        rec.take(),
        lines(&["on_enable lamp", "on_update lamp", "on_late_update lamp"])
    );
}

#[test]
fn a_disable_requested_with_a_load_takes_effect_in_the_same_frame() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "other").unwrap();
    rt.commands().load(object);
    rt.commands().set_enabled(object, false);
    let mut rec = Recorder::default();

    rt.tick(0.0, &mut rec);

    assert!(rt.is_loaded(object));
    assert!(!rt.is_enabled(object));
    assert!(rt.is_ready(object));
    assert_eq!(
        rec.take(),
        lines(&[
            "on_load other",
            "on_enable other",
            "on_ready other",
            "on_update other",
            "on_late_update other",
            "on_disable other",
        ])
    );
}

#[test]
fn updates_follow_stable_id_order_not_name_order() {
    let mut rt = Runtime::new();
    let zebra = rt.spawn(None, "zebra").unwrap();
    let apple = rt.spawn(None, "apple").unwrap();
    rt.commands().load(apple);
    rt.commands().load(zebra);
    let mut rec = Recorder::default();
    rt.tick(0.0, &mut rec);

    let log = rec.take();
    let updates: Vec<&str> = log
        .iter()
        .filter(|l| l.starts_with("on_update"))
        .map(|l| l.as_str())
        .collect();
    assert_eq!(updates, vec!["on_update zebra", "on_update apple"]);
}

#[test]
fn commands_are_applied_at_the_frame_boundary() {
    let mut rt = Runtime::new();
    rt.commands().spawn(None, "late");
    assert_eq!(rt.commands().len(), 1);
    assert!(rt.world().is_empty());

    let report = rt.tick(0.0, &mut Recorder::default());

    assert!(report.spawn_errors.is_empty());
    assert!(rt.commands().is_empty());
    assert!(rt.world().lookup("late").is_some());
}

#[test]
fn stale_commands_are_skipped_and_reported_not_applied() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "gone").unwrap();
    rt.commands().despawn(object);
    rt.tick(0.0, &mut Recorder::default());

    rt.commands().load(object);
    rt.commands().set_transform(object, Transform::default());
    rt.commands().set_enabled(object, false);
    let report = rt.tick(0.0, &mut Recorder::default());

    let reasons: Vec<_> = report
        .skipped
        .iter()
        .map(|s| (s.command, s.reason))
        .collect();
    assert_eq!(
        reasons,
        vec![
            ("set_transform", SkipReason::StaleHandle),
            ("set_enabled", SkipReason::StaleHandle),
            ("load", SkipReason::StaleHandle),
        ]
    );
    assert_eq!(rt.loaded_count(), 0);
}

#[test]
fn enabling_an_object_that_is_not_loaded_is_reported() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "idle").unwrap();
    rt.commands().set_enabled(object, false);
    let report = rt.tick(0.0, &mut Recorder::default());
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].reason, SkipReason::NotLoaded);
}

#[test]
fn a_failed_spawn_command_is_reported() {
    let mut rt = Runtime::new();
    rt.spawn(None, "dup").unwrap();
    rt.commands().spawn(None, "dup");
    let report = rt.tick(0.0, &mut Recorder::default());
    assert_eq!(report.spawn_errors.len(), 1);
    assert_eq!(rt.world().len(), 1);
}

#[test]
fn set_transform_command_is_applied_at_the_boundary() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "mover").unwrap();
    let moved = Transform {
        translation: [4.0, 0.0, -1.0],
        ..Transform::default()
    };
    rt.commands().set_transform(object, moved);
    rt.tick(0.0, &mut Recorder::default());
    assert_eq!(rt.world().transform(object), Some(moved));
}
