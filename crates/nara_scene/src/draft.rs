//! Private flat compiler storage, independent of authored hierarchy declarations.

use std::collections::BTreeMap;

use nara_reflect::{
    ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueRef,
    PersistentComponentSchemaOwnerId,
};

use crate::{SceneEntityId, effective_component_payload::EffectiveComponentPayload};

#[derive(Debug, PartialEq)]
pub(crate) struct SceneDraft {
    pub(crate) roots: Vec<SceneEntityId>,
    pub(crate) entities: Vec<ExpandedSceneEntity>,
}

impl SceneDraft {
    #[cfg(test)]
    pub(crate) fn new(
        roots: impl IntoIterator<Item = SceneEntityId>,
        entities: impl IntoIterator<Item = ExpandedSceneEntity>,
    ) -> Self {
        let mut draft = Self {
            roots: roots.into_iter().collect(),
            entities: entities.into_iter().collect(),
        };
        draft.canonicalize();
        draft
    }

    pub(crate) fn canonicalize(&mut self) {
        self.entities.sort_by(|left, right| left.id.cmp(&right.id));
    }
}

/// One compiler-owned authoring observation with an already expanded hierarchy.
///
/// Components may remain unavailable for Native presentation. This borrowed projection grants
/// neither source editing authority nor the complete value proof of a compiled Scene.
///
/// ```compile_fail
/// use nara_scene::ExpandedSceneEntity;
/// fn escape(entity: &ExpandedSceneEntity) -> ExpandedSceneEntity {
///     entity.clone()
/// }
/// ```
///
/// ```compile_fail
/// use nara_scene::ExpandedSceneEntity;
/// fn remove_components(entity: &mut ExpandedSceneEntity) {
///     entity.components().clear();
/// }
/// ```
#[derive(Debug, PartialEq)]
pub struct ExpandedSceneEntity {
    pub(crate) id: SceneEntityId,
    pub(crate) children: Vec<SceneEntityId>,
    pub(crate) components: BTreeMap<ComponentTypeId, ExpandedSceneComponent>,
}

impl ExpandedSceneEntity {
    pub(crate) fn new(id: SceneEntityId) -> Self {
        Self {
            id,
            children: Vec::new(),
            components: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn id(&self) -> &SceneEntityId {
        &self.id
    }

    #[must_use]
    pub fn children(&self) -> &[SceneEntityId] {
        &self.children
    }

    /// Allocation capacity retained by this entity's hierarchy observation.
    #[must_use]
    pub fn children_capacity(&self) -> usize {
        self.children.capacity()
    }

    #[must_use]
    pub const fn components(&self) -> &BTreeMap<ComponentTypeId, ExpandedSceneComponent> {
        &self.components
    }
}

/// A compiler-owned component observation, separate from its omission-preserving source record.
#[derive(Debug, PartialEq)]
pub struct ExpandedSceneComponent {
    pub owner: Option<PersistentComponentSchemaOwnerId>,
    pub version: ComponentSchemaVersion,
    pub(crate) value: EffectiveComponentPayload,
    pub contextual_defaults: BTreeMap<ComponentFieldId, ComponentValue>,
}

impl ExpandedSceneComponent {
    #[must_use]
    pub const fn owner(&self) -> Option<&PersistentComponentSchemaOwnerId> {
        self.owner.as_ref()
    }

    #[must_use]
    pub fn value(&self) -> ComponentValueRef<'_> {
        self.value.read()
    }

    /// Raw root backing and frozen insertion storage, independently charged for this owner.
    /// Complete effective logical value cost is accounted separately.
    #[must_use]
    pub fn value_storage_retained_bytes(&self) -> usize {
        self.value.retained_bytes()
    }

    pub(crate) fn materialize_value_mut(&mut self) -> &mut ComponentValue {
        self.value.materialize_mut()
    }

    pub(crate) fn set_value(&mut self, value: ComponentValue) {
        self.value = value.into();
    }

    pub(crate) fn into_compiled_parts(
        self,
    ) -> (
        Option<PersistentComponentSchemaOwnerId>,
        ComponentSchemaVersion,
        EffectiveComponentPayload,
    ) {
        (self.owner, self.version, self.value)
    }

    #[cfg(feature = "serde")]
    pub(crate) fn into_source_record(self) -> crate::SceneComponentRecord {
        let mut record = crate::SceneComponentRecord::new(self.version, self.value.into_owned());
        record.owner = self.owner;
        record.contextual_defaults = self.contextual_defaults;
        record
    }
}
