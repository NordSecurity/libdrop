mod id;
mod reader;

#[cfg(unix)]
use std::os::unix::prelude::*;
use std::{
    fs::{
        OpenOptions, {self},
    },
    io::{self, Read},
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

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum FileKind {
    FileToSend {
        meta: Hidden<fs::Metadata>,
        source: FileSource,
        mime_type: Option<Hidden<String>>,
    },
    FileToRecv {
        size: u64,
    },
}

#[derive(Clone, Debug)]
pub enum FileSource {
    Path(Hidden<PathBuf>),
    #[cfg(unix)]
    Fd(RawFd),
}

#[derive(Clone, Debug)]
pub struct File {
    pub(crate) file_id: FileId,
    pub(crate) subpath: FileSubPath,
    pub(crate) kind: FileKind,
}

impl File {
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
            let file_id = file_id_from_path(&path)?;
            let file = File::new_to_send(subpath, path, meta, file_id)?;
            files.push(file);
        }

        Ok(files)
    }

    pub fn from_path(path: impl Into<PathBuf>, config: &DropConfig) -> Result<Vec<Self>, Error> {
        let path = path.into();
        let meta = fs::symlink_metadata(&path)?;
        let file_id = file_id_from_path(&path)?;

        let files = if meta.is_dir() {
            File::walk(&path, config)?
        } else {
            let file = File::new_to_send(FileSubPath::from_file_name(&path)?, path, meta, file_id)?;
            vec![file]
        };

        Ok(files)
    }

    pub(crate) fn new_to_send(
        subpath: FileSubPath,
        path: PathBuf,
        meta: fs::Metadata,
        file_id: FileId,
    ) -> Result<Self, Error> {
        assert!(!meta.is_dir(), "Did not expect directory metadata");

        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW);

        // Check if we are allowed to read the file
        let mut file = options.open(&path)?;
        let mut buf = vec![0u8; HEADER_SIZE];
        let header_len = file.read(&mut buf)?;
        let mime_type = infer::get(&buf[0..header_len])
            .map_or("unknown", |t| t.mime_type())
            .to_string();

        Ok(Self {
            file_id,
            subpath,
            kind: FileKind::FileToSend {
                meta: Hidden(meta),
                source: FileSource::Path(Hidden(path)),
                mime_type: Some(Hidden(mime_type)),
            },
        })
    }

    #[cfg(unix)]
    pub fn from_fd(path: impl AsRef<Path>, fd: RawFd, unique_id: usize) -> Result<Self, Error> {
        let subpath = FileSubPath::from_file_name(path.as_ref())?;
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

            let mut hash = sha2::Sha256::new();
            hash.update(path.as_ref().as_os_str().as_bytes());
            hash.update(unique_id.to_ne_bytes());

            Ok(Self {
                file_id: FileId::from(hash),
                subpath,
                kind: FileKind::FileToSend {
                    meta: Hidden(meta),
                    source: FileSource::Fd(fd),
                    mime_type: Some(Hidden(mime_type)),
                },
            })
        };
        let result = create_file();

        // Prevent rust from closing the file
        let _ = f.into_raw_fd();

        result
    }

    pub fn size(&self) -> u64 {
        match &self.kind {
            FileKind::FileToSend { meta, .. } => meta.len(),
            FileKind::FileToRecv { size } => *size,
        }
    }

    pub fn id(&self) -> &FileId {
        &self.file_id
    }

    pub fn subpath(&self) -> &FileSubPath {
        &self.subpath
    }

    pub fn info(&self) -> Option<FileInfo> {
        match &self.kind {
            FileKind::FileToSend { mime_type, .. } => Some(FileInfo {
                mime_type: mime_type
                    .as_deref()
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string()),
                extension: AsRef::<Path>::as_ref(self.subpath.name())
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                size_kb: (self.size() as f64 / 1024.0).ceil() as i32,
            }),
            _ => None,
        }
    }

    // Open the file if it wasn't already opened and return the std::fs::File
    // instance
    pub(crate) fn open(&self, offset: u64) -> crate::Result<FileReader> {
        match &self.kind {
            FileKind::FileToSend { meta, source, .. } => {
                let mut reader = reader::open(source)?;
                reader.seek(io::SeekFrom::Start(offset))?;
                FileReader::new(reader, meta.0.clone())
            }
            _ => Err(Error::BadFile),
        }
    }

    /// Calculate sha2 of a file. This is a blocking operation
    pub(crate) fn checksum(&self, limit: u64) -> crate::Result<[u8; 32]> {
        let reader = match &self.kind {
            FileKind::FileToSend { source, .. } => reader::open(source)?,
            _ => return Err(Error::BadFile),
        };

        let mut reader = io::BufReader::new(reader).take(limit);
        let csum = checksum(&mut reader)?;
        Ok(csum)
    }
}

pub fn checksum(reader: &mut impl io::Read) -> io::Result<[u8; 32]> {
    let mut csum = sha2::Sha256::new();
    io::copy(reader, &mut csum)?;
    Ok(csum.finalize().into())
}

fn file_id_from_path(path: impl AsRef<Path>) -> crate::Result<FileId> {
    let abs = crate::utils::make_path_absolute(path)?;
    let mut hash = sha2::Sha256::new();
    hash.update(abs.to_string_lossy().as_bytes());
    Ok(FileId::from(hash))
}

#[cfg(test)]
mod tests {
    const TEST: &[u8] = b"abc";
    const EXPECTED: &[u8] = b"\xba\x78\x16\xbf\x8f\x01\xcf\xea\x41\x41\x40\xde\x5d\xae\x22\x23\xb0\x03\x61\xa3\x96\x17\x7a\x9c\xb4\x10\xff\x61\xf2\x00\x15\xad";

    #[test]
    fn checksum() {
        let csum = super::checksum(&mut &TEST[..]).unwrap();
        assert_eq!(csum.as_slice(), EXPECTED);
    }

    #[test]
    fn file_checksum() {
        use std::io::Write;

        use drop_config::DropConfig;

        let csum = {
            let mut tmp = tempfile::NamedTempFile::new().expect("Failed to create tmp file");
            tmp.write_all(TEST).unwrap();

            let file = &super::File::from_path(tmp.path(), &DropConfig::default()).unwrap()[0];
            let size = file.size();

            assert_eq!(size, TEST.len() as u64);

            file.checksum(size).unwrap()
        };

        assert_eq!(csum.as_slice(), EXPECTED);
    }
}
