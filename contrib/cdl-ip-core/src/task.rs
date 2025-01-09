use std::{io, thread::spawn};

use crate::node::LocalNode;

/// A generic task builder.
///
pub trait TaskBuilder {
    /// An error type.
    type Error;

    /// A target task.
    type Task: Task<Error = Self::Error>;

    /// Create a new task starting with the given node.
    #[must_use]
    fn open(&self, sink: LocalNode) -> Self::Task;
}

/// A generic task trait.
///
pub trait Task {
    /// An error type.
    type Error;

    /// A target node.
    type Node;

    /// Register a typed node into the task.
    #[must_use]
    fn map(&mut self, sink: Self::Node, src: Self::Node) -> &mut Self;

    /// Complete building a task.
    /// Note that this task cannot be reused.
    #[must_use]
    fn finish(
        &mut self,
        src: Self::Node,
    ) -> Result<TaskContext<Self::Node, Self::Error>, Self::Error>;
}

/// An additional task trait that derives automatically.
///
pub trait TaskExt: Task {
    /// Register a node into the task.
    #[inline]
    fn push<Tx, Rx>(&mut self, sink: Tx, src: Rx) -> Result<(), Self::Error>
    where
        Tx: TryInto<Self::Node>,
        Rx: TryInto<Self::Node>,
        Self::Error: From<Tx::Error> + From<Rx::Error>,
    {
        let _ = self.map(sink.try_into()?, src.try_into()?);
        Ok(())
    }

    /// Spawn the task into a new thread and return the last node.
    #[inline]
    fn to_node(&mut self, src: Self::Node) -> Result<Self::Node, Self::Error>
    where
        Self::Error: 'static + Send + From<io::Error>,
    {
        self.finish(src)?.into_node()
    }

    /// Spawn the task into the current thread and wait until finished.
    #[inline]
    fn wait(&mut self, src: Self::Node) -> Result<(), Self::Error>
    where
        Self::Error: 'static + Send,
    {
        self.finish(src)?.wait()
    }
}

impl<T> TaskExt for T where Self: Task {}

/// A lazy task that contains actual task information.
///
pub struct TaskContext<N, E> {
    bound: TaskBound<E>,
    node: N,
}

impl<N, E> TaskContext<N, E>
where
    E: 'static + Send,
{
    /// Create a new task context.
    pub fn new(bound: TaskBound<E>, node: N) -> Self {
        Self { bound, node }
    }

    /// Spawn the task into a new thread and return the last node.
    pub fn into_node(self) -> Result<N, E>
    where
        E: From<io::Error>,
    {
        let Self { bound, node } = self;
        match bound {
            #[cfg(feature = "local")]
            TaskBound::LocalThread(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "the task is not thread-safe",
                )
                .into());
            }
            #[cfg(feature = "local")]
            TaskBound::LocalProcess(f) => {
                spawn(f);
                Ok(node)
            }
        }
    }

    /// Spawn the task into the current thread and wait until finished.
    #[cfg(feature = "local")]
    pub fn wait(self) -> Result<(), E> {
        let Self { bound, node } = self;
        let result = match bound {
            #[cfg(feature = "local")]
            TaskBound::LocalThread(f) => f(),
            #[cfg(feature = "local")]
            TaskBound::LocalProcess(f) => f(),
        };
        drop(node);
        result
    }
}

/// A generic task context.
///
pub enum TaskBound<E> {
    /// A thread-unsafe local task.
    #[cfg(feature = "local")]
    LocalThread(Box<dyn FnOnce() -> Result<(), E>>),
    /// A thread-safe local task.
    #[cfg(feature = "local")]
    LocalProcess(Box<dyn Send + FnOnce() -> Result<(), E>>),
}
