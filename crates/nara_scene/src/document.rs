use std::collections::BTreeMap;

use nara_asset::ProjectAssetDatabase;
use nara_diagnostic::DiagnosticReport;
use nara_identity::SceneEntityId;
use nara_reflect::{
    ComponentDecodeContext, ComponentRegistry, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, PersistentComponentSchemaOwnerId,
};

use crate::{
    PrefabInstance,
    validation::{
        preflight_authoring_scene, preflight_authoring_scene_for_patch,
        preflight_authoring_scene_with_context, preflight_authoring_scene_with_context_for_patch,
        preflight_scene, preflight_scene_with_context,
    },
};

#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SceneDocument {
    pub roots: Vec<SceneEntityId>,
    pub entities: Vec<SceneEntityRecord>,
}

impl SceneDocument {
    #[must_use]
    pub fn new(
        roots: impl IntoIterator<Item = SceneEntityId>,
        entities: impl IntoIterator<Item = SceneEntityRecord>,
    ) -> Self {
        let mut entities = entities.into_iter().collect::<Vec<_>>();
        entities.sort_by(|left, right| left.id.cmp(&right.id));
        Self {
            roots: roots.into_iter().collect(),
            entities,
        }
    }

    /// Creates a flat document whose input record order is its root order.
    #[must_use]
    pub fn from_ordered_roots(entities: impl IntoIterator<Item = SceneEntityRecord>) -> Self {
        let entities = entities.into_iter().collect::<Vec<_>>();
        let roots = entities
            .iter()
            .map(|entity| entity.id.clone())
            .collect::<Vec<_>>();
        Self::new(roots, entities)
    }

    pub fn canonicalize(&mut self) {
        self.entities.sort_by(|left, right| left.id.cmp(&right.id));
    }

    #[must_use]
    pub fn validate(&self, registry: &ComponentRegistry) -> DiagnosticReport {
        preflight_scene(self, registry).diagnostics
    }

    #[must_use]
    pub fn validate_with_asset_database(
        &self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> DiagnosticReport {
        let mut context = ComponentDecodeContext::new().with_project_asset_database(database);
        preflight_scene_with_context(self, registry, &mut context).diagnostics
    }

    pub(crate) fn validate_authoring(&self, registry: &ComponentRegistry) -> DiagnosticReport {
        preflight_authoring_scene(self, registry).diagnostics
    }

    pub(crate) fn validate_authoring_for_patch(
        &self,
        registry: &ComponentRegistry,
        operation_index: usize,
    ) -> DiagnosticReport {
        preflight_authoring_scene_for_patch(self, registry, operation_index).diagnostics
    }

    pub(crate) fn validate_authoring_with_asset_database(
        &self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> DiagnosticReport {
        let mut context = ComponentDecodeContext::new().with_project_asset_database(database);
        preflight_authoring_scene_with_context(self, registry, &mut context).diagnostics
    }

    pub(crate) fn validate_authoring_for_patch_with_asset_database(
        &self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
        operation_index: usize,
    ) -> DiagnosticReport {
        let mut context = ComponentDecodeContext::new().with_project_asset_database(database);
        preflight_authoring_scene_with_context_for_patch(
            self,
            registry,
            &mut context,
            operation_index,
        )
        .diagnostics
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SceneEntityRecord {
    pub id: SceneEntityId,
    #[cfg_attr(feature = "serde", serde(default))]
    pub children: Vec<SceneEntityId>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub components: BTreeMap<ComponentTypeId, SceneComponentRecord>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub prefab: Option<PrefabInstance>,
}

impl SceneEntityRecord {
    #[must_use]
    pub fn new(id: SceneEntityId) -> Self {
        Self {
            id,
            children: Vec::new(),
            components: BTreeMap::new(),
            prefab: None,
        }
    }

    #[must_use]
    pub fn with_children(mut self, children: impl IntoIterator<Item = SceneEntityId>) -> Self {
        self.children = children.into_iter().collect();
        self
    }

    #[must_use]
    pub fn with_component(
        mut self,
        component_type: ComponentTypeId,
        component: SceneComponentRecord,
    ) -> Self {
        self.components.insert(component_type, component);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SceneComponentRecord {
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub owner: Option<PersistentComponentSchemaOwnerId>,
    pub version: ComponentSchemaVersion,
    pub value: ComponentValue,
}

impl SceneComponentRecord {
    #[must_use]
    pub fn new(version: ComponentSchemaVersion, value: ComponentValue) -> Self {
        Self {
            owner: None,
            version,
            value,
        }
    }

    #[must_use]
    pub fn with_owner(mut self, owner: PersistentComponentSchemaOwnerId) -> Self {
        self.owner = Some(owner);
        self
    }

    #[must_use]
    pub const fn owner(&self) -> Option<&PersistentComponentSchemaOwnerId> {
        self.owner.as_ref()
    }
}
