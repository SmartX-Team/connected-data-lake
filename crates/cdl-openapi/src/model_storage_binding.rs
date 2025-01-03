use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::ResourceRequirements;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::{model::ModelSpec, model_storage::ModelStorageSpec};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "k8s", derive(::kube::CustomResource))]
#[cfg_attr(
    feature = "k8s",
    kube(
        group = "cdl.ulagbulag.io",
        version = "v1alpha1",
        kind = "ModelStorageBinding",
        root = "ModelStorageBindingCrd",
        status = "ModelStorageBindingStatus",
        shortname = "msb",
        namespaced,
        printcolumn = r#"{
            "name": "state",
            "type": "string",
            "description": "state of the binding",
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
        printcolumn = r#"{
            "name": "version",
            "type": "integer",
            "description": "binding version",
            "jsonPath": ".metadata.generation"
        }"#,
    )
)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageBindingSpec {
    #[serde(default)]
    pub deletion_policy: ModelStorageBindingDeletionPolicy,
    pub model: String,
    #[serde(default)]
    pub resources: Option<ResourceRequirements>,
    pub storage: ModelStorageBindingStorageKind<String>,
}

#[cfg(feature = "k8s")]
impl ModelStorageBindingCrd {
    pub const FINALIZER_NAME: &'static str = "cdl.ulagbulag.io/finalizer-model-storage-bindings";
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModelStorageBindingStorageKind<Storage> {
    Cloned(ModelStorageBindingStorageKindClonedSpec<Storage>),
    Owned(ModelStorageBindingStorageKindOwnedSpec<Storage>),
}

impl<Storage> ModelStorageBindingStorageKind<Storage> {
    pub fn source(&self) -> Option<(&Storage, ModelStorageBindingSyncPolicy)> {
        match self {
            Self::Cloned(spec) => Some((&spec.source, spec.sync_policy)),
            Self::Owned(_) => None,
        }
    }

    pub fn source_binding_name(&self) -> Option<&str> {
        match self {
            Self::Cloned(spec) => spec.source_binding_name.as_deref(),
            Self::Owned(_) => None,
        }
    }

    pub fn sync_policy(&self) -> Option<ModelStorageBindingSyncPolicy> {
        match self {
            Self::Cloned(spec) => Some(spec.sync_policy),
            Self::Owned(_) => None,
        }
    }

    pub fn target(&self) -> &Storage {
        match self {
            Self::Cloned(spec) => &spec.target,
            Self::Owned(spec) => &spec.target,
        }
    }

    pub fn into_target(self) -> Storage {
        match self {
            Self::Cloned(spec) => spec.target,
            Self::Owned(spec) => spec.target,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageBindingStorageKindClonedSpec<Storage> {
    pub source: Storage,
    #[serde(default)]
    pub source_binding_name: Option<String>,
    pub target: Storage,
    #[serde(default)]
    pub sync_policy: ModelStorageBindingSyncPolicy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageBindingStorageSpec<'name, Storage> {
    pub source: Option<ModelStorageBindingStorageSourceSpec<'name, Storage>>,
    pub source_binding_name: Option<&'name str>,
    pub target: Storage,
    pub target_name: &'name str,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageBindingStorageSourceSpec<'name, Storage> {
    pub name: &'name str,
    pub storage: Storage,
    pub sync_policy: ModelStorageBindingSyncPolicy,
}

impl<'name, Storage> ModelStorageBindingStorageSourceSpec<'name, Storage> {
    pub fn as_deref(&self) -> ModelStorageBindingStorageSourceSpec<'name, &'_ Storage> {
        ModelStorageBindingStorageSourceSpec {
            name: self.name,
            storage: &self.storage,
            sync_policy: self.sync_policy,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageBindingStorageKindOwnedSpec<Storage> {
    pub target: Storage,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageBindingSyncPolicy {
    #[serde(default)]
    pub pull: ModelStorageBindingSyncPolicyPull,
    #[serde(default)]
    pub push: ModelStorageBindingSyncPolicyPush,
}

impl ModelStorageBindingSyncPolicy {
    pub fn is_none(&self) -> bool {
        self.pull == ModelStorageBindingSyncPolicyPull::Never
            && self.push == ModelStorageBindingSyncPolicyPush::Never
    }
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
pub enum ModelStorageBindingSyncPolicyPull {
    #[default]
    Always,
    OnCreate,
    Never,
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
pub enum ModelStorageBindingSyncPolicyPush {
    #[default]
    Always,
    OnDelete,
    Never,
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
pub enum ModelStorageBindingDeletionPolicy {
    Delete,
    #[default]
    Retain,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageBindingStatus {
    #[serde(default)]
    pub state: ModelStorageBindingState,
    #[serde(default)]
    pub deletion_policy: ModelStorageBindingDeletionPolicy,
    #[serde(default)]
    pub model: Option<ModelSpec>,
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default)]
    pub resources: Option<ResourceRequirements>,
    #[serde(default)]
    pub storage_source: Option<ModelStorageSpec>,
    #[serde(default)]
    pub storage_source_binding_name: Option<String>,
    #[serde(default)]
    pub storage_source_name: Option<String>,
    #[serde(default)]
    pub storage_source_uid: Option<String>,
    #[serde(default)]
    pub storage_sync_policy: Option<ModelStorageBindingSyncPolicy>,
    #[serde(default)]
    pub storage_target: Option<ModelStorageSpec>,
    #[serde(default)]
    pub storage_target_name: Option<String>,
    #[serde(default)]
    pub storage_target_uid: Option<String>,
    pub last_updated: DateTime<Utc>,
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
pub enum ModelStorageBindingState {
    #[default]
    Pending,
    Ready,
    Deleting,
}
