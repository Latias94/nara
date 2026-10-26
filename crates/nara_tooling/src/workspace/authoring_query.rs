use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldSchema, ComponentMemberPathSegment,
    ComponentPropertyPath, ComponentPropertyReadBudget, ComponentPropertyStep, ComponentTypeId,
};
use nara_scene::{AuthoringEntityView, AuthoringPropertyValue, AuthoringRecordFact, SceneEntityId};

use super::{EditorDocumentSlot, EditorWorkspace};
use crate::{
    EditorAuthoringCursor, EditorAuthoringMemberStatus, EditorAuthoringPage,
    EditorAuthoringProjection, EditorAuthoringQuery, EditorAuthoringQueryPosition,
    EditorAuthoringRow, EditorDocumentKind, EditorDocumentQueryBasis, EditorDocumentQueryRejection,
    EditorPropertyObservation, EditorPropertyRead, EditorPropertyValue,
    authoring_query::AuthoringPagePosition,
};

impl EditorWorkspace {
    /// Checks a captured read basis before an adapter prepares an edit from its observation.
    /// This does not reserve a revision or replace the candidate's commit-time admission.
    pub fn validate_document_query_basis(
        &self,
        basis: &EditorDocumentQueryBasis,
    ) -> Result<(), EditorDocumentQueryRejection> {
        self.resolve_document_query_slot(basis, None).map(|_| ())
    }

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

    /// Discovers actual direct components, declared roots, or one-level Record members without
    /// reading their value trees.
    /// Entries without direct durable identity consume scan work without producing a row.
    /// Metadata is preflighted for the complete page and its continuation before any source IDs
    /// are copied; insufficient budgets reject terminally.
    pub fn query_document_authoring(
        &self,
        query: EditorAuthoringQuery,
    ) -> Result<EditorAuthoringPage, EditorDocumentQueryRejection> {
        let (target, expected_kind, position) = match query.position {
            EditorAuthoringQueryPosition::Start(target) => {
                let position = match &target.projection {
                    EditorAuthoringProjection::Components => AuthoringPagePosition::Components {
                        after: None,
                        visited: 0,
                    },
                    EditorAuthoringProjection::Roots { .. } => {
                        AuthoringPagePosition::Roots { next_root: 0 }
                    }
                    EditorAuthoringProjection::Members { .. } => AuthoringPagePosition::Members {
                        next_member: 0,
                        next_tombstone: 0,
                    },
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
        let (rows, next, scanned_entries): (Vec<_>, _, _) = match (&target.projection, position) {
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
                let scanned_entries = rows.len();
                (rows, next, scanned_entries)
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
                let scanned_entries = rows.len();
                (rows, next, scanned_entries)
            }
            (
                EditorAuthoringProjection::Members { component, root },
                AuthoringPagePosition::Members {
                    next_member,
                    next_tombstone,
                },
            ) => {
                let direct = entity.component(component)?.direct_members(root)?;
                let members = direct.members();
                let tombstones = direct.tombstones();
                if next_member > members.len() || next_tombstone > tombstones.len() {
                    return Err(EditorDocumentQueryRejection::InvalidContinuation);
                }

                let mut member_index = next_member;
                let mut tombstone_index = next_tombstone;
                let mut scanned_entries = 0;
                let mut rows = Vec::new();
                while scanned_entries < query.limits.max_scan_entries()
                    && rows.len() < query.limits.max_rows()
                {
                    if let Some(member) = members.get(member_index) {
                        member_index += 1;
                        scanned_entries += 1;
                        let Some(id) = member.id.as_ref() else {
                            continue;
                        };
                        rows.push(BorrowedRow::Member {
                            root,
                            id,
                            kind: Some(member.shape.value_kind()),
                            edit_capability: direct.edit_capability(),
                            retired: false,
                        });
                    } else if let Some(tombstone) = tombstones.get(tombstone_index) {
                        tombstone_index += 1;
                        scanned_entries += 1;
                        let [ComponentMemberPathSegment::Member(id)] = tombstone.segments() else {
                            continue;
                        };
                        rows.push(BorrowedRow::Member {
                            root,
                            id,
                            kind: None,
                            edit_capability: false,
                            retired: true,
                        });
                    } else {
                        break;
                    }
                }
                let next = (member_index < members.len() || tombstone_index < tombstones.len())
                    .then_some(NextPosition::Members {
                        next_member: member_index,
                        next_tombstone: tombstone_index,
                    });
                (rows, next, scanned_entries)
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
        let rows = rows.into_iter().map(BorrowedRow::into_owned).collect();
        Ok(EditorAuthoringPage {
            target,
            rows,
            scanned_entries,
            metadata_bytes,
            next,
        })
    }

    /// Copies only the selected property after exact-basis, scan and aggregate copy admission.
    /// Missing/default interpretation remains with Scene/Prefab, not an Inspector or Runtime.
    pub fn read_document_property(
        &self,
        request: EditorPropertyRead,
    ) -> Result<EditorPropertyObservation, EditorDocumentQueryRejection> {
        let slot = self.resolve_document_query_slot(&request.target.basis, None)?;
        if request.target.property.steps.len() > nara_reflect::ComponentPropertyPath::MAX_STEPS {
            return Err(nara_scene::AuthoringReadError::Property(
                nara_reflect::ComponentPropertyError::PathDepthLimit,
            )
            .into());
        }
        let metadata_bytes = request.target.metadata_bytes();
        if metadata_bytes > request.limits.max_metadata_bytes() {
            return Err(EditorDocumentQueryRejection::MetadataBudgetExceeded);
        }
        let mut budget = ComponentPropertyReadBudget::new(
            ItemLimit::new(request.limits.max_scan_entries()).expect("validated scan limit"),
            ByteLimit::new(request.limits.max_scan_logical_bytes()).expect("validated scan limit"),
        );
        let root = entity_view(slot, &request.target.entity)?
            .component(&request.target.component)?
            .property_value(&request.target.property, &mut budget)?;
        let (value_nodes, value_logical_bytes) = match root {
            AuthoringPropertyValue::Present(value) | AuthoringPropertyValue::Defaulted(value) => {
                let cost = value
                    .cost_up_to(
                        request.limits.max_value_nodes(),
                        request.limits.max_logical_bytes(),
                    )
                    .ok_or(EditorDocumentQueryRejection::ValueBudgetExceeded)?;
                (cost.nodes(), cost.logical_bytes())
            }
            AuthoringPropertyValue::Missing => (0, 0),
        };
        let value = match root {
            AuthoringPropertyValue::Present(value) => EditorPropertyValue::Present(value.clone()),
            AuthoringPropertyValue::Defaulted(value) => {
                EditorPropertyValue::Defaulted(value.clone())
            }
            AuthoringPropertyValue::Missing => EditorPropertyValue::Missing,
        };
        Ok(EditorPropertyObservation {
            target: request.target,
            value,
            metadata_bytes,
            value_nodes,
            value_logical_bytes,
            scanned_entries: budget.scanned_entries(),
            scanned_logical_bytes: budget.scanned_logical_bytes(),
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
    Member {
        root: &'a ComponentFieldId,
        id: &'a ComponentFieldId,
        kind: Option<nara_reflect::ComponentValueKind>,
        edit_capability: bool,
        retired: bool,
    },
}

impl BorrowedRow<'_> {
    fn metadata_bytes(&self) -> usize {
        match self {
            Self::Component(fact) => fact.component().as_str().len(),
            Self::Root(field) => field.id().as_str().len(),
            Self::Member { root, id, .. } => root.as_str().len().saturating_add(id.as_str().len()),
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
            Self::Member {
                root,
                id,
                kind,
                edit_capability,
                retired,
            } => EditorAuthoringRow::Member {
                property: ComponentPropertyPath::new(root.clone())
                    .with_steps([ComponentPropertyStep::Member(id.clone())]),
                kind,
                edit_capability,
                status: if retired {
                    EditorAuthoringMemberStatus::Retired
                } else {
                    EditorAuthoringMemberStatus::Available
                },
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
    Members {
        next_member: usize,
        next_tombstone: usize,
    },
}

impl NextPosition<'_> {
    fn metadata_bytes(&self) -> usize {
        match self {
            Self::Components { after, .. } => after.as_str().len(),
            Self::Roots { .. } => 0,
            Self::Members { .. } => 0,
        }
    }
    fn into_owned(self) -> AuthoringPagePosition {
        match self {
            Self::Components { after, visited } => AuthoringPagePosition::Components {
                after: Some(after.clone()),
                visited,
            },
            Self::Roots { next_root } => AuthoringPagePosition::Roots { next_root },
            Self::Members {
                next_member,
                next_tombstone,
            } => AuthoringPagePosition::Members {
                next_member,
                next_tombstone,
            },
        }
    }
}
