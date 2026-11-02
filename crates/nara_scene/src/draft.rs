//! Private flat compiler storage, independent of authored hierarchy declarations.

use std::collections::BTreeMap;

use nara_reflect::ComponentTypeId;

use crate::{SceneComponentRecord, SceneEntityId};

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
    pub(crate) components: BTreeMap<ComponentTypeId, SceneComponentRecord>,
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
    pub const fn components(&self) -> &BTreeMap<ComponentTypeId, SceneComponentRecord> {
        &self.components
    }
}
