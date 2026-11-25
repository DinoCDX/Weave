use std::fmt;

use crate::object::{ObjectHandle, ObjectId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpawnError {
    InvalidName(String),
    DuplicateName {
        parent: Option<ObjectHandle>,
        name: String,
    },
    DuplicateId(ObjectId),
    StaleParent(ObjectHandle),
}

impl fmt::Display for SpawnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(name) => write!(
                f,
                "invalid object name {name:?}: names must be non-empty and contain no '/'"
            ),
            Self::DuplicateName { parent, name } => match parent {
                Some(_) => write!(
                    f,
                    "a sibling named {name:?} already exists under this parent"
                ),
                None => write!(f, "a root object named {name:?} already exists"),
            },
            Self::DuplicateId(id) => write!(f, "object id {} is already in use", id.get()),
            Self::StaleParent(_) => write!(f, "the parent object no longer exists"),
        }
    }
}

impl std::error::Error for SpawnError {}
