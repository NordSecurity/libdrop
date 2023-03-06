use std::{fs, mem::ManuallyDrop, os::unix::prelude::*};

// This reader performs positional reads from the given file descriptor
pub struct FileReader {
    file: ManuallyDrop<fs::File>,
    pos: u64,
}

impl FileReader {
    pub unsafe fn new(fd: RawFd) -> Self {
        let file = fs::File::from_raw_fd(fd);
        Self {
            file: ManuallyDrop::new(file),
            pos: 0,
        }
    }
}

impl Drop for FileReader {
    fn drop(&mut self) {
        // We do not own the FD so we cannot allow rust to close the descriptor
        let _ = unsafe { ManuallyDrop::take(&mut self.file) }.into_raw_fd();
    }
}

impl super::Reader for FileReader {
    fn read(&mut self, buf: &mut [u8]) -> crate::Result<usize> {
        // Use positional read in order to not use the internal FD cursor
        let n = self.file.read_at(buf, self.pos)?;
        self.pos += n as u64;
        Ok(n)
    }

    fn bytes_read(&self) -> u64 {
        self.pos
    }
}
