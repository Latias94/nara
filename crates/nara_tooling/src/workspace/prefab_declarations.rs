//! Selection-independent access to declarations owned by an exact source session.

use std::sync::Arc;

use nara_reflect::ComponentPropertyPath;
use nara_scene::{
    PrefabDeclarationError, PrefabDeclarationLimits, PrefabInstanceObservation, PrefabInstancePath,
    PrefabOverlaySlot, PrefabPropertyIntent, PrefabRevert,
};

use super::{
    EditorDocumentSlot, EditorWorkspace, EditorWorkspaceCommandReport,
    authoring_query::{AuthoringReadScope, AuthoringSource, rejected_basis_report},
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

impl<'a> AuthoringReadScope<'a> {
    pub(super) fn query_prefab_property_claims(
        self,
        source: Arc<PrefabPropertyClaimSource>,
        expected_kind: Option<EditorDocumentKind>,
        mut component_index: usize,
        mut claim_index: usize,
        limits: EditorAuthoringQueryLimits,
    ) -> Result<EditorAuthoringPage, EditorDocumentQueryRejection> {
        let document = self.resolve(&source.basis, expected_kind)?;
        let path_bytes = instance_path_bytes(&source.path);
        let max_metadata = limits.max_metadata_bytes();
        if path_bytes > max_metadata {
            return Err(EditorDocumentQueryRejection::MetadataBudgetExceeded);
        }
        let claims = document.prefab_property_claims(&source.path)?;
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
            owner: self.owner().clone(),
            kind: document.kind(),
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
                    owner: self.owner().clone(),
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
            target: Arc::new(EditorAuthoringQueryTarget {
                basis: source.basis.clone(),
                entity: source.path.anchor.clone(),
                projection: EditorAuthoringProjection::PrefabPropertyClaims {
                    steps: source.path.steps.clone(),
                },
            }),
            rows,
            scanned_entries,
            scanned_logical_bytes,
            metadata_bytes,
            next,
        })
    }

    pub(super) fn read_prefab_property_claim(
        self,
        claim: &EditorPrefabPropertyClaimTarget,
        limits: EditorPropertyReadLimits,
    ) -> Result<PrefabPropertyIntent, EditorDocumentQueryRejection> {
        let source = self.resolve_prefab_property_claim(claim)?;
        let path = claim.path();
        if instance_path_bytes(path) > limits.max_metadata_bytes() {
            return Err(EditorDocumentQueryRejection::MetadataBudgetExceeded);
        }
        let intent = source.prefab_property_claims(path)?.intent(claim.slot())?;
        if let PrefabPropertyIntent::Set(value) = intent
            && value
                .cost_up_to(limits.max_value_nodes(), limits.max_logical_bytes())
                .is_none()
        {
            return Err(EditorDocumentQueryRejection::ValueBudgetExceeded);
        }
        Ok(intent.clone())
    }

    fn resolve_prefab_property_claim(
        self,
        claim: &EditorPrefabPropertyClaimTarget,
    ) -> Result<AuthoringSource<'a>, EditorDocumentQueryRejection> {
        self.owner().require(&claim.owner)?;
        self.resolve(claim.basis(), None)
    }
}

impl EditorWorkspace {
    /// Copies one stored property intent from the original live claim target.
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
        AuthoringReadScope::Live(self).read_prefab_property_claim(claim, limits)
    }

    /// Resolves the original live claim and passes its source borrow to ordinary Revert
    /// preparation. A prepared preview's target has a different query owner.
    pub fn prepare_prefab_property_revert(
        &self,
        claim: &EditorPrefabPropertyClaimTarget,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedEditorDocumentEdit, EditorWorkspaceCommandReport> {
        let declaration = AuthoringReadScope::Live(self)
            .resolve_prefab_property_claim(claim)
            .and_then(|source| {
                source
                    .prefab_property_claims(claim.path())?
                    .claim_at(claim.slot())
                    .ok_or_else(|| PrefabDeclarationError::SlotNotFound.into())
            })
            .map_err(|error| rejected_basis_report(self, claim.basis(), error))?;
        self.prepare_prefab_revert(
            claim.basis(),
            claim.path(),
            PrefabRevert::Property(declaration),
            limits,
        )
    }

    /// Checks a retained claim before a Host prepares an edit or processes its result.
    pub fn validate_prefab_property_claim_target(
        &self,
        claim: &EditorPrefabPropertyClaimTarget,
    ) -> Result<(), EditorDocumentQueryRejection> {
        AuthoringReadScope::Live(self)
            .resolve_prefab_property_claim(claim)
            .map(|_| ())
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
