use std::time::Duration;

use anyhow::{Error, Result};
use async_trait::async_trait;
use cdl_openapi::{
    model_claim::{
        ModelClaimAffinity, ModelClaimBindingPolicy, ModelClaimCrd, ModelClaimDeletionPolicy,
        ModelClaimSpec, ModelClaimState,
    },
    model_storage::ModelStorageKind,
};
use futures::{
    stream::{self, FuturesUnordered},
    TryStreamExt,
};
use k8s_openapi::api::core::v1::Namespace;
use kube::{
    api::{DeleteParams, ListParams, ObjectMeta, PostParams, PropagationPolicy},
    Api, Client, ResourceExt,
};
use maplit::btreemap;
use tokio::time::sleep;
use tracing::{info, instrument, Level};

use crate::args::CommonArgs;

use super::Metrics;

#[derive(Copy, Clone, Debug)]
pub struct Instruction {
    pub num_k: usize,
}

#[async_trait]
impl super::Instruction for Instruction {
    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn apply(&self, kube: &Client, args: &CommonArgs, _metrics: &mut Metrics) -> Result<()> {
        let Self { num_k } = *self;
        info!("create_datasets: create {num_k}");

        let namespaces = {
            let api = Api::<Namespace>::all(kube.clone());
            let lp = ListParams {
                label_selector: Some("cdl.ulagbulag.io/benchmark=true".into()),
                ..Default::default()
            };
            api.list_metadata(&lp)
                .await?
                .items
                .into_iter()
                .map(|item| item.name_any())
                .cycle()
        };

        let objects: Vec<_> = (0..num_k)
            .map(|k| format!("cdl-benchmark-dataset-{k:07}"))
            .zip(namespaces)
            .map(|(name, namespace)| ModelClaimCrd {
                metadata: ObjectMeta {
                    name: Some(name),
                    namespace: Some(namespace),
                    labels: Some(btreemap! {
                        "cdl.ulagbulag.io/benchmark".into() => "true".into(),
                    }),
                    ..Default::default()
                },
                spec: ModelClaimSpec {
                    affinity: ModelClaimAffinity::default(),
                    allow_replacement: args.connected,
                    binding_policy: ModelClaimBindingPolicy::LowestCopy,
                    deletion_policy: ModelClaimDeletionPolicy::Delete,
                    resources: None,
                    storage: Some(ModelStorageKind::ObjectStorage),
                    storage_name: Some("object-storage-pool".into()),
                },
                status: None,
            })
            .collect();

        let pp = PostParams::default();
        for object in &objects {
            let api = Api::namespaced(kube.clone(), &object.namespace().unwrap());
            api.create(&pp, object).await?;
            sleep(Duration::from_millis(
                args.apply_interval_ms / args.num_threads as u64,
            ))
            .await;
        }

        stream::iter(objects.iter().map(|x| Ok(x)))
            .try_for_each_concurrent(args.num_threads, |object| async {
                let api = Api::namespaced(kube.clone(), &object.namespace().unwrap());
                let name = object.name_any();
                loop {
                    let object: ModelClaimCrd = api.get(&name).await?;
                    if object
                        .status
                        .as_ref()
                        .map(|status| status.state == ModelClaimState::Ready)
                        .unwrap_or_default()
                    {
                        break;
                    }
                    sleep(Duration::from_millis(args.apply_interval_ms)).await;
                }
                Ok::<_, Error>(())
            })
            .await
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn delete(&self, kube: &Client, args: &CommonArgs, _metrics: &mut Metrics) -> Result<()> {
        let Self { num_k } = *self;
        info!("create_datasets: delete {num_k}");

        let items: Vec<_> = {
            let api = Api::<ModelClaimCrd>::all(kube.clone());
            let lp = ListParams {
                label_selector: Some("cdl.ulagbulag.io/benchmark=true".into()),
                ..Default::default()
            };
            api.list_metadata(&lp)
                .await?
                .items
                .into_iter()
                .filter_map(|item| {
                    let name = item.name_any();
                    let namespace = item.namespace()?;
                    Some((namespace, name))
                })
                .collect()
        };

        let dp = DeleteParams {
            propagation_policy: Some(PropagationPolicy::Foreground),
            ..Default::default()
        };
        items
            .iter()
            .map(|x| async move { Ok(x) })
            .collect::<FuturesUnordered<_>>()
            .try_for_each_concurrent(args.num_threads, |(namespace, name)| async {
                let api = Api::<ModelClaimCrd>::namespaced(kube.clone(), namespace);
                api.delete(name, &dp).await?;
                sleep(Duration::from_millis(args.apply_interval_ms)).await;
                Ok::<_, Error>(())
            })
            .await
    }
}
