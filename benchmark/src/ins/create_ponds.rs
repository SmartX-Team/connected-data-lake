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
use tracing::{instrument, Level};

use crate::args::CommonArgs;

use super::Metrics;

#[derive(Debug)]
pub struct Instruction {
    pub num_k: usize,
}

#[async_trait]
impl super::Instruction for Instruction {
    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn apply(&self, kube: &Client, args: &CommonArgs, _metrics: &mut Metrics) -> Result<()> {
        let objects: Vec<_> = (0..self.num_k)
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
                                    requests: Some(btreemap! {
                                        "storage".into() => Quantity("10Ti".into()),
                                    }),
                                    ..Default::default()
                                },
                                ..Default::default()
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
        stream::iter(objects.iter().map(|x| Ok(x)))
            .try_for_each_concurrent(args.num_threads, |object| async {
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
                    sleep(Duration::from_millis(args.check_interval_ms)).await;
                }

                let api = Api::namespaced(kube.clone(), &namespace);
                api.create(&pp, object).await?;

                let name = object.name_any();
                loop {
                    let object = api.get(&name).await?;
                    if object
                        .status
                        .as_ref()
                        .map(|status| status.state == ModelStorageState::Ready)
                        .unwrap_or_default()
                    {
                        break;
                    }
                    sleep(Duration::from_millis(args.check_interval_ms)).await;
                }
                Ok::<_, Error>(())
            })
            .await
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn delete(&self, kube: &Client, args: &CommonArgs, _metrics: &mut Metrics) -> Result<()> {
        let namespaces: Vec<_> = (0..self.num_k)
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
                sleep(Duration::from_millis(args.check_interval_ms)).await;
                Ok::<_, Error>(())
            })
            .await
    }
}
