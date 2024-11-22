use std::{sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use cdl_k8s_core::k8s_operator::{Operator, TryDefault};
use cdl_openapi::model_storage_binding::{
    ModelStorageBindingCrd, ModelStorageBindingState, ModelStorageBindingStatus,
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

use crate::{
    consts::infer_prometheus_url,
    validator::{
        model::ModelValidator,
        model_storage::ModelStorageValidator,
        model_storage_binding::{ModelStorageBindingValidator, UpdateContext},
    },
};

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
    type Data = ModelStorageBindingCrd;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::cdl_openapi::consts::NAMESPACE;
    const FALLBACK: Duration = Duration::from_secs(10); // 10 seconds
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
                .map(|status| status.state != ModelStorageBindingState::Deleting)
                .unwrap_or(true)
        {
            let status = data.status.as_ref();
            return Self::update_state_or_requeue(
                &namespace,
                &operator.kube,
                &name,
                UpdateContext {
                    deletion_policy: status
                        .map(|status| status.deletion_policy)
                        .unwrap_or(data.spec.deletion_policy),
                    model: status.and_then(|status| status.model.clone()),
                    model_name: status.and_then(|status| status.model_name.clone()),
                    owner_references: None,
                    resources: status
                        .map(|status: &ModelStorageBindingStatus| status.resources.clone())
                        .unwrap_or_default(),
                    state: ModelStorageBindingState::Deleting,
                    storage_source: status
                        .and_then(|status| status.storage_source.as_ref())
                        .cloned(),
                    storage_source_binding_name: status
                        .and_then(|status| status.storage_source_binding_name.clone()),
                    storage_source_name: status
                        .and_then(|status| status.storage_source_name.clone()),
                    storage_source_uid: status.and_then(|status| status.storage_source_uid.clone()),
                    storage_sync_policy: status.and_then(|status| status.storage_sync_policy),
                    storage_target: status.and_then(|status| status.storage_target.clone()),
                    storage_target_name: status
                        .and_then(|status| status.storage_target_name.clone()),
                    storage_target_uid: status.and_then(|status| status.storage_target_uid.clone()),
                },
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

        let kubernetes_storage = KubernetesStorageClient {
            namespace: &namespace,
            kube: &operator.kube,
        };
        let validator = ModelStorageBindingValidator {
            model: ModelValidator { kubernetes_storage },
            model_storage: ModelStorageValidator {
                kubernetes_storage,
                prometheus_url: &operator.ctx.prometheus_url,
            },
            namespace: &namespace,
            name: &name,
        };

        match data
            .status
            .as_ref()
            .map(|status| status.state)
            .unwrap_or_default()
        {
            ModelStorageBindingState::Pending => {
                match validator.validate_model_storage_binding(&data).await {
                    Ok(ctx) => {
                        Self::update_state_or_requeue(&namespace, &operator.kube, &name, ctx).await
                    }
                    Err(e) => {
                        warn!("failed to validate model storage binding: {name:?}: {e}");
                        Ok(Action::requeue(
                            <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                        ))
                    }
                }
            }
            ModelStorageBindingState::Ready => {
                match validator.update(&data, data.status.as_ref().unwrap()).await {
                    Ok(Some(ctx)) => {
                        Self::update_state_or_requeue(&namespace, &operator.kube, &name, ctx).await
                    }
                    Ok(None) => Ok(Action::await_change()),
                    Err(e) => {
                        warn!("failed to update model storage binding: {name:?}: {e}");
                        Ok(Action::requeue(
                            <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                        ))
                    }
                }
            }
            ModelStorageBindingState::Deleting => match validator.delete(&data.spec).await {
                Ok(()) => {
                    <Self as ::cdl_k8s_core::k8s_operator::Ctx>::remove_finalizer_or_requeue_namespaced(
                        operator.kube.clone(),
                        &namespace,
                        &name,
                    )
                    .await
                }
                Err(e) => {
                    warn!("failed to delete model storage binding ({namespace}/{name}): {e}");
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
        ctx: UpdateContext,
    ) -> Result<Action, Error> {
        match Self::update_state(namespace, kube, name, ctx).await {
            Ok(()) => {
                info!("model storage binding is ready: {namespace}/{name}");
                Ok(Action::requeue(
                    <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                ))
            }
            Err(e) => {
                warn!("failed to validate model storage binding ({namespace}/{name}): {e}");
                Ok(Action::requeue(
                    <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                ))
            }
        }
    }

    #[instrument(level = Level::INFO, skip(kube), err(Display))]
    async fn update_state(
        namespace: &str,
        kube: &Client,
        name: &str,
        UpdateContext {
            deletion_policy,
            model,
            model_name,
            owner_references,
            resources,
            state,
            storage_source,
            storage_source_binding_name,
            storage_source_name,
            storage_source_uid,
            storage_sync_policy,
            storage_target,
            storage_target_name,
            storage_target_uid,
        }: UpdateContext,
    ) -> Result<()> {
        let api = Api::<<Self as ::cdl_k8s_core::k8s_operator::Ctx>::Data>::namespaced(
            kube.clone(),
            namespace,
        );
        let crd = <Self as ::cdl_k8s_core::k8s_operator::Ctx>::Data::api_resource();

        {
            let patch = Patch::Merge(json!({
                "apiVersion": crd.api_version,
                "kind": crd.kind,
                "status": ModelStorageBindingStatus {
                    state,
                    deletion_policy,
                    model,
                    model_name,
                    resources,
                    storage_source,
                    storage_source_binding_name,
                    storage_source_name,
                    storage_source_uid,
                    storage_sync_policy,
                    storage_target,
                    storage_target_name,
                    storage_target_uid,
                    last_updated: Utc::now(),
                },
            }));
            let pp = PatchParams::apply(<Self as ::cdl_k8s_core::k8s_operator::Ctx>::NAME);
            api.patch_status(name, &pp, &patch).await?;
        }

        if let Some(owner_references) = owner_references {
            let patch = Patch::Merge(json!({
                "ownerReferences": owner_references,
            }));
            let pp = PatchParams::apply(<Self as ::cdl_k8s_core::k8s_operator::Ctx>::NAME);
            api.patch_metadata(name, &pp, &patch).await?;
        }
        Ok(())
    }
}
