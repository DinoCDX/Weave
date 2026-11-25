use crate::object::{ObjectHandle, Transform};

#[derive(Clone, Debug, PartialEq)]
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
}

#[derive(Debug, Default)]
pub struct CommandBuffer {
    pub(crate) queue: Vec<Command>,
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

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}
