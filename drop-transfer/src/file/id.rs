use std::{
    fmt,
    path::{Path, PathBuf},
};

use crate::utils::Hidden;

#[derive(Hash, Clone, PartialEq, Eq)]
pub struct FileSubPath(Vec<String>);

const SEPARATOR: &str = "/";

impl FileSubPath {
    pub fn from_name(name: String) -> Self {
        Self(vec![name])
    }

    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.0.iter()
    }

    pub fn append(&mut self, name: String) {
        self.0.push(name);
    }
}

impl<T> From<T> for FileSubPath
where
    T: AsRef<str>,
{
    fn from(value: T) -> Self {
        let vec = value
            .as_ref()
            .split(SEPARATOR)
            .map(ToString::to_string)
            .collect();
        Self(vec)
    }
}

impl From<FileSubPath> for PathBuf {
    fn from(FileSubPath(value): FileSubPath) -> Self {
        value.into_iter().collect()
    }
}

impl From<&FileSubPath> for PathBuf {
    fn from(value: &FileSubPath) -> Self {
        value.0.iter().collect()
    }
}

impl From<FileSubPath> for Box<Path> {
    fn from(value: FileSubPath) -> Self {
        PathBuf::from(value).into_boxed_path()
    }
}

impl From<&FileSubPath> for Box<Path> {
    fn from(value: &FileSubPath) -> Self {
        PathBuf::from(value).into_boxed_path()
    }
}

impl ToString for FileSubPath {
    fn to_string(&self) -> String {
        self.0.join(SEPARATOR)
    }
}

impl fmt::Debug for FileSubPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("FileId")
            .field(&Hidden(self.to_string()))
            .finish()
    }
}

impl serde::Serialize for FileSubPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for FileSubPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let str = String::deserialize(deserializer)?;
        Ok(Self::from(str.as_str()))
    }
}
