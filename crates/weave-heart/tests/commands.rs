use weave_heart::{
    AttachError, Listener, Name, ObjectHandle, Runtime, Script, ScriptContext, Stage, StageContext,
    Transform,
};
use weave_macros::Component as DeriveComponent;

struct Silent;

impl Listener for Silent {
    fn on_stage(&mut self, _stage: Stage, _object: ObjectHandle, _cx: &mut StageContext<'_>) {}
}

#[derive(Debug, Clone, PartialEq, DeriveComponent)]
struct Health {
    current: i64,
    max: i64,
}

#[derive(Default)]
struct Counter {
    ticks: u32,
}

impl Script for Counter {
    fn on_update(&mut self, _cx: &mut ScriptContext<'_>) {
        self.ticks += 1;
    }
}

struct Attacher {
    target: ObjectHandle,
    done: bool,
}

impl Script for Attacher {
    fn on_update(&mut self, cx: &mut ScriptContext<'_>) {
        if !self.done {
            self.done = true;
            let target = self.target;
            cx.commands().attach_modular(target, "Counter");
        }
    }
}

#[test]
fn components_can_be_inserted_edited_and_removed_from_the_boundary() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "unit").unwrap();

    rt.commands().insert_component(
        object,
        Health {
            current: 5,
            max: 10,
        },
    );
    rt.tick(0.0, &mut Silent);
    assert_eq!(
        rt.world()
            .with_component::<Health, _>(object, |h| h.current),
        Some(5)
    );

    rt.commands()
        .edit_component::<Health>(object, |h| h.current -= 2);
    rt.tick(0.0, &mut Silent);
    assert_eq!(
        rt.world()
            .with_component::<Health, _>(object, |h| h.current),
        Some(3)
    );

    rt.commands().remove_component::<Health>(object);
    rt.tick(0.0, &mut Silent);
    assert!(!rt.world().has_component::<Health>(object));
}

#[test]
fn a_component_edit_on_a_missing_component_is_reported() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "bare").unwrap();
    rt.commands()
        .edit_component::<Health>(object, |h| h.current = 0);
    let report = rt.tick(0.0, &mut Silent);
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].command, "edit_component");
}

#[test]
fn reserved_components_are_rejected_from_commands_too() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "named").unwrap();
    rt.commands()
        .insert_component(object, Name { value: "x".into() });
    rt.commands().remove_component::<Transform>(object);
    let report = rt.tick(0.0, &mut Silent);
    assert_eq!(report.skipped.len(), 2);
    assert!(
        report
            .skipped
            .iter()
            .all(|s| s.reason == weave_heart::SkipReason::Rejected)
    );
    assert_eq!(rt.world().name(object).as_deref(), Some("named"));
    assert_eq!(rt.world().transform(object), Some(Transform::default()));
}

#[test]
fn a_script_can_attach_another_script_at_the_next_boundary() {
    let mut rt = Runtime::new();
    rt.register_script("Counter", Counter::default).unwrap();
    let target = rt.spawn(None, "target").unwrap();
    let driver = rt.spawn(None, "driver").unwrap();
    rt.attach(
        driver,
        Attacher {
            target,
            done: false,
        },
    )
    .unwrap();
    rt.commands().load(target);
    rt.commands().load(driver);

    rt.tick(0.0, &mut Silent);
    assert_eq!(rt.script_count(target), 0);

    let report = rt.tick(0.0, &mut Silent);
    assert!(report.attach_errors.is_empty());
    assert_eq!(rt.script_count(target), 1);
    rt.tick(0.0, &mut Silent);
    assert_eq!(rt.script::<Counter>(target, 0).unwrap().ticks, 2);
}

#[test]
fn a_script_can_attach_an_unknown_modular_script_and_the_error_is_reported() {
    let mut rt = Runtime::new();
    let target = rt.spawn(None, "target").unwrap();
    rt.commands().attach_modular(target, "Missing");
    rt.commands().load(target);
    let report = rt.tick(0.0, &mut Silent);
    assert_eq!(
        report.attach_errors,
        vec![AttachError::UnknownScript("Missing".to_owned())]
    );
}
