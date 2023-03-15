use std::{
    borrow::{Borrow, Cow},
    ffi::OsString,
    fmt, io, ops,
    path::{Component, Path, PathBuf},
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

/// Replace invalid characters or invalid file names
/// Rules taken from: https://stackoverflow.com/questions/1976007/what-characters-are-forbidden-in-windows-and-linux-directory-names
pub fn normalize_path(path: impl AsRef<Path>) -> PathBuf {
    const REPLACEMENT_CHAR: &str = "_";

    #[cfg(windows)]
    const ILLEGAL_CHARS: &[char] = &['<', '>', ':', '"', '|', '?', '*'];

    #[cfg(all(not(windows), any(target_os = "macos", target_os = "ios")))]
    const ILLEGAL_CHARS: &[char] = &[':'];

    #[cfg(all(not(windows), not(any(target_os = "macos", target_os = "ios"))))]
    const ILLEGAL_CHARS: &[char] = &[];

    #[cfg(windows)]
    fn check_illegal_filenames(mut name: String) -> String {
        const ILLEGAL: &[&str] = &[
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];

        // file name cannot end with .
        if name.ends_with('.') {
            // append the replacement char
            name.push_str(REPLACEMENT_CHAR);
        }

        // check illegal names
        if let Some(prefix) = name.split('.').next() {
            if ILLEGAL.contains(&prefix) {
                // prepend the replacement char
                name.insert_str(0, REPLACEMENT_CHAR);
            }
        }

        name
    }

    #[cfg(not(windows))]
    fn check_illegal_filenames(name: String) -> String {
        name
    }

    path.as_ref()
        .components()
        .map(|comp| match comp {
            Component::Normal(name) => {
                let name = name
                    .to_string_lossy()
                    .replace(ILLEGAL_CHARS, REPLACEMENT_CHAR)
                    .replace(|c: char| c.is_ascii_control(), REPLACEMENT_CHAR);

                let name = check_illegal_filenames(name);

                Cow::Owned(OsString::from(name))
            }
            comp => Cow::Borrowed(comp.as_os_str()),
        })
        .collect()
}

#[cfg(test)]
mod tests {

    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn path_normalization() {
        let valid_path = "this/.././is/a/valid/path/1234/$$%^/😀";
        let norm = normalize_path(valid_path);
        assert_eq!(norm, Path::new(valid_path));

        let ascii_control = "a/b/c/\x03/d/\n/e/\x09/f";
        let norm = normalize_path(ascii_control);
        assert_eq!(norm, Path::new("a/b/c/_/d/_/e/_/f"));
    }

    #[cfg(windows)]
    #[test]
    fn path_normalization() {
        let valid_path = "C:\\this\\..\\.\\is\\a\\valid\\path\\1234\\$$%^\\😀";
        let norm = normalize_path(valid_path);
        assert_eq!(norm, Path::new(valid_path));

        let ascii_control = "a\\b\\c\\\x03\\d\\\n\\e\\\x09\\f";
        let norm = normalize_path(ascii_control);
        assert_eq!(norm, Path::new("a\\b\\c\\_\\d\\_\\e\\_\\f"));

        let special_char = "a\\<\\asdf>asdf";
        let norm = normalize_path(special_char);
        assert_eq!(norm, Path::new("a\\_\\asdf_asdf"));

        let dot_at_end = "a\\b.\\c\\d.";
        let norm = normalize_path(dot_at_end);
        assert_eq!(norm, Path::new("a\\b._\\c\\d._"));

        let special_name = "a\\NUL\\b\\COM1.txt.png";
        let norm = normalize_path(special_name);
        assert_eq!(norm, Path::new("a\\_NUL\\b\\_COM1.txt.png"));
    }
}
