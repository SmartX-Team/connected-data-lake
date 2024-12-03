use std::time::Duration;

use anyhow::{Error, Result};
use async_trait::async_trait;
use cdl_openapi::model_storage::{
    object::{
        ModelStorageObjectOwnedReplicationSpec, ModelStorageObjectOwnedSpec, ModelStorageObjectSpec,
    },
    ModelStorageCrd, ModelStorageKindSpec, ModelStorageSpec, ModelStorageState,
};
use futures::{
    stream::{self, FuturesUnordered},
    TryStreamExt,
};
use k8s_openapi::{
    api::core::v1::{Namespace, ResourceRequirements},
    apimachinery::pkg::api::resource::Quantity,
};
use kube::{
    api::{DeleteParams, ObjectMeta, PostParams, PropagationPolicy},
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
        info!("create_ponds: create {num_k}");

        let objects: Vec<_> = (0..num_k)
            .map(|k| format!("cdl-benchmark-{k:07}"))
            .map(|namespace| ModelStorageCrd {
                metadata: ObjectMeta {
                    name: Some("object-storage-pool".into()),
                    namespace: Some(namespace),
                    labels: Some(btreemap! {
                        "cdl.ulagbulag.io/benchmark".into() => "true".into(),
                    }),
                    ..Default::default()
                },
                spec: ModelStorageSpec {
                    kind: ModelStorageKindSpec::ObjectStorage(ModelStorageObjectSpec::Owned(
                        ModelStorageObjectOwnedSpec {
                            replication: ModelStorageObjectOwnedReplicationSpec {
                                resources: ResourceRequirements {
                                    limits: Some(btreemap! {
                                        "cpu".into() => Quantity("1".into()),
                                        "memory".into() => Quantity("1Gi".into()),
                                    }),
                                    requests: Some(btreemap! {
                                        "storage".into() => Quantity("10Ti".into()),
                                    }),
                                    ..Default::default()
                                },
                                total_nodes: 1,
                                total_volumes_per_node: 1,
                            },
                            ..Default::default()
                        },
                    )),
                    default: true,
                },
                status: None,
            })
            .collect();

        let api_ns = Api::all(kube.clone());
        let pp = PostParams::default();
        for object in &objects {
            let ns = Namespace {
                metadata: ObjectMeta {
                    name: object.namespace(),
                    labels: Some(btreemap! {
                        "cdl.ulagbulag.io/benchmark".into() => "true".into(),
                    }),
                    ..Default::default()
                },
                spec: None,
                status: None,
            };
            api_ns.create(&pp, &ns).await?;

            let namespace = ns.name_any();
            loop {
                if api_ns.get_metadata_opt(&namespace).await?.is_some() {
                    break;
                }
                sleep(Duration::from_millis(args.apply_interval_ms)).await;
            }

            let api = Api::namespaced(kube.clone(), &namespace);
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
                    let object: ModelStorageCrd = api.get(&name).await?;
                    if object
                        .status
                        .as_ref()
                        .map(|status| status.state == ModelStorageState::Ready)
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
        info!("create_ponds: delete {num_k}");

        let namespaces: Vec<_> = (0..num_k)
            .map(|k| format!("cdl-benchmark-{k:07}"))
            .collect();

        let api = Api::<Namespace>::all(kube.clone());
        let dp = DeleteParams {
            propagation_policy: Some(PropagationPolicy::Foreground),
            ..Default::default()
        };
        namespaces
            .iter()
            .map(|x| async move { Ok(x) })
            .collect::<FuturesUnordered<_>>()
            .try_for_each_concurrent(args.num_threads, |namespace| async {
                api.delete(namespace, &dp).await?;
                sleep(Duration::from_millis(args.apply_interval_ms)).await;
                Ok::<_, Error>(())
            })
            .await?;

        namespaces
            .iter()
            .map(|x| async move { Ok(x) })
            .collect::<FuturesUnordered<_>>()
            .try_for_each_concurrent(args.num_threads, |namespace| async {
                loop {
                    let object = api.get_metadata_opt(namespace).await?;
                    if object.is_none() {
                        break;
                    }
                    sleep(Duration::from_millis(args.apply_interval_ms)).await;
                }
                Ok::<_, Error>(())
            })
            .await
    }
}
