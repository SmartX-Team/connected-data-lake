use anyhow::{anyhow, bail, Result};
use cdl_k8s_provider::{assert_source_is_same, KubernetesStorageClient, ObjectStorageClient};
use cdl_openapi::{
    model::ModelCrd,
    model_storage::{
        object::ModelStorageObjectSpec, ModelStorageCrd, ModelStorageKind, ModelStorageKindSpec,
        ModelStorageSpec, StorageResourceRequirements,
    },
    model_storage_binding::{
        ModelStorageBindingCrd, ModelStorageBindingDeletionPolicy, ModelStorageBindingStorageSpec,
    },
};
use futures::TryFutureExt;
use itertools::Itertools;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use kube::{api::ObjectMeta, Resource, ResourceExt};
use tracing::{instrument, Level};

pub struct ModelStorageValidator<'namespace, 'kube> {
    pub kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
    pub prometheus_url: &'kube str,
}

impl<'namespace, 'kube> ModelStorageValidator<'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn validate_model_storage(
        &self,
        name: &str,
        metadata: &ObjectMeta,
        spec: &ModelStorageSpec,
    ) -> Result<Option<u128>> {
        if spec.kind.is_unique() {
            self.validate_model_storage_conflict(name, spec.kind.to_kind())
                .await?;
        }

        match &spec.kind {
            ModelStorageKindSpec::ObjectStorage(spec) => {
                self.validate_model_storage_object(name, metadata, spec)
                    .await
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn validate_model_storage_conflict(
        &self,
        name: &str,
        kind: ModelStorageKind,
    ) -> Result<()> {
        let conflicted = self
            .kubernetes_storage
            .load_model_storages_by(|k| k.is_unique() && kind == k.to_kind())
            .await?;

        if conflicted.is_empty() {
            Ok(())
        } else {
            bail!(
                "model storage already exists ({name} => {kind}): {list:?}",
                list = conflicted.into_iter().map(|item| item.name_any()).join(","),
            )
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn validate_model_storage_object(
        &self,
        name: &str,
        metadata: &ObjectMeta,
        storage: &ModelStorageObjectSpec,
    ) -> Result<Option<u128>> {
        let storage = ModelStorageBindingStorageSpec {
            source: None,
            source_binding_name: None,
            target: storage,
            target_name: name,
        };
        ObjectStorageClient::try_new(
            self.kubernetes_storage.kube,
            self.kubernetes_storage.namespace,
            Some(metadata),
            storage,
            Some(self.prometheus_url),
        )
        .and_then(|client| async move {
            client
                .target()
                .get_capacity_global()
                .await
                .map(|capacity| Some(capacity.capacity.as_u128()))
        })
        .await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub(crate) async fn bind_model(
        &self,
        binding: &ModelStorageBindingCrd,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageSpec>,
        model: &ModelCrd,
    ) -> Result<()> {
        match &storage.target.kind {
            ModelStorageKindSpec::ObjectStorage(spec) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_same(storage.source, "ObjectStorage", |source| {
                        match &source.kind {
                            ModelStorageKindSpec::ObjectStorage(source) => Ok(source),
                        }
                    })?,
                    source_binding_name: storage.source_binding_name,
                    target: spec,
                    target_name: storage.target_name,
                };
                self.bind_model_to_object_storage(binding, storage, model)
                    .await
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn bind_model_to_object_storage(
        &self,
        binding: &ModelStorageBindingCrd,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageObjectSpec>,
        model: &ModelCrd,
    ) -> Result<()> {
        let KubernetesStorageClient { kube, namespace } = self.kubernetes_storage;

        let owner_references = {
            let name = binding.name_any();
            let uid = binding
                .uid()
                .ok_or_else(|| anyhow!("failed to get model storage binding uid: {name}"))?;

            vec![OwnerReference {
                api_version: ModelStorageBindingCrd::api_version(&()).into(),
                block_owner_deletion: Some(true),
                controller: None,
                kind: ModelStorageBindingCrd::kind(&()).into(),
                name,
                uid,
            }]
        };
        let quota = binding.spec.resources.quota();

        ObjectStorageClient::try_new(kube, namespace, None, storage, Some(self.prometheus_url))
            .await?
            .get_session(kube, namespace, model)
            .create_bucket(owner_references, quota)
            .await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub(crate) async fn unbind_model(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageSpec>,
        model: &ModelCrd,
        deletion_policy: ModelStorageBindingDeletionPolicy,
    ) -> Result<()> {
        match &storage.target.kind {
            ModelStorageKindSpec::ObjectStorage(spec) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_same(storage.source, "ObjectStorage", |source| {
                        match &source.kind {
                            ModelStorageKindSpec::ObjectStorage(source) => Ok(source),
                        }
                    })?,
                    source_binding_name: storage.source_binding_name,
                    target: spec,
                    target_name: storage.target_name,
                };
                self.unbind_model_to_object_storage(storage, model, deletion_policy)
                    .await
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn unbind_model_to_object_storage(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageObjectSpec>,
        model: &ModelCrd,
        deletion_policy: ModelStorageBindingDeletionPolicy,
    ) -> Result<()> {
        let KubernetesStorageClient { kube, namespace } = self.kubernetes_storage;

        let client =
            ObjectStorageClient::try_new(kube, namespace, None, storage, Some(self.prometheus_url))
                .await?;
        let session = client.get_session(kube, namespace, model);
        match deletion_policy {
            ModelStorageBindingDeletionPolicy::Delete => session.delete_bucket().await,
            ModelStorageBindingDeletionPolicy::Retain => {
                session.unsync_bucket(None, false).await.map(|_| ())
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn delete(&self, crd: &ModelStorageCrd) -> Result<()> {
        let bindings = self
            .kubernetes_storage
            .load_model_storage_bindings_by_storage(&crd.name_any())
            .await?;

        if bindings.is_empty() {
            Ok(())
        } else {
            bail!("storage is binded")
        }
    }
}
