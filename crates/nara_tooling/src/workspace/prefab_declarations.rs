//! Selection-independent access to declarations owned by an exact source session.

use std::sync::Arc;

use nara_reflect::ComponentPropertyPath;
use nara_scene::{
    PrefabDeclarationError, PrefabDeclarationLimits, PrefabInstanceObservation, PrefabInstancePath,
    PrefabOverlaySlot, PrefabPropertyClaims, PrefabPropertyIntent, PrefabRevert,
};

use super::{
    EditorDocumentSlot, EditorWorkspace, EditorWorkspaceCommandReport,
    authoring_query::rejected_basis_report,
};
use crate::{
    EditorAuthoringCursor, EditorAuthoringPage, EditorAuthoringProjection,
    EditorAuthoringQueryLimits, EditorAuthoringQueryTarget, EditorAuthoringRow, EditorDocumentKind,
    EditorDocumentQueryBasis, EditorDocumentQueryRejection, EditorPrefabPropertyClaimTarget,
    EditorPropertyReadLimits, MAX_EDITOR_AUTHORING_QUERY_METADATA_BYTES,
    PreparedEditorDocumentEdit,
    authoring_query::{
        AuthoringCursorPosition, PrefabPropertyClaimSource, prefab_path_step_bytes,
        property_metadata_bytes_up_to,
    },
};

impl EditorWorkspace {
    pub(super) fn query_prefab_property_claims(
        &self,
        source: Arc<PrefabPropertyClaimSource>,
        expected_kind: Option<EditorDocumentKind>,
        mut component_index: usize,
        mut claim_index: usize,
        limits: EditorAuthoringQueryLimits,
    ) -> Result<EditorAuthoringPage, EditorDocumentQueryRejection> {
        let slot = self.resolve_document_query_slot(&source.basis, expected_kind)?;
        let path_bytes = instance_path_bytes(&source.path);
        let max_metadata = limits.max_metadata_bytes();
        if path_bytes > max_metadata {
            return Err(EditorDocumentQueryRejection::MetadataBudgetExceeded);
        }
        let claims = prefab_property_claims(slot, &source.path)?;
        let count = claims.component_intent_count();
        let retained_path_bytes = path_bytes * 2;
        let header_cap =
            MAX_EDITOR_AUTHORING_QUERY_METADATA_BYTES.saturating_sub(retained_path_bytes);
        let mut metadata_bytes = retained_path_bytes;
        let mut scanned_entries = 0;
        let mut scanned_logical_bytes = 0;
        let mut rows = Vec::new();
        while component_index < count
            && scanned_entries < limits.max_scan_entries()
            && scanned_logical_bytes < limits.max_scan_logical_bytes()
            && rows.len() < limits.max_rows()
        {
            scanned_entries += 1;
            let Some((component, claim)) =
                claims.component(component_index).and_then(|component| {
                    component
                        .claims
                        .get(claim_index)
                        .map(|claim| (component, claim))
                })
            else {
                component_index += 1;
                claim_index = 0;
                continue;
            };
            let remaining_scan = limits.max_scan_logical_bytes() - scanned_logical_bytes;
            let allowance = remaining_scan.min(header_cap);
            let bytes = if claim.path.steps.len() > ComponentPropertyPath::MAX_STEPS {
                None
            } else {
                let fixed = component.component.as_str().len();
                allowance.checked_sub(fixed).and_then(|remaining| {
                    let property = property_metadata_bytes_up_to(&claim.path, remaining)?;
                    let route = component.target.logical_bytes_up_to(remaining - property)?;
                    Some(fixed + property + route)
                })
            };
            let header = match bytes {
                Some(bytes) => {
                    metadata_bytes = metadata_bytes.saturating_add(bytes);
                    scanned_logical_bytes += bytes;
                    Some((component, claim))
                }
                None if claim.path.steps.len() > ComponentPropertyPath::MAX_STEPS => None,
                None if remaining_scan >= header_cap => {
                    scanned_logical_bytes += header_cap;
                    None
                }
                None if scanned_entries > 1 => {
                    scanned_logical_bytes = limits.max_scan_logical_bytes();
                    break;
                }
                None => return Err(PrefabDeclarationError::BudgetExceeded.into()),
            };
            rows.push((
                PrefabOverlaySlot::Property {
                    component_index,
                    claim_index,
                },
                header,
            ));
            claim_index += 1;
            if claim_index == component.claims.len() {
                component_index += 1;
                claim_index = 0;
            }
        }
        let has_next = component_index < count;
        if !has_next && rows.is_empty() {
            metadata_bytes = path_bytes;
        }
        if metadata_bytes > max_metadata {
            return Err(EditorDocumentQueryRejection::MetadataBudgetExceeded);
        }
        let next = has_next.then(|| EditorAuthoringCursor {
            owner: self.document_query_owner.clone(),
            kind: slot.kind(),
            position: AuthoringCursorPosition::PrefabClaims {
                source: Arc::clone(&source),
                component_index,
                claim_index,
            },
        });
        let rows = rows
            .into_iter()
            .map(|(slot, header)| {
                let claim = EditorPrefabPropertyClaimTarget {
                    owner: self.document_query_owner.clone(),
                    source: Arc::clone(&source),
                    slot,
                };
                match header {
                    Some((component, property)) => EditorAuthoringRow::PrefabPropertyClaim {
                        claim,
                        target: component.target.clone(),
                        component: component.component.clone(),
                        version: component.version,
                        property: property.path.clone(),
                        has_value: matches!(property.intent, PrefabPropertyIntent::Set(_)),
                    },
                    None => EditorAuthoringRow::OversizedPrefabPropertyClaim { claim },
                }
            })
            .collect();
        Ok(EditorAuthoringPage {
            target: EditorAuthoringQueryTarget {
                basis: source.basis.clone(),
                entity: source.path.anchor.clone(),
                projection: EditorAuthoringProjection::PrefabPropertyClaims {
                    steps: source.path.steps.clone(),
                },
            },
            rows,
            scanned_entries,
            scanned_logical_bytes,
            metadata_bytes,
            next,
        })
    }

    /// Copies one stored property intent from the original claim target.
    ///
    /// A property slot from `PrefabPropertyClaims` is an indexed lookup. Metadata limits apply
    /// to the path and value limits apply to this Set payload. Absent has no payload. The read
    /// preserves raw intent even when the base or Schema interpretation is unavailable. Indexed
    /// path steps have a separate source depth cap and do not consume selector scan limits.
    pub fn read_prefab_property_claim(
        &self,
        claim: &EditorPrefabPropertyClaimTarget,
        limits: EditorPropertyReadLimits,
    ) -> Result<PrefabPropertyIntent, EditorDocumentQueryRejection> {
        let source = self.resolve_prefab_property_claim_slot(claim)?;
        let path = claim.path();
        if instance_path_bytes(path) > limits.max_metadata_bytes() {
            return Err(EditorDocumentQueryRejection::MetadataBudgetExceeded);
        }
        let intent = prefab_property_claims(source, path)?.intent(claim.slot())?;
        if let PrefabPropertyIntent::Set(value) = intent
            && value
                .cost_up_to(limits.max_value_nodes(), limits.max_logical_bytes())
                .is_none()
        {
            return Err(EditorDocumentQueryRejection::ValueBudgetExceeded);
        }
        Ok(intent.clone())
    }

    /// Retains the discovered claim's exact source identity through ordinary Revert preparation.
    pub fn prepare_prefab_property_revert(
        &self,
        claim: &EditorPrefabPropertyClaimTarget,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedEditorDocumentEdit, EditorWorkspaceCommandReport> {
        if claim.owner != self.document_query_owner {
            return Err(rejected_basis_report(
                self,
                claim.basis(),
                EditorDocumentQueryRejection::WrongWorkspace,
            ));
        }
        self.prepare_prefab_revert(
            claim.basis(),
            claim.path(),
            PrefabRevert::Intent(claim.slot()),
            limits,
        )
    }

    /// Checks a retained claim before a Host prepares an edit or processes its result.
    pub fn validate_prefab_property_claim_target(
        &self,
        claim: &EditorPrefabPropertyClaimTarget,
    ) -> Result<(), EditorDocumentQueryRejection> {
        self.resolve_prefab_property_claim_slot(claim).map(|_| ())
    }

    fn resolve_prefab_property_claim_slot(
        &self,
        claim: &EditorPrefabPropertyClaimTarget,
    ) -> Result<&EditorDocumentSlot, EditorDocumentQueryRejection> {
        if claim.owner != self.document_query_owner {
            return Err(EditorDocumentQueryRejection::WrongWorkspace);
        }
        self.resolve_document_query_slot(claim.basis(), None)
    }

    /// Observes an optional source declaration without changing navigation or selection.
    ///
    /// The exact query basis is checked before any selector or payload is read. The source
    /// session owns bounded copying; this does not resolve inherited entities or Prefab sources.
    pub fn observe_prefab_instance(
        &self,
        basis: &EditorDocumentQueryBasis,
        path: &PrefabInstancePath,
        limits: PrefabDeclarationLimits,
    ) -> Result<PrefabInstanceObservation, EditorDocumentQueryRejection> {
        let slot = self.resolve_document_query_slot(basis, None)?;
        let observation = match slot {
            EditorDocumentSlot::Scene(slot) => slot.session.observe_prefab_instance(path, limits),
            EditorDocumentSlot::Prefab(slot) => slot.session.observe_prefab_instance(path, limits),
            EditorDocumentSlot::Extension(_) => {
                return Err(EditorDocumentQueryRejection::UnsupportedDocumentKind {
                    kind: slot.kind(),
                });
            }
        };
        observation.map_err(EditorDocumentQueryRejection::PrefabDeclaration)
    }
}

fn instance_path_bytes(path: &PrefabInstancePath) -> usize {
    path.anchor
        .as_str()
        .len()
        .saturating_add(prefab_path_step_bytes(&path.steps))
}

fn prefab_property_claims<'a>(
    slot: &'a EditorDocumentSlot,
    path: &PrefabInstancePath,
) -> Result<PrefabPropertyClaims<'a>, EditorDocumentQueryRejection> {
    match slot {
        EditorDocumentSlot::Scene(slot) => Ok(slot
            .session
            .prefab_property_claims(&path.anchor, &path.steps)?),
        EditorDocumentSlot::Prefab(slot) => Ok(slot
            .session
            .prefab_property_claims(&path.anchor, &path.steps)?),
        EditorDocumentSlot::Extension(_) => {
            Err(EditorDocumentQueryRejection::UnsupportedDocumentKind { kind: slot.kind() })
        }
    }
}
