//! Readonly source-local topology observations with no Prefab resolution or Native authority.

use std::collections::BTreeMap;

use crate::SceneEntityId;

/// Ordered topology of directly owned declarations, evaluated by the ordinary layout kernel.
///
/// A query requiring a projected target refuses instead of guessing the missing occurrence.
/// Prefab slots remain local anchors here. Their expanded hierarchy requires ordinary Scene
/// compilation with the actual selected sources. Component values are not interpreted.
#[derive(Debug, PartialEq, Eq)]
pub struct SourceLocalSceneHierarchy {
    pub(crate) roots: Vec<SceneEntityId>,
    pub(crate) children: BTreeMap<SceneEntityId, Vec<SceneEntityId>>,
}

impl SourceLocalSceneHierarchy {
    #[must_use]
    pub fn roots(&self) -> &[SceneEntityId] {
        &self.roots
    }

    #[must_use]
    pub fn children(&self, entity: &SceneEntityId) -> Option<&[SceneEntityId]> {
        self.children.get(entity).map(Vec::as_slice)
    }
}
