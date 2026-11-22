use std::sync::Arc;

use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldSchema, ComponentMemberPathSegment,
    ComponentPropertyPath, ComponentPropertyStep, ComponentTypeId,
};
use nara_scene::{
    AuthoringComponentView, AuthoringEntityView, AuthoringPropertyChild,
    AuthoringPropertyChildrenLimits, AuthoringPropertyChildrenPosition, AuthoringPropertyValue,
    AuthoringReadError, AuthoringRecordFact, PrefabAuthoringSession, PrefabDeclarationError,
    PrefabInstancePath, PrefabPropertyClaims, PrefabPropertyIntent, PreparedPrefabAuthoringEdit,
    PreparedSceneAuthoringEdit, SceneAuthoringSession, SceneEntityId, SceneEntityRecord,
};

use super::{
    EditorDocumentSlot, EditorWorkspace, EditorWorkspaceCommandReport, WorkspaceDocumentContext,
    authoring_command_error_report, workspace_document_error_report, workspace_error_report,
};
use crate::{
    EditorAuthoringCursor, EditorAuthoringMemberStatus, EditorAuthoringPage,
    EditorAuthoringProjection, EditorAuthoringPropertySelector, EditorAuthoringPropertyStatus,
    EditorAuthoringQuery, EditorAuthoringQueryPosition, EditorAuthoringQueryTarget,
    EditorAuthoringRow, EditorDocumentKind, EditorDocumentQueryBasis, EditorDocumentQueryRejection,
    EditorEntityPage, EditorEntityQuery, EditorPrefabPropertyClaimTarget,
    EditorPropertyObservation, EditorPropertyPage, EditorPropertyPageRead, EditorPropertyRead,
    EditorPropertyReadLimits, EditorPropertyTarget, EditorPropertyValue,
    authoring_query::{
        AuthoringCursorPosition, AuthoringPagePosition, PrefabPropertyClaimSource,
        PropertyPagePosition,
    },
    document_query::EditorDocumentQueryOwner,
};

mod entity_page;
mod value_page;

/// A read-only view of the exact candidate kept by a prepared edit or construction.
///
/// Use [`Self::basis`] to start a query and this preparation to continue it. The candidate keeps
/// its original basis when the live document changes. Commit independently checks that basis.
#[derive(Clone, Copy)]
pub struct EditorPreparedDocumentPreview<'a> {
    basis: &'a EditorDocumentQueryBasis,
    owner: &'a EditorDocumentQueryOwner,
    source: AuthoringSource<'a>,
}

impl<'a> EditorPreparedDocumentPreview<'a> {
    pub(super) const fn new(
        basis: &'a EditorDocumentQueryBasis,
        owner: &'a EditorDocumentQueryOwner,
        source: AuthoringSource<'a>,
    ) -> Self {
        Self {
            basis,
            owner,
            source,
        }
    }

    /// The exact live document observation from which this candidate was prepared.
    #[must_use]
    pub const fn basis(&self) -> &'a EditorDocumentQueryBasis {
        self.basis
    }

    /// Enumerates direct entities, including additions and removals in the candidate.
    pub fn query_entities(
        &self,
        request: EditorEntityQuery,
    ) -> Result<EditorEntityPage, EditorDocumentQueryRejection> {
        AuthoringReadScope::Prepared(*self).query_entities(request)
    }

    /// Discovers direct component and property metadata or stored Prefab claims.
    pub fn query_authoring(
        &self,
        request: EditorAuthoringQuery,
    ) -> Result<EditorAuthoringPage, EditorDocumentQueryRejection> {
        AuthoringReadScope::Prepared(*self).query_authoring(request)
    }

    /// Copies one selected candidate property under the existing read limits.
    pub fn read_property(
        &self,
        request: EditorPropertyRead,
    ) -> Result<EditorPropertyObservation, EditorDocumentQueryRejection> {
        AuthoringReadScope::Prepared(*self).read_property(request)
    }

    /// Reads a bounded page of one candidate value. Continue on this same preparation.
    pub fn read_property_page(
        &self,
        request: EditorPropertyPageRead,
    ) -> Result<EditorPropertyPage, EditorDocumentQueryRejection> {
        AuthoringReadScope::Prepared(*self).read_property_page(request)
    }

    /// Reads the stored intent of a claim discovered in this candidate.
    pub fn read_prefab_property_claim(
        &self,
        claim: &EditorPrefabPropertyClaimTarget,
        limits: EditorPropertyReadLimits,
    ) -> Result<PrefabPropertyIntent, EditorDocumentQueryRejection> {
        AuthoringReadScope::Prepared(*self).read_prefab_property_claim(claim, limits)
    }
}

#[derive(Clone, Copy)]
pub(super) enum AuthoringReadScope<'a> {
    Live(&'a EditorWorkspace),
    Prepared(EditorPreparedDocumentPreview<'a>),
}

impl<'a> AuthoringReadScope<'a> {
    pub(super) fn owner(self) -> &'a EditorDocumentQueryOwner {
        match self {
            Self::Live(workspace) => &workspace.document_query_owner,
            Self::Prepared(preview) => preview.owner,
        }
    }

    pub(super) fn resolve(
        self,
        basis: &EditorDocumentQueryBasis,
        expected_kind: Option<EditorDocumentKind>,
    ) -> Result<AuthoringSource<'a>, EditorDocumentQueryRejection> {
        match self {
            Self::Live(workspace) => match workspace
                .resolve_document_query_slot(basis, expected_kind)?
            {
                EditorDocumentSlot::Scene(slot) => Ok(AuthoringSource::Scene(&slot.session)),
                EditorDocumentSlot::Prefab(slot) => Ok(AuthoringSource::Prefab(&slot.session)),
                slot @ EditorDocumentSlot::Extension(_) => {
                    Err(EditorDocumentQueryRejection::UnsupportedDocumentKind { kind: slot.kind() })
                }
            },
            Self::Prepared(preview) => {
                if basis.document != preview.basis.document {
                    return Err(EditorDocumentQueryRejection::WrongPreparedEdit);
                }
                basis.require_exact_source(preview.basis.revision, || {
                    preview.basis.schema.clone()
                })?;
                Ok(preview.source)
            }
        }
    }
}

pub(super) fn rejected_basis_report(
    workspace: &EditorWorkspace,
    basis: &EditorDocumentQueryBasis,
    rejection: EditorDocumentQueryRejection,
) -> EditorWorkspaceCommandReport {
    let context = WorkspaceDocumentContext::new(workspace.active_document, Some(basis.document));
    let (code, summary) = match rejection {
        EditorDocumentQueryRejection::DocumentNotOpen { document } => {
            return workspace_document_error_report(context, document);
        }
        EditorDocumentQueryRejection::UnsupportedDocumentKind { .. } => {
            return authoring_command_error_report(workspace.active_document, basis.document);
        }
        EditorDocumentQueryRejection::StaleIncarnation => (
            "tooling.workspace-stale-document-incarnation",
            "Authoring operation targets a different source incarnation",
        ),
        EditorDocumentQueryRejection::StaleRevision => (
            "tooling.workspace-stale-document-revision",
            "Authoring operation targets a stale document revision",
        ),
        EditorDocumentQueryRejection::StaleSchema => (
            "tooling.workspace-stale-schema-basis",
            "Authoring operation requires the exact current Schema admission",
        ),
        _ => (
            "tooling.workspace-authoring-basis-rejected",
            "Authoring operation requires a current Scene or Prefab document query basis",
        ),
    };
    workspace_error_report(context, code, summary)
}

impl EditorWorkspace {
    /// Checks a captured observation for read/result processing without refreshing it.
    /// Prepared-authoring entry points already require and validate this complete basis.
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
        basis.require_exact_source(slot.revision(), || slot.authoring_schema_basis())?;
        Ok(slot)
    }

    /// Discovers direct component/property metadata or stored Prefab property claims.
    /// Only collection selectors inspect the selected source's actual/defaulted identities.
    /// Entries without direct durable identity consume scan work without producing a row.
    /// Metadata is preflighted for the complete page and its continuation before any source IDs
    /// are copied; insufficient budgets reject terminally.
    pub fn query_document_authoring(
        &self,
        query: EditorAuthoringQuery,
    ) -> Result<EditorAuthoringPage, EditorDocumentQueryRejection> {
        AuthoringReadScope::Live(self).query_authoring(query)
    }

    /// Copies only the selected property after exact-basis, scan and aggregate copy admission.
    /// Missing/default interpretation remains with Scene/Prefab, not an Inspector or Runtime.
    pub fn read_document_property(
        &self,
        request: EditorPropertyRead,
    ) -> Result<EditorPropertyObservation, EditorDocumentQueryRejection> {
        AuthoringReadScope::Live(self).read_property(request)
    }
}

impl AuthoringReadScope<'_> {
    fn query_authoring(
        self,
        query: EditorAuthoringQuery,
    ) -> Result<EditorAuthoringPage, EditorDocumentQueryRejection> {
        let (target, expected_kind, position) = match query.position {
            EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                basis,
                entity,
                projection: EditorAuthoringProjection::PrefabPropertyClaims { steps },
            }) => {
                return self.query_prefab_property_claims(
                    Arc::new(PrefabPropertyClaimSource {
                        basis,
                        path: PrefabInstancePath {
                            anchor: entity,
                            steps,
                        },
                    }),
                    None,
                    0,
                    0,
                    query.limits,
                );
            }
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
                    EditorAuthoringProjection::Properties { .. } => {
                        AuthoringPagePosition::Properties {
                            position: PropertyPagePosition::Initial,
                        }
                    }
                    EditorAuthoringProjection::PrefabPropertyClaims { .. } => unreachable!(),
                };
                (Arc::new(target), None, position)
            }
            EditorAuthoringQueryPosition::Continue(cursor) => {
                self.owner().require(&cursor.owner)?;
                match cursor.position {
                    AuthoringCursorPosition::Direct { target, next } => {
                        (target, Some(cursor.kind), next)
                    }
                    AuthoringCursorPosition::PrefabClaims {
                        source,
                        component_index,
                        claim_index,
                    } => {
                        return self.query_prefab_property_claims(
                            source,
                            Some(cursor.kind),
                            component_index,
                            claim_index,
                            query.limits,
                        );
                    }
                }
            }
        };
        let source = self.resolve(&target.basis, expected_kind)?;
        let target_bytes = target.metadata_bytes();
        if target_bytes > query.limits.max_metadata_bytes() {
            return Err(EditorDocumentQueryRejection::MetadataBudgetExceeded);
        }
        if let EditorAuthoringProjection::Properties { property, .. } = &target.projection
            && property.steps.len() > ComponentPropertyPath::MAX_STEPS
        {
            return Err(nara_scene::AuthoringReadError::Property(
                nara_reflect::ComponentPropertyError::PathDepthLimit,
            )
            .into());
        }
        let entity = source.entity_view(&target.entity)?;
        let count = query.limits.max_scan_entries().min(query.limits.max_rows());
        let mut scanned_logical_bytes = 0;
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
            (
                EditorAuthoringProjection::Properties {
                    component,
                    property,
                },
                AuthoringPagePosition::Properties { position },
            ) => {
                let page = entity
                    .component(component)?
                    .property_children(
                        property,
                        to_scene_property_position(&position)?,
                        AuthoringPropertyChildrenLimits {
                            max_scan_entries: query.limits.max_scan_entries(),
                            max_rows: query.limits.max_rows(),
                            max_scan_logical_bytes: query.limits.max_scan_logical_bytes(),
                            max_child_metadata_bytes: query.limits.max_metadata_bytes()
                                - target_bytes,
                        },
                    )
                    .map_err(|error| match error {
                        AuthoringReadError::ChildMetadataBudgetExceeded => {
                            EditorDocumentQueryRejection::MetadataBudgetExceeded
                        }
                        error => error.into(),
                    })?;
                scanned_logical_bytes = page.scanned_logical_bytes;
                let rows = page
                    .children
                    .into_iter()
                    .map(|child| BorrowedRow::Property {
                        parent: property,
                        child,
                    })
                    .collect();
                let next = page
                    .next
                    .map(|continuation| NextPosition::Properties { continuation });
                (rows, next, page.scanned_entries)
            }
            _ => return Err(EditorDocumentQueryRejection::InvalidContinuation),
        };
        let mut metadata_bytes = target_bytes;
        for row in &rows {
            metadata_bytes = metadata_bytes.saturating_add(row.metadata_bytes());
        }
        if let Some(next) = &next {
            metadata_bytes = metadata_bytes.saturating_add(next.metadata_bytes());
        }
        if metadata_bytes > query.limits.max_metadata_bytes() {
            return Err(EditorDocumentQueryRejection::MetadataBudgetExceeded);
        }
        let rows: Vec<_> = rows.into_iter().map(BorrowedRow::into_owned).collect();
        let next = next
            .map(|position| position.into_owned(rows.last()))
            .transpose()?
            .map(|next| EditorAuthoringCursor {
                owner: self.owner().clone(),
                kind: source.kind(),
                position: AuthoringCursorPosition::Direct {
                    target: Arc::clone(&target),
                    next,
                },
            });
        Ok(EditorAuthoringPage {
            target,
            rows,
            scanned_entries,
            scanned_logical_bytes,
            metadata_bytes,
            next,
        })
    }

    fn read_property(
        self,
        request: EditorPropertyRead,
    ) -> Result<EditorPropertyObservation, EditorDocumentQueryRejection> {
        let source = self.resolve(&request.target.basis, None)?;
        let metadata_bytes = request
            .target
            .admitted_metadata_bytes(request.limits.max_metadata_bytes())?;
        let mut budget = request.limits.scan_budget();
        let root = source
            .component(&request.target)?
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

#[derive(Clone, Copy)]
pub(super) enum AuthoringSource<'a> {
    Scene(&'a SceneAuthoringSession),
    Prefab(&'a PrefabAuthoringSession),
    PreparedScene(&'a PreparedSceneAuthoringEdit),
    PreparedPrefab(&'a PreparedPrefabAuthoringEdit),
}

impl<'a> AuthoringSource<'a> {
    pub(super) fn kind(self) -> EditorDocumentKind {
        match self {
            Self::Scene(_) | Self::PreparedScene(_) => EditorDocumentKind::Scene,
            Self::Prefab(_) | Self::PreparedPrefab(_) => EditorDocumentKind::Prefab,
        }
    }

    fn entities(self) -> &'a [SceneEntityRecord] {
        match self {
            Self::Scene(session) => &session.document().entities,
            Self::Prefab(session) => &session.document().entities,
            Self::PreparedScene(edit) => &edit.document().entities,
            Self::PreparedPrefab(edit) => &edit.document().entities,
        }
    }

    fn entity_view(
        self,
        entity: &SceneEntityId,
    ) -> Result<AuthoringEntityView<'a>, EditorDocumentQueryRejection> {
        let result = match self {
            Self::Scene(session) => session.authoring_entity_view(entity),
            Self::Prefab(session) => session.authoring_entity_view(entity),
            Self::PreparedScene(edit) => edit.authoring_entity_view(entity),
            Self::PreparedPrefab(edit) => edit.authoring_entity_view(entity),
        };
        Ok(result?)
    }

    fn component(
        self,
        target: &EditorPropertyTarget,
    ) -> Result<AuthoringComponentView<'a>, EditorDocumentQueryRejection> {
        Ok(self
            .entity_view(&target.entity)?
            .component(&target.component)?)
    }

    pub(super) fn prefab_property_claims(
        self,
        path: &PrefabInstancePath,
    ) -> Result<PrefabPropertyClaims<'a>, PrefabDeclarationError> {
        let PrefabInstancePath { anchor, steps } = path;
        match self {
            Self::Scene(session) => session.prefab_property_claims(anchor, steps),
            Self::Prefab(session) => session.prefab_property_claims(anchor, steps),
            Self::PreparedScene(edit) => edit.prefab_property_claims(anchor, steps),
            Self::PreparedPrefab(edit) => edit.prefab_property_claims(anchor, steps),
        }
    }
}

fn to_scene_property_position(
    position: &PropertyPagePosition,
) -> Result<AuthoringPropertyChildrenPosition<'_>, EditorDocumentQueryRejection> {
    Ok(match position {
        PropertyPagePosition::Initial => AuthoringPropertyChildrenPosition::Initial,
        PropertyPagePosition::Record {
            next_member,
            next_tombstone,
        } => AuthoringPropertyChildrenPosition::Record {
            next_member: *next_member,
            next_tombstone: *next_tombstone,
        },
        PropertyPagePosition::Map { after } => {
            let ComponentPropertyStep::MapEntry(key) = after.as_ref() else {
                return Err(EditorDocumentQueryRejection::InvalidContinuation);
            };
            AuthoringPropertyChildrenPosition::Map { after: Some(key) }
        }
        PropertyPagePosition::KeyedList { next_item } => {
            AuthoringPropertyChildrenPosition::KeyedList {
                next_item: *next_item,
            }
        }
    })
}

fn from_scene_property_position(
    position: AuthoringPropertyChildrenPosition<'_>,
    last_row: Option<&EditorAuthoringRow>,
) -> Result<PropertyPagePosition, EditorDocumentQueryRejection> {
    Ok(match position {
        AuthoringPropertyChildrenPosition::Record {
            next_member,
            next_tombstone,
        } => PropertyPagePosition::Record {
            next_member,
            next_tombstone,
        },
        AuthoringPropertyChildrenPosition::Map { after: Some(_) } => {
            let Some(EditorAuthoringRow::Property { property, .. }) = last_row else {
                return Err(EditorDocumentQueryRejection::InvalidContinuation);
            };
            PropertyPagePosition::Map {
                after: Arc::clone(&property.step),
            }
        }
        AuthoringPropertyChildrenPosition::KeyedList { next_item } => {
            PropertyPagePosition::KeyedList { next_item }
        }
        _ => return Err(EditorDocumentQueryRejection::InvalidContinuation),
    })
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
    Property {
        parent: &'a Arc<ComponentPropertyPath>,
        child: AuthoringPropertyChild<'a>,
    },
}

impl BorrowedRow<'_> {
    fn metadata_bytes(&self) -> usize {
        match self {
            Self::Component(fact) => fact.component().as_str().len(),
            Self::Root(field) => field.id().as_str().len(),
            Self::Member { root, id, .. } => root.as_str().len().saturating_add(id.as_str().len()),
            Self::Property { child, .. } => child.metadata_bytes(),
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
            Self::Property { parent, child } => EditorAuthoringRow::Property {
                property: EditorAuthoringPropertySelector {
                    parent: Arc::clone(parent),
                    step: Arc::new(child.to_step()),
                },
                kind: child.kind,
                edit_capability: child.edit_capability,
                status: if child.retired {
                    EditorAuthoringPropertyStatus::Retired
                } else {
                    EditorAuthoringPropertyStatus::Available
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
    Properties {
        continuation: AuthoringPropertyChildrenPosition<'a>,
    },
}

impl NextPosition<'_> {
    fn metadata_bytes(&self) -> usize {
        match self {
            Self::Components { after, .. } => after.as_str().len(),
            Self::Roots { .. } | Self::Members { .. } | Self::Properties { .. } => 0,
        }
    }
    fn into_owned(
        self,
        last_row: Option<&EditorAuthoringRow>,
    ) -> Result<AuthoringPagePosition, EditorDocumentQueryRejection> {
        Ok(match self {
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
            Self::Properties { continuation } => AuthoringPagePosition::Properties {
                position: from_scene_property_position(continuation, last_row)?,
            },
        })
    }
}
