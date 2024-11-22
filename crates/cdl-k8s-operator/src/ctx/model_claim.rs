use std::{sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use cdl_k8s_core::k8s_operator::{Operator, TryDefault};
use cdl_openapi::model_claim::{ModelClaimCrd, ModelClaimState, ModelClaimStatus};
use cdl_k8s_provider::KubernetesStorageClient;
use chrono::Utc;
use kube::{
    api::{Patch, PatchParams},
    runtime::controller::Action,
    Api, Client, CustomResourceExt, Error, ResourceExt,
};
use prometheus_http_query::Client as PrometheusClient;
use serde_json::json;
use tracing::{info, instrument, warn, Level};

use crate::{
    consts::infer_prometheus_url,
    validator::model_claim::{ModelClaimValidator, UpdateContext},
};

pub struct Ctx {
    prometheus_client: PrometheusClient,
    prometheus_url: String,
}

#[async_trait]
impl TryDefault for Ctx {
    async fn try_default() -> Result<Self> {
        let prometheus_url = infer_prometheus_url();
        Ok(Self {
            prometheus_client: prometheus_url.parse()?,
            prometheus_url,
        })
    }
}

#[async_trait]
impl ::cdl_k8s_core::k8s_operator::Ctx for Ctx {
    type Data = ModelClaimCrd;

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
                .map(|status| status.state != ModelClaimState::Deleting)
                .unwrap_or(true)
        {
            let status = data.status.as_ref();
            let ctx = UpdateContext {
                owner_references: None,
                resources: status.and_then(|status| status.resources.clone()),
                state: ModelClaimState::Deleting,
                storage: status.and_then(|status| status.storage),
                storage_name: status.and_then(|status| status.storage_name.clone()),
            };
            return Self::update_fields_or_requeue(&namespace, &operator.kube, &name, ctx).await;
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

        let validator = ModelClaimValidator {
            kubernetes_storage: KubernetesStorageClient {
                namespace: &namespace,
                kube: &operator.kube,
            },
            prometheus_client: &operator.ctx.prometheus_client,
            prometheus_url: &operator.ctx.prometheus_url,
        };

        match data
            .status
            .as_ref()
            .map(|status| status.state)
            .unwrap_or_default()
        {
            ModelClaimState::Pending => match validator
                .validate_model_claim(<Self as ::cdl_k8s_core::k8s_operator::Ctx>::NAME, &data)
                .await
            {
                Ok(ctx) => {
                    Self::update_fields_or_requeue(&namespace, &operator.kube, &name, ctx).await
                }
                Err(e) => {
                    warn!("failed to validate model claim: {name:?}: {e}");
                    Ok(Action::requeue(
                        <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                    ))
                }
            },
            ModelClaimState::Ready => {
                match validator
                    .update(
                        <Self as ::cdl_k8s_core::k8s_operator::Ctx>::NAME,
                        &data,
                        data.status.as_ref().unwrap(),
                    )
                    .await
                {
                    Ok(Some(ctx)) => {
                        Self::update_fields_or_requeue(&namespace, &operator.kube, &name, ctx).await
                    }
                    Ok(None) => Ok(Action::await_change()),
                    Err(e) => {
                        warn!("failed to update model claim: {name:?}: {e}");
                        Ok(Action::requeue(
                            <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                        ))
                    }
                }
            }
            ModelClaimState::Replacing => {
                match validator
                    .validate_model_claim_replacement(
                        <Self as ::cdl_k8s_core::k8s_operator::Ctx>::NAME,
                        &data,
                        data.status.as_ref().unwrap(),
                    )
                    .await
                {
                    Ok(Some(ctx)) => {
                        Self::update_fields_or_requeue(&namespace, &operator.kube, &name, ctx).await
                    }
                    Ok(None) => Ok(Action::requeue(
                        <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                    )),
                    Err(e) => {
                        warn!("failed to replace model claim storage: {name:?}: {e}");
                        Ok(Action::requeue(
                            <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                        ))
                    }
                }
            }
            ModelClaimState::Deleting => match validator.delete(&data).await {
                Ok(()) => {
                    <Self as ::cdl_k8s_core::k8s_operator::Ctx>::remove_finalizer_or_requeue_namespaced(
                        operator.kube.clone(),
                        &namespace,
                        &name,
                    )
                    .await
                }
                Err(e) => {
                    warn!("failed to delete model claim ({namespace}/{name}): {e}");
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
    async fn update_fields_or_requeue(
        namespace: &str,
        kube: &Client,
        name: &str,
        ctx: UpdateContext,
    ) -> Result<Action, Error> {
        let state = ctx.state;
        match Self::update_fields(namespace, kube, name, ctx).await {
            Ok(()) => {
                info!("model claim is {state}: {namespace}/{name}");
                Ok(Action::requeue(
                    <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                ))
            }
            Err(e) => {
                warn!("failed to validate model claim ({namespace}/{name}): {e}");
                Ok(Action::requeue(
                    <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                ))
            }
        }
    }

    #[instrument(level = Level::INFO, skip(kube, owner_references, resources, state), err(Display))]
    async fn update_fields(
        namespace: &str,
        kube: &Client,
        name: &str,
        UpdateContext {
            owner_references,
            resources,
            state,
            storage,
            storage_name,
        }: UpdateContext,
    ) -> Result<()> {
        let api = Api::<<Self as ::cdl_k8s_core::k8s_operator::Ctx>::Data>::namespaced(
            kube.clone(),
            namespace,
        );
        let crd = <Self as ::cdl_k8s_core::k8s_operator::Ctx>::Data::api_resource();

        let patch = Patch::Merge(json!({
            "apiVersion": crd.api_version,
            "kind": crd.kind,
            "status": ModelClaimStatus {
                resources,
                state,
                storage,
                storage_name: storage_name.clone(),
                last_updated: Utc::now(),
            },
        }));
        let pp = PatchParams::apply(<Self as ::cdl_k8s_core::k8s_operator::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;

        let patch = Patch::Apply(json!({
            "apiVersion": &crd.api_version,
            "kind": crd.kind,
            "spec": {
                "storageName": storage_name,
            },
        }));
        let pp = pp.force();
        api.patch(name, &pp, &patch).await?;

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
