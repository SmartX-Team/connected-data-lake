pub mod object;

use std::collections::BTreeMap;

use byte_unit::Byte;
use cdl_k8s_core::openapi::Url;
use chrono::{DateTime, Utc};
use k8s_openapi::{
    api::core::v1::ResourceRequirements, apimachinery::pkg::api::resource::Quantity,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "k8s", derive(::kube::CustomResource))]
#[cfg_attr(
    feature = "k8s",
    kube(
        group = "cdl.ulagbulag.io",
        version = "v1alpha1",
        kind = "ModelStorage",
        root = "ModelStorageCrd",
        status = "ModelStorageStatus",
        shortname = "ms",
        namespaced,
        printcolumn = r#"{
            "name": "state",
            "type": "string",
            "description": "state of the model storage",
            "jsonPath": ".status.state"
        }"#,
        printcolumn = r#"{
            "name": "created-at",
            "type": "date",
            "description": "created time",
            "jsonPath": ".metadata.creationTimestamp"
        }"#,
        printcolumn = r#"{
            "name": "updated-at",
            "type": "date",
            "description": "updated time",
            "jsonPath": ".status.lastUpdated"
        }"#,
    )
)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageSpec {
    #[serde(flatten)]
    pub kind: ModelStorageKindSpec,
    #[serde(default)]
    pub default: bool,
}

#[cfg(feature = "k8s")]
impl ModelStorageCrd {
    pub const FINALIZER_NAME: &'static str = "cdl.ulagbulag.io/finalizer-model-storages";

    pub const LABEL_IS_EXTERNAL: &'static str = "ark.ulagbulag.io/is-external";
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModelStorageKindSpec {
    ObjectStorage(#[serde(default)] self::object::ModelStorageObjectSpec),
}

impl ModelStorageKindSpec {
    #[inline]
    pub fn endpoint(&self, namespace: &str) -> Option<Url> {
        match self {
            Self::ObjectStorage(spec) => spec.endpoint(namespace),
        }
    }

    pub const fn is_unique(&self) -> bool {
        match self {
            Self::ObjectStorage(spec) => spec.is_unique(),
        }
    }

    pub const fn to_kind(&self) -> ModelStorageKind {
        match self {
            Self::ObjectStorage(_) => ModelStorageKind::ObjectStorage,
        }
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum ModelStorageKind {
    ObjectStorage,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageStatus {
    #[serde(default)]
    pub state: ModelStorageState,
    pub kind: Option<ModelStorageKindSpec>,
    pub last_updated: DateTime<Utc>,
    #[serde(default)]
    pub total_quota: Option<u128>,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    Default,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum ModelStorageState {
    #[default]
    Pending,
    Ready,
    Deleting,
}

pub trait StorageResourceRequirements {
    fn quota(&self) -> Option<Byte>;
}

impl<T> StorageResourceRequirements for Option<T>
where
    T: StorageResourceRequirements,
{
    fn quota(&self) -> Option<Byte> {
        self.as_ref().and_then(|this| this.quota())
    }
}

impl StorageResourceRequirements for ResourceRequirements {
    fn quota(&self) -> Option<Byte> {
        self.requests.quota()
    }
}

impl StorageResourceRequirements for BTreeMap<String, Quantity> {
    fn quota(&self) -> Option<Byte> {
        self.get("storage").and_then(|quota| quota.0.parse().ok())
    }
}
