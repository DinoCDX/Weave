use crate::component::Component;
use crate::object::{ObjectHandle, Transform};
use crate::script::{ErasedScript, Script};
use crate::world::ObjectWorld;

pub(crate) type Edit = Box<dyn FnOnce(&mut ObjectWorld, ObjectHandle) -> bool + Send>;

pub(crate) enum Command {
    Spawn {
        parent: Option<ObjectHandle>,
        name: String,
    },
    Despawn(ObjectHandle),
    Load(ObjectHandle),
    Unload(ObjectHandle),
    SetEnabled {
        object: ObjectHandle,
        enabled: bool,
    },
    SetTransform {
        object: ObjectHandle,
        transform: Transform,
    },
    Reparent {
        object: ObjectHandle,
        parent: Option<ObjectHandle>,
    },
    Edit {
        object: ObjectHandle,
        name: &'static str,
        edit: Edit,
    },
    Attach {
        object: ObjectHandle,
        script: Box<dyn ErasedScript>,
    },
    AttachModular {
        object: ObjectHandle,
        name: String,
    },
}

#[derive(Default)]
pub struct CommandBuffer {
    pub(crate) queue: Vec<Command>,
}

impl std::fmt::Debug for CommandBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandBuffer")
            .field("queued", &self.queue.len())
            .finish()
    }
}

impl CommandBuffer {
    pub fn spawn(&mut self, parent: Option<ObjectHandle>, name: impl Into<String>) {
        self.queue.push(Command::Spawn {
            parent,
            name: name.into(),
        });
    }

    pub fn despawn(&mut self, object: ObjectHandle) {
        self.queue.push(Command::Despawn(object));
    }

    pub fn load(&mut self, object: ObjectHandle) {
        self.queue.push(Command::Load(object));
    }

    pub fn unload(&mut self, object: ObjectHandle) {
        self.queue.push(Command::Unload(object));
    }

    pub fn set_enabled(&mut self, object: ObjectHandle, enabled: bool) {
        self.queue.push(Command::SetEnabled { object, enabled });
    }

    pub fn set_transform(&mut self, object: ObjectHandle, transform: Transform) {
        self.queue.push(Command::SetTransform { object, transform });
    }

    pub fn reparent(&mut self, object: ObjectHandle, parent: Option<ObjectHandle>) {
        self.queue.push(Command::Reparent { object, parent });
    }

    pub fn insert_component<C: Component>(&mut self, object: ObjectHandle, component: C) {
        self.queue.push(Command::Edit {
            object,
            name: "insert_component",
            edit: Box::new(move |world, object| world.insert_component(object, component)),
        });
    }

    pub fn remove_component<C: Component>(&mut self, object: ObjectHandle) {
        self.queue.push(Command::Edit {
            object,
            name: "remove_component",
            edit: Box::new(move |world, object| world.remove_component::<C>(object).is_some()),
        });
    }

    pub fn edit_component<C: Component>(
        &mut self,
        object: ObjectHandle,
        edit: impl FnOnce(&mut C) + Send + 'static,
    ) {
        self.queue.push(Command::Edit {
            object,
            name: "edit_component",
            edit: Box::new(move |world, object| {
                world.with_component_mut::<C, _>(object, edit).is_some()
            }),
        });
    }

    pub fn attach<S: Script>(&mut self, object: ObjectHandle, script: S) {
        self.queue.push(Command::Attach {
            object,
            script: Box::new(script),
        });
    }

    pub fn attach_modular(&mut self, object: ObjectHandle, name: impl Into<String>) {
        self.queue.push(Command::AttachModular {
            object,
            name: name.into(),
        });
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}
