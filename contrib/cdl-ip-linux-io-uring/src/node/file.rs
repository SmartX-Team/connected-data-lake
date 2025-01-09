use std::{
    fs::File,
    io::{self, Seek, SeekFrom},
    os::{
        fd::{FromRawFd, IntoRawFd, OwnedFd},
        unix::fs::OpenOptionsExt,
    },
    path::{Path, PathBuf},
};

use cdl_ip_core::node::{LocalNode, Node, NodeFlags};
use nix::fcntl::OFlag;

use crate::task::LocalTask;

pub struct ReadFile {
    path: PathBuf,
}

impl ReadFile {
    #[inline]
    pub fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            path: path.as_ref().canonicalize()?,
        })
    }
}

impl TryFrom<ReadFile> for LocalNode {
    type Error = io::Error;

    fn try_from(value: ReadFile) -> Result<Self, Self::Error> {
        let ReadFile { path } = value;
        let mut file = File::options()
            .read(true)
            .write(false)
            .create(false)
            .custom_flags((OFlag::O_ASYNC | OFlag::O_NONBLOCK).bits())
            .open(path)?;

        let old_pos = file.stream_position()?;
        let len = file.seek(SeekFrom::End(0))?;

        // Avoid seeking a third time when we were already at the end of the
        // stream. The branch is usually way cheaper than a seek operation.
        if old_pos != len {
            file.seek(SeekFrom::Start(old_pos))?;
        }

        if len > i64::MAX as u64 {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let len = len as _;

        let fd = unsafe { OwnedFd::from_raw_fd(file.into_raw_fd()) };
        let flags = NodeFlags::MODE_READ;
        Ok(Self {
            fd,
            offset: 0,
            len,
            flags,
        })
    }
}

pub struct WriteFile {
    path: PathBuf,
}

impl WriteFile {
    #[inline]
    pub fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            path: path.as_ref().canonicalize()?,
        })
    }
}

impl TryFrom<WriteFile> for LocalNode {
    type Error = io::Error;

    fn try_from(value: WriteFile) -> Result<Self, Self::Error> {
        let WriteFile { path } = value;
        let file = File::options()
            .read(false)
            .write(true)
            .create(true)
            .truncate(true)
            .custom_flags((OFlag::O_ASYNC | OFlag::O_DIRECT | OFlag::O_NONBLOCK).bits())
            .open(path)?;

        let fd = unsafe { OwnedFd::from_raw_fd(file.into_raw_fd()) };
        let flags = NodeFlags::MODE_WRITE | NodeFlags::FEAT_TRUNCATE;
        Ok(Self {
            fd,
            offset: 0,
            len: -1,
            flags,
        })
    }
}
