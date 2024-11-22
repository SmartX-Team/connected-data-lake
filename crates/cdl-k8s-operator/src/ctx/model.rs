use std::{sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use cdl_k8s_core::k8s_operator::Operator;
use cdl_openapi::model::{ModelCrd, ModelFieldsNativeSpec, ModelState, ModelStatus};
use cdl_k8s_provider::KubernetesStorageClient;
use chrono::Utc;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{
    api::{Patch, PatchParams},
    runtime::controller::Action,
    Api, Client, CustomResourceExt, Error, ResourceExt,
};
use serde_json::json;
use tracing::{info, instrument, warn, Level};

use crate::validator::model::ModelValidator;

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::cdl_k8s_core::k8s_operator::Ctx for Ctx {
    type Data = ModelCrd;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::cdl_openapi::consts::NAMESPACE;
    const FALLBACK: Duration = Duration::from_secs(30); // 30 seconds
    const FINALIZER_NAME: &'static str =
        <Self as ::cdl_k8s_core::k8s_operator::Ctx>::Data::FINALIZER_NAME;

    fn get_subcrds() -> Vec<CustomResourceDefinition> {
        vec![::cdl_openapi::model_user::ModelUserCrd::crd()]
    }

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
                .map(|status| status.state != ModelState::Deleting)
                .unwrap_or(true)
        {
            let status = data.status.as_ref();
            return Self::update_fields_or_requeue(
                &namespace,
                &operator.kube,
                &name,
                status.and_then(|status| status.fields.clone()),
                ModelState::Deleting,
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

        let validator = ModelValidator {
            kubernetes_storage: KubernetesStorageClient {
                namespace: &namespace,
                kube: &operator.kube,
            },
        };

        match data
            .status
            .as_ref()
            .map(|status| status.state)
            .unwrap_or_default()
        {
            ModelState::Pending => match validator.validate_model(data.spec.clone()).await {
                Ok(fields) => {
                    Self::update_fields_or_requeue(
                        &namespace,
                        &operator.kube,
                        &name,
                        Some(fields),
                        ModelState::Ready,
                    )
                    .await
                }
                Err(e) => {
                    warn!("failed to validate model: {name:?}: {e}");
                    Ok(Action::requeue(
                        <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                    ))
                }
            },
            ModelState::Ready => {
                // TODO: implement to finding changes
                Ok(Action::await_change())
            }
            ModelState::Deleting => match validator.delete(&data).await {
                Ok(()) => {
                    <Self as ::cdl_k8s_core::k8s_operator::Ctx>::remove_finalizer_or_requeue_namespaced(
                        operator.kube.clone(),
                        &namespace,
                        &name,
                    )
                    .await
                }
                Err(e) => {
                    warn!("failed to delete model ({namespace}/{name}): {e}");
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
        fields: Option<ModelFieldsNativeSpec>,
        state: ModelState,
    ) -> Result<Action, Error> {
        match Self::update_fields(namespace, kube, name, fields, state).await {
            Ok(()) => {
                info!("model is ready: {namespace}/{name}");
                Ok(Action::requeue(
                    <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                ))
            }
            Err(e) => {
                warn!("failed to validate model ({namespace}/{name}): {e}");
                Ok(Action::requeue(
                    <Self as ::cdl_k8s_core::k8s_operator::Ctx>::FALLBACK,
                ))
            }
        }
    }

    #[instrument(level = Level::INFO, skip(kube, fields), err(Display))]
    async fn update_fields(
        namespace: &str,
        kube: &Client,
        name: &str,
        fields: Option<ModelFieldsNativeSpec>,
        state: ModelState,
    ) -> Result<()> {
        let api = Api::<<Self as ::cdl_k8s_core::k8s_operator::Ctx>::Data>::namespaced(
            kube.clone(),
            namespace,
        );
        let crd = <Self as ::cdl_k8s_core::k8s_operator::Ctx>::Data::api_resource();

        let patch = Patch::Merge(json!({
            "apiVersion": crd.api_version,
            "kind": crd.kind,
            "status": ModelStatus {
                state,
                fields,
                last_updated: Utc::now(),
            },
        }));
        let pp = PatchParams::apply(<Self as ::cdl_k8s_core::k8s_operator::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;
        Ok(())
    }
}
