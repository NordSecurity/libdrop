#[cfg(unix)]
mod fd;

mod path;

use std::{fs, io};

use crate::Error;

/// Number of bytes read from files when uploading
const CHUNK_SIZE: usize = 1024 * 1024;

pub struct FileReader {
    inner: Box<dyn Reader>,
    buffer: Box<[u8]>,
    meta: fs::Metadata,
}

pub(super) fn open(source: &super::FileSource) -> crate::Result<Box<dyn Reader>> {
    let reader: Box<dyn Reader> = match source {
        super::FileSource::Path(path) => Box::new(path::FileReader::new(path)?),
        #[cfg(unix)]
        super::FileSource::Fd { fd, .. } => Box::new(unsafe { fd::FileReader::new(*fd) }),
    };

    Ok(reader)
}

impl FileReader {
    pub(super) fn new(reader: Box<dyn Reader>, meta: fs::Metadata) -> crate::Result<Self> {
        Ok(Self {
            inner: reader,
            buffer: vec![0u8; CHUNK_SIZE].into_boxed_slice(),
            meta,
        })
    }

    pub fn read_chunk(&mut self) -> crate::Result<Option<&[u8]>> {
        let n = self.inner.read(&mut self.buffer)?;

        if !self.is_mtime_ok().unwrap_or(true) {
            return Err(Error::FileModified);
        }

        let total_read = self.inner.bytes_read();

        if n == 0 {
            // File size might have been reduced while in the loop which
            // will result in an error
            if total_read != self.meta.len() {
                return Err(Error::MismatchedSize);
            } else {
                return Ok(None);
            }
        }

        if total_read > self.meta.len() {
            return Err(Error::MismatchedSize);
        }

        let chunk = &self.buffer[..n];
        Ok(Some(chunk))
    }

    fn is_mtime_ok(&mut self) -> crate::Result<bool> {
        let mtime_orig = self.meta.modified()?;
        let mtime_act = self.inner.meta()?.modified()?;

        Ok(mtime_orig == mtime_act)
    }
}

pub(super) trait Reader: io::Read + io::Seek + Send + Sync {
    fn bytes_read(&self) -> u64;
    fn meta(&mut self) -> crate::Result<fs::Metadata>;
}
