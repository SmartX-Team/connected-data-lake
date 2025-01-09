#[cfg(feature = "local")]
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, IntoRawFd, OwnedFd, RawFd};

use bitflags::bitflags;

/// A generic node.
///
pub enum Node {
    /// Local node
    #[cfg(feature = "local")]
    Local(LocalNode),
}

/// A local node.
///
#[derive(Clone)]
#[cfg(feature = "local")]
pub struct LocalNode<Fd = OwnedFd> {
    /// A target file descriptor.
    pub fd: Fd,
    /// An offset to be accessed.
    /// Negative values (e.g. -1) annotates that the offset is not given.
    pub offset: i64,
    /// A fixed length.
    /// Negative values (e.g. -1) annotates that the length is not given.
    pub len: i64,
    /// Target file descriptor's flags.
    pub flags: NodeFlags,
}

#[cfg(feature = "local")]
impl<Fd> AsFd for LocalNode<Fd>
where
    Fd: AsFd,
{
    #[inline]
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

#[cfg(feature = "local")]
impl<Fd> AsRawFd for LocalNode<Fd>
where
    Fd: AsRawFd,
{
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

#[cfg(feature = "local")]
impl<Fd> IntoRawFd for LocalNode<Fd>
where
    Fd: IntoRawFd,
{
    #[inline]
    fn into_raw_fd(self) -> RawFd {
        self.fd.into_raw_fd()
    }
}

#[cfg(feature = "local")]
impl<Fd> LocalNode<Fd> {
    #[inline]
    pub fn as_raw(&self) -> LocalNode<RawFd>
    where
        Fd: AsRawFd,
    {
        let Self {
            fd,
            offset,
            len,
            flags,
        } = self;
        LocalNode {
            fd: fd.as_raw_fd(),
            offset: *offset,
            len: *len,
            flags: *flags,
        }
    }

    /// Return true when the node can be read.
    pub const fn is_readable(&self) -> bool {
        self.flags.contains(NodeFlags::MODE_READ)
    }

    /// Return true when the node can be written.
    pub const fn is_writeable(&self) -> bool {
        self.flags.contains(NodeFlags::MODE_WRITE)
    }

    /// Return true when the node is a kind of pipes.
    pub const fn is_piped(&self) -> bool {
        self.flags.contains(NodeFlags::MODE_WRITE)
    }

    /// Return true when the node's size can be truncated.
    pub const fn is_feature_truncate(&self) -> bool {
        self.flags.contains(NodeFlags::FEAT_TRUNCATE)
    }
}

bitflags! {
    /// A node's flags that contains modes and features.
    ///
    #[derive(Copy, Clone,Debug, PartialEq, Eq)]
    pub struct NodeFlags: u8 {
        /// Ensures the node can be read.
        const MODE_READ = 1 << 0;
        /// Ensures the node can be written.
        const MODE_WRITE = 1 << 1;

        /// Ensures the node is a kind of pipes.
        const FEAT_PIPE = 1 << 2;
        /// Ensures the node's size metedata can be accessed on runtime.
        const FEAT_SIZED = 1 << 3;
        /// Ensures the node's size can be truncated.
        const FEAT_TRUNCATE = 1 << 4;
    }
}
