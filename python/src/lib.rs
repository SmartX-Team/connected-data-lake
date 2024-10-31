use anyhow::Error;
use cdl_catalog::{Compression, DatasetCatalog, Url};
use cdl_core::wrap_tokio;
use cdl_fs::{CdlFS, FileMetadataRecord, FileRecordPy, FileRecordRefPy, GlobalPath};
use clap::Parser;
use pyo3::{
    pyclass, pymethods, pymodule,
    types::{PyModule, PyModuleMethods},
    Bound, PyResult,
};
use tracing::info;

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
        catalog.init();

        Ok(Self { catalog })
    }

    #[pyo3(signature = (
        /,
        url,
    ))]
    fn open(&self, url: String) -> PyResult<CdlFS> {
        let path: GlobalPath = url.parse()?;
        wrap_tokio(path.open(self.catalog.clone())).map_err(Into::into)
    }
}

#[pymodule]
fn _internal(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Init
    ::ark_core::tracer::init_once();
    info!("Welcome to Connected Data Lake!");

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
    m.add_class::<FileMetadataRecord>()?;
    m.add_class::<FileRecordRefPy>()?;
    m.add_class::<FileRecordPy>()?;
    m.add_class::<Url>()?;

    // Functions

    Ok(())
}
