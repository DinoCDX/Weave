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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReparentError {
    StaleObject(ObjectHandle),
    StaleParent(ObjectHandle),
    Cycle,
    DuplicateName {
        parent: Option<ObjectHandle>,
        name: String,
    },
}

impl fmt::Display for ReparentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleObject(_) => write!(f, "the object no longer exists"),
            Self::StaleParent(_) => write!(f, "the new parent no longer exists"),
            Self::Cycle => write!(f, "an object cannot be moved under itself or its own child"),
            Self::DuplicateName { name, .. } => {
                write!(
                    f,
                    "a sibling named {name:?} already exists under the new parent"
                )
            }
        }
    }
}

impl std::error::Error for ReparentError {}
