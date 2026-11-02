//! Unadmitted flat data. Only consuming Scene admission may turn these claims into compiled data.
//!
//! Source-only declarations are deliberately absent. The processed reader owns its bounded
//! decoding and cannot obtain activation authority by constructing these crate-private records.

use std::collections::BTreeMap;

use nara_reflect::{
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, PersistentComponentSchemaOwnerId,
};

use crate::{SceneEntityId, SceneEntityProvenanceMap};

#[derive(Debug)]
pub(crate) struct FlatSceneClaims {
    pub(crate) roots: Vec<SceneEntityId>,
    pub(crate) entities: Vec<FlatSceneEntityClaim>,
    pub(crate) canonical_provenance: SceneEntityProvenanceMap,
}

#[derive(Debug)]
pub(crate) struct FlatSceneEntityClaim {
    pub(crate) id: SceneEntityId,
    pub(crate) children: Vec<SceneEntityId>,
    pub(crate) components: BTreeMap<ComponentTypeId, FlatSceneComponentClaim>,
}

#[derive(Debug)]
pub(crate) struct FlatSceneComponentClaim {
    pub(crate) schema_owner: Option<PersistentComponentSchemaOwnerId>,
    pub(crate) version: ComponentSchemaVersion,
    pub(crate) value: ComponentValue,
}
