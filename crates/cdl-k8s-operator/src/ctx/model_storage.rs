use std::{sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use cdl_k8s_core::k8s_operator::{Operator, TryDefault};
use cdl_openapi::model_storage::{
    ModelStorageCrd, ModelStorageKindSpec, ModelStorageState, ModelStorageStatus,
};
use cdl_k8s_provider::KubernetesStorageClient;
use chrono::Utc;
use kube::{
    api::{Patch, PatchParams},
    runtime::controller::Action,
    Api, Client, CustomResourceExt, Error, ResourceExt,
};
use serde_json::json;
use tracing::{info, instrument, warn, Level};

use crate::{consts::infer_prometheus_url, validator::model_storage::ModelStorageValidator};

pub struct Ctx {
    prometheus_url: String,
}

#[async_trait]
impl TryDefault for Ctx {
    async fn try_default() -> Result<Self> {
        Ok(Self {
            prometheus_url: infer_prometheus_url(),
        })
    }
}

#[async_trait]
impl ::cdl_k8s_core::k8s_operator::Ctx for Ctx {
    type Data = ModelStorageCrd;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::cdl_openapi::consts::NAMESPACE;
    const FALLBACK: Duration = Duration::from_secs(30); // 30 seconds
    const FINALIZER_NAME: &'static str =
        <Self as ::cdl_k8s_core::k8s_operator::Ctx>::Data::FINALIZER_NAME;

    #[instrument(level = Level::INFO, skip_all, fields(name = %data.name_any(), namespace = data.namespace()), err(Display))]
    async fn reconcile(
        operator: Arc<Operator<Self>>,
        data: Arc<<Self as ::cdl_k8s_core::k8s_operator::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let name = data.name_any();
        let namespace = data.namespace().unwrap();

        if data.metadata.deletion_timestamp.is_some()
            && data
                .status
                .as_ref()
                .map(|status| status.state != ModelStorageState::Deleting)
                .unwrap_or(true)
        {
            let status = data.status.as_ref();
            return Self::update_state_or_requeue(
                &namespace,
                &operator.kube,
                &name,
                status.and_then(|status| status.kind.clone()),
                ModelStorageState::Deleting,
                None,
            )
            .await;
        } else if !data.finalizers().iter().any(|finalizer| {
            finalizer == <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FINALIZER_NAME
        }) {
            return <Self as ::cdl_k8s_core::k8s_operator::Ctx>::add_finalizer_or_requeue_namespaced(
                operator.kube.clone(),
                &namespace,
                &name,
            )
            .await;
        }

        let validator = ModelStorageValidator {
            kubernetes_storage: KubernetesStorageClient {
                namespace: &namespace,
                kube: &operator.kube,
            },
            prometheus_url: &operator.ctx.prometheus_url,
        };

        match data
            .status
            .as_ref()
            .map(|status| status.state)
            .unwrap_or_default()
        {
            ModelStorageState::Pending => {
                match validator
                    .validate_model_storage(&name, &data.metadata, &data.spec)
                    .await
                {
                    Ok(total_quota) => {
                        Self::update_state_or_requeue(
                            &namespace,
                            &operator.kube,
                            &name,
                            Some(data.spec.kind.clone()),
                            ModelStorageState::Ready,
                            total_quota,
                        )
                        .await
                    }
                    Err(e) => {
                        warn!("failed to validate model storage: {name:?}: {e}");
                        Ok(Action::requeue(
                            <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                        ))
                    }
                }
            }
            ModelStorageState::Ready => {
                // TODO: implement to finding changes
                Ok(Action::await_change())
            }
            ModelStorageState::Deleting => match validator.delete(&data).await {
                Ok(()) => {
                    <Self as ::cdl_k8s_core::k8s_operator::Ctx>::remove_finalizer_or_requeue_namespaced(
                        operator.kube.clone(),
                        &namespace,
                        &name,
                    )
                    .await
                }
                Err(e) => {
                    warn!("failed to delete model storage ({namespace}/{name}): {e}");
                    Ok(Action::requeue(
                        <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                    ))
                }
            },
        }
    }
}

impl Ctx {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn update_state_or_requeue(
        namespace: &str,
        kube: &Client,
        name: &str,
        kind: Option<ModelStorageKindSpec>,
        state: ModelStorageState,
        total_quota: Option<u128>,
    ) -> Result<Action, Error> {
        match Self::update_state(namespace, kube, name, kind, state, total_quota).await {
            Ok(()) => {
                info!("model storage is ready: {namespace}/{name}");
                Ok(Action::requeue(
                    <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                ))
            }
            Err(e) => {
                warn!("failed to update model storage state ({namespace}/{name}): {e}");
                Ok(Action::requeue(
                    <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                ))
            }
        }
    }

    #[instrument(level = Level::INFO, skip(kube, kind), err(Display))]
    async fn update_state(
        namespace: &str,
        kube: &Client,
        name: &str,
        kind: Option<ModelStorageKindSpec>,
        state: ModelStorageState,
        total_quota: Option<u128>,
    ) -> Result<()> {
        let api = Api::<<Self as ::cdl_k8s_core::k8s_operator::Ctx>::Data>::namespaced(
            kube.clone(),
            namespace,
        );
        let crd = <Self as ::cdl_k8s_core::k8s_operator::Ctx>::Data::api_resource();

        let patch = Patch::Merge(json!({
            "apiVersion": crd.api_version,
            "kind": crd.kind,
            "status": ModelStorageStatus {
                state,
                kind,
                last_updated: Utc::now(),
                total_quota,
            },
        }));
        let pp = PatchParams::apply(<Self as ::cdl_k8s_core::k8s_operator::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;
        Ok(())
    }
}
