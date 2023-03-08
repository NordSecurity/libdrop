use std::{
    borrow::Borrow,
    fmt, io, ops,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[serde(transparent)]
pub struct Hidden<T>(pub T);

impl<T> fmt::Debug for Hidden<T>
where
    T: fmt::Debug,
{
    #[cfg(debug_assertions)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }

    #[cfg(not(debug_assertions))]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("****")
    }
}

impl<T> From<T> for Hidden<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T> ops::Deref for Hidden<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> ops::DerefMut for Hidden<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Borrow<T> for Hidden<T> {
    fn borrow(&self) -> &T {
        &self.0
    }
}

pub fn map_path_if_exists(location: &Path) -> crate::Result<PathBuf> {
    let filename = location
        .file_stem()
        .ok_or(crate::Error::BadPath)?
        .to_string_lossy();

    let mut dst_location = location.to_path_buf();

    for rep_count in 1.. {
        // Skip if there is already a file with the same name.
        // Additionaly there could be a dangling symlink with the same name,
        // the `symlink_metadata()` ensures we can catch that.
        if matches!(dst_location.symlink_metadata() , Err(err) if err.kind() == io::ErrorKind::NotFound)
        {
            break;
        } else {
            let mut filename = format!("{filename}({rep_count})");
            if let Some(extension) = location.extension() {
                filename.extend([".", &extension.to_string_lossy()]);
            };

            dst_location.set_file_name(filename);
        };
    }

    Ok(dst_location)
}
