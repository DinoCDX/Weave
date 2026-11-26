mod command;
mod component;
mod error;
mod object;
mod platform;
mod runtime;
mod script;
mod stage;
mod world;

#[doc = include_str!("../../../docs/services.md")]
pub mod services;

#[doc = include_str!("../../../docs/derive.md")]
pub mod derive {}

pub use command::CommandBuffer;
pub use component::{
    Component, ComponentSchema, DecodeError, Field, FieldKind, FieldSchema, Value, take_field,
};
pub use error::{ReparentError, SpawnError};
pub use object::{Name, ObjectHandle, ObjectId, Transform};
pub use platform::{PlatformKey, PlatformOverride};
pub use runtime::{FrameReport, Runtime, RuntimeStats, SkipReason, Skipped};
pub use script::{AttachError, RegistryError, Script, ScriptContext, ScriptFailure};
pub use stage::{Listener, NoListener, Stage, StageContext};
pub use world::ObjectWorld;
