mod id;
mod reader;

#[cfg(unix)]
use std::os::unix::prelude::*;
use std::{
    fs::{
        OpenOptions, {self},
    },
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use drop_analytics::FileInfo;
use drop_config::DropConfig;
pub use id::{FileId, FileSubPath};
pub use reader::FileReader;
use sha2::Digest;
use walkdir::WalkDir;

use crate::{utils::Hidden, Error};

#[cfg(unix)]
pub type FdResolver = dyn Fn(&str) -> Option<RawFd> + Send + Sync;

const HEADER_SIZE: usize = 1024;
const UNKNOWN_STR: &str = "unknown";

pub trait File {
    fn id(&self) -> &FileId;
    fn subpath(&self) -> &FileSubPath;
    fn size(&self) -> u64;
    fn mime_type(&self) -> &str;

    fn info(&self) -> FileInfo {
        FileInfo {
            mime_type: self.mime_type().to_string(),
            extension: self.subpath().extension().unwrap_or("none").to_string(),
            size_kb: (self.size() as f64 / 1024.0).ceil() as i32,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileToSend {
    file_id: FileId,
    subpath: FileSubPath,
    meta: Hidden<fs::Metadata>,
    pub(crate) source: FileSource,
    mime_type: Option<Hidden<String>>,
}

#[derive(Clone, Debug)]
pub struct FileToRecv {
    file_id: FileId,
    subpath: FileSubPath,
    size: u64,
}

#[derive(Clone, Debug)]
pub enum FileSource {
    Path(Hidden<PathBuf>),
    #[cfg(unix)]
    Fd {
        fd: RawFd,
        content_uri: url::Url,
    },
}

impl File for FileToSend {
    fn id(&self) -> &FileId {
        &self.file_id
    }

    fn subpath(&self) -> &FileSubPath {
        &self.subpath
    }

    fn size(&self) -> u64 {
        self.meta.len()
    }

    fn mime_type(&self) -> &str {
        self.mime_type
            .as_ref()
            .map(|x| x.as_str())
            .unwrap_or(UNKNOWN_STR)
    }
}

impl File for FileToRecv {
    fn id(&self) -> &FileId {
        &self.file_id
    }

    fn subpath(&self) -> &FileSubPath {
        &self.subpath
    }

    fn size(&self) -> u64 {
        self.size
    }

    fn mime_type(&self) -> &str {
        UNKNOWN_STR
    }
}

impl FileToRecv {
    pub fn new(file_id: FileId, subpath: FileSubPath, size: u64) -> Self {
        Self {
            file_id,
            subpath,
            size,
        }
    }
}

impl FileToSend {
    pub fn from_path(path: impl Into<PathBuf>, config: &DropConfig) -> Result<Vec<Self>, Error> {
        let path = path.into();
        let meta = fs::symlink_metadata(&path)?;
        let abspath = crate::utils::make_path_absolute(&path)?;
        let file_id = file_id_from_path(&abspath)?;

        let files = if meta.is_dir() {
            Self::walk(&path, config)?
        } else {
            let file = Self::new(FileSubPath::from_file_name(&path)?, abspath, meta, file_id)?;
            vec![file]
        };

        Ok(files)
    }

    pub(crate) fn new(
        subpath: FileSubPath,
        abspath: PathBuf,
        meta: fs::Metadata,
        file_id: FileId,
    ) -> Result<Self, Error> {
        assert!(!meta.is_dir(), "Did not expect directory metadata");
        assert!(abspath.is_absolute(), "Expecting absolute path only");

        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW);

        // Check if we are allowed to read the file
        let mut file = options.open(&abspath)?;
        let mut buf = vec![0u8; HEADER_SIZE];
        let header_len = file.read(&mut buf)?;
        let mime_type = infer::get(&buf[0..header_len])
            .map_or("unknown", |t| t.mime_type())
            .to_string();

        Ok(Self {
            file_id,
            subpath,
            meta: Hidden(meta),
            source: FileSource::Path(Hidden(abspath)),
            mime_type: Some(Hidden(mime_type)),
        })
    }

    #[cfg(unix)]
    pub fn from_fd(
        path: impl AsRef<Path>,
        content_uri: url::Url,
        fd: RawFd,
        unique_id: usize,
    ) -> Result<Self, Error> {
        let subpath = FileSubPath::from_file_name(path.as_ref())?;

        let mut hash = sha2::Sha256::new();
        hash.update(path.as_ref().as_os_str().as_bytes());
        hash.update(unique_id.to_ne_bytes());
        let file_id = FileId::from(hash);

        Self::new_from_fd(subpath, content_uri, fd, file_id)
    }

    #[cfg(unix)]
    pub fn new_from_fd(
        subpath: FileSubPath,
        content_uri: url::Url,
        fd: RawFd,
        file_id: FileId,
    ) -> Result<Self, Error> {
        let f = unsafe { fs::File::from_raw_fd(fd) };

        let create_file = || {
            let meta = f.metadata()?;

            if meta.is_dir() {
                return Err(Error::DirectoryNotExpected);
            }

            let mut buf = vec![0u8; HEADER_SIZE];
            let header_len = f.read_at(&mut buf, 0)?;
            let mime_type = infer::get(&buf[0..header_len])
                .map_or("unknown", |t| t.mime_type())
                .to_string();

            Ok(Self {
                file_id,
                subpath,
                meta: Hidden(meta),
                source: FileSource::Fd { fd, content_uri },
                mime_type: Some(Hidden(mime_type)),
            })
        };
        let result = create_file();

        // Prevent rust from closing the file
        let _ = f.into_raw_fd();

        result
    }

    fn walk(path: &Path, config: &DropConfig) -> Result<Vec<Self>, Error> {
        let parent = path
            .parent()
            .ok_or_else(|| crate::Error::BadPath("Missing parent directory".into()))?;

        let mut files = Vec::new();
        let mut breadth = 0;

        for entry in WalkDir::new(path).min_depth(1).into_iter() {
            let entry = entry?;
            let meta = entry.metadata()?;

            if !meta.is_file() {
                continue;
            }

            if entry.depth() > config.dir_depth_limit {
                return Err(Error::TransferLimitsExceeded);
            }

            breadth += 1;

            if breadth > config.transfer_file_limit {
                return Err(Error::TransferLimitsExceeded);
            }

            let subpath = entry
                .path()
                .strip_prefix(parent)
                .map_err(|err| crate::Error::BadPath(err.to_string()))?;

            let subpath = FileSubPath::from_path(subpath)?;

            let path = entry.into_path();
            let abspath = crate::utils::make_path_absolute(&path)?;
            let file_id = file_id_from_path(&abspath)?;

            let file = Self::new(subpath, abspath, meta, file_id)?;
            files.push(file);
        }

        Ok(files)
    }

    // Open the file if it wasn't already opened and return the std::fs::File
    // instance
    pub(crate) fn open(&self, offset: u64) -> crate::Result<FileReader> {
        let mut reader = reader::open(&self.source)?;
        reader.seek(io::SeekFrom::Start(offset))?;
        FileReader::new(reader, self.meta.0.clone())
    }

    /// Calculate sha2 of a file. This is a blocking operation
    pub(crate) async fn checksum(&self, limit: u64) -> crate::Result<[u8; 32]> {
        let reader = reader::open(&self.source)?;
        let mut reader = io::BufReader::new(reader).take(limit);
        let csum = checksum(&mut reader).await?;
        Ok(csum)
    }
}

pub async fn checksum(reader: &mut impl io::Read) -> io::Result<[u8; 32]> {
    let mut csum = sha2::Sha256::new();

    let mut buf = vec![0u8; 8 * 1024]; // 8kB buffer

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }

        csum.write_all(&buf[..n])?;

        // Since these are all blocking operation we need to give tokio runtime a
        // timeslice
        tokio::task::yield_now().await;
    }

    Ok(csum.finalize().into())
}

fn file_id_from_path(path: impl AsRef<Path>) -> crate::Result<FileId> {
    let mut hash = sha2::Sha256::new();
    hash.update(path.as_ref().to_string_lossy().as_bytes());
    Ok(FileId::from(hash))
}

#[cfg(test)]
mod tests {
    const TEST: &[u8] = b"abc";
    const EXPECTED: &[u8] = b"\xba\x78\x16\xbf\x8f\x01\xcf\xea\x41\x41\x40\xde\x5d\xae\x22\x23\xb0\x03\x61\xa3\x96\x17\x7a\x9c\xb4\x10\xff\x61\xf2\x00\x15\xad";

    #[tokio::test]
    async fn checksum() {
        let csum = super::checksum(&mut &TEST[..]).await.unwrap();
        assert_eq!(csum.as_slice(), EXPECTED);
    }

    #[tokio::test]
    async fn file_checksum() {
        use std::io::Write;

        use drop_config::DropConfig;

        let csum = {
            let mut tmp = tempfile::NamedTempFile::new().expect("Failed to create tmp file");
            tmp.write_all(TEST).unwrap();

            let file =
                &super::FileToSend::from_path(tmp.path(), &DropConfig::default()).unwrap()[0];
            let size = file.meta.len();

            assert_eq!(size, TEST.len() as u64);

            file.checksum(size).await.unwrap()
        };

        assert_eq!(csum.as_slice(), EXPECTED);
    }
}
