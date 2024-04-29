mod gather;
mod id;
mod reader;

use std::{
    fmt,
    future::Future,
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
};
#[cfg(unix)]
use std::{os::unix::prelude::*, sync::Arc};

use drop_analytics::TransferDirection;
use drop_config::DropConfig;
pub use gather::*;
pub use id::{FileId, FileSubPath};
use once_cell::sync::OnceCell;
pub use reader::FileReader;
use sha2::Digest;
use walkdir::WalkDir;

use crate::{utils::Hidden, Error};

pub struct FileInfo {
    pub path_id: String,
    pub direction: TransferDirection,
}

#[cfg(unix)]
pub type FdResolver = dyn Fn(&str) -> Option<RawFd> + Send + Sync;

const HEADER_SIZE: usize = 1024;
const UNKNOWN_STR: &str = "unknown";

const CHECKSUM_CHUNK_SIZE: usize = 256 * 1024; // 256 KiB

pub trait File {
    fn id(&self) -> &FileId;
    fn subpath(&self) -> &FileSubPath;
    fn size(&self) -> u64;
    fn mime_type(&self) -> &str;

    fn direction() -> TransferDirection;

    fn info(&self) -> FileInfo {
        FileInfo {
            path_id: self.id().to_string(),
            direction: Self::direction(),
        }
    }
}

#[derive(Debug)]
pub struct FileToSend {
    file_id: FileId,
    subpath: FileSubPath,
    size: u64,
    pub(crate) source: FileSource,
    mime_type: OnceCell<Hidden<String>>,
}

#[derive(Debug, Clone)]
pub struct FileToRecv {
    file_id: FileId,
    subpath: FileSubPath,
    size: u64,
}

pub enum FileSource {
    Path(Hidden<PathBuf>),
    #[cfg(unix)]
    Fd {
        fd: OnceCell<RawFd>,
        resolver: Option<Arc<FdResolver>>,
        content_uri: url::Url,
    },
}

impl fmt::Debug for FileSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileSource::Path(path) => f.debug_tuple("FileSource::Path").field(path).finish(),
            #[cfg(unix)]
            FileSource::Fd {
                fd, content_uri, ..
            } => f
                .debug_struct("FileSource::Fd")
                .field("uri", content_uri)
                .field("fd", fd)
                .finish_non_exhaustive(),
        }
    }
}

impl File for FileToSend {
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
        self.mime_type
            .get_or_try_init(|| {
                let reader = reader::open(&self.source)?;
                let mime = infer_mime(reader)?;
                crate::Result::Ok(Hidden(mime))
            })
            .map(|s| s.as_str())
            .unwrap_or(UNKNOWN_STR)
    }

    fn direction() -> TransferDirection {
        TransferDirection::Upload
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

    fn direction() -> TransferDirection {
        TransferDirection::Download
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
    pub fn base_dir(&self) -> Option<&str> {
        let fullpath = match &self.source {
            FileSource::Path(fullpath) => fullpath,
            #[cfg(unix)]
            FileSource::Fd { .. } => return None,
        };

        let base_dir = fullpath.ancestors().nth(self.subpath.len())?;
        base_dir.to_str()
    }

    fn from_path(path: impl AsRef<Path>, size: u64) -> crate::Result<Self> {
        let path = path.as_ref();
        let abspath = crate::utils::make_path_absolute(path)?;
        let file_id = file_id_from_path(&abspath)?;

        Ok(Self::new(
            FileSubPath::from_file_name(path)?,
            abspath,
            size,
            file_id,
        ))
    }

    pub(crate) fn new(subpath: FileSubPath, abspath: PathBuf, size: u64, file_id: FileId) -> Self {
        assert!(abspath.is_absolute(), "Expecting absolute path only");

        Self {
            file_id,
            subpath,
            size,
            source: FileSource::Path(Hidden(abspath)),
            mime_type: OnceCell::new(),
        }
    }

    #[cfg(unix)]
    fn from_fd(
        path: &Path,
        subpath: FileSubPath,
        content_uri: url::Url,
        fd: RawFd,
        unique_id: usize,
    ) -> Result<Self, Error> {
        let mut hash = sha2::Sha256::new();
        hash.update(path.as_os_str().as_bytes());
        hash.update(unique_id.to_ne_bytes());
        let file_id = FileId::from(hash);

        let f = unsafe { std::fs::File::from_raw_fd(fd) };

        let create_file = || {
            let meta = f.metadata()?;

            if meta.is_dir() {
                return Err(Error::DirectoryNotExpected);
            }

            Ok(Self {
                file_id,
                subpath,
                size: meta.len(),
                source: FileSource::Fd {
                    resolver: None,
                    fd: OnceCell::with_value(fd),
                    content_uri,
                },
                mime_type: OnceCell::new(),
            })
        };
        let result = create_file();

        // Prevent rust from closing the file
        let _ = f.into_raw_fd();

        result
    }

    #[cfg(unix)]
    pub fn new_from_content_uri(
        resolver: Arc<FdResolver>,
        subpath: FileSubPath,
        content_uri: url::Url,
        size: u64,
        file_id: FileId,
    ) -> Self {
        Self {
            file_id,
            subpath,
            size,
            source: FileSource::Fd {
                resolver: Some(resolver),
                fd: OnceCell::new(),
                content_uri,
            },
            mime_type: OnceCell::new(),
        }
    }

    fn walk(path: &Path, subname: &Path, config: &DropConfig) -> Result<Vec<Self>, Error> {
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

            let relpath = entry
                .path()
                .strip_prefix(path)
                .map_err(|err| crate::Error::BadPath(err.to_string()))?;

            let subpath = PathBuf::from_iter([subname, relpath]);
            let subpath = FileSubPath::from_path(subpath)?;

            let path = entry.into_path();
            let abspath = crate::utils::make_path_absolute(&path)?;
            let file_id = file_id_from_path(&abspath)?;

            let file = Self::new(subpath, abspath, meta.len(), file_id);
            files.push(file);
        }

        Ok(files)
    }

    // Open the file if it wasn't already opened and return the std::fs::File
    // instance
    pub(crate) fn open(&self, offset: u64) -> crate::Result<FileReader> {
        let mut reader = reader::open(&self.source)?;
        let meta = reader.meta()?;

        reader.seek(io::SeekFrom::Start(offset))?;
        FileReader::new(reader, meta)
    }

    /// Calculate sha2 of a file. This is a blocking operation
    pub(crate) async fn checksum<F, Fut>(
        &self,
        limit: u64,
        progress_cb: Option<F>,
        event_granularity: Option<u64>,
    ) -> crate::Result<[u8; 32]>
    where
        F: FnMut(u64) -> Fut + Send + Sync,
        Fut: Future<Output = ()>,
    {
        let reader = reader::open(&self.source)?.take(limit);
        let csum = checksum(reader, progress_cb, event_granularity).await?;
        Ok(csum)
    }
}

/// This function performs buffering internally. No need to use buffered
/// readers.
pub async fn checksum<F, Fut>(
    reader: impl io::Read,
    mut progress_cb: Option<F>,
    event_granularity: Option<u64>,
) -> io::Result<[u8; 32]>
where
    F: FnMut(u64) -> Fut + Send + Sync,
    Fut: Future<Output = ()>,
{
    let mut csum = sha2::Sha256::new();

    let mut reader = io::BufReader::with_capacity(CHECKSUM_CHUNK_SIZE, reader);

    let mut total_n: u64 = 0;
    let mut announced_bytes: u64 = 0;
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            // If we reached the end of file and the already announced_bytes are different
            // than the total file size, we announce the total file size
            // It simply means that the file size is not a multiple of granularity
            if let Some(progress_cb) = progress_cb.as_mut() {
                if announced_bytes != total_n {
                    progress_cb(total_n).await;
                }
            }

            break;
        }

        csum.write_all(buf)?;

        let n = buf.len();
        reader.consume(n);

        total_n += n as u64;

        if let (Some(progress_cb), Some(granularity)) = (progress_cb.as_mut(), event_granularity) {
            while announced_bytes + granularity <= total_n {
                announced_bytes += granularity;
                progress_cb(announced_bytes).await;
            }
        }

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

fn infer_mime(mut reader: impl io::Read) -> io::Result<String> {
    let mut buf = vec![0u8; HEADER_SIZE];
    let header_len = reader.read(&mut buf)?;
    let mime_type = infer::get(&buf[0..header_len])
        .map_or(UNKNOWN_STR, |t| t.mime_type())
        .to_string();

    Ok(mime_type)
}

#[cfg(test)]
mod tests {

    const TEST: &[u8] = b"abc";
    const EXPECTED: &[u8] = b"\xba\x78\x16\xbf\x8f\x01\xcf\xea\x41\x41\x40\xde\x5d\xae\x22\x23\xb0\x03\x61\xa3\x96\x17\x7a\x9c\xb4\x10\xff\x61\xf2\x00\x15\xad";

    #[tokio::test]
    async fn checksum() {
        let csum = super::checksum(
            &mut &TEST[..],
            None::<fn(u64) -> futures::future::Ready<()>>,
            None,
        )
        .await
        .unwrap();
        assert_eq!(csum.as_slice(), EXPECTED);
    }

    #[tokio::test]
    async fn file_checksum() {
        use std::io::Write;

        let csum = {
            let mut tmp = tempfile::NamedTempFile::new().expect("Failed to create tmp file");
            tmp.write_all(TEST).unwrap();

            let size = TEST.len() as _;
            let file = super::FileToSend::from_path(tmp.path(), size).unwrap();
            file.checksum(size, None::<fn(u64) -> futures::future::Ready<()>>, None)
                .await
                .unwrap()
        };

        assert_eq!(csum.as_slice(), EXPECTED);
    }

    #[test]
    fn checksum_yielding() {
        use std::{
            future::Future,
            io,
            pin::Pin,
            task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
        };

        let buf = vec![0xffu8; 20 * 1024]; // 20kB of data

        fn empty(_: *const ()) {}
        fn make_raw(_: *const ()) -> RawWaker {
            RawWaker::new(
                std::ptr::null(),
                &RawWakerVTable::new(make_raw, empty, empty, empty),
            )
        }
        let waker = unsafe { Waker::from_raw(make_raw(std::ptr::null())) };
        let mut cx = Context::from_waker(&waker);

        let mut cursor = io::Cursor::new(&buf);
        let mut future = super::checksum(
            &mut cursor,
            None::<fn(u64) -> futures::future::Ready<()>>,
            None,
        );
        let mut future = unsafe { Pin::new_unchecked(&mut future) };

        // expect it to yield 3 times (one at the very end)
        assert!(future.as_mut().poll(&mut cx).is_pending());
        assert!(matches!(future.as_mut().poll(&mut cx), Poll::Ready(Ok(_))));
    }
}
