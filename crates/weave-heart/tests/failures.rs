use weave_heart::{Listener, ObjectHandle, Runtime, Script, Stage, StageContext};

struct Silent;

impl Listener for Silent {
    fn on_stage(&mut self, _stage: Stage, _object: ObjectHandle, _cx: &mut StageContext<'_>) {}
}

#[derive(Default)]
struct Fragile {
    calls: u32,
}

impl Script for Fragile {
    fn on_update(&mut self, _cx: &mut weave_heart::ScriptContext<'_>) {
        self.calls += 1;
        if self.calls == 2 {
            panic!("count {}", self.calls);
        }
    }
}

#[derive(Default)]
struct Steady {
    updates: u32,
}

impl Script for Steady {
    fn on_update(&mut self, _cx: &mut weave_heart::ScriptContext<'_>) {
        self.updates += 1;
    }
}

#[derive(Default)]
struct BadLoad;

impl Script for BadLoad {
    fn on_load(&mut self, _cx: &mut weave_heart::ScriptContext<'_>) {
        panic!("bad load");
    }
}

#[test]
fn a_panicking_script_is_disabled_and_its_siblings_keep_running() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "crate").unwrap();
    rt.attach(object, Fragile::default()).unwrap();
    rt.attach(object, Steady::default()).unwrap();
    rt.commands().load(object);

    let mut failures = Vec::new();
    for _ in 0..4 {
        let report = rt.tick(0.0, &mut Silent);
        failures.extend(report.script_failures);
    }

    assert_eq!(rt.script::<Fragile>(object, 0).unwrap().calls, 2);
    assert_eq!(rt.script::<Steady>(object, 1).unwrap().updates, 4);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].path, "crate");
    assert_eq!(failures[0].stage, Stage::Update);
    assert_eq!(failures[0].message, "count 2");
    assert!(failures[0].to_string().contains("disabled"));
}

#[test]
fn a_panic_during_load_is_reported_and_the_script_never_runs_again() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "bad").unwrap();
    rt.attach(object, BadLoad).unwrap();
    rt.commands().load(object);

    let first = rt.tick(0.0, &mut Silent);
    let second = rt.tick(0.0, &mut Silent);

    assert_eq!(first.script_failures.len(), 1);
    assert_eq!(first.script_failures[0].stage, Stage::Load);
    assert_eq!(first.script_failures[0].message, "bad load");
    assert!(second.script_failures.is_empty());
    assert!(rt.is_ready(object));
}

#[test]
fn a_failure_on_one_object_does_not_touch_another() {
    let mut rt = Runtime::new();
    let bad = rt.spawn(None, "bad").unwrap();
    let good = rt.spawn(None, "good").unwrap();
    rt.attach(bad, BadLoad).unwrap();
    rt.attach(good, Steady::default()).unwrap();
    rt.commands().load(bad);
    rt.commands().load(good);

    rt.tick(0.0, &mut Silent);
    rt.tick(0.0, &mut Silent);

    assert_eq!(rt.script::<Steady>(good, 0).unwrap().updates, 2);
}
