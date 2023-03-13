use std::path::{Path, PathBuf};

use crate::utils::Hidden;

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct FileId(Hidden<Vec<String>>);

const SEPARATOR: &str = "/";

impl FileId {
    pub fn from_name(name: String) -> Self {
        Self(Hidden(vec![name]))
    }

    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.0.iter()
    }

    pub fn append(&mut self, name: String) {
        self.0.push(name);
    }
}

impl<T> From<T> for FileId
where
    T: AsRef<str>,
{
    fn from(value: T) -> Self {
        let vec = value
            .as_ref()
            .split(SEPARATOR)
            .map(ToString::to_string)
            .collect();
        Self(Hidden(vec))
    }
}

impl From<FileId> for PathBuf {
    fn from(FileId(Hidden(value)): FileId) -> Self {
        value.into_iter().collect()
    }
}

impl From<&FileId> for PathBuf {
    fn from(value: &FileId) -> Self {
        value.0.iter().collect()
    }
}

impl From<FileId> for Box<Path> {
    fn from(value: FileId) -> Self {
        PathBuf::from(value).into_boxed_path()
    }
}

impl From<&FileId> for Box<Path> {
    fn from(value: &FileId) -> Self {
        PathBuf::from(value).into_boxed_path()
    }
}

impl ToString for FileId {
    fn to_string(&self) -> String {
        self.0.join(SEPARATOR)
    }
}

impl serde::Serialize for FileId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for FileId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let str = String::deserialize(deserializer)?;
        Ok(Self::from(str.as_str()))
    }
}
