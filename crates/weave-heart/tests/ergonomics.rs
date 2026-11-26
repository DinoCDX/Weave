use std::sync::{Arc, Mutex};

use weave_heart::Component;
use weave_heart::{LogEvent, Runtime, Script, ScriptContext, SkipReason, Transform};
use weave_macros::Component as DeriveComponent;

#[derive(Debug, PartialEq, DeriveComponent)]
struct Charge {
    value: i64,
}

struct Mover {
    target: Transform,
    once: bool,
}

impl Script for Mover {
    type Services = ();

    fn on_update(&mut self, cx: &mut ScriptContext<'_, ()>) {
        if self.once {
            self.once = false;
            cx.set_transform(self.target);
            cx.insert_component(Charge { value: 1 });
        } else {
            cx.edit_component::<Charge>(|c| c.value += 10);
        }
    }
}

struct Doomed;

impl Script for Doomed {
    type Services = ();

    fn on_update(&mut self, cx: &mut ScriptContext<'_, ()>) {
        cx.despawn();
    }
}

struct Sleeper;

impl Script for Sleeper {
    type Services = ();

    fn on_update(&mut self, cx: &mut ScriptContext<'_, ()>) {
        cx.set_enabled(false);
    }
}

#[test]
fn scripts_act_on_their_own_object_through_helpers() {
    let target = Transform {
        translation: [9.0, 0.0, 0.0],
        ..Transform::default()
    };
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "mover").unwrap();
    rt.attach(object, Mover { target, once: true }).unwrap();
    rt.commands().load(object);

    rt.update(0.0);
    assert_eq!(rt.world().transform(object), Some(Transform::default()));

    rt.update(0.0);
    assert_eq!(rt.world().transform(object), Some(target));
    assert!(rt.world().has_component::<Charge>(object));

    rt.update(0.0);
    assert_eq!(
        rt.world().with_component::<Charge, _>(object, |c| c.value),
        Some(11)
    );
}

#[test]
fn a_script_can_despawn_and_disable_its_own_object() {
    let mut rt = Runtime::new();
    let gone = rt.spawn(None, "gone").unwrap();
    rt.attach(gone, Doomed).unwrap();
    rt.commands().load(gone);
    rt.update(0.0);
    rt.update(0.0);
    assert!(!rt.world().contains(gone));

    let idle = rt.spawn(None, "idle").unwrap();
    rt.attach(idle, Sleeper).unwrap();
    rt.commands().load(idle);
    rt.update(0.0);
    rt.update(0.0);
    assert!(!rt.is_enabled(idle));
}

#[test]
fn the_log_sink_receives_every_issue_in_the_frame() {
    let seen: Arc<Mutex<Vec<LogEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);

    let mut rt = Runtime::new();
    rt.set_log_sink(move |event| sink.lock().unwrap().push(event.clone()));
    let object = rt.spawn(None, "x").unwrap();
    rt.spawn(None, "taken").unwrap();
    rt.commands().despawn(object);
    rt.update(0.0);
    rt.commands().load(object);
    rt.commands().spawn(None, "taken");
    rt.update(0.0);

    let events = seen.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        &events[0],
        LogEvent::Skipped(s) if s.command == "load" && s.reason == SkipReason::StaleHandle
    ));
    assert!(matches!(&events[1], LogEvent::Rejected(text) if text.contains("spawn failed")));
    assert!(events[0].to_string().contains("skipped load"));
}

#[test]
fn a_clean_frame_sends_nothing_to_the_sink() {
    let seen: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let count = Arc::clone(&seen);
    let mut rt = Runtime::new();
    rt.set_log_sink(move |_| *count.lock().unwrap() += 1);
    let object = rt.spawn(None, "fine").unwrap();
    rt.commands().load(object);
    rt.update(0.0);
    rt.update(0.0);
    assert_eq!(*seen.lock().unwrap(), 0);
    rt.clear_log_sink();
}

#[test]
fn parent_accessors_distinguish_roots_children_and_stale_handles() {
    let mut rt = Runtime::new();
    let root = rt.spawn(None, "root").unwrap();
    let child = rt.spawn(Some(root), "child").unwrap();
    let world = rt.world();

    assert_eq!(world.parent_of(child), Some(root));
    assert_eq!(world.parent_of(root), None);
    assert_eq!(world.is_root(root), Some(true));
    assert_eq!(world.is_root(child), Some(false));

    rt.commands().despawn(root);
    rt.update(0.0);
    assert_eq!(rt.world().is_root(child), None);
}

#[test]
fn names_are_shared_not_copied_on_each_read() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "shared").unwrap();
    let first = rt.world().name(object).unwrap();
    let second = rt.world().name(object).unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(&*first, "shared");
}

#[test]
fn transform_is_still_a_component_for_schemas() {
    assert_eq!(Transform::SCHEMA.fields.len(), 3);
}
