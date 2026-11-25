use std::sync::{Arc, Mutex};

use weave_heart::services::{Scene, Time};
use weave_heart::{
    AttachError, Listener, ObjectHandle, RegistryError, Runtime, Script, ScriptContext, Stage,
    StageContext, Transform,
};

struct Silent;

impl Listener for Silent {
    fn on_stage(&mut self, _stage: Stage, _object: ObjectHandle, _cx: &mut StageContext<'_>) {}
}

#[derive(Default)]
struct Counter {
    owner: Option<u64>,
    ticks: u32,
    last_dt: f32,
}

impl Script for Counter {
    type Services = (Time, Scene);

    fn on_load(&mut self, cx: &mut ScriptContext<'_, Self::Services>) {
        self.owner = cx.scene().id(cx.object()).map(|id| id.get());
    }

    fn on_update(&mut self, cx: &mut ScriptContext<'_, Self::Services>) {
        self.ticks += 1;
        self.last_dt = cx.time().delta();
    }
}

#[derive(Default)]
struct Recorder {
    log: Vec<&'static str>,
}

impl Script for Recorder {
    type Services = ();

    fn on_load(&mut self, _: &mut ScriptContext<'_, ()>) {
        self.log.push("load");
    }
    fn on_enable(&mut self, _: &mut ScriptContext<'_, ()>) {
        self.log.push("enable");
    }
    fn on_ready(&mut self, _: &mut ScriptContext<'_, ()>) {
        self.log.push("ready");
    }
    fn on_update(&mut self, _: &mut ScriptContext<'_, ()>) {
        self.log.push("update");
    }
    fn on_late_update(&mut self, _: &mut ScriptContext<'_, ()>) {
        self.log.push("late");
    }
    fn on_disable(&mut self, _: &mut ScriptContext<'_, ()>) {
        self.log.push("disable");
    }
    fn on_unload(&mut self, _: &mut ScriptContext<'_, ()>) {
        self.log.push("unload");
    }
}

struct Tagged {
    tag: &'static str,
    sink: Arc<Mutex<Vec<&'static str>>>,
}

impl Script for Tagged {
    type Services = ();

    fn on_update(&mut self, _: &mut ScriptContext<'_, ()>) {
        self.sink.lock().unwrap().push(self.tag);
    }
}

struct Mover {
    target: Transform,
}

impl Script for Mover {
    type Services = ();

    fn on_update(&mut self, cx: &mut ScriptContext<'_, ()>) {
        let object = cx.object();
        cx.commands().set_transform(object, self.target);
    }
}

fn register_counter(rt: &mut Runtime) {
    rt.register_script("Counter", Counter::default).unwrap();
}

#[test]
fn one_modular_script_on_ten_objects_keeps_ten_states() {
    let mut rt = Runtime::new();
    register_counter(&mut rt);
    let mut objects = Vec::new();
    for i in 0..10 {
        let object = rt.spawn(None, &format!("crate{i}")).unwrap();
        rt.attach_modular(object, "Counter").unwrap();
        rt.commands().load(object);
        objects.push(object);
    }

    rt.tick(0.25, &mut Silent);
    rt.tick(0.5, &mut Silent);
    rt.tick(0.25, &mut Silent);

    let mut owners = Vec::new();
    for &object in &objects {
        let counter = rt.script::<Counter>(object, 0).unwrap();
        assert_eq!(counter.ticks, 3);
        assert_eq!(counter.last_dt, 0.25);
        owners.push(counter.owner.unwrap());
    }
    owners.sort_unstable();
    owners.dedup();
    assert_eq!(owners.len(), 10, "each attachment has its own owner state");
    assert_eq!(rt.world().id(objects[0]).unwrap().get(), owners[0]);
}

#[test]
fn an_inline_script_sees_every_stage_in_order() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "lamp").unwrap();
    rt.attach(object, Recorder::default()).unwrap();
    rt.commands().load(object);
    rt.tick(0.0, &mut Silent);
    assert_eq!(
        rt.script::<Recorder>(object, 0).unwrap().log,
        ["load", "enable", "ready", "update", "late"]
    );

    rt.commands().set_enabled(object, false);
    rt.tick(0.0, &mut Silent);
    rt.commands().unload(object);
    rt.tick(0.0, &mut Silent);

    assert_eq!(
        rt.script::<Recorder>(object, 0).unwrap().log,
        [
            "load", "enable", "ready", "update", "late", "update", "late", "disable", "unload",
        ]
    );
}

#[test]
fn scripts_on_one_object_run_in_attach_order() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "stack").unwrap();
    rt.attach(
        object,
        Tagged {
            tag: "first",
            sink: sink.clone(),
        },
    )
    .unwrap();
    rt.attach(
        object,
        Tagged {
            tag: "second",
            sink: sink.clone(),
        },
    )
    .unwrap();
    rt.commands().load(object);

    rt.tick(0.0, &mut Silent);

    assert_eq!(*sink.lock().unwrap(), vec!["first", "second"]);
}

#[test]
fn scripts_do_not_run_until_their_object_is_loaded() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "idle").unwrap();
    rt.attach(object, Recorder::default()).unwrap();

    rt.tick(0.0, &mut Silent);
    assert!(rt.script::<Recorder>(object, 0).unwrap().log.is_empty());

    rt.commands().load(object);
    rt.tick(0.0, &mut Silent);
    assert_eq!(
        &rt.script::<Recorder>(object, 0).unwrap().log[..3],
        ["load", "enable", "ready"]
    );
}

#[test]
fn a_script_attached_to_a_loaded_object_starts_at_the_next_frame() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "live").unwrap();
    rt.commands().load(object);
    rt.tick(0.0, &mut Silent);

    rt.attach(object, Recorder::default()).unwrap();
    rt.tick(0.0, &mut Silent);

    assert_eq!(
        rt.script::<Recorder>(object, 0).unwrap().log,
        ["load", "enable", "ready", "update", "late"]
    );
}

#[test]
fn scripts_can_queue_commands_that_apply_at_the_next_boundary() {
    let target = Transform {
        translation: [2.0, 0.0, 0.0],
        ..Transform::default()
    };
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "mover").unwrap();
    rt.attach(object, Mover { target }).unwrap();
    rt.commands().load(object);

    rt.tick(0.0, &mut Silent);
    assert_eq!(rt.world().transform(object), Some(Transform::default()));

    rt.tick(0.0, &mut Silent);
    assert_eq!(rt.world().transform(object), Some(target));
}

#[test]
fn despawning_removes_the_object_and_its_scripts() {
    let mut rt = Runtime::new();
    register_counter(&mut rt);
    let object = rt.spawn(None, "gone").unwrap();
    rt.attach_modular(object, "Counter").unwrap();
    rt.commands().load(object);
    rt.tick(0.0, &mut Silent);
    assert_eq!(rt.script_count(object), 1);

    rt.commands().despawn(object);
    rt.tick(0.0, &mut Silent);

    assert_eq!(rt.script_count(object), 0);
    assert!(rt.script::<Counter>(object, 0).is_none());
    assert!(!rt.world().contains(object));
}

#[test]
fn attach_errors_are_reported() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "thing").unwrap();

    assert_eq!(
        rt.attach_modular(object, "Missing").unwrap_err(),
        AttachError::UnknownScript("Missing".to_owned())
    );

    rt.spawn(None, "other").unwrap();
    rt.commands().despawn(object);
    rt.tick(0.0, &mut Silent);
    assert_eq!(
        rt.attach(object, Recorder::default()).unwrap_err(),
        AttachError::StaleObject(object)
    );
}

#[test]
fn registering_a_name_twice_is_an_error() {
    let mut rt = Runtime::new();
    register_counter(&mut rt);
    assert_eq!(
        rt.register_script("Counter", Counter::default).unwrap_err(),
        RegistryError::Duplicate("Counter".to_owned())
    );
}

#[test]
fn script_lookup_by_wrong_type_returns_none() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "typed").unwrap();
    rt.attach(object, Recorder::default()).unwrap();
    assert!(rt.script::<Counter>(object, 0).is_none());
    assert!(rt.script::<Recorder>(object, 0).is_some());
    assert!(rt.script::<Recorder>(object, 1).is_none());
}
