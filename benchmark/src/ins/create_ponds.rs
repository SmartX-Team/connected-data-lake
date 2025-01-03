use std::{net::Ipv4Addr, time::Duration};

use anyhow::{Error, Result};
use async_trait::async_trait;
use cdl_openapi::model_storage::{
    object::{
        ModelStorageObjectBorrowedSpec, ModelStorageObjectOwnedExternalServiceSpec,
        ModelStorageObjectOwnedReplicationSpec, ModelStorageObjectOwnedSpec,
        ModelStorageObjectRefSpec, ModelStorageObjectSpec,
    },
    ModelStorageCrd, ModelStorageKindSpec, ModelStorageSpec, ModelStorageState,
};
use futures::{
    stream::{self, FuturesUnordered},
    TryStreamExt,
};
use k8s_openapi::{
    api::core::v1::{Namespace, ResourceRequirements, Secret},
    apimachinery::pkg::api::resource::Quantity,
};
use kube::{
    api::{DeleteParams, ObjectMeta, PostParams, PropagationPolicy},
    Api, ResourceExt,
};
use maplit::btreemap;
use tokio::time::sleep;
use tracing::{info, instrument, Level};

use super::InstructionStack;

#[derive(Clone, Debug)]
pub struct Instruction {
    pub address: Option<Ipv4Addr>,
    pub name: Option<String>,
    pub num_k: usize,
}

#[async_trait]
impl super::Instruction for Instruction {
    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn apply(&self, stack: &mut InstructionStack) -> Result<()> {
        let Self {
            address,
            name,
            num_k,
        } = self.clone();
        let InstructionStack { kube, args, .. } = stack;
        info!("create_ponds: create {num_k}");

        let objects: Vec<_> = (0..num_k)
            .map(|k| format!("cdl-benchmark-{k:07}"))
            .map(|namespace| ModelStorageCrd {
                metadata: ObjectMeta {
                    name: Some(name.clone().unwrap_or_else(|| "object-storage-pool".into())),
                    namespace: Some(namespace),
                    labels: Some(btreemap! {
                        "ark.ulagbulag.io/is-external".into() => "true".into(),
                        "cdl.ulagbulag.io/benchmark".into() => "true".into(),
                    }),
                    ..Default::default()
                },
                spec: ModelStorageSpec {
                    kind: ModelStorageKindSpec::ObjectStorage(match address.zip(name.as_ref()) {
                        Some((remote, _)) => {
                            ModelStorageObjectSpec::Borrowed(ModelStorageObjectBorrowedSpec {
                                reference: ModelStorageObjectRefSpec {
                                    endpoint: format!("http://{remote}").parse().unwrap(),
                                    secret_ref: Default::default(),
                                },
                            })
                        }
                        None => ModelStorageObjectSpec::Owned(ModelStorageObjectOwnedSpec {
                            minio_external_service: ModelStorageObjectOwnedExternalServiceSpec {
                                ip: address,
                                ..Default::default()
                            },
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
                        }),
                    }),
                    default: true,
                },
                status: None,
            })
            .collect();

        let api_ns = Api::all(kube.clone());
        let is_remote = address.zip(name.as_ref()).is_some();
        let pp = PostParams::default();
        for object in &objects {
            let namespace = object.namespace().unwrap();
            if !is_remote {
                // create a namespace
                let ns = Namespace {
                    metadata: ObjectMeta {
                        name: Some(namespace.clone()),
                        labels: Some(btreemap! {
                            "cdl.ulagbulag.io/benchmark".into() => "true".into(),
                        }),
                        ..Default::default()
                    },
                    spec: None,
                    status: None,
                };
                api_ns.create(&pp, &ns).await?;

                loop {
                    if api_ns.get_metadata_opt(&namespace).await?.is_some() {
                        break;
                    }
                    sleep(Duration::from_millis(args.apply_interval_ms)).await;
                }

                // create a static secret
                let api = Api::namespaced(kube.clone(), &namespace);
                let secret = Secret {
                    metadata: ObjectMeta {
                        name: Some("object-storage-user-0".into()),
                        namespace: Some(namespace.clone()),
                        ..Default::default()
                    },
                    immutable: Some(true),
                    string_data: Some(btreemap! {
                        "CONSOLE_ACCESS_KEY".into() => "abcdefgh12345678".into(),
                        "CONSOLE_SECRET_KEY".into() => "abcdefgh12345678abcdefgh12345678".into(),
                    }),
                    ..Default::default()
                };
                api.create(&pp, &secret).await?;
                sleep(Duration::from_millis(
                    args.apply_interval_ms / args.num_threads as u64,
                ))
                .await;
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
    async fn delete(&self, stack: &mut InstructionStack) -> Result<()> {
        let Self {
            address,
            name,
            num_k,
            ..
        } = self;
        let InstructionStack { kube, args, .. } = stack;
        info!("create_ponds: delete {num_k}");

        let is_remote = address.zip(name.as_ref()).is_some();
        if is_remote {
            return Ok(());
        }

        let namespaces: Vec<_> = (0..*num_k)
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
