use anyhow::Result;
use async_trait::async_trait;
use cdl_k8s_core::openapi::Capacity;
use cdl_openapi::{
    model::ModelCrd, model_storage::object::ModelStorageObjectSpec,
    model_storage_binding::ModelStorageBindingStorageSpec,
};
use cdl_k8s_provider::{ObjectStorageClient, ObjectStorageSession};
use futures::TryFutureExt;
use kube::Client;
use prometheus_http_query::Client as PrometheusClient;
use tracing::{instrument, Level};

#[async_trait]
impl super::GetCapacity for ModelStorageObjectSpec {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity<'namespace, 'kube>(
        &self,
        kube: &'kube Client,
        namespace: &'namespace str,
        model: &ModelCrd,
        storage_name: &str,
    ) -> Result<Option<Capacity>> {
        let storage = ModelStorageBindingStorageSpec {
            source: None,
            source_binding_name: None,
            target: self,
            target_name: storage_name,
        };

        let client = ObjectStorageClient::try_new(kube, namespace, None, storage, None).await?;
        let session = client.get_session(kube, namespace, model);
        session.get_capacity().map_ok(Some).await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity_global<'namespace, 'kube>(
        &self,
        kube: &'kube Client,
        namespace: &'namespace str,
        storage_name: &str,
    ) -> Result<Option<Capacity>> {
        let storage = ObjectStorageSession::load_storage_provider(
            kube,
            namespace,
            &storage_name,
            None,
            self,
            None,
        )
        .await?;
        storage.get_capacity_global().map_ok(Some).await
    }
}

#[async_trait]
impl super::GetTraffic for ModelStorageObjectSpec {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_traffic<'namespace, 'kube>(
        &self,
        prometheus_client: &'kube PrometheusClient,
        _namespace: &'namespace str,
        _model: &ModelCrd,
        _storage_name: &str,
    ) -> Result<super::TrafficMetrics> {
        const QUERY: &str = "minio_s3_requests_ttfb_seconds_distribution{api=~\"[Gg]et[Oo]bject\"}";

        let (data, _stats) = prometheus_client.query(QUERY).get().await?.into_inner();
        dbg!(data, _stats);
        Ok(super::TrafficMetrics::default())
    }
}
