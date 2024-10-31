use std::future::Future;

pub fn wrap_tokio<F>(future: F) -> F::Output
where
    F: Future,
{
    #[cfg(feature = "pyo3")]
    {
        use std::sync::OnceLock;

        use tokio::runtime::Runtime;

        static RT: OnceLock<Runtime> = OnceLock::new();
        let rt = RT.get_or_init(|| Runtime::new().unwrap());
        rt.block_on(future)
    }

    #[cfg(not(feature = "pyo3"))]
    {
        use tokio::runtime::Handle;

        let handle = Handle::current();
        handle.block_on(future)
    }
}
