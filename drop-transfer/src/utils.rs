use std::{
    fmt, io, iter,
    net::SocketAddr,
    ops,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::{
    net::{TcpSocket, TcpStream},
    sync::watch,
};

#[derive(Deserialize, Serialize, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[serde(transparent)]
pub struct Hidden<T>(pub T);

pub struct RetryTrigger {
    chan: watch::Receiver<()>,
    retry: usize,
}

impl RetryTrigger {
    pub fn new(chan: watch::Receiver<()>) -> Self {
        Self { chan, retry: 0 }
    }

    pub async fn backoff(&mut self) {
        let delay = drop_config::RETRY_INTERVALS
            .get(self.retry)
            .map_or(Duration::MAX, |d| *d);

        self.retry = tokio::select! {
            _ = self.chan.changed() => 0,
            _ = tokio::time::sleep(delay) => self.retry + 1,
        };
    }
}

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

/// Returns an iterator yielding first the original path and then appends (i) i
/// = 1,2,3 ... to the file name
pub fn filepath_variants(location: &'_ Path) -> crate::Result<impl Iterator<Item = PathBuf> + '_> {
    let filename = location
        .file_stem()
        .ok_or_else(|| crate::Error::BadPath("Missing file stem".into()))?
        .to_string_lossy();

    let iter = iter::once(location.to_path_buf()).chain((1..).map(move |i| {
        let mut filename = format!("{filename}({i})");
        if let Some(extension) = location.extension() {
            filename.extend([".", &extension.to_string_lossy()]);
        };

        location.with_file_name(filename)
    }));

    Ok(iter)
}

/// Replace invalid characters or invalid file names
/// Rules taken from: <https://stackoverflow.com/questions/1976007/what-characters-are-forbidden-in-windows-and-linux-directory-names>
pub fn normalize_filename(filename: impl AsRef<str>) -> String {
    const REPLACEMENT_CHAR: &str = "_";

    #[cfg(windows)]
    const ILLEGAL_CHARS: &[char] = &['<', '>', ':', '"', '\\', '/', '|', '?', '*'];

    #[cfg(all(not(windows), any(target_os = "macos", target_os = "ios")))]
    const ILLEGAL_CHARS: &[char] = &[':', '/'];

    #[cfg(all(not(windows), not(any(target_os = "macos", target_os = "ios"))))]
    const ILLEGAL_CHARS: &[char] = &['/'];

    #[cfg(windows)]
    fn check_illegal_filename(mut name: String) -> String {
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
    fn check_illegal_filename(name: String) -> String {
        name
    }

    let name = filename
        .as_ref()
        .replace(ILLEGAL_CHARS, REPLACEMENT_CHAR)
        .replace(|c: char| c.is_ascii_control(), REPLACEMENT_CHAR);

    check_illegal_filename(name)
}

pub fn make_path_absolute(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    let path = path.as_ref();

    let abs = if path.is_absolute() {
        path.canonicalize()?
    } else {
        std::env::current_dir()?.join(path).canonicalize()?
    };

    Ok(abs)
}

/// Makes the TCP connection with a given local IP address
pub async fn connect(local: SocketAddr, remote: SocketAddr) -> io::Result<TcpStream> {
    let sock = if local.is_ipv4() {
        TcpSocket::new_v4()
    } else {
        TcpSocket::new_v6()
    }?;

    sock.bind(local)?;

    sock.connect(remote).await
}

/// Returns the number of kilobytes rounded up, used for moose event size
/// calculations
pub fn to_kb(bytes: u64) -> i32 {
    (bytes as f64 / 1024.0).ceil() as i32
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn path_normalization() {
        let valid_path = "this...isavalidpath1234$$%^😀";
        let norm = normalize_filename(valid_path);
        assert_eq!(norm, valid_path);

        let ascii_control = "abc\x03d\ne\x09f";
        let norm = normalize_filename(ascii_control);
        assert_eq!(norm, "abc_d_e_f");

        #[cfg(windows)]
        {
            let special_char = "a\\b<\\asdf>as:d?f";
            let norm = normalize_filename(special_char);
            assert_eq!(norm, "a_b__asdf_as_d_f");

            let dot_at_end = "asdf.";
            let norm = normalize_filename(dot_at_end);
            assert_eq!(norm, "asdf._");

            let special_name = "COM1.txt.png";
            let norm = normalize_filename(special_name);
            assert_eq!(norm, "_COM1.txt.png");
        }
    }

    #[test]
    fn filepath_variant_iteration() {
        let mut iter = filepath_variants("file.ext".as_ref()).unwrap();

        assert_eq!(iter.next(), Some(PathBuf::from("file.ext")));
        assert_eq!(iter.next(), Some(PathBuf::from("file(1).ext")));
        assert_eq!(iter.next(), Some(PathBuf::from("file(2).ext")));
        assert_eq!(iter.next(), Some(PathBuf::from("file(3).ext")));
    }
}
