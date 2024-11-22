mod kubernetes;
mod object;
pub mod parser;

use anyhow::{bail, Result};
use cdl_openapi::{
    model::ModelCrd,
    model_storage_binding::{ModelStorageBindingStatus, ModelStorageBindingStorageSourceSpec},
};
use kube::{api::ObjectMeta, Client};
use tracing::{instrument, Level};

pub use self::{
    kubernetes::KubernetesStorageClient,
    object::{ObjectStorageClient, ObjectStorageSession},
};

pub mod consts {
    pub const NAME: &str = "cdl-provider";
}

pub struct StorageClient<'namespace, 'kube> {
    pub namespace: &'namespace str,
    pub kube: &'kube Client,
}

impl<'namespace, 'kube> StorageClient<'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn get_model(&self, model_name: &str) -> Result<ModelCrd> {
        let storage = KubernetesStorageClient {
            namespace: self.namespace,
            kube: self.kube,
        };
        storage.load_model(model_name).await
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn get_model_storage_bindings(
        &self,
        model_name: &str,
    ) -> Result<Vec<(ObjectMeta, ModelStorageBindingStatus)>> {
        let storage = KubernetesStorageClient {
            namespace: self.namespace,
            kube: self.kube,
        };

        let storages = storage.load_model_storage_bindings(model_name).await?;
        if storages.is_empty() {
            bail!("model has not been binded: {model_name:?}")
        }
        Ok(storages)
    }
}

pub fn assert_source_is_none<T, R>(source: Option<T>, name: &'static str) -> Result<Option<R>> {
    if source.is_some() {
        bail!("Sync to {name} is not supported")
    } else {
        Ok(None)
    }
}

pub fn assert_source_is_same<'a, T, R>(
    source: Option<ModelStorageBindingStorageSourceSpec<'a, T>>,
    name: &'static str,
    map: impl FnOnce(T) -> Result<R, &'static str>,
) -> Result<Option<ModelStorageBindingStorageSourceSpec<'a, R>>> {
    source
        .map(
            |ModelStorageBindingStorageSourceSpec {
                 name: source_name,
                 storage: source,
                 sync_policy,
             }| match map(source) {
                Ok(source) => Ok(ModelStorageBindingStorageSourceSpec {
                    name: source_name,
                    storage: source,
                    sync_policy,
                }),
                Err(source) => {
                    bail!("Sync to {name} from other source ({source}) is not supported")
                }
            },
        )
        .transpose()
}
