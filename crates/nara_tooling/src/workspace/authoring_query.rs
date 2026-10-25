use nara_reflect::{ComponentCapability, ComponentFieldSchema, ComponentTypeId};
use nara_scene::{AuthoringEntityView, AuthoringRecordFact, AuthoringRootValue, SceneEntityId};

use super::{EditorDocumentSlot, EditorWorkspace};
use crate::{
    EditorAuthoringCursor, EditorAuthoringPage, EditorAuthoringProjection, EditorAuthoringQuery,
    EditorAuthoringQueryPosition, EditorAuthoringRow, EditorDocumentKind, EditorDocumentQueryBasis,
    EditorDocumentQueryRejection, EditorRootObservation, EditorRootRead, EditorRootValue,
    authoring_query::AuthoringPagePosition,
};

impl EditorWorkspace {
    pub(super) fn resolve_document_query_slot(
        &self,
        basis: &EditorDocumentQueryBasis,
        expected_kind: Option<EditorDocumentKind>,
    ) -> Result<&EditorDocumentSlot, EditorDocumentQueryRejection> {
        let slot = self.documents.get(&basis.document).ok_or(
            EditorDocumentQueryRejection::DocumentNotOpen {
                document: basis.document,
            },
        )?;
        let kind = slot.kind();
        if !matches!(kind, EditorDocumentKind::Scene | EditorDocumentKind::Prefab) {
            return Err(EditorDocumentQueryRejection::UnsupportedDocumentKind { kind });
        }
        if let Some(expected) = expected_kind
            && expected != kind
        {
            return Err(EditorDocumentQueryRejection::DocumentKindChanged {
                expected,
                actual: kind,
            });
        }
        let revision = slot.revision();
        if basis.revision.source_id() != revision.source_id() {
            return Err(EditorDocumentQueryRejection::StaleIncarnation);
        }
        if basis.revision != revision {
            return Err(EditorDocumentQueryRejection::StaleRevision);
        }
        if basis.schema != slot.authoring_schema_basis() {
            return Err(EditorDocumentQueryRejection::StaleSchema);
        }
        Ok(slot)
    }

    /// Discovers actual direct components or declared roots without reading their value trees.
    /// Every inspected entry produces one row. Metadata is preflighted for the complete page and
    /// its continuation before any source IDs are copied; insufficient budgets reject terminally.
    pub fn query_document_authoring(
        &self,
        query: EditorAuthoringQuery,
    ) -> Result<EditorAuthoringPage, EditorDocumentQueryRejection> {
        let (target, expected_kind, position) = match query.position {
            EditorAuthoringQueryPosition::Start(target) => {
                let position = match target.projection {
                    EditorAuthoringProjection::Components => AuthoringPagePosition::Components {
                        after: None,
                        visited: 0,
                    },
                    EditorAuthoringProjection::Roots { .. } => {
                        AuthoringPagePosition::Roots { next_root: 0 }
                    }
                };
                (target, None, position)
            }
            EditorAuthoringQueryPosition::Continue(cursor) => {
                if cursor.owner != self.document_query_owner {
                    return Err(EditorDocumentQueryRejection::WrongWorkspace);
                }
                (cursor.target, Some(cursor.kind), cursor.position)
            }
        };
        let slot = self.resolve_document_query_slot(&target.basis, expected_kind)?;
        let entity = entity_view(slot, &target.entity)?;
        let count = query.limits.max_scan_entries().min(query.limits.max_rows());
        let (rows, next): (Vec<_>, _) = match (&target.projection, position) {
            (
                EditorAuthoringProjection::Components,
                AuthoringPagePosition::Components { after, visited },
            ) => {
                if visited > entity.component_count() {
                    return Err(EditorDocumentQueryRejection::InvalidContinuation);
                }
                let rows = entity
                    .components_after(after.as_ref())
                    .take(count)
                    .map(|component| BorrowedRow::Component(component.fact()))
                    .collect::<Vec<_>>();
                let visited = visited + rows.len();
                let next = if visited < entity.component_count() {
                    let Some(BorrowedRow::Component(last)) = rows.last() else {
                        return Err(EditorDocumentQueryRejection::InvalidContinuation);
                    };
                    Some(NextPosition::Components {
                        after: last.component(),
                        visited,
                    })
                } else {
                    None
                };
                (rows, next)
            }
            (
                EditorAuthoringProjection::Roots { component },
                AuthoringPagePosition::Roots { next_root },
            ) => {
                let fields = entity.component(component)?.root_fields()?;
                let remaining = fields
                    .get(next_root..)
                    .ok_or(EditorDocumentQueryRejection::InvalidContinuation)?;
                let rows = remaining
                    .iter()
                    .take(count)
                    .map(BorrowedRow::Root)
                    .collect::<Vec<_>>();
                let next_root = next_root + rows.len();
                let next = (next_root < fields.len()).then_some(NextPosition::Roots { next_root });
                (rows, next)
            }
            _ => return Err(EditorDocumentQueryRejection::InvalidContinuation),
        };
        let mut metadata_bytes = target.metadata_bytes();
        for row in &rows {
            metadata_bytes = metadata_bytes.saturating_add(row.metadata_bytes());
        }
        if let Some(next) = &next {
            metadata_bytes = metadata_bytes
                .saturating_add(target.metadata_bytes())
                .saturating_add(next.metadata_bytes());
        }
        if metadata_bytes > query.limits.max_metadata_bytes() {
            return Err(EditorDocumentQueryRejection::MetadataBudgetExceeded);
        }
        let next = next.map(|position| EditorAuthoringCursor {
            owner: self.document_query_owner.clone(),
            target: target.clone(),
            kind: slot.kind(),
            position: position.into_owned(),
        });
        let scanned_entries = rows.len();
        Ok(EditorAuthoringPage {
            target,
            rows: rows.into_iter().map(BorrowedRow::into_owned).collect(),
            scanned_entries,
            metadata_bytes,
            next,
        })
    }

    /// Copies only one requested complete root after exact-basis and aggregate cost admission.
    /// Missing/default interpretation remains with Scene/Prefab, not an Inspector or Runtime.
    pub fn read_document_root(
        &self,
        request: EditorRootRead,
    ) -> Result<EditorRootObservation, EditorDocumentQueryRejection> {
        let slot = self.resolve_document_query_slot(&request.target.basis, None)?;
        let metadata_bytes = request.target.metadata_bytes();
        if metadata_bytes > request.limits.max_metadata_bytes() {
            return Err(EditorDocumentQueryRejection::MetadataBudgetExceeded);
        }
        let root = entity_view(slot, &request.target.entity)?
            .component(&request.target.component)?
            .root_value(&request.target.field)?;
        let (value_nodes, value_logical_bytes) = match root {
            AuthoringRootValue::Present(value) | AuthoringRootValue::Defaulted(value) => {
                let cost = value
                    .cost_up_to(
                        request.limits.max_value_nodes(),
                        request.limits.max_logical_bytes(),
                    )
                    .ok_or(EditorDocumentQueryRejection::ValueBudgetExceeded)?;
                (cost.nodes(), cost.logical_bytes())
            }
            AuthoringRootValue::Missing => (0, 0),
        };
        let value = match root {
            AuthoringRootValue::Present(value) => EditorRootValue::Present(value.clone()),
            AuthoringRootValue::Defaulted(value) => EditorRootValue::Defaulted(value.clone()),
            AuthoringRootValue::Missing => EditorRootValue::Missing,
        };
        Ok(EditorRootObservation {
            target: request.target,
            value,
            metadata_bytes,
            value_nodes,
            value_logical_bytes,
        })
    }
}

fn entity_view<'a>(
    slot: &'a EditorDocumentSlot,
    entity: &SceneEntityId,
) -> Result<AuthoringEntityView<'a>, EditorDocumentQueryRejection> {
    match slot {
        EditorDocumentSlot::Scene(slot) => Ok(slot.session.authoring_entity_view(entity)?),
        EditorDocumentSlot::Prefab(slot) => Ok(slot.session.authoring_entity_view(entity)?),
        EditorDocumentSlot::Extension(_) => {
            Err(EditorDocumentQueryRejection::UnsupportedDocumentKind { kind: slot.kind() })
        }
    }
}

enum BorrowedRow<'a> {
    Component(&'a AuthoringRecordFact),
    Root(&'a ComponentFieldSchema),
}

impl BorrowedRow<'_> {
    fn metadata_bytes(&self) -> usize {
        match self {
            Self::Component(fact) => fact.component().as_str().len(),
            Self::Root(field) => field.id().as_str().len(),
        }
    }
    fn into_owned(self) -> EditorAuthoringRow {
        match self {
            Self::Component(fact) => EditorAuthoringRow::Component {
                component: fact.component().clone(),
                version: fact.version(),
                readiness: fact.readiness(),
            },
            Self::Root(field) => EditorAuthoringRow::Root {
                field: field.id().clone(),
                kind: field.value_kind(),
                required: field.required,
                edit_capability: field.has_capability(ComponentCapability::Edit),
            },
        }
    }
}

enum NextPosition<'a> {
    Components {
        after: &'a ComponentTypeId,
        visited: usize,
    },
    Roots {
        next_root: usize,
    },
}

impl NextPosition<'_> {
    fn metadata_bytes(&self) -> usize {
        match self {
            Self::Components { after, .. } => after.as_str().len(),
            Self::Roots { .. } => 0,
        }
    }
    fn into_owned(self) -> AuthoringPagePosition {
        match self {
            Self::Components { after, visited } => AuthoringPagePosition::Components {
                after: Some(after.clone()),
                visited,
            },
            Self::Roots { next_root } => AuthoringPagePosition::Roots { next_root },
        }
    }
}
