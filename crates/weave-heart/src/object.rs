use std::sync::Arc;

use crate::component::{
    Component, ComponentSchema, DecodeError, Field, FieldSchema, Value, take_field,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(u64);

impl ObjectId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectHandle(pub(crate) hecs::Entity);

#[derive(Default)]
pub(crate) struct HandleHasher(u64);

const HASH_K: u64 = 0x517c_c1b7_2722_0a95;

impl std::hash::Hasher for HandleHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0.rotate_left(5) ^ u64::from(byte)).wrapping_mul(HASH_K);
        }
    }

    fn write_u32(&mut self, value: u32) {
        self.0 = (self.0.rotate_left(5) ^ u64::from(value)).wrapping_mul(HASH_K);
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(HASH_K);
    }
}

pub(crate) type HandleBuild = std::hash::BuildHasherDefault<HandleHasher>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        }
    }
}

impl Component for Transform {
    const SCHEMA: ComponentSchema = ComponentSchema::new(
        "Transform",
        &[
            FieldSchema::new("translation", <[f32; 3] as Field>::KIND),
            FieldSchema::new("rotation", <[f32; 4] as Field>::KIND),
            FieldSchema::new("scale", <[f32; 3] as Field>::KIND),
        ],
    );

    fn to_values(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("translation", Field::to_value(&self.translation)),
            ("rotation", Field::to_value(&self.rotation)),
            ("scale", Field::to_value(&self.scale)),
        ]
    }

    fn from_values(values: &[(&str, Value)]) -> Result<Self, DecodeError> {
        Ok(Self {
            translation: take_field(values, "translation")?,
            rotation: take_field(values, "rotation")?,
            scale: take_field(values, "scale")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    pub value: Arc<str>,
}

impl Component for Name {
    const SCHEMA: ComponentSchema = ComponentSchema::new(
        "Name",
        &[FieldSchema::new("value", <String as Field>::KIND)],
    );

    fn to_values(&self) -> Vec<(&'static str, Value)> {
        vec![("value", Field::to_value(&self.value))]
    }

    fn from_values(values: &[(&str, Value)]) -> Result<Self, DecodeError> {
        Ok(Self {
            value: take_field(values, "value")?,
        })
    }
}
