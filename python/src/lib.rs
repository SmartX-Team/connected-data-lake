use std::{future::Future, sync::OnceLock};

use anyhow::{Context, Error, Result};
use cdl_catalog::{Compression, DatasetCatalog, Url};
use cdl_fs::{register_handlers, GlobalPath};
use clap::Parser;
use deltalake::{
    arrow::{array::RecordBatch, compute::concat_batches, pyarrow::PyArrowType},
    datafusion::execution::SendableRecordBatchStream,
};
use futures::{TryFutureExt, TryStreamExt};
use pyo3::{pyclass, pymethods, pymodule, types::PyModule, Bound, PyResult};
use tokio::runtime::Runtime;
use tracing::debug;

#[pyclass]
pub struct Cdl {
    catalog: DatasetCatalog,
}

#[pymethods]
impl Cdl {
    #[new]
    #[pyo3(signature = (
        /,
        catalog = None,
    ))]
    fn new(catalog: Option<DatasetCatalog>) -> PyResult<Self> {
        let catalog = match catalog {
            Some(catalog) => catalog,
            None => DatasetCatalog::try_parse_from::<[_; 0], &str>([]).map_err(Error::from)?,
        };
        register_handlers();

        Ok(Self { catalog })
    }

    #[pyo3(signature = (
        url,
        /,
    ))]
    fn open(&self, url: String) -> PyResult<CdlFS> {
        let path: GlobalPath = url.parse()?;
        wrap_tokio(path.open(self.catalog.clone()))
            .map(CdlFS)
            .map_err(Into::into)
    }
}

#[pyclass]
#[repr(transparent)]
pub struct CdlFS(::cdl_fs::CdlFS);

#[pymethods]
impl CdlFS {
    #[pyo3(signature = (
        dst,
        /,
    ))]
    fn copy_to(&self, dst: String) -> PyResult<()> {
        let dst: GlobalPath = dst.parse()?;
        wrap_tokio(self.0.copy_to(&dst)).map_err(Into::into)
    }

    #[pyo3(signature = (
        path = "/",
        /,
    ))]
    fn read_dir(&self, path: &str) -> PyResult<PyArrowType<RecordBatch>> {
        wrap_tokio(
            self.0
                .read_dir(path)
                .and_then(|stream| collect_batches(stream, "directory data")),
        )
        .map_err(Into::into)
    }

    #[pyo3(signature = (
        /,
    ))]
    fn read_dir_all(&self) -> PyResult<PyArrowType<RecordBatch>> {
        wrap_tokio(
            self.0
                .read_dir_all()
                .and_then(|stream| collect_batches(stream, "all directory data")),
        )
        .map_err(Into::into)
    }

    #[pyo3(signature = (
        files,
        /,
    ))]
    fn read_files(&self, files: PyArrowType<RecordBatch>) -> PyResult<Vec<Vec<u8>>> {
        wrap_tokio(
            self.0
                .read_files(&files.0)
                .and_then(|stream| stream.map_ok(|record| record.data).try_collect()),
        )
        .map_err(Into::into)
    }

    #[pyo3(signature = (
        sql,
        /,
    ))]
    fn sql(&self, sql: &str) -> PyResult<PyArrowType<RecordBatch>> {
        wrap_tokio(
            self.0
                .query(sql)
                .and_then(|df| df.execute_stream().map_err(Into::into))
                .and_then(|stream| collect_batches(stream, "sql results")),
        )
        .map_err(Into::into)
    }
}

async fn collect_batches(
    stream: SendableRecordBatchStream,
    kind: &'static str,
) -> Result<PyArrowType<RecordBatch>> {
    let schema = stream.schema();
    let input_batches: Vec<_> = stream
        .try_collect()
        .await
        .with_context(|| format!("Failed to collect {kind}"))?;
    let batch = concat_batches(&schema, &input_batches)
        .with_context(|| format!("Failed to concat {kind}"))?;
    Ok(PyArrowType(batch))
}

fn wrap_tokio<F>(future: F) -> F::Output
where
    F: Future,
{
    static RT: OnceLock<Runtime> = OnceLock::new();
    let rt = RT.get_or_init(|| Runtime::new().unwrap());
    rt.block_on(future)
}

#[pymodule]
fn _internal(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Init
    ::ark_core::tracer::init_once();
    debug!("Welcome to Connected Data Lake!");

    // Metadata
    m.add("__author__", env!("CARGO_PKG_AUTHORS"))?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    // Types
    // let py = m.py();
    // m.add("DatasetCatalog", py.get_type_bound::<DatasetCatalog>())?;

    // Classes
    m.add_class::<Cdl>()?;
    m.add_class::<CdlFS>()?;
    m.add_class::<Compression>()?;
    m.add_class::<DatasetCatalog>()?;
    m.add_class::<Url>()?;

    // Functions

    Ok(())
}
