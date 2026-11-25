use crate::command::CommandBuffer;
use crate::object::ObjectHandle;
use crate::script::{Attachments, ScriptFailure, invoke};
use crate::services::{InputFrame, TimeView};
use crate::world::ObjectWorld;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Load,
    Enable,
    Ready,
    Update,
    LateUpdate,
    Disable,
    Unload,
}

impl Stage {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Load => "on_load",
            Self::Enable => "on_enable",
            Self::Ready => "on_ready",
            Self::Update => "on_update",
            Self::LateUpdate => "on_late_update",
            Self::Disable => "on_disable",
            Self::Unload => "on_unload",
        }
    }
}

pub struct StageContext<'a> {
    pub world: &'a ObjectWorld,
    pub commands: &'a mut CommandBuffer,
    pub dt: f32,
}

pub trait Listener {
    fn on_stage(&mut self, stage: Stage, object: ObjectHandle, cx: &mut StageContext<'_>);
}

pub(crate) struct Env<'a> {
    pub(crate) world: &'a ObjectWorld,
    pub(crate) commands: &'a mut CommandBuffer,
    pub(crate) input: &'a InputFrame,
    pub(crate) time: TimeView,
    pub(crate) failures: &'a mut Vec<ScriptFailure>,
}

pub(crate) fn emit(
    listener: &mut dyn Listener,
    attachments: &mut Attachments,
    stage: Stage,
    object: ObjectHandle,
    env: &mut Env<'_>,
) {
    if let Some(list) = attachments.by_object.get_mut(&object) {
        for attachment in list.iter_mut() {
            invoke(attachment, object, stage, env);
        }
    }
    listener.on_stage(
        stage,
        object,
        &mut StageContext {
            world: env.world,
            commands: &mut *env.commands,
            dt: env.time.delta(),
        },
    );
}
