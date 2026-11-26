use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::command::CommandBuffer;
use crate::component::Component;
use crate::object::{HandleBuild, ObjectHandle, ObjectId, Transform};
use crate::services::{Contains, Input, InputFrame, InputView, Scene, SceneView, Time, TimeView};
use crate::stage::{Env, Stage};
use crate::world::ObjectWorld;

pub trait Script<S = ()>: Send + 'static {
    fn on_load(&mut self, _cx: &mut ScriptContext<'_, S>) {}

    fn on_enable(&mut self, _cx: &mut ScriptContext<'_, S>) {}

    fn on_ready(&mut self, _cx: &mut ScriptContext<'_, S>) {}

    fn on_update(&mut self, _cx: &mut ScriptContext<'_, S>) {}

    fn on_late_update(&mut self, _cx: &mut ScriptContext<'_, S>) {}

    fn on_disable(&mut self, _cx: &mut ScriptContext<'_, S>) {}

    fn on_unload(&mut self, _cx: &mut ScriptContext<'_, S>) {}
}

pub(crate) struct RawContext<'a> {
    pub(crate) object: ObjectHandle,
    pub(crate) world: &'a ObjectWorld,
    pub(crate) commands: &'a mut CommandBuffer,
    pub(crate) input: &'a InputFrame,
    pub(crate) time: TimeView,
}

pub struct ScriptContext<'a, S = ()> {
    raw: RawContext<'a>,
    services: PhantomData<fn() -> S>,
}

impl<'a, S> ScriptContext<'a, S> {
    fn new(raw: RawContext<'a>) -> Self {
        Self {
            raw,
            services: PhantomData,
        }
    }

    pub fn object(&self) -> ObjectHandle {
        self.raw.object
    }

    pub fn commands(&mut self) -> &mut CommandBuffer {
        &mut *self.raw.commands
    }

    pub fn set_transform(&mut self, transform: Transform) {
        let object = self.raw.object;
        self.raw.commands.set_transform(object, transform);
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        let object = self.raw.object;
        self.raw.commands.set_enabled(object, enabled);
    }

    pub fn despawn(&mut self) {
        let object = self.raw.object;
        self.raw.commands.despawn(object);
    }

    pub fn reparent(&mut self, parent: Option<ObjectHandle>) {
        let object = self.raw.object;
        self.raw.commands.reparent(object, parent);
    }

    pub fn insert_component<C: Component>(&mut self, component: C) {
        let object = self.raw.object;
        self.raw.commands.insert_component(object, component);
    }

    pub fn edit_component<C: Component>(&mut self, edit: impl FnOnce(&mut C) + Send + 'static) {
        let object = self.raw.object;
        self.raw.commands.edit_component::<C>(object, edit);
    }

    pub fn time<I>(&self) -> TimeView
    where
        S: Contains<Time, I>,
    {
        self.raw.time
    }

    pub fn input<I>(&self) -> InputView<'a>
    where
        S: Contains<Input, I>,
    {
        InputView::new(self.raw.input)
    }

    pub fn scene<I>(&self) -> SceneView<'a>
    where
        S: Contains<Scene, I>,
    {
        SceneView::new(self.raw.world)
    }
}

pub(crate) trait ErasedScript: Send {
    fn dispatch(&mut self, stage: Stage, raw: RawContext<'_>);

    fn inner(&self) -> &dyn Any;
}

struct Adapter<T, S> {
    script: T,
    services: PhantomData<fn() -> S>,
}

impl<T: Script<S>, S: 'static> ErasedScript for Adapter<T, S> {
    fn dispatch(&mut self, stage: Stage, raw: RawContext<'_>) {
        let mut cx = ScriptContext::<S>::new(raw);
        match stage {
            Stage::Load => self.script.on_load(&mut cx),
            Stage::Enable => self.script.on_enable(&mut cx),
            Stage::Ready => self.script.on_ready(&mut cx),
            Stage::Update => self.script.on_update(&mut cx),
            Stage::LateUpdate => self.script.on_late_update(&mut cx),
            Stage::Disable => self.script.on_disable(&mut cx),
            Stage::Unload => self.script.on_unload(&mut cx),
        }
    }

    fn inner(&self) -> &dyn Any {
        &self.script
    }
}

pub(crate) fn erase<S: 'static, T: Script<S>>(script: T) -> Box<dyn ErasedScript> {
    Box::new(Adapter {
        script,
        services: PhantomData,
    })
}

pub(crate) struct Attachment {
    pub(crate) script: Box<dyn ErasedScript>,
    pub(crate) started: bool,
    pub(crate) failed: bool,
}

#[derive(Default)]
pub(crate) struct Attachments {
    pub(crate) by_object: HashMap<ObjectHandle, Vec<Attachment>, HandleBuild>,
    pub(crate) unstarted: BTreeMap<ObjectId, ObjectHandle>,
}

pub(crate) type Factory = Box<dyn Fn() -> Box<dyn ErasedScript>>;

pub(crate) fn push_script(
    attachments: &mut Attachments,
    object: ObjectHandle,
    script: Box<dyn ErasedScript>,
) -> usize {
    let list = attachments.by_object.entry(object).or_default();
    list.push(Attachment {
        script,
        started: false,
        failed: false,
    });
    list.len() - 1
}

pub(crate) fn invoke(
    attachment: &mut Attachment,
    object: ObjectHandle,
    stage: Stage,
    env: &mut Env<'_>,
) {
    attachment.started = true;
    if attachment.failed {
        return;
    }
    let raw = RawContext {
        object,
        world: env.world,
        commands: &mut *env.commands,
        input: env.input,
        time: env.time,
    };
    if !cfg!(debug_assertions) {
        attachment.script.dispatch(stage, raw);
        return;
    }
    let outcome = catch_unwind(AssertUnwindSafe(|| attachment.script.dispatch(stage, raw)));
    if let Err(payload) = outcome {
        attachment.failed = true;
        let failure = ScriptFailure {
            object,
            path: env.world.path(object).unwrap_or_default(),
            stage,
            message: panic_message(payload.as_ref()),
        };
        env.failures.push(failure);
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptFailure {
    pub object: ObjectHandle,
    pub path: String,
    pub stage: Stage,
    pub message: String,
}

impl fmt::Display for ScriptFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "script on {:?} failed during {}: {}; it is disabled for the rest of this run",
            self.path,
            self.stage.name(),
            self.message
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttachError {
    StaleObject(ObjectHandle),
    UnknownScript(String),
}

impl fmt::Display for AttachError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleObject(_) => write!(f, "the object no longer exists"),
            Self::UnknownScript(name) => {
                write!(f, "no modular script named {name:?} is registered")
            }
        }
    }
}

impl std::error::Error for AttachError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryError {
    Duplicate(String),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate(name) => {
                write!(f, "a modular script named {name:?} is already registered")
            }
        }
    }
}

impl std::error::Error for RegistryError {}
