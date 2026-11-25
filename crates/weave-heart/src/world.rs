use std::collections::{BTreeMap, HashMap};

use crate::component::Component;
use crate::error::SpawnError;
use crate::object::{ObjectHandle, ObjectId, Transform};

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

struct Record {
    id: ObjectId,
    name: Box<str>,
    parent: Option<ObjectHandle>,
    children: Children,
}

pub struct ObjectWorld {
    store: hecs::World,
    records: HashMap<ObjectHandle, Record>,
    by_id: HashMap<ObjectId, ObjectHandle>,
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
            records: HashMap::new(),
            by_id: HashMap::new(),
            roots: Children::default(),
            next_id: 1,
        }
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
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
        let Some(record) = self.records.get(&handle) else {
            return false;
        };
        let (parent, id, name) = (record.parent, record.id, record.name.clone());
        self.siblings_mut(parent).remove(id, &name);

        let mut stack = vec![handle];
        while let Some(current) = stack.pop() {
            let Some(record) = self.records.remove(&current) else {
                continue;
            };
            self.by_id.remove(&record.id);
            stack.extend(record.children.by_id.values().copied());
            let _ = self.store.despawn(current.0);
        }
        true
    }

    pub fn contains(&self, handle: ObjectHandle) -> bool {
        self.records.contains_key(&handle)
    }

    pub fn id(&self, handle: ObjectHandle) -> Option<ObjectId> {
        self.records.get(&handle).map(|record| record.id)
    }

    pub fn by_id(&self, id: ObjectId) -> Option<ObjectHandle> {
        self.by_id.get(&id).copied()
    }

    pub fn name(&self, handle: ObjectHandle) -> Option<&str> {
        self.records.get(&handle).map(|record| record.name.as_ref())
    }

    pub fn path(&self, handle: ObjectHandle) -> Option<String> {
        let mut segments = Vec::new();
        let mut current = Some(handle);
        while let Some(h) = current {
            let record = self.records.get(&h)?;
            segments.push(record.name.as_ref());
            current = record.parent;
        }
        segments.reverse();
        Some(segments.join("/"))
    }

    pub fn lookup(&self, path: &str) -> Option<ObjectHandle> {
        let mut segments = path.split('/');
        let mut current = *self.roots.by_name.get(segments.next()?)?;
        for segment in segments {
            current = *self.records.get(&current)?.children.by_name.get(segment)?;
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
        self.records
            .get(&handle)
            .map(|record| record.children.by_id.values().copied())
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

        let handle = ObjectHandle(self.store.spawn((Transform::default(),)));
        let name: Box<str> = name.into();
        self.records.insert(
            handle,
            Record {
                id,
                name: name.clone(),
                parent,
                children: Children::default(),
            },
        );
        self.by_id.insert(id, handle);
        self.siblings_mut(parent).insert(id, name, handle);
        Ok(handle)
    }

    fn siblings(&self, parent: Option<ObjectHandle>) -> Result<&Children, SpawnError> {
        match parent {
            None => Ok(&self.roots),
            Some(p) => self
                .records
                .get(&p)
                .map(|record| &record.children)
                .ok_or(SpawnError::StaleParent(p)),
        }
    }

    fn siblings_mut(&mut self, parent: Option<ObjectHandle>) -> &mut Children {
        match parent {
            None => &mut self.roots,
            Some(p) => {
                let record = self
                    .records
                    .get_mut(&p)
                    .expect("parent is validated before it is used");
                &mut record.children
            }
        }
    }
}

fn validate_name(name: &str) -> Result<(), SpawnError> {
    if name.is_empty() || name.contains('/') {
        return Err(SpawnError::InvalidName(name.to_owned()));
    }
    Ok(())
}

impl ObjectWorld {
    pub fn insert_component<C: Component>(&mut self, object: ObjectHandle, component: C) -> bool {
        self.contains(object) && self.store.insert_one(object.0, component).is_ok()
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
        self.store
            .get::<&mut C>(object.0)
            .ok()
            .map(|mut component| f(&mut *component))
    }

    pub fn remove_component<C: Component>(&mut self, object: ObjectHandle) -> Option<C> {
        self.store.remove_one::<C>(object.0).ok()
    }
}
