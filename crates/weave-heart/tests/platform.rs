use weave_heart::{
    AttachError, Component, Listener, ObjectHandle, PlatformKey, PlatformOverride, Runtime, Script,
    Stage, StageContext,
};

struct Silent;

impl Listener for Silent {
    fn on_stage(&mut self, _stage: Stage, _object: ObjectHandle, _cx: &mut StageContext<'_>) {}
}

#[derive(Default)]
struct WinOnly;

impl Script for WinOnly {}

#[derive(Default)]
struct LinuxOnly;

impl Script for LinuxOnly {}

fn setup(rt: &mut Runtime, overrides: PlatformOverride) -> ObjectHandle {
    rt.register_script("WinOnly", WinOnly::default).unwrap();
    rt.register_script("LinuxOnly", LinuxOnly::default).unwrap();
    let object = rt.spawn(None, "crate").unwrap();
    assert!(rt.insert_component(object, overrides));
    rt.commands().load(object);
    object
}

fn both() -> PlatformOverride {
    PlatformOverride {
        windows: vec!["WinOnly".to_owned()],
        linux: vec!["LinuxOnly".to_owned()],
        android: Vec::new(),
    }
}

#[test]
fn a_windows_runtime_attaches_only_the_windows_scripts() {
    let mut rt = Runtime::for_platform(Some(PlatformKey::Windows));
    let object = setup(&mut rt, both());

    let report = rt.tick(0.0, &mut Silent);

    assert_eq!(rt.script_count(object), 1);
    assert!(rt.script::<WinOnly>(object, 0).is_some());
    assert!(report.override_errors.is_empty());
}

#[test]
fn a_linux_runtime_attaches_only_the_linux_scripts() {
    let mut rt = Runtime::for_platform(Some(PlatformKey::Linux));
    let object = setup(&mut rt, both());

    rt.tick(0.0, &mut Silent);

    assert_eq!(rt.script_count(object), 1);
    assert!(rt.script::<LinuxOnly>(object, 0).is_some());
}

#[test]
fn stripping_for_a_build_leaves_the_override_only_in_that_platform() {
    let mut overrides = both();
    overrides.keep_only(PlatformKey::Linux);
    assert!(overrides.windows.is_empty());
    assert_eq!(overrides.linux, ["LinuxOnly"]);

    let mut windows = Runtime::for_platform(Some(PlatformKey::Windows));
    let win_object = setup(&mut windows, overrides.clone());
    windows.tick(0.0, &mut Silent);
    assert_eq!(windows.script_count(win_object), 0);

    let mut linux = Runtime::for_platform(Some(PlatformKey::Linux));
    let linux_object = setup(&mut linux, overrides);
    linux.tick(0.0, &mut Silent);
    assert_eq!(linux.script_count(linux_object), 1);
}

#[test]
fn an_unsupported_platform_applies_no_overrides() {
    let mut rt = Runtime::for_platform(None);
    let object = setup(&mut rt, both());
    rt.tick(0.0, &mut Silent);
    assert_eq!(rt.script_count(object), 0);
    assert_eq!(rt.platform(), None);
}

#[test]
fn overrides_apply_once_and_survive_reloads() {
    let mut rt = Runtime::for_platform(Some(PlatformKey::Windows));
    let object = setup(&mut rt, both());
    rt.tick(0.0, &mut Silent);

    rt.commands().unload(object);
    rt.tick(0.0, &mut Silent);
    rt.commands().load(object);
    rt.tick(0.0, &mut Silent);

    assert_eq!(rt.script_count(object), 1);
}

#[test]
fn a_missing_override_script_is_reported_not_silent() {
    let mut rt = Runtime::for_platform(Some(PlatformKey::Windows));
    let object = setup(
        &mut rt,
        PlatformOverride {
            windows: vec!["Gone".to_owned()],
            ..PlatformOverride::default()
        },
    );

    let report = rt.tick(0.0, &mut Silent);

    assert_eq!(
        report.override_errors,
        vec![AttachError::UnknownScript("Gone".to_owned())]
    );
    assert_eq!(rt.script_count(object), 0);
}

#[test]
fn the_override_component_round_trips_through_its_schema() {
    let original = both();
    assert_eq!(PlatformOverride::SCHEMA.name, "PlatformOverride");
    assert_eq!(PlatformOverride::SCHEMA.fields.len(), 3);

    let values = original.to_values();
    let borrowed: Vec<_> = values.iter().map(|(n, v)| (*n, v.clone())).collect();
    assert_eq!(PlatformOverride::from_values(&borrowed).unwrap(), original);
}

#[cfg(target_os = "linux")]
#[test]
fn a_linux_build_reports_the_linux_platform() {
    assert_eq!(PlatformKey::current(), Some(PlatformKey::Linux));
    assert_eq!(Runtime::new().platform(), Some(PlatformKey::Linux));
}

#[cfg(target_os = "windows")]
#[test]
fn a_windows_build_reports_the_windows_platform() {
    assert_eq!(PlatformKey::current(), Some(PlatformKey::Windows));
    assert_eq!(Runtime::new().platform(), Some(PlatformKey::Windows));
}
