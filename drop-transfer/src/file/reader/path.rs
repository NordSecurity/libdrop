use std::{fs, io::Read, path::Path};

// Reads a file from the given path
pub struct FileReader {
    file: fs::File,
    pos: u64,
}

impl FileReader {
    pub fn new(path: &Path) -> crate::Result<Self> {
        let file = fs::File::open(path)?;
        Ok(Self { file, pos: 0 })
    }
}

impl super::Reader for FileReader {
    fn read(&mut self, buf: &mut [u8]) -> crate::Result<usize> {
        let n = self.file.read(buf)?;
        self.pos += n as u64;
        Ok(n)
    }

    fn bytes_read(&self) -> u64 {
        self.pos
    }

    fn meta(&mut self) -> crate::Result<fs::Metadata> {
        let meta = self.file.metadata()?;
        Ok(meta)
    }
}
