use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::fmt::Write as _;

use crate::command::{Command, CommandBuffer};
use crate::component::Component;
use crate::error::{ReparentError, SpawnError};
use crate::object::{HandleBuild, ObjectHandle, ObjectId};
use crate::platform::{PlatformKey, PlatformOverride};
use crate::script::{
    AttachError, Attachments, ErasedScript, Factory, RegistryError, Script, ScriptFailure, erase,
    invoke, push_script,
};
use crate::services::{InputFrame, TimeView};
use crate::stage::{Env, Listener, NoListener, Stage, emit};
use crate::world::ObjectWorld;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    StaleHandle,
    NotLoaded,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub command: &'static str,
    pub handle: ObjectHandle,
    pub reason: SkipReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeStats {
    pub objects: usize,
    pub loaded: usize,
    pub active: usize,
    pub scripts: usize,
    pub queued_commands: usize,
}

#[derive(Debug, Default)]
pub struct FrameReport {
    pub skipped: Vec<Skipped>,
    pub spawn_errors: Vec<SpawnError>,
    pub script_failures: Vec<ScriptFailure>,
    pub override_errors: Vec<AttachError>,
    pub reparent_errors: Vec<ReparentError>,
    pub attach_errors: Vec<AttachError>,
}

impl FrameReport {
    pub fn issue_count(&self) -> usize {
        self.skipped.len()
            + self.spawn_errors.len()
            + self.script_failures.len()
            + self.override_errors.len()
            + self.reparent_errors.len()
            + self.attach_errors.len()
    }

    pub fn is_clean(&self) -> bool {
        self.issue_count() == 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogEvent {
    Skipped(Skipped),
    ScriptFailed(ScriptFailure),
    Rejected(String),
}

impl fmt::Display for LogEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Skipped(skipped) => write!(
                f,
                "skipped {} on {:?}: {:?}",
                skipped.command, skipped.handle, skipped.reason
            ),
            Self::ScriptFailed(failure) => write!(f, "{failure}"),
            Self::Rejected(text) => write!(f, "{text}"),
        }
    }
}

type LogSink = Box<dyn FnMut(&LogEvent) + Send>;

struct Core {
    world: ObjectWorld,
    commands: CommandBuffer,
    input: InputFrame,
    time: TimeView,
}

struct Entry {
    handle: ObjectHandle,
    enabled: bool,
    ready: bool,
}

struct Unload {
    handle: ObjectHandle,
    despawn: bool,
}

#[derive(Default)]
struct Lifecycle {
    loaded: BTreeMap<ObjectId, Entry>,
    active: BTreeMap<ObjectId, ObjectHandle>,
    pending_ready: BTreeSet<ObjectId>,
    pending_load: Vec<ObjectHandle>,
    pending_unload: Vec<Unload>,
    pending_enable: BTreeMap<ObjectId, (ObjectHandle, bool)>,
}

struct Scripts {
    attachments: Attachments,
    registry: HashMap<String, Factory>,
    platform: Option<PlatformKey>,
    overrides_applied: HashSet<ObjectHandle, HandleBuild>,
}

pub struct Runtime {
    core: Core,
    life: Lifecycle,
    scripts: Scripts,
    log_sink: Option<LogSink>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self::for_platform(PlatformKey::current())
    }

    pub fn for_platform(platform: Option<PlatformKey>) -> Self {
        Self {
            core: Core {
                world: ObjectWorld::new(),
                commands: CommandBuffer::default(),
                input: InputFrame::default(),
                time: TimeView::new(0.0, 0.0),
            },
            life: Lifecycle::default(),
            scripts: Scripts {
                attachments: Attachments::default(),
                registry: HashMap::new(),
                platform,
                overrides_applied: HashSet::default(),
            },
            log_sink: None,
        }
    }

    pub fn set_log_sink(&mut self, sink: impl FnMut(&LogEvent) + Send + 'static) {
        self.log_sink = Some(Box::new(sink));
    }

    pub fn clear_log_sink(&mut self) {
        self.log_sink = None;
    }

    pub fn platform(&self) -> Option<PlatformKey> {
        self.scripts.platform
    }

    pub fn world(&self) -> &ObjectWorld {
        &self.core.world
    }

    pub fn spawn(
        &mut self,
        parent: Option<ObjectHandle>,
        name: &str,
    ) -> Result<ObjectHandle, SpawnError> {
        self.core.world.spawn(parent, name)
    }

    pub fn insert_component<C: Component>(&mut self, object: ObjectHandle, component: C) -> bool {
        self.core.world.insert_component(object, component)
    }

    pub fn reparent(
        &mut self,
        object: ObjectHandle,
        parent: Option<ObjectHandle>,
    ) -> Result<(), ReparentError> {
        self.core.world.reparent(object, parent)
    }

    pub fn commands(&mut self) -> &mut CommandBuffer {
        &mut self.core.commands
    }

    pub fn input(&self) -> &InputFrame {
        &self.core.input
    }

    pub fn input_mut(&mut self) -> &mut InputFrame {
        &mut self.core.input
    }

    pub fn elapsed(&self) -> f64 {
        self.core.time.elapsed()
    }

    pub fn is_loaded(&self, object: ObjectHandle) -> bool {
        self.entry(object).is_some()
    }

    pub fn is_enabled(&self, object: ObjectHandle) -> bool {
        self.entry(object).is_some_and(|entry| entry.enabled)
    }

    pub fn is_ready(&self, object: ObjectHandle) -> bool {
        self.entry(object).is_some_and(|entry| entry.ready)
    }

    pub fn loaded_count(&self) -> usize {
        self.life.loaded.len()
    }

    pub fn register_script<S, T, F>(
        &mut self,
        name: impl Into<String>,
        factory: F,
    ) -> Result<(), RegistryError>
    where
        S: 'static,
        T: Script<S>,
        F: Fn() -> T + 'static,
    {
        let name = name.into();
        if self.scripts.registry.contains_key(&name) {
            return Err(RegistryError::Duplicate(name));
        }
        let boxed: Factory = Box::new(move || erase::<S, T>(factory()));
        self.scripts.registry.insert(name, boxed);
        Ok(())
    }

    pub fn attach<S, T>(&mut self, object: ObjectHandle, script: T) -> Result<usize, AttachError>
    where
        S: 'static,
        T: Script<S>,
    {
        self.attach_boxed(object, erase::<S, T>(script))
    }

    pub fn attach_modular(
        &mut self,
        object: ObjectHandle,
        name: &str,
    ) -> Result<usize, AttachError> {
        let script = match self.scripts.registry.get(name) {
            Some(factory) => factory(),
            None => return Err(AttachError::UnknownScript(name.to_owned())),
        };
        self.attach_boxed(object, script)
    }

    pub fn script_count(&self, object: ObjectHandle) -> usize {
        self.scripts
            .attachments
            .by_object
            .get(&object)
            .map_or(0, Vec::len)
    }

    pub fn script<T: 'static>(&self, object: ObjectHandle, index: usize) -> Option<&T> {
        let inner = self
            .scripts
            .attachments
            .by_object
            .get(&object)?
            .get(index)?
            .script
            .inner();
        inner.downcast_ref::<T>()
    }

    pub fn update(&mut self, dt: f32) -> FrameReport {
        self.tick(dt, &mut NoListener)
    }

    pub fn stats(&self) -> RuntimeStats {
        RuntimeStats {
            objects: self.core.world.len(),
            loaded: self.life.loaded.len(),
            active: self.life.active.len(),
            scripts: self
                .scripts
                .attachments
                .by_object
                .values()
                .map(Vec::len)
                .sum(),
            queued_commands: self.core.commands.len(),
        }
    }

    pub fn describe(&self) -> String {
        let mut out = String::new();
        for root in self.core.world.roots() {
            self.describe_into(root, 0, &mut out);
        }
        out
    }

    pub fn tick(&mut self, dt: f32, listener: &mut dyn Listener) -> FrameReport {
        let elapsed = self.core.time.elapsed() + f64::from(dt);
        self.core.time = TimeView::new(dt, elapsed);
        let mut report = FrameReport::default();
        self.intake(&mut report);
        self.late_start(&mut report);
        self.load(listener, &mut report);
        self.enable(listener, &mut report);
        self.ready(listener, &mut report);
        self.frame_stage(Stage::Update, listener, &mut report);
        self.frame_stage(Stage::LateUpdate, listener, &mut report);
        self.disable(listener, &mut report);
        self.unload(listener, &mut report);
        self.flush_log(&report);
        report
    }

    fn entry(&self, object: ObjectHandle) -> Option<&Entry> {
        let id = self.core.world.id(object)?;
        self.life.loaded.get(&id)
    }

    fn describe_into(&self, object: ObjectHandle, depth: usize, out: &mut String) {
        let name = self
            .core
            .world
            .name(object)
            .map(|name| name.to_string())
            .unwrap_or_default();
        let state = match self.entry(object) {
            None => "unloaded",
            Some(entry) if !entry.enabled => "disabled",
            Some(entry) if !entry.ready => "loaded",
            Some(_) => "ready",
        };
        let scripts = self.script_count(object);
        let _ = writeln!(
            out,
            "{:indent$}{name} [{state}, {scripts} scripts]",
            "",
            indent = depth * 2
        );
        for child in self.core.world.children(object).into_iter().flatten() {
            self.describe_into(child, depth + 1, out);
        }
    }

    fn attach_boxed(
        &mut self,
        object: ObjectHandle,
        script: Box<dyn ErasedScript>,
    ) -> Result<usize, AttachError> {
        let id = self
            .core
            .world
            .id(object)
            .ok_or(AttachError::StaleObject(object))?;
        let index = push_script(&mut self.scripts.attachments, object, script);
        if self.life.loaded.contains_key(&id) {
            self.scripts.attachments.unstarted.insert(id, object);
        }
        Ok(index)
    }

    fn flush_log(&mut self, report: &FrameReport) {
        if self.log_sink.is_none() && !cfg!(debug_assertions) {
            return;
        }
        for skipped in &report.skipped {
            self.log(LogEvent::Skipped(*skipped));
        }
        for failure in &report.script_failures {
            self.log(LogEvent::ScriptFailed(failure.clone()));
        }
        for error in &report.spawn_errors {
            self.log(LogEvent::Rejected(format!("spawn failed: {error}")));
        }
        for error in &report.reparent_errors {
            self.log(LogEvent::Rejected(format!("reparent failed: {error}")));
        }
        for error in &report.attach_errors {
            self.log(LogEvent::Rejected(format!("attach failed: {error}")));
        }
        for error in &report.override_errors {
            self.log(LogEvent::Rejected(format!(
                "platform override failed: {error}"
            )));
        }
    }

    fn log(&mut self, event: LogEvent) {
        match self.log_sink.as_mut() {
            Some(sink) => sink(&event),
            None => eprintln!("{event}"),
        }
    }

    fn intake(&mut self, report: &mut FrameReport) {
        let queued = std::mem::take(&mut self.core.commands.queue);
        for command in queued {
            match command {
                Command::Spawn { parent, name } => {
                    if let Err(error) = self.core.world.spawn(parent, &name) {
                        report.spawn_errors.push(error);
                    }
                }
                Command::Despawn(object) => self.life.pending_unload.push(Unload {
                    handle: object,
                    despawn: true,
                }),
                Command::Unload(object) => self.life.pending_unload.push(Unload {
                    handle: object,
                    despawn: false,
                }),
                Command::Load(object) => self.life.pending_load.push(object),
                Command::Reparent { object, parent } => {
                    if let Err(error) = self.core.world.reparent(object, parent) {
                        report.reparent_errors.push(error);
                    }
                }
                Command::Edit { object, name, edit } => {
                    if !self.core.world.contains(object) {
                        report.skipped.push(Skipped {
                            command: name,
                            handle: object,
                            reason: SkipReason::StaleHandle,
                        });
                    } else if !edit(&mut self.core.world, object) {
                        report.skipped.push(Skipped {
                            command: name,
                            handle: object,
                            reason: SkipReason::Rejected,
                        });
                    }
                }
                Command::Attach { object, script } => {
                    if let Err(error) = self.attach_boxed(object, script) {
                        report.attach_errors.push(error);
                    }
                }
                Command::AttachModular { object, name } => match self.scripts.registry.get(&name) {
                    Some(factory) => {
                        let script = factory();
                        if let Err(error) = self.attach_boxed(object, script) {
                            report.attach_errors.push(error);
                        }
                    }
                    None => report.attach_errors.push(AttachError::UnknownScript(name)),
                },
                Command::SetEnabled { object, enabled } => match self.core.world.id(object) {
                    Some(id) => {
                        self.life.pending_enable.insert(id, (object, enabled));
                    }
                    None => report.skipped.push(Skipped {
                        command: "set_enabled",
                        handle: object,
                        reason: SkipReason::StaleHandle,
                    }),
                },
                Command::SetTransform { object, transform } => {
                    if !self.core.world.set_transform(object, transform) {
                        report.skipped.push(Skipped {
                            command: "set_transform",
                            handle: object,
                            reason: SkipReason::StaleHandle,
                        });
                    }
                }
            }
        }
    }

    fn take_enable_requests(&mut self, enable: bool) -> Vec<ObjectHandle> {
        let ids: Vec<ObjectId> = self
            .life
            .pending_enable
            .iter()
            .filter(|(_, (_, on))| *on == enable)
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter_map(|id| {
                self.life
                    .pending_enable
                    .remove(&id)
                    .map(|(object, _)| object)
            })
            .collect()
    }

    fn late_start(&mut self, report: &mut FrameReport) {
        let pending = std::mem::take(&mut self.scripts.attachments.unstarted);
        let Self {
            core,
            life,
            scripts,
            ..
        } = self;
        let mut env = env(core, &mut report.script_failures);
        for (id, object) in pending {
            if !env.world.contains(object) {
                continue;
            }
            let Some(entry) = life.loaded.get(&id) else {
                continue;
            };
            let (enabled, ready) = (entry.enabled, entry.ready);
            let Some(list) = scripts.attachments.by_object.get_mut(&object) else {
                continue;
            };
            for attachment in list.iter_mut().filter(|attachment| !attachment.started) {
                invoke(attachment, object, Stage::Load, &mut env);
                if enabled {
                    invoke(attachment, object, Stage::Enable, &mut env);
                }
                if ready {
                    invoke(attachment, object, Stage::Ready, &mut env);
                }
            }
        }
    }

    fn load(&mut self, listener: &mut dyn Listener, report: &mut FrameReport) {
        let mut requests = std::mem::take(&mut self.life.pending_load);
        requests.sort_by_key(|object| self.core.world.id(*object));
        let Self {
            core,
            life,
            scripts,
            ..
        } = self;
        let mut env = env(core, &mut report.script_failures);
        for root in requests {
            if !env.world.contains(root) {
                report.skipped.push(Skipped {
                    command: "load",
                    handle: root,
                    reason: SkipReason::StaleHandle,
                });
                continue;
            }
            for object in subtree_preorder(env.world, root) {
                let Some(id) = env.world.id(object) else {
                    continue;
                };
                if life.loaded.contains_key(&id) {
                    continue;
                }
                life.loaded.insert(
                    id,
                    Entry {
                        handle: object,
                        enabled: true,
                        ready: false,
                    },
                );
                life.pending_ready.insert(id);
                apply_platform_overrides(object, env.world, scripts, &mut report.override_errors);
                emit(
                    listener,
                    &mut scripts.attachments,
                    Stage::Load,
                    object,
                    &mut env,
                );
                emit(
                    listener,
                    &mut scripts.attachments,
                    Stage::Enable,
                    object,
                    &mut env,
                );
            }
        }
    }

    fn enable(&mut self, listener: &mut dyn Listener, report: &mut FrameReport) {
        let requests = self.take_enable_requests(true);
        let Self {
            core,
            life,
            scripts,
            ..
        } = self;
        let mut env = env(core, &mut report.script_failures);
        for object in requests {
            let Some(id) = env.world.id(object) else {
                report.skipped.push(Skipped {
                    command: "set_enabled",
                    handle: object,
                    reason: SkipReason::StaleHandle,
                });
                continue;
            };
            let Some(entry) = life.loaded.get_mut(&id) else {
                report.skipped.push(Skipped {
                    command: "set_enabled",
                    handle: object,
                    reason: SkipReason::NotLoaded,
                });
                continue;
            };
            if entry.enabled {
                continue;
            }
            entry.enabled = true;
            if entry.ready {
                life.active.insert(id, object);
            } else {
                life.pending_ready.insert(id);
            }
            emit(
                listener,
                &mut scripts.attachments,
                Stage::Enable,
                object,
                &mut env,
            );
        }
    }

    fn ready(&mut self, listener: &mut dyn Listener, report: &mut FrameReport) {
        let ids = std::mem::take(&mut self.life.pending_ready);
        let Self {
            core,
            life,
            scripts,
            ..
        } = self;
        let mut env = env(core, &mut report.script_failures);
        for id in ids {
            let Some(entry) = life.loaded.get_mut(&id) else {
                continue;
            };
            if entry.ready {
                continue;
            }
            if !entry.enabled {
                life.pending_ready.insert(id);
                continue;
            }
            entry.ready = true;
            let object = entry.handle;
            life.active.insert(id, object);
            emit(
                listener,
                &mut scripts.attachments,
                Stage::Ready,
                object,
                &mut env,
            );
        }
    }

    fn frame_stage(&mut self, stage: Stage, listener: &mut dyn Listener, report: &mut FrameReport) {
        let Self {
            core,
            life,
            scripts,
            ..
        } = self;
        let mut env = env(core, &mut report.script_failures);
        for object in life.active.values().copied() {
            emit(listener, &mut scripts.attachments, stage, object, &mut env);
        }
    }

    fn disable(&mut self, listener: &mut dyn Listener, report: &mut FrameReport) {
        let requests = self.take_enable_requests(false);
        let Self {
            core,
            life,
            scripts,
            ..
        } = self;
        let mut env = env(core, &mut report.script_failures);
        for object in requests {
            let Some(id) = env.world.id(object) else {
                report.skipped.push(Skipped {
                    command: "set_enabled",
                    handle: object,
                    reason: SkipReason::StaleHandle,
                });
                continue;
            };
            let Some(entry) = life.loaded.get_mut(&id) else {
                report.skipped.push(Skipped {
                    command: "set_enabled",
                    handle: object,
                    reason: SkipReason::NotLoaded,
                });
                continue;
            };
            if !entry.enabled {
                continue;
            }
            entry.enabled = false;
            life.active.remove(&id);
            life.pending_ready.remove(&id);
            emit(
                listener,
                &mut scripts.attachments,
                Stage::Disable,
                object,
                &mut env,
            );
        }
    }

    fn unload(&mut self, listener: &mut dyn Listener, report: &mut FrameReport) {
        let requests = std::mem::take(&mut self.life.pending_unload);
        let Self {
            core,
            life,
            scripts,
            ..
        } = self;
        for request in requests {
            if !core.world.contains(request.handle) {
                report.skipped.push(Skipped {
                    command: if request.despawn { "despawn" } else { "unload" },
                    handle: request.handle,
                    reason: SkipReason::StaleHandle,
                });
                continue;
            }
            let subtree = subtree_postorder(&core.world, request.handle);
            {
                let mut env = env(core, &mut report.script_failures);
                for object in subtree.iter().copied() {
                    let Some(id) = env.world.id(object) else {
                        continue;
                    };
                    let Some(entry) = life.loaded.remove(&id) else {
                        continue;
                    };
                    life.active.remove(&id);
                    life.pending_ready.remove(&id);
                    if entry.enabled {
                        emit(
                            listener,
                            &mut scripts.attachments,
                            Stage::Disable,
                            object,
                            &mut env,
                        );
                    }
                    emit(
                        listener,
                        &mut scripts.attachments,
                        Stage::Unload,
                        object,
                        &mut env,
                    );
                }
            }
            if request.despawn {
                for object in &subtree {
                    scripts.attachments.by_object.remove(object);
                    scripts.overrides_applied.remove(object);
                }
                core.world.despawn(request.handle);
            }
        }
    }
}

fn env<'a>(core: &'a mut Core, failures: &'a mut Vec<ScriptFailure>) -> Env<'a> {
    Env {
        world: &core.world,
        commands: &mut core.commands,
        input: &core.input,
        time: core.time,
        failures,
    }
}

fn apply_platform_overrides(
    object: ObjectHandle,
    world: &ObjectWorld,
    scripts: &mut Scripts,
    errors: &mut Vec<AttachError>,
) {
    let Some(platform) = scripts.platform else {
        return;
    };
    if !world.has_component::<PlatformOverride>(object) || !scripts.overrides_applied.insert(object)
    {
        return;
    }
    let names = world
        .with_component::<PlatformOverride, _>(object, |overrides| {
            overrides.scripts(platform).to_vec()
        })
        .unwrap_or_default();
    for name in names {
        match scripts.registry.get(&name) {
            Some(factory) => {
                push_script(&mut scripts.attachments, object, factory());
            }
            None => errors.push(AttachError::UnknownScript(name)),
        }
    }
}

fn subtree_preorder(world: &ObjectWorld, root: ObjectHandle) -> Vec<ObjectHandle> {
    let mut order = Vec::new();
    let mut stack = vec![root];
    while let Some(object) = stack.pop() {
        order.push(object);
        let children: Vec<ObjectHandle> = world.children(object).into_iter().flatten().collect();
        stack.extend(children.into_iter().rev());
    }
    order
}

fn subtree_postorder(world: &ObjectWorld, root: ObjectHandle) -> Vec<ObjectHandle> {
    let mut order = Vec::new();
    let mut stack = vec![root];
    while let Some(object) = stack.pop() {
        order.push(object);
        let children: Vec<ObjectHandle> = world.children(object).into_iter().flatten().collect();
        stack.extend(children);
    }
    order.reverse();
    order
}
