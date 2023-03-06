#[cfg(unix)]
mod fd;

mod path;

use std::path::Path;

use crate::Error;

/// Number of bytes read from files when uploading
const CHUNK_SIZE: usize = 1024 * 1024;

pub struct FileReader {
    inner: Box<dyn Reader>,
    buffer: Box<[u8]>,
    size: u64,
}

impl FileReader {
    pub(super) fn new(source: super::FileSource, size: u64, path: &Path) -> crate::Result<Self> {
        let inner: Box<dyn Reader> = match source {
            super::FileSource::Path => Box::new(path::FileReader::new(path)?),
            #[cfg(unix)]
            super::FileSource::Fd(fd) => Box::new(unsafe { fd::FileReader::new(fd) }),
        };

        Ok(Self {
            inner,
            buffer: vec![0u8; CHUNK_SIZE].into_boxed_slice(),
            size,
        })
    }

    pub fn read_chunk(&mut self) -> crate::Result<Option<&[u8]>> {
        let n = self.inner.read(&mut self.buffer)?;
        let total_read = self.inner.bytes_read();

        if n == 0 {
            // File size might have been reduced while in the loop which
            // will result in an error
            if total_read != self.size {
                return Err(Error::MismatchedSize);
            } else {
                return Ok(None);
            }
        }

        if total_read > self.size {
            return Err(Error::MismatchedSize);
        }

        let chunk = &self.buffer[..n];
        Ok(Some(chunk))
    }
}

trait Reader: Send + Sync {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error>;
    fn bytes_read(&self) -> u64;
}
