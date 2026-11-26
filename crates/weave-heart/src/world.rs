use std::any::TypeId;
use std::collections::{BTreeMap, HashMap};

use crate::component::Component;
use crate::error::{ReparentError, SpawnError};
use crate::object::{Name, ObjectHandle, ObjectId, Transform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IdTag(ObjectId);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParentLink(Option<ObjectHandle>);

#[derive(Default)]
struct Children {
    by_id: BTreeMap<ObjectId, ObjectHandle>,
    by_name: HashMap<Box<str>, ObjectHandle>,
}

impl Children {
    fn insert(&mut self, id: ObjectId, name: Box<str>, handle: ObjectHandle) {
        self.by_id.insert(id, handle);
        self.by_name.insert(name, handle);
    }

    fn remove(&mut self, id: ObjectId, name: &str) {
        self.by_id.remove(&id);
        self.by_name.remove(name);
    }
}

pub struct ObjectWorld {
    store: hecs::World,
    by_id: BTreeMap<ObjectId, ObjectHandle>,
    children: HashMap<ObjectHandle, Children>,
    roots: Children,
    next_id: u64,
}

impl Default for ObjectWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectWorld {
    pub fn new() -> Self {
        Self {
            store: hecs::World::new(),
            by_id: BTreeMap::new(),
            children: HashMap::new(),
            roots: Children::default(),
            next_id: 1,
        }
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    pub fn spawn(
        &mut self,
        parent: Option<ObjectHandle>,
        name: &str,
    ) -> Result<ObjectHandle, SpawnError> {
        let id = ObjectId::new(self.next_id);
        let handle = self.insert(id, parent, name)?;
        self.next_id = self.next_id.saturating_add(1);
        Ok(handle)
    }

    pub fn spawn_with_id(
        &mut self,
        id: ObjectId,
        parent: Option<ObjectHandle>,
        name: &str,
    ) -> Result<ObjectHandle, SpawnError> {
        let handle = self.insert(id, parent, name)?;
        self.next_id = self.next_id.max(id.get().saturating_add(1));
        Ok(handle)
    }

    pub fn despawn(&mut self, handle: ObjectHandle) -> bool {
        if !self.contains(handle) {
            return false;
        }
        let parent = self.parent_link(handle).unwrap_or(None);
        let id = self.id_of(handle).expect("live objects have an id");
        let name = self.name_of(handle).expect("live objects have a name");
        self.siblings_mut(parent).remove(id, &name);

        let mut stack = vec![handle];
        while let Some(current) = stack.pop() {
            if let Some(children) = self.children.remove(&current) {
                stack.extend(children.by_id.values().copied());
            }
            if let Some(id) = self.id_of(current) {
                self.by_id.remove(&id);
            }
            let _ = self.store.despawn(current.0);
        }
        true
    }

    pub fn reparent(
        &mut self,
        object: ObjectHandle,
        parent: Option<ObjectHandle>,
    ) -> Result<(), ReparentError> {
        let Some(old_parent) = self.parent_link(object) else {
            return Err(ReparentError::StaleObject(object));
        };
        if let Some(target) = parent {
            if !self.contains(target) {
                return Err(ReparentError::StaleParent(target));
            }
            if self.is_at_or_below(target, object) {
                return Err(ReparentError::Cycle);
            }
        }
        if old_parent == parent {
            return Ok(());
        }

        let id = self.id_of(object).expect("live objects have an id");
        let name = self.name_of(object).expect("live objects have a name");
        if self
            .siblings_checked(parent)
            .by_name
            .contains_key(name.as_str())
        {
            return Err(ReparentError::DuplicateName { parent, name });
        }

        self.siblings_mut(old_parent).remove(id, &name);
        self.siblings_mut(parent)
            .insert(id, name.into_boxed_str(), object);
        if let Ok(mut link) = self.store.get::<&mut ParentLink>(object.0) {
            link.0 = parent;
        }
        Ok(())
    }

    pub fn contains(&self, handle: ObjectHandle) -> bool {
        self.store.contains(handle.0)
    }

    pub fn id(&self, handle: ObjectHandle) -> Option<ObjectId> {
        self.id_of(handle)
    }

    pub fn by_id(&self, id: ObjectId) -> Option<ObjectHandle> {
        self.by_id.get(&id).copied()
    }

    pub fn objects(&self) -> impl Iterator<Item = ObjectHandle> + '_ {
        self.by_id.values().copied()
    }

    pub fn name(&self, handle: ObjectHandle) -> Option<String> {
        self.name_of(handle)
    }

    pub fn parent(&self, handle: ObjectHandle) -> Option<Option<ObjectHandle>> {
        self.parent_link(handle)
    }

    pub fn path(&self, handle: ObjectHandle) -> Option<String> {
        let mut segments = Vec::new();
        let mut current = Some(handle);
        while let Some(h) = current {
            segments.push(self.name_of(h)?);
            current = self.parent_link(h)?;
        }
        segments.reverse();
        Some(segments.join("/"))
    }

    pub fn lookup(&self, path: &str) -> Option<ObjectHandle> {
        let mut segments = path.split('/');
        let mut current = *self.roots.by_name.get(segments.next()?)?;
        for segment in segments {
            current = *self.children.get(&current)?.by_name.get(segment)?;
        }
        Some(current)
    }

    pub fn roots(&self) -> impl Iterator<Item = ObjectHandle> + '_ {
        self.roots.by_id.values().copied()
    }

    pub fn children(
        &self,
        handle: ObjectHandle,
    ) -> Option<impl Iterator<Item = ObjectHandle> + '_> {
        self.children
            .get(&handle)
            .map(|children| children.by_id.values().copied())
    }

    pub fn transform(&self, handle: ObjectHandle) -> Option<Transform> {
        self.store.get::<&Transform>(handle.0).ok().map(|t| *t)
    }

    pub fn set_transform(&mut self, handle: ObjectHandle, transform: Transform) -> bool {
        match self.store.get::<&mut Transform>(handle.0) {
            Ok(mut slot) => {
                *slot = transform;
                true
            }
            Err(_) => false,
        }
    }

    pub fn insert_component<C: Component>(&mut self, object: ObjectHandle, component: C) -> bool {
        !is_reserved::<C>()
            && self.contains(object)
            && self.store.insert_one(object.0, component).is_ok()
    }

    pub fn has_component<C: Component>(&self, object: ObjectHandle) -> bool {
        self.store.get::<&C>(object.0).is_ok()
    }

    pub fn with_component<C: Component, R>(
        &self,
        object: ObjectHandle,
        f: impl FnOnce(&C) -> R,
    ) -> Option<R> {
        self.store
            .get::<&C>(object.0)
            .ok()
            .map(|component| f(&*component))
    }

    pub fn with_component_mut<C: Component, R>(
        &mut self,
        object: ObjectHandle,
        f: impl FnOnce(&mut C) -> R,
    ) -> Option<R> {
        if is_reserved::<C>() {
            return None;
        }
        self.store
            .get::<&mut C>(object.0)
            .ok()
            .map(|mut component| f(&mut *component))
    }

    pub fn remove_component<C: Component>(&mut self, object: ObjectHandle) -> Option<C> {
        if is_reserved::<C>() {
            return None;
        }
        self.store.remove_one::<C>(object.0).ok()
    }

    fn insert(
        &mut self,
        id: ObjectId,
        parent: Option<ObjectHandle>,
        name: &str,
    ) -> Result<ObjectHandle, SpawnError> {
        validate_name(name)?;
        if self.by_id.contains_key(&id) {
            return Err(SpawnError::DuplicateId(id));
        }
        if self.siblings(parent)?.by_name.contains_key(name) {
            return Err(SpawnError::DuplicateName {
                parent,
                name: name.to_owned(),
            });
        }

        let handle = ObjectHandle(self.store.spawn((
            IdTag(id),
            Name {
                value: name.to_owned(),
            },
            ParentLink(parent),
            Transform::default(),
        )));
        self.by_id.insert(id, handle);
        self.children.insert(handle, Children::default());
        self.siblings_mut(parent).insert(id, name.into(), handle);
        Ok(handle)
    }

    fn id_of(&self, handle: ObjectHandle) -> Option<ObjectId> {
        self.store.get::<&IdTag>(handle.0).ok().map(|tag| tag.0)
    }

    fn name_of(&self, handle: ObjectHandle) -> Option<String> {
        self.store
            .get::<&Name>(handle.0)
            .ok()
            .map(|name| name.value.clone())
    }

    fn parent_link(&self, handle: ObjectHandle) -> Option<Option<ObjectHandle>> {
        self.store
            .get::<&ParentLink>(handle.0)
            .ok()
            .map(|link| link.0)
    }

    fn is_at_or_below(&self, candidate: ObjectHandle, ancestor: ObjectHandle) -> bool {
        let mut current = Some(candidate);
        while let Some(h) = current {
            if h == ancestor {
                return true;
            }
            current = self.parent_link(h).flatten();
        }
        false
    }

    fn siblings(&self, parent: Option<ObjectHandle>) -> Result<&Children, SpawnError> {
        match parent {
            None => Ok(&self.roots),
            Some(p) => self.children.get(&p).ok_or(SpawnError::StaleParent(p)),
        }
    }

    fn siblings_checked(&self, parent: Option<ObjectHandle>) -> &Children {
        match parent {
            None => &self.roots,
            Some(p) => self
                .children
                .get(&p)
                .expect("parent is validated before it is used"),
        }
    }

    fn siblings_mut(&mut self, parent: Option<ObjectHandle>) -> &mut Children {
        match parent {
            None => &mut self.roots,
            Some(p) => self
                .children
                .get_mut(&p)
                .expect("parent is validated before it is used"),
        }
    }
}

fn is_reserved<C: 'static>() -> bool {
    let kind = TypeId::of::<C>();
    kind == TypeId::of::<Name>()
        || kind == TypeId::of::<Transform>()
        || kind == TypeId::of::<IdTag>()
        || kind == TypeId::of::<ParentLink>()
}

fn validate_name(name: &str) -> Result<(), SpawnError> {
    if name.is_empty() || name.contains('/') {
        return Err(SpawnError::InvalidName(name.to_owned()));
    }
    Ok(())
}
