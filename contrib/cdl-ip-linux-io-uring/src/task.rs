use std::{
    io,
    os::fd::{AsFd, AsRawFd},
};

use cdl_ip_core::{
    node::{LocalNode, NodeFlags},
    task::{Task, TaskBound, TaskBuilder, TaskContext},
};
use io_uring::{IoUring, opcode, squeue::Flags, types};
use nix::{fcntl::SpliceFFlags, unistd::ftruncate};

use crate::pipe::{self, Pipe};

#[derive(Clone, Debug)]
#[must_use]
pub struct LocalTaskBuilder {
    num_entries_per_pair: u32,
}

impl Default for LocalTaskBuilder {
    fn default() -> Self {
        Self {
            num_entries_per_pair: 8,
        }
    }
}

impl TaskBuilder for LocalTaskBuilder {
    type Error = io::Error;
    type Task = LocalTask;

    fn open(&self, sink: LocalNode) -> Self::Task {
        LocalTask {
            builder: self.clone(),
            nodes: Some(vec![sink]),
        }
    }
}

/// A local worker running on the io-uring runtime.
///
pub struct LocalTask {
    builder: LocalTaskBuilder,
    nodes: Option<Vec<LocalNode>>,
}

impl Task for LocalTask {
    type Error = io::Error;
    type Node = LocalNode;

    fn map(&mut self, sink: Self::Node, src: Self::Node) -> &mut Self {
        if let Some(nodes) = self.nodes.as_mut() {
            nodes.push(sink);
            nodes.push(src);
        }
        self
    }

    fn finish(
        &mut self,
        src: Self::Node,
    ) -> Result<TaskContext<Self::Node, Self::Error>, Self::Error> {
        let Self {
            builder: LocalTaskBuilder {
                num_entries_per_pair,
            },
            nodes,
        } = self;

        let mut nodes = match nodes.take() {
            Some(nodes) => nodes,
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "already consumed",
                ));
            }
        };
        nodes.push(src);

        // FIXME: check `len` (-1: piped)
        let len = nodes[0].len;

        let mut pairs = Vec::with_capacity(nodes.len() - 1);
        let mut index_sink = None;
        for index_src in 0..nodes.len() {
            let index_sink = match index_sink.take() {
                Some(index_sink) => index_sink,
                None => {
                    index_sink = Some(index_src);
                    continue;
                }
            };
            let sink = &nodes[index_sink];
            let src = &nodes[index_src];
            let packet_size = pipe::get_max_size() as _;

            if !sink.is_readable() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "sink node is not readable",
                ));
            }
            if !src.is_writeable() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "src node is not writeable",
                ));
            }

            // Truncate space
            if len >= 0 && src.is_feature_truncate() {
                ftruncate(src.as_fd(), len)?;
            }

            // Interpolate pipe2
            if !sink.is_piped() && !sink.is_piped() {
                let Pipe { tx, rx, size } = pipe2(sink, len)?;
                pairs.push(Pipe {
                    tx: types::Fd(sink.as_raw_fd()),
                    rx: types::Fd(tx.as_raw_fd()),
                    size: packet_size,
                });
                pairs.push(Pipe {
                    tx: types::Fd(rx.as_raw_fd()),
                    rx: types::Fd(src.as_raw_fd()),
                    size,
                });
                nodes.push(tx);
                nodes.push(rx);
            } else {
                pairs.push(Pipe {
                    tx: types::Fd(sink.as_raw_fd()),
                    rx: types::Fd(src.as_raw_fd()),
                    size: packet_size,
                })
            }
        }
        // assert: pairs.len() <= u32::MAX
        let num_pairs = pairs.len();
        let num_pairs_u32 = match u32::try_from(num_pairs) {
            Ok(len) => len,
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "too many pairs",
                ));
            }
        };
        let last_pair = (num_pairs_u32 - 1) as _;

        let node = nodes.pop().unwrap();
        let packet_size = pairs.iter().map(|p| p.size).min().unwrap();
        let packet_size_i64 = packet_size as i64;

        let iter = if len >= 0 {
            BufIter::Sized {
                i: 0,
                len,
                step: packet_size_i64,
            }
        } else {
            BufIter::Infinity { step: packet_size }
        };

        let entry_flags = Flags::ASYNC | Flags::IO_LINK;
        let splice_flags = SpliceFFlags::SPLICE_F_NONBLOCK | SpliceFFlags::SPLICE_F_MORE;

        let entries = match (*num_entries_per_pair).checked_mul(pairs.len() as u32) {
            Some(entries) => entries,
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "too large parameter: num_entries_per_pair",
                ));
            }
        };
        let mut ring: IoUring = IoUring::builder()
            .dontfork()
            .setup_coop_taskrun()
            // .setup_iopoll()
            // .setup_sqpoll(100)
            // .setup_sqpoll_cpu(12)
            .build(entries)?;

        let mut submitted_bytes = 0;
        let mut completed_bytes = 0;
        let f = move || {
            let (submitter, mut submission, mut completion) = ring.split();
            for len in iter {
                for (
                    pipe_index,
                    &Pipe {
                        tx: fd_in,
                        rx: fd_out,
                        size: _,
                    },
                ) in pairs.iter().enumerate()
                {
                    let pipe_index = pipe_index as _;
                    let entry = opcode::Splice::new(fd_in, -1, fd_out, -1, len)
                        .flags(splice_flags.bits())
                        .build()
                        .flags(entry_flags)
                        .user_data(pipe_index);
                    unsafe { submission.push(&entry).unwrap() };
                }

                if submission.is_full() {
                    submitter.submit_and_wait(submission.len() - num_pairs)?;
                }
                submission.sync();
                submitted_bytes += len as i64;

                completion.sync();
                while let Some(c) = completion.next() {
                    let result = cvt(c.result())?;
                    if c.user_data() == last_pair {
                        completed_bytes += result as i64;
                    }
                }
            }

            submitter.submit()?;
            #[cfg(feature = "tracing")]
            ::tracing::debug!("Submitted {submitted_bytes} bytes");

            while completed_bytes < submitted_bytes {
                completion.sync();
                while let Some(c) = completion.next() {
                    let result = cvt(c.result())?;
                    if c.user_data() == last_pair {
                        completed_bytes += result as i64;
                    }
                }
            }

            #[cfg(feature = "tracing")]
            ::tracing::debug!("Completed {completed_bytes} bytes");
            drop(nodes); // Release all fds
            Ok(())

            // FIXME: handle Close instead of OwnedFd: tx, rx, in, out
            // {
            //     let fd_out = types::Fd(d_out.into_raw_fd());
            //     let entry_sync = opcode::Fsync::new(fd_out)
            //         .build()
            //         .flags(Flags::IO_LINK)
            //         .user_data(2);
            //     unsafe { submission.push(&entry_sync).unwrap() };

            //     let entry_close = opcode::Close::new(fd_out)
            //         .build()
            //         .flags(Flags::IO_DRAIN)
            //         .user_data(3);
            //     unsafe { submission.push(&entry_close).unwrap() };
            // }
            // dbg!(drop(submission));
            // dbg!(drop(submitter));
        };

        let bound = TaskBound::LocalProcess(Box::new(f));
        Ok(TaskContext::new(bound, node))
    }
}

enum BufIter {
    Infinity { step: u32 },
    Sized { i: i64, len: i64, step: i64 },
}

impl Iterator for BufIter {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Infinity { step } => Some(*step),
            Self::Sized { i, len, step } => {
                let step = *step;
                let offset = *i * step;
                let len = (*len - offset).min(step);
                *i += 1;
                if len > 0 { Some(len as _) } else { None }
            }
        }
    }
}

#[inline]
fn cvt(result: i32) -> io::Result<i32> {
    if result >= 0 {
        Ok(result)
    } else {
        Err(io::Error::from_raw_os_error(-result))
    }
}

#[inline]
fn pipe2(sink: &LocalNode, len: i64) -> io::Result<Pipe<LocalNode>> {
    let Pipe { tx, rx, size } = {
        let hint_len = if len < 0 {
            0
        } else if len >= u32::MAX as i64 {
            u32::MAX
        } else {
            len as _
        };
        Pipe::open(hint_len)?
    };

    let tx = LocalNode {
        fd: tx,
        offset: -1,
        len: -1,
        flags: NodeFlags::MODE_WRITE | NodeFlags::FEAT_PIPE,
    };
    let rx = LocalNode {
        fd: rx,
        offset: -1,
        len: -1,
        flags: NodeFlags::MODE_READ | NodeFlags::FEAT_PIPE,
    };

    Ok(Pipe { tx, rx, size })
}
