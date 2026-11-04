use std::{mem::size_of, ops::Range};

use nara::tooling::{EditorFrameRevision, SceneInspectorEntityRow};

use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct HierarchyProjectionWork {
    pub refreshes: u64,
    pub source_rows: u64,
    pub filter_refreshes: u64,
    pub filter_rows: u64,
    pub early_reused_frames: u64,
    pub retained_index_bytes: usize,
}

/// Derived rows for one immutable workspace/project frame, without presentation authority.
pub(super) struct SourceProjection {
    frame: EditorFrameRevision,
    read_only: bool,
    pub basis: Option<HierarchyBasis>,
    pub selected_index: Option<usize>,
    row_count: usize,
    filter: Option<FilteredRows>,
}

struct FilteredRows {
    committed: String,
    indices: Option<Vec<usize>>,
}

impl SourceProjection {
    pub fn capture(
        model: &EditorFrameModel,
        read_only: bool,
        work: &mut HierarchyProjectionWork,
    ) -> Self {
        work.refreshes = work.refreshes.saturating_add(1);
        work.retained_index_bytes = 0;
        let mut projection = Self {
            frame: model.revision(),
            read_only,
            basis: None,
            selected_index: None,
            row_count: 0,
            filter: None,
        };
        let Some(scene) = model.workspace().active_scene.as_ref().filter(|scene| {
            model.workspace().active_document == Some(scene.document)
                && matches!(
                    scene.kind,
                    EditorDocumentKind::Scene | EditorDocumentKind::Prefab
                )
        }) else {
            return projection;
        };
        let inspector = &scene.editor.inspector;
        let mut has_prefab = false;
        for (index, row) in inspector.entities.iter().enumerate() {
            work.source_rows = work.source_rows.saturating_add(1);
            if projection.selected_index.is_none()
                && Some(&row.id) == inspector.selected_entity.as_ref()
            {
                projection.selected_index = Some(index);
            }
            has_prefab |= row.has_prefab;
        }
        projection.row_count = inspector.entities.len();
        let selected = projection
            .selected_index
            .map(|index| &inspector.entities[index]);
        projection.basis = Some(HierarchyBasis {
            target: EditorDocumentQueryBasis {
                document: scene.document,
                revision: scene.revision,
                schema: model
                    .workspace()
                    .document(scene.document)
                    .and_then(|tab| tab.authoring_schema_basis.clone()),
            },
            kind: scene.kind,
            selected: inspector.selected_entity.clone(),
            placement: selected.and_then(|row| row.placement.clone()),
            capabilities: inspector.composition.clone(),
            editing: !read_only && authoring::editing_enabled(model),
            navigation: !read_only && model.workspace_intent().intent().is_none(),
            read_only,
            removal_reason: authoring::entity_removal_unavailable_reason(model, scene, has_prefab),
        });
        projection
    }

    pub fn matches(&self, model: &EditorFrameModel, read_only: bool) -> bool {
        self.frame == model.revision() && self.read_only == read_only
    }

    pub fn refilter(
        &mut self,
        rows: &[SceneInspectorEntityRow],
        committed: &str,
        work: &mut HierarchyProjectionWork,
    ) {
        if self
            .filter
            .as_ref()
            .is_some_and(|filter| filter.committed == committed)
        {
            return;
        }
        work.filter_refreshes = work.filter_refreshes.saturating_add(1);
        let indices = if committed.is_empty() {
            None
        } else {
            let query = committed.to_lowercase();
            let mut indices = Vec::with_capacity(rows.len());
            for (index, row) in rows.iter().enumerate() {
                work.filter_rows = work.filter_rows.saturating_add(1);
                if row.id.as_str().to_lowercase().contains(&query)
                    || row
                        .name
                        .as_ref()
                        .is_some_and(|name| name.to_lowercase().contains(&query))
                {
                    indices.push(index);
                }
            }
            Some(indices)
        };
        work.retained_index_bytes = indices.as_ref().map_or(0, |indices| {
            indices.capacity().saturating_mul(size_of::<usize>())
        });
        self.filter = Some(FilteredRows {
            committed: committed.to_owned(),
            indices,
        });
    }

    pub fn matching_count(&self) -> usize {
        self.filter
            .as_ref()
            .and_then(|filter| filter.indices.as_ref())
            .map_or(self.row_count, Vec::len)
    }

    pub fn visible_sources(&self, range: Range<usize>) -> Vec<usize> {
        let count = self.matching_count();
        let range = range.start.min(count)..range.end.min(count);
        if let Some(indices) = self
            .filter
            .as_ref()
            .and_then(|filter| filter.indices.as_ref())
        {
            indices[range].to_vec()
        } else {
            range.collect()
        }
    }

    pub fn other_source(&self, ordinal: usize) -> Option<usize> {
        let index =
            ordinal + usize::from(self.selected_index.is_some_and(|index| ordinal >= index));
        (index < self.row_count).then_some(index)
    }
}
