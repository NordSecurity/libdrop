mod id;
mod reader;

#[cfg(unix)]
use std::os::unix::prelude::*;
use std::{
    collections::HashMap,
    fs::{
        OpenOptions, {self},
    },
    io::Read,
    iter,
    path::{Path, PathBuf},
};

use drop_analytics::FileInfo;
use drop_config::DropConfig;
pub use id::FileId;
pub use reader::FileReader;

use crate::{utils::Hidden, Error};

const HEADER_SIZE: usize = 1024;

#[derive(Clone, Debug)]
pub enum FileKind {
    Dir {
        children: HashMap<Hidden<String>, File>,
    },
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
    pub(crate) name: Hidden<String>,
    pub(crate) kind: FileKind,
}

impl File {
    fn walk(path: &Path, config: &DropConfig) -> Result<Self, Error> {
        fn walk(
            path: &Path,
            depth: &mut usize,
            breadth: &mut usize,
            config: &DropConfig,
        ) -> Result<File, Error> {
            let name = path
                .file_name()
                .ok_or(crate::Error::BadPath)?
                .to_string_lossy()
                .to_string();

            let mut children = HashMap::new();

            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let ft = entry.file_type()?;

                let child = if ft.is_dir() {
                    *depth += 1;
                    if *depth > config.dir_depth_limit {
                        return Err(Error::TransferLimitsExceeded);
                    }

                    let subdir = walk(&entry.path(), depth, breadth, config)?;
                    *depth -= 1;

                    subdir
                } else if ft.is_file() {
                    File::new(entry.path(), entry.metadata()?)?
                } else {
                    continue;
                };

                children.insert(child.name.clone(), child);

                *breadth += 1;
                if *breadth > config.transfer_file_limit {
                    return Err(Error::TransferLimitsExceeded);
                }
            }

            Ok(File {
                name: Hidden(name),
                kind: FileKind::Dir { children },
            })
        }

        let mut depth = 1;
        let mut breadth = 0;
        walk(path, &mut depth, &mut breadth, config)
    }

    pub fn child(&self, name: &String) -> Option<&Self> {
        self.children_inner()?.get(name)
    }

    pub fn children(&self) -> impl Iterator<Item = &File> {
        self.children_inner().into_iter().flat_map(|hm| hm.values())
    }

    fn children_inner(&self) -> Option<&HashMap<Hidden<String>, Self>> {
        match &self.kind {
            FileKind::Dir { children } => Some(children),
            _ => None,
        }
    }

    pub fn from_path(
        path: impl Into<PathBuf>,
        fd: Option<i32>,
        config: &DropConfig,
    ) -> Result<Self, Error> {
        let path = path.into();

        if let Some(fd) = fd {
            #[cfg(target_os = "windows")]
            return Err(Error::InvalidArgument);
            #[cfg(not(target_os = "windows"))]
            File::new_with_fd(&path, fd).or(Err(Error::BadFile))
        } else {
            let meta = fs::symlink_metadata(&path)?;

            if meta.is_dir() {
                File::walk(&path, config)
            } else {
                File::new(path, meta)
            }
        }
    }

    fn new(path: PathBuf, meta: fs::Metadata) -> Result<Self, Error> {
        assert!(!meta.is_dir(), "Did not expect directory metadata");

        let name = path
            .file_name()
            .ok_or(crate::Error::BadPath)?
            .to_str()
            .ok_or(crate::Error::BadPath)?
            .to_string();

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
            name: Hidden(name),
            kind: FileKind::FileToSend {
                meta: Hidden(meta),
                source: FileSource::Path(Hidden(path)),
                mime_type: Some(Hidden(mime_type)),
            },
        })
    }

    #[cfg(unix)]
    fn new_with_fd(path: &Path, fd: RawFd) -> Result<Self, Error> {
        let name = path
            .file_name()
            .ok_or(crate::Error::BadPath)?
            .to_str()
            .ok_or(crate::Error::BadPath)?
            .to_string();

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
                name: Hidden(name),
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

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn size(&self) -> Option<u64> {
        match &self.kind {
            FileKind::FileToSend { meta, .. } => Some(meta.len()),
            FileKind::FileToRecv { size } => Some(*size),
            _ => None,
        }
    }

    pub fn info(&self) -> Option<FileInfo> {
        match &self.kind {
            FileKind::FileToSend { mime_type, .. } => Some(FileInfo {
                mime_type: mime_type
                    .as_deref()
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string()),
                extension: AsRef::<Path>::as_ref(self.name.as_str())
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                size_kb: self
                    .size()
                    .map(|s| (s as f64 / 1024.0).ceil() as i32)
                    .unwrap_or_default(),
            }),
            _ => None,
        }
    }

    // Open the file if it wasn't already opened and return the std::fs::File
    // instance
    pub(crate) fn open(&self) -> Result<FileReader, Error> {
        match &self.kind {
            FileKind::FileToSend { meta, source, .. } => FileReader::new(source, meta.0.clone()),
            _ => Err(Error::BadFile),
        }
    }

    pub fn is_dir(&self) -> bool {
        self.children_inner().is_some()
    }

    pub fn iter(&self) -> Box<dyn Iterator<Item = &File> + '_> {
        Box::new(
            self.children_inner()
                .into_iter()
                .flat_map(|hm| hm.values())
                .flat_map(|c| iter::once(c).chain(c.iter())),
        )
    }
}
