#[cfg(unix)]
use std::os::unix::prelude::*;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use drop_config::DropConfig;

use crate::FileToSend;

pub enum GatherSrc {
    Path(PathBuf),
    #[cfg(unix)]
    ContentUri {
        uri: url::Url,
        subpath: String,
        fd: Option<RawFd>,
    },
}

pub struct GatherCtx<'a> {
    config: &'a DropConfig,
    #[cfg(unix)]
    fdresolv: Option<&'a super::FdResolver>,
    files: Vec<FileToSend>,
    used_names: HashSet<PathBuf>,
}

impl<'a> GatherCtx<'a> {
    pub fn new(config: &'a DropConfig) -> Self {
        Self {
            config,
            fdresolv: None,
            files: Vec::new(),
            used_names: HashSet::new(),
        }
    }

    pub fn with_fd_resover(&mut self, fdresolv: &'a super::FdResolver) -> &mut Self {
        self.fdresolv = Some(fdresolv);
        self
    }

    pub fn take(&mut self) -> Vec<FileToSend> {
        self.used_names.clear();
        std::mem::take(&mut self.files)
    }

    fn fetch_free_name(&mut self, path: &Path) -> crate::Result<PathBuf> {
        let file_name = path
            .file_name()
            .ok_or_else(|| crate::Error::BadPath("Missing file name".into()))?;

        let name = crate::utils::filepath_variants(Path::new(file_name))?
            .find(|name| !self.used_names.contains(name))
            .expect("The filename variants interator is unbounded");

        self.used_names.insert(name.clone());
        Ok(name)
    }

    pub fn gather_from_path(&mut self, path: impl AsRef<Path>) -> crate::Result<&mut Self> {
        let path = path.as_ref();
        let name = self.fetch_free_name(path)?;

        let batch = super::FileToSend::from_path(path, &name, self.config)?;
        self.files.extend(batch);
        Ok(self)
    }

    #[cfg(unix)]
    pub fn gather_from_content_uri(
        &mut self,
        path: impl AsRef<Path>,
        uri: url::Url,
        fd: Option<RawFd>,
    ) -> crate::Result<&mut Self> {
        use super::FileSubPath;

        let path = path.as_ref();

        let fd = if let Some(fd) = fd {
            fd
        } else {
            let fdresolv = if let Some(fdresolv) = self.fdresolv.as_ref() {
                fdresolv
            } else {
                return Err(crate::Error::BadTransferState(
                    "Content URI provided but RD resolver callback is not set up".into(),
                ));
            };

            if let Some(fd) = fdresolv(uri.as_str()) {
                fd
            } else {
                return Err(crate::Error::BadTransferState(format!(
                    "Failed to fetch FD for file: {uri}"
                )));
            }
        };

        let name = self.fetch_free_name(path)?;

        let file = FileToSend::from_fd(
            path,
            FileSubPath::from_path(name)?,
            uri,
            fd,
            self.files.len(),
        )?;

        self.files.push(file);
        Ok(self)
    }
}
