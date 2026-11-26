use std::collections::BTreeSet;
use std::marker::PhantomData;

use crate::object::{ObjectHandle, ObjectId, Transform};
use crate::world::ObjectWorld;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Input;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scene;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Here;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct There<I>(PhantomData<fn() -> I>);

pub trait Contains<Service, Index> {}

impl<A> Contains<A, Here> for (A,) {}

impl<A, B> Contains<A, Here> for (A, B) {}
impl<A, B> Contains<B, There<Here>> for (A, B) {}

impl<A, B, C> Contains<A, Here> for (A, B, C) {}
impl<A, B, C> Contains<B, There<Here>> for (A, B, C) {}
impl<A, B, C> Contains<C, There<There<Here>>> for (A, B, C) {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeView {
    delta: f32,
    elapsed: f64,
}

impl TimeView {
    pub(crate) const fn new(delta: f32, elapsed: f64) -> Self {
        Self { delta, elapsed }
    }

    pub const fn delta(self) -> f32 {
        self.delta
    }

    pub const fn elapsed(self) -> f64 {
        self.elapsed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyCode(pub u32);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputFrame {
    down: BTreeSet<KeyCode>,
}

impl InputFrame {
    pub fn set(&mut self, key: KeyCode, down: bool) {
        if down {
            self.down.insert(key);
        } else {
            self.down.remove(&key);
        }
    }

    pub fn is_down(&self, key: KeyCode) -> bool {
        self.down.contains(&key)
    }
}

#[derive(Clone, Copy)]
pub struct InputView<'a> {
    frame: &'a InputFrame,
}

impl<'a> InputView<'a> {
    pub(crate) fn new(frame: &'a InputFrame) -> Self {
        Self { frame }
    }

    pub fn is_down(self, key: KeyCode) -> bool {
        self.frame.is_down(key)
    }
}

#[derive(Clone, Copy)]
pub struct SceneView<'a> {
    world: &'a ObjectWorld,
}

impl<'a> SceneView<'a> {
    pub(crate) fn new(world: &'a ObjectWorld) -> Self {
        Self { world }
    }

    pub fn lookup(self, path: &str) -> Option<ObjectHandle> {
        self.world.lookup(path)
    }

    pub fn path(self, object: ObjectHandle) -> Option<String> {
        self.world.path(object)
    }

    pub fn id(self, object: ObjectHandle) -> Option<ObjectId> {
        self.world.id(object)
    }

    pub fn name(self, object: ObjectHandle) -> Option<String> {
        self.world.name(object)
    }

    pub fn parent(self, object: ObjectHandle) -> Option<Option<ObjectHandle>> {
        self.world.parent(object)
    }

    pub fn transform(self, object: ObjectHandle) -> Option<Transform> {
        self.world.transform(object)
    }
}
