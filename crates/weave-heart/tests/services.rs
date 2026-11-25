use weave_heart::services::{Input, KeyCode, Scene, Time};
use weave_heart::{Listener, ObjectHandle, Runtime, Script, ScriptContext, Stage, StageContext};

struct Silent;

impl Listener for Silent {
    fn on_stage(&mut self, _stage: Stage, _object: ObjectHandle, _cx: &mut StageContext<'_>) {}
}

#[derive(Default)]
struct Probe {
    delta: f32,
    elapsed: f64,
    jump_down: bool,
    found: Option<ObjectHandle>,
    own_path: Option<String>,
}

impl Script for Probe {
    type Services = (Time, Input, Scene);

    fn on_update(&mut self, cx: &mut ScriptContext<'_, Self::Services>) {
        self.delta = cx.time().delta();
        self.elapsed = cx.time().elapsed();
        self.jump_down = cx.input().is_down(KeyCode(32));
        self.found = cx.scene().lookup("world/child");
        self.own_path = cx.scene().path(cx.object());
    }
}

#[test]
fn declared_services_reach_the_script() {
    let mut rt = Runtime::new();
    let world = rt.spawn(None, "world").unwrap();
    let child = rt.spawn(Some(world), "child").unwrap();
    rt.attach(child, Probe::default()).unwrap();
    rt.commands().load(child);
    rt.input_mut().set(KeyCode(32), true);

    rt.tick(0.5, &mut Silent);
    rt.tick(0.25, &mut Silent);

    let probe = rt.script::<Probe>(child, 0).unwrap();
    assert_eq!(probe.delta, 0.25);
    assert_eq!(probe.elapsed, 0.75);
    assert!(probe.jump_down);
    assert_eq!(probe.found, Some(child));
    assert_eq!(probe.own_path.as_deref(), Some("world/child"));
    assert_eq!(rt.elapsed(), 0.75);
}

#[test]
fn input_changes_are_seen_on_the_next_frame() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "player").unwrap();
    rt.attach(object, Probe::default()).unwrap();
    rt.commands().load(object);

    rt.input_mut().set(KeyCode(32), true);
    rt.tick(0.0, &mut Silent);
    assert!(rt.script::<Probe>(object, 0).unwrap().jump_down);

    rt.input_mut().set(KeyCode(32), false);
    rt.tick(0.0, &mut Silent);
    assert!(!rt.script::<Probe>(object, 0).unwrap().jump_down);
}
