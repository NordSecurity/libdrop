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
    ops::Deref,
    path::{Path, PathBuf},
};

use drop_analytics::FileInfo;
use drop_config::DropConfig;
pub use reader::FileReader;
use walkdir::WalkDir;

use crate::{utils::Hidden, Error};

const HEADER_SIZE: usize = 1024;

#[derive(Clone, Debug)]
pub enum FileKind {
    Dir {
        children: HashMap<Hidden<Box<Path>>, File>,
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

#[derive(Clone, Copy, Debug)]
pub enum FileSource {
    Path,
    #[cfg(unix)]
    Fd(RawFd),
}

#[derive(Clone, Debug)]
pub struct File {
    pub(crate) path: Hidden<Box<Path>>,
    pub(crate) kind: FileKind,
}

impl File {
    fn walk(path: &Path, config: &DropConfig) -> Result<Self, Error> {
        let mut root = Self::new_dir(path.into());

        let mut breadth = 0;
        for entry in WalkDir::new(path).min_depth(1).into_iter() {
            let entry = entry.map_err(|_| Error::BadFile)?;

            if entry.file_type().is_dir() {
                continue;
            }

            if entry.depth() > config.dir_depth_limit {
                return Err(Error::TransferLimitsExceeded);
            }

            breadth += 1;

            if breadth > config.transfer_file_limit {
                return Err(Error::TransferLimitsExceeded);
            }

            root.insert(File::new(entry.path(), entry.path().symlink_metadata()?)?)?;
        }

        Ok(root)
    }

    pub fn child(&self, path: &Path) -> Option<&Self> {
        let mut file = self;
        let mut ret = None;

        for component in path.components() {
            let key: Box<Path> = AsRef::<Path>::as_ref(&component).into();
            let child = file.children_inner()?.get(&key)?;

            file = child;
            ret = Some(child);
        }

        ret
    }

    pub fn children(&self) -> impl Iterator<Item = &File> {
        self.children_inner().into_iter().flat_map(|hm| hm.values())
    }

    fn children_inner(&self) -> Option<&HashMap<Hidden<Box<Path>>, Self>> {
        match &self.kind {
            FileKind::Dir { children } => Some(children),
            _ => None,
        }
    }

    fn children_inner_mut(&mut self) -> Option<&mut HashMap<Hidden<Box<Path>>, Self>> {
        match &mut self.kind {
            FileKind::Dir { children } => Some(children),
            _ => None,
        }
    }

    fn insert(&mut self, child: File) -> Result<(), Error> {
        let lhs = self.path();
        let rhs = child.path();

        if !rhs.starts_with(lhs) {
            return Err(Error::BadPath);
        }

        let path: Box<Path> = rhs
            .components()
            .take(lhs.components().count() + 1)
            .map(|c| Into::<Box<Path>>::into(AsRef::<Path>::as_ref(&c)))
            .reduce(|lhs, p| lhs.join(p).into())
            .ok_or(Error::BadPath)?;

        let filename = PathBuf::from(&path.file_name().ok_or(Error::BadPath)?).into_boxed_path();
        let children = self.children_inner_mut().expect("Expected directory");

        // We‘re inserting a parent
        if child.path() != &*path {
            children
                .entry(Hidden(filename))
                .or_insert_with(|| Self::new_dir(path))
                .insert(child)?;
        } else {
            children.insert(
                Hidden(filename),
                Self {
                    path: Hidden(path),
                    ..child
                },
            );
        }

        Ok(())
    }

    fn new_dir(path: Box<Path>) -> Self {
        Self {
            path: Hidden(path),
            kind: FileKind::Dir {
                children: HashMap::new(),
            },
        }
    }

    pub fn from_path(path: &Path, fd: Option<i32>, config: &DropConfig) -> Result<Self, Error> {
        if let Some(fd) = fd {
            #[cfg(target_os = "windows")]
            return Err(Error::InvalidArgument);
            #[cfg(not(target_os = "windows"))]
            File::new_with_fd(path, fd).or(Err(Error::BadFile))
        } else {
            let meta = fs::symlink_metadata(path)?;

            if meta.is_dir() {
                File::walk(path, config)
            } else {
                File::new(path, meta)
            }
        }
    }

    fn new(path: &Path, meta: fs::Metadata) -> Result<Self, Error> {
        assert!(!meta.is_dir(), "Did not expect directory metadata");

        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW);

        // Check if we are allowed to read the file
        let mut file = options.open(path)?;
        let mut buf = vec![0u8; HEADER_SIZE];
        let header_len = file.read(&mut buf)?;
        let mime_type = infer::get(&buf[0..header_len])
            .map_or("unknown", |t| t.mime_type())
            .to_string();

        Ok(Self {
            path: Hidden(path.into()),
            kind: FileKind::FileToSend {
                meta: Hidden(meta),
                source: FileSource::Path,
                mime_type: Some(Hidden(mime_type)),
            },
        })
    }

    #[cfg(unix)]
    fn new_with_fd(path: &Path, fd: RawFd) -> Result<Self, Error> {
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
                path: Hidden(path.into()),
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

    pub fn name(&self) -> Option<String> {
        Some(String::from(self.path.file_name()?.to_string_lossy()))
    }

    pub fn path(&self) -> &Path {
        self.path.deref()
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
                extension: self
                    .path
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
            FileKind::FileToSend { meta, source, .. } => {
                FileReader::new(*source, meta.0.clone(), &self.path)
            }
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
