use std::{
    fs, io,
    os::fd::{AsRawFd, OwnedFd},
    sync::LazyLock,
};

use nix::{
    fcntl::{FcntlArg, OFlag, fcntl},
    unistd::pipe2,
};

pub(crate) struct Pipe<Fd = OwnedFd> {
    pub(crate) tx: Fd,
    pub(crate) rx: Fd,
    pub(crate) size: u32,
}

impl Pipe {
    pub(crate) fn open(hint_len: u32) -> io::Result<Self> {
        let size = get_best_size(hint_len);

        let (rx, tx) = pipe2(OFlag::O_CLOEXEC | OFlag::O_DIRECT | OFlag::O_NONBLOCK)?;
        fcntl(rx.as_raw_fd(), FcntlArg::F_SETPIPE_SZ(size))?;
        fcntl(tx.as_raw_fd(), FcntlArg::F_SETPIPE_SZ(size))?;
        Ok(Self {
            tx,
            rx,
            size: size as _,
        })
    }
}

pub(super) fn get_max_size() -> i32 {
    static VALUE: LazyLock<i32> = LazyLock::new(|| {
        fs::read_to_string("/proc/sys/fs/pipe-max-size")
            .expect("invalid /proc/sys/fs/pipe-max-size")
            .trim_end()
            .parse()
            .expect("invalid pipe-max-size")
    });
    *VALUE
}

#[inline]
const fn get_min_size() -> i32 {
    512 // needs 512 bytes alignment for NVMe volumes
}

fn get_best_size(hint_len: u32) -> i32 {
    if hint_len == 0 || hint_len > i32::MAX as u32 {
        return get_max_size();
    }
    let hint_len = hint_len as i32;

    let min = get_min_size();
    if hint_len < min {
        min
    } else {
        let max = get_max_size();
        hint_len.min(max)
    }
}
