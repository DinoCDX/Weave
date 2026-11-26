use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FieldKind {
    Bool = 1,
    Int = 2,
    Float = 3,
    Text = 4,
    Vec3 = 5,
    Vec4 = 6,
    TextList = 7,
    Double = 8,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f32),
    Text(String),
    Floats(Vec<f32>),
    TextList(Vec<String>),
    Double(f64),
    Nothing,
}

pub trait Field: Sized {
    const KIND: FieldKind;

    fn to_value(&self) -> Value;

    fn from_value(value: &Value) -> Option<Self>;

    fn absent() -> Option<Self> {
        None
    }
}

impl<T: Field> Field for Option<T> {
    const KIND: FieldKind = T::KIND;

    fn to_value(&self) -> Value {
        match self {
            Some(value) => value.to_value(),
            None => Value::Nothing,
        }
    }

    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Nothing => Some(None),
            other => T::from_value(other).map(Some),
        }
    }

    fn absent() -> Option<Self> {
        Some(None)
    }
}

impl Field for i32 {
    const KIND: FieldKind = FieldKind::Int;

    fn to_value(&self) -> Value {
        Value::Int(i64::from(*self))
    }

    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Int(v) => i32::try_from(*v).ok(),
            _ => None,
        }
    }
}

impl Field for u32 {
    const KIND: FieldKind = FieldKind::Int;

    fn to_value(&self) -> Value {
        Value::Int(i64::from(*self))
    }

    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Int(v) => u32::try_from(*v).ok(),
            _ => None,
        }
    }
}

impl Field for f64 {
    const KIND: FieldKind = FieldKind::Double;

    fn to_value(&self) -> Value {
        Value::Double(*self)
    }

    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Double(v) => Some(*v),
            _ => None,
        }
    }
}

impl Field for bool {
    const KIND: FieldKind = FieldKind::Bool;

    fn to_value(&self) -> Value {
        Value::Bool(*self)
    }

    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }
}

impl Field for i64 {
    const KIND: FieldKind = FieldKind::Int;

    fn to_value(&self) -> Value {
        Value::Int(*self)
    }

    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Int(v) => Some(*v),
            _ => None,
        }
    }
}

impl Field for f32 {
    const KIND: FieldKind = FieldKind::Float;

    fn to_value(&self) -> Value {
        Value::Float(*self)
    }

    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Float(v) => Some(*v),
            _ => None,
        }
    }
}

impl Field for String {
    const KIND: FieldKind = FieldKind::Text;

    fn to_value(&self) -> Value {
        Value::Text(self.clone())
    }

    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Text(v) => Some(v.clone()),
            _ => None,
        }
    }
}

impl Field for [f32; 3] {
    const KIND: FieldKind = FieldKind::Vec3;

    fn to_value(&self) -> Value {
        Value::Floats(self.to_vec())
    }

    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Floats(v) => <[f32; 3]>::try_from(v.as_slice()).ok(),
            _ => None,
        }
    }
}

impl Field for [f32; 4] {
    const KIND: FieldKind = FieldKind::Vec4;

    fn to_value(&self) -> Value {
        Value::Floats(self.to_vec())
    }

    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Floats(v) => <[f32; 4]>::try_from(v.as_slice()).ok(),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldSchema {
    pub name: &'static str,
    pub kind: FieldKind,
}

impl FieldSchema {
    pub const fn new(name: &'static str, kind: FieldKind) -> Self {
        Self { name, kind }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentSchema {
    pub name: &'static str,
    pub hash: u64,
    pub fields: &'static [FieldSchema],
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

const fn fnv_byte(hash: u64, byte: u8) -> u64 {
    (hash ^ byte as u64).wrapping_mul(FNV_PRIME)
}

const fn fnv_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    let mut i = 0;
    while i < bytes.len() {
        hash = fnv_byte(hash, bytes[i]);
        i += 1;
    }
    hash
}

impl ComponentSchema {
    pub const fn new(name: &'static str, fields: &'static [FieldSchema]) -> Self {
        let mut hash = fnv_bytes(FNV_OFFSET, name.as_bytes());
        let mut i = 0;
        while i < fields.len() {
            hash = fnv_byte(hash, 0);
            hash = fnv_bytes(hash, fields[i].name.as_bytes());
            hash = fnv_byte(hash, fields[i].kind as u8);
            i += 1;
        }
        Self { name, hash, fields }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    MissingField(&'static str),
    WrongType(&'static str),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(name) => write!(f, "component is missing field {name:?}"),
            Self::WrongType(name) => write!(f, "field {name:?} has the wrong type"),
        }
    }
}

impl std::error::Error for DecodeError {}

pub trait Component: Send + Sync + 'static + Sized {
    const SCHEMA: ComponentSchema;

    fn to_values(&self) -> Vec<(&'static str, Value)>;

    fn from_values(values: &[(&str, Value)]) -> Result<Self, DecodeError>;
}

pub fn take_field<T: Field>(
    values: &[(&str, Value)],
    name: &'static str,
) -> Result<T, DecodeError> {
    let found = values
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value);
    match found {
        Some(value) => T::from_value(value).ok_or(DecodeError::WrongType(name)),
        None => T::absent().ok_or(DecodeError::MissingField(name)),
    }
}

impl Field for Vec<String> {
    const KIND: FieldKind = FieldKind::TextList;

    fn to_value(&self) -> Value {
        Value::TextList(self.clone())
    }

    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::TextList(v) => Some(v.clone()),
            _ => None,
        }
    }
}
