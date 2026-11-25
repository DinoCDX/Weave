use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::command::{Command, CommandBuffer};
use crate::component::Component;
use crate::error::SpawnError;
use crate::object::{ObjectHandle, ObjectId};
use crate::platform::{PlatformKey, PlatformOverride};
use crate::script::{
    AttachError, Attachments, ErasedScript, Factory, RegistryError, Script, ScriptFailure, invoke,
    push_script,
};
use crate::services::{InputFrame, TimeView};
use crate::stage::{Env, Listener, Stage, emit};
use crate::world::ObjectWorld;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    StaleHandle,
    NotLoaded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub command: &'static str,
    pub handle: ObjectHandle,
    pub reason: SkipReason,
}

#[derive(Debug, Default)]
pub struct FrameReport {
    pub skipped: Vec<Skipped>,
    pub spawn_errors: Vec<SpawnError>,
    pub script_failures: Vec<ScriptFailure>,
    pub override_errors: Vec<AttachError>,
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

pub struct Runtime {
    world: ObjectWorld,
    commands: CommandBuffer,
    loaded: BTreeMap<ObjectId, Entry>,
    active: BTreeMap<ObjectId, ObjectHandle>,
    pending_ready: BTreeSet<ObjectId>,
    pending_load: Vec<ObjectHandle>,
    pending_unload: Vec<Unload>,
    pending_enable: BTreeMap<ObjectId, (ObjectHandle, bool)>,
    attachments: Attachments,
    registry: HashMap<String, Factory>,
    platform: Option<PlatformKey>,
    overrides_applied: HashSet<ObjectHandle>,
    input: InputFrame,
    time: TimeView,
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
            world: ObjectWorld::new(),
            commands: CommandBuffer::default(),
            loaded: BTreeMap::new(),
            active: BTreeMap::new(),
            pending_ready: BTreeSet::new(),
            pending_load: Vec::new(),
            pending_unload: Vec::new(),
            pending_enable: BTreeMap::new(),
            attachments: Attachments::default(),
            registry: HashMap::new(),
            platform,
            overrides_applied: HashSet::new(),
            input: InputFrame::default(),
            time: TimeView::new(0.0, 0.0),
        }
    }

    pub fn platform(&self) -> Option<PlatformKey> {
        self.platform
    }

    pub fn world(&self) -> &ObjectWorld {
        &self.world
    }

    pub fn spawn(
        &mut self,
        parent: Option<ObjectHandle>,
        name: &str,
    ) -> Result<ObjectHandle, SpawnError> {
        self.world.spawn(parent, name)
    }

    pub fn insert_component<C: Component>(&mut self, object: ObjectHandle, component: C) -> bool {
        self.world.insert_component(object, component)
    }

    pub fn commands(&mut self) -> &mut CommandBuffer {
        &mut self.commands
    }

    pub fn input(&self) -> &InputFrame {
        &self.input
    }

    pub fn input_mut(&mut self) -> &mut InputFrame {
        &mut self.input
    }

    pub fn elapsed(&self) -> f64 {
        self.time.elapsed()
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
        self.loaded.len()
    }

    pub fn register_script<S, F>(
        &mut self,
        name: impl Into<String>,
        factory: F,
    ) -> Result<(), RegistryError>
    where
        S: Script,
        F: Fn() -> S + 'static,
    {
        let name = name.into();
        if self.registry.contains_key(&name) {
            return Err(RegistryError::Duplicate(name));
        }
        let boxed: Factory = Box::new(move || -> Box<dyn ErasedScript> { Box::new(factory()) });
        self.registry.insert(name, boxed);
        Ok(())
    }

    pub fn attach<S: Script>(
        &mut self,
        object: ObjectHandle,
        script: S,
    ) -> Result<usize, AttachError> {
        self.attach_boxed(object, Box::new(script))
    }

    pub fn attach_modular(
        &mut self,
        object: ObjectHandle,
        name: &str,
    ) -> Result<usize, AttachError> {
        let script = match self.registry.get(name) {
            Some(factory) => factory(),
            None => return Err(AttachError::UnknownScript(name.to_owned())),
        };
        self.attach_boxed(object, script)
    }

    pub fn script_count(&self, object: ObjectHandle) -> usize {
        self.attachments.by_object.get(&object).map_or(0, Vec::len)
    }

    pub fn script<T: Script>(&self, object: ObjectHandle, index: usize) -> Option<&T> {
        let erased: &dyn ErasedScript = self
            .attachments
            .by_object
            .get(&object)?
            .get(index)?
            .script
            .as_ref();
        let any: &dyn Any = erased;
        any.downcast_ref::<T>()
    }

    pub fn tick(&mut self, dt: f32, listener: &mut dyn Listener) -> FrameReport {
        let elapsed = self.time.elapsed() + f64::from(dt);
        self.time = TimeView::new(dt, elapsed);
        let mut report = FrameReport::default();
        self.intake(&mut report);
        self.run_late_start(&mut report);
        self.run_load(listener, &mut report);
        self.run_enable(listener, &mut report);
        self.run_ready(listener, &mut report);
        self.run_frame_stage(Stage::Update, listener, &mut report);
        self.run_frame_stage(Stage::LateUpdate, listener, &mut report);
        self.run_disable(listener, &mut report);
        self.run_unload(listener, &mut report);
        report
    }

    fn entry(&self, object: ObjectHandle) -> Option<&Entry> {
        let id = self.world.id(object)?;
        self.loaded.get(&id)
    }

    fn attach_boxed(
        &mut self,
        object: ObjectHandle,
        script: Box<dyn ErasedScript>,
    ) -> Result<usize, AttachError> {
        let id = self
            .world
            .id(object)
            .ok_or(AttachError::StaleObject(object))?;
        let index = push_script(&mut self.attachments, object, script);
        if self.loaded.contains_key(&id) {
            self.attachments.unstarted.insert(id, object);
        }
        Ok(index)
    }

    fn intake(&mut self, report: &mut FrameReport) {
        let queued = std::mem::take(&mut self.commands.queue);
        for command in queued {
            match command {
                Command::Spawn { parent, name } => {
                    if let Err(error) = self.world.spawn(parent, &name) {
                        report.spawn_errors.push(error);
                    }
                }
                Command::Despawn(object) => self.pending_unload.push(Unload {
                    handle: object,
                    despawn: true,
                }),
                Command::Unload(object) => self.pending_unload.push(Unload {
                    handle: object,
                    despawn: false,
                }),
                Command::Load(object) => self.pending_load.push(object),
                Command::SetEnabled { object, enabled } => match self.world.id(object) {
                    Some(id) => {
                        self.pending_enable.insert(id, (object, enabled));
                    }
                    None => report.skipped.push(Skipped {
                        command: "set_enabled",
                        handle: object,
                        reason: SkipReason::StaleHandle,
                    }),
                },
                Command::SetTransform { object, transform } => {
                    if !self.world.set_transform(object, transform) {
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
            .pending_enable
            .iter()
            .filter(|(_, (_, on))| *on == enable)
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter_map(|id| self.pending_enable.remove(&id).map(|(object, _)| object))
            .collect()
    }

    fn run_late_start(&mut self, report: &mut FrameReport) {
        let pending = std::mem::take(&mut self.attachments.unstarted);
        let Self {
            world,
            commands,
            input,
            time,
            loaded,
            attachments,
            ..
        } = self;
        let mut env = Env {
            world: &*world,
            commands: &mut *commands,
            input: &*input,
            time: *time,
            failures: &mut report.script_failures,
        };
        for (id, object) in pending {
            if !world.contains(object) {
                continue;
            }
            let Some(entry) = loaded.get(&id) else {
                continue;
            };
            let (enabled, ready) = (entry.enabled, entry.ready);
            let Some(list) = attachments.by_object.get_mut(&object) else {
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

    fn run_load(&mut self, listener: &mut dyn Listener, report: &mut FrameReport) {
        let mut requests = std::mem::take(&mut self.pending_load);
        requests.sort_by_key(|object| self.world.id(*object));
        let Self {
            world,
            commands,
            input,
            time,
            loaded,
            pending_ready,
            attachments,
            registry,
            platform,
            overrides_applied,
            ..
        } = self;
        let mut env = Env {
            world: &*world,
            commands: &mut *commands,
            input: &*input,
            time: *time,
            failures: &mut report.script_failures,
        };
        for root in requests {
            if !world.contains(root) {
                report.skipped.push(Skipped {
                    command: "load",
                    handle: root,
                    reason: SkipReason::StaleHandle,
                });
                continue;
            }
            for object in subtree_preorder(world, root) {
                let Some(id) = world.id(object) else {
                    continue;
                };
                if loaded.contains_key(&id) {
                    continue;
                }
                loaded.insert(
                    id,
                    Entry {
                        handle: object,
                        enabled: true,
                        ready: false,
                    },
                );
                pending_ready.insert(id);
                apply_platform_overrides(
                    object,
                    world,
                    *platform,
                    registry,
                    attachments,
                    overrides_applied,
                    &mut report.override_errors,
                );
                emit(listener, attachments, Stage::Load, object, &mut env);
                emit(listener, attachments, Stage::Enable, object, &mut env);
            }
        }
    }

    fn run_enable(&mut self, listener: &mut dyn Listener, report: &mut FrameReport) {
        let requests = self.take_enable_requests(true);
        let Self {
            world,
            commands,
            input,
            time,
            loaded,
            active,
            pending_ready,
            attachments,
            ..
        } = self;
        let mut env = Env {
            world: &*world,
            commands: &mut *commands,
            input: &*input,
            time: *time,
            failures: &mut report.script_failures,
        };
        for object in requests {
            let Some(id) = world.id(object) else {
                report.skipped.push(Skipped {
                    command: "set_enabled",
                    handle: object,
                    reason: SkipReason::StaleHandle,
                });
                continue;
            };
            let Some(entry) = loaded.get_mut(&id) else {
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
                active.insert(id, object);
            } else {
                pending_ready.insert(id);
            }
            emit(listener, attachments, Stage::Enable, object, &mut env);
        }
    }

    fn run_ready(&mut self, listener: &mut dyn Listener, report: &mut FrameReport) {
        let ids = std::mem::take(&mut self.pending_ready);
        let Self {
            world,
            commands,
            input,
            time,
            loaded,
            active,
            pending_ready,
            attachments,
            ..
        } = self;
        let mut env = Env {
            world: &*world,
            commands: &mut *commands,
            input: &*input,
            time: *time,
            failures: &mut report.script_failures,
        };
        for id in ids {
            let Some(entry) = loaded.get_mut(&id) else {
                continue;
            };
            if entry.ready {
                continue;
            }
            if !entry.enabled {
                pending_ready.insert(id);
                continue;
            }
            entry.ready = true;
            let object = entry.handle;
            active.insert(id, object);
            emit(listener, attachments, Stage::Ready, object, &mut env);
        }
    }

    fn run_frame_stage(
        &mut self,
        stage: Stage,
        listener: &mut dyn Listener,
        report: &mut FrameReport,
    ) {
        let Self {
            world,
            commands,
            input,
            time,
            active,
            attachments,
            ..
        } = self;
        let mut env = Env {
            world: &*world,
            commands: &mut *commands,
            input: &*input,
            time: *time,
            failures: &mut report.script_failures,
        };
        for object in active.values().copied() {
            emit(listener, attachments, stage, object, &mut env);
        }
    }

    fn run_disable(&mut self, listener: &mut dyn Listener, report: &mut FrameReport) {
        let requests = self.take_enable_requests(false);
        let Self {
            world,
            commands,
            input,
            time,
            loaded,
            active,
            pending_ready,
            attachments,
            ..
        } = self;
        let mut env = Env {
            world: &*world,
            commands: &mut *commands,
            input: &*input,
            time: *time,
            failures: &mut report.script_failures,
        };
        for object in requests {
            let Some(id) = world.id(object) else {
                report.skipped.push(Skipped {
                    command: "set_enabled",
                    handle: object,
                    reason: SkipReason::StaleHandle,
                });
                continue;
            };
            let Some(entry) = loaded.get_mut(&id) else {
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
            active.remove(&id);
            pending_ready.remove(&id);
            emit(listener, attachments, Stage::Disable, object, &mut env);
        }
    }

    fn run_unload(&mut self, listener: &mut dyn Listener, report: &mut FrameReport) {
        let requests = std::mem::take(&mut self.pending_unload);
        let Self {
            world,
            commands,
            input,
            time,
            loaded,
            active,
            pending_ready,
            attachments,
            overrides_applied,
            ..
        } = self;
        for request in requests {
            if !world.contains(request.handle) {
                report.skipped.push(Skipped {
                    command: if request.despawn { "despawn" } else { "unload" },
                    handle: request.handle,
                    reason: SkipReason::StaleHandle,
                });
                continue;
            }
            let subtree = subtree_postorder(world, request.handle);
            {
                let mut env = Env {
                    world: &*world,
                    commands: &mut *commands,
                    input: &*input,
                    time: *time,
                    failures: &mut report.script_failures,
                };
                for object in subtree.iter().copied() {
                    let Some(id) = world.id(object) else {
                        continue;
                    };
                    let Some(entry) = loaded.remove(&id) else {
                        continue;
                    };
                    active.remove(&id);
                    pending_ready.remove(&id);
                    if entry.enabled {
                        emit(listener, attachments, Stage::Disable, object, &mut env);
                    }
                    emit(listener, attachments, Stage::Unload, object, &mut env);
                }
            }
            if request.despawn {
                for object in &subtree {
                    attachments.by_object.remove(object);
                    overrides_applied.remove(object);
                }
                world.despawn(request.handle);
            }
        }
    }
}

fn apply_platform_overrides(
    object: ObjectHandle,
    world: &ObjectWorld,
    platform: Option<PlatformKey>,
    registry: &HashMap<String, Factory>,
    attachments: &mut Attachments,
    applied: &mut HashSet<ObjectHandle>,
    errors: &mut Vec<AttachError>,
) {
    let Some(platform) = platform else {
        return;
    };
    if !world.has_component::<PlatformOverride>(object) || !applied.insert(object) {
        return;
    }
    let names = world
        .with_component::<PlatformOverride, _>(object, |overrides| {
            overrides.scripts(platform).to_vec()
        })
        .unwrap_or_default();
    for name in names {
        match registry.get(&name) {
            Some(factory) => {
                push_script(attachments, object, factory());
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
