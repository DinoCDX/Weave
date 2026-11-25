use crate::component::{
    Component, ComponentSchema, DecodeError, Field, FieldSchema, Value, take_field,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlatformKey {
    Windows,
    Linux,
    Android,
}

impl PlatformKey {
    pub const fn current() -> Option<Self> {
        if cfg!(target_os = "windows") {
            Some(Self::Windows)
        } else if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else if cfg!(target_os = "android") {
            Some(Self::Android)
        } else {
            None
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Android => "android",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlatformOverride {
    pub windows: Vec<String>,
    pub linux: Vec<String>,
    pub android: Vec<String>,
}

impl PlatformOverride {
    pub fn scripts(&self, key: PlatformKey) -> &[String] {
        match key {
            PlatformKey::Windows => &self.windows,
            PlatformKey::Linux => &self.linux,
            PlatformKey::Android => &self.android,
        }
    }

    pub fn set_scripts(&mut self, key: PlatformKey, names: Vec<String>) {
        match key {
            PlatformKey::Windows => self.windows = names,
            PlatformKey::Linux => self.linux = names,
            PlatformKey::Android => self.android = names,
        }
    }

    pub fn keep_only(&mut self, key: PlatformKey) {
        if key != PlatformKey::Windows {
            self.windows.clear();
        }
        if key != PlatformKey::Linux {
            self.linux.clear();
        }
        if key != PlatformKey::Android {
            self.android.clear();
        }
    }
}

impl Component for PlatformOverride {
    const SCHEMA: ComponentSchema = ComponentSchema::new(
        "PlatformOverride",
        &[
            FieldSchema::new("windows", <Vec<String> as Field>::KIND),
            FieldSchema::new("linux", <Vec<String> as Field>::KIND),
            FieldSchema::new("android", <Vec<String> as Field>::KIND),
        ],
    );

    fn to_values(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("windows", Field::to_value(&self.windows)),
            ("linux", Field::to_value(&self.linux)),
            ("android", Field::to_value(&self.android)),
        ]
    }

    fn from_values(values: &[(&str, Value)]) -> Result<Self, DecodeError> {
        Ok(Self {
            windows: take_field(values, "windows")?,
            linux: take_field(values, "linux")?,
            android: take_field(values, "android")?,
        })
    }
}
