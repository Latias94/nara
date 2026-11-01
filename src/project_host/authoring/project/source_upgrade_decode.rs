//! Bounded wire decoding shared by live preparation and explicit process recovery.

use super::*;
use crate::project_host::authoring::source_upgrade_recovery::{
    RecoveryMember, RecoverySourceEncoding,
};
use nara_scene::SceneDocumentCandidate;

pub(super) enum SourceTree {
    Scene(SceneDocument),
    Prefab(PrefabDocument),
}

impl SourceTree {
    pub(super) fn retained_bytes(&self) -> Result<usize, SourceUpgradeDecodeError> {
        match self {
            Self::Scene(document) => document.retained_bytes(),
            Self::Prefab(document) => document.retained_bytes(),
        }
        .map_err(SourceUpgradeDecodeError::Project)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SourceUpgradeDecodeError {
    #[error("source decoding exceeded the shared Host allowance")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("source decoding exceeded its admitted allowance")]
    Project(#[from] AuthoringProjectSourceAdmissionError),
    #[error("source wire decoding failed")]
    Format(#[from] SceneFormatError),
    #[error("source structure is invalid")]
    Structure(#[source] SceneFilePublicationError),
}

pub(super) fn release_tree(
    tree: SourceTree,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradeDecodeError> {
    let bytes = tree.retained_bytes()?;
    drop(tree);
    budget.reconcile_retained_reservation(bytes, 0)?;
    Ok(())
}

pub(super) fn decode_source(
    member: &RecoveryMember,
    bytes: &[u8],
    limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<SourceTree, SourceUpgradeDecodeError> {
    let before = budget.retained_candidate_bytes;
    let result = (|| {
        let reservation =
            reserve_source_decode(bytes.len(), limits.encoded_bytes().get(), 0, limits, budget)?;
        let tree = match (member.key.kind(), member.encoding) {
            (nara_scene::SourceDocumentKind::Scene, RecoverySourceEncoding::Json) => {
                SceneDocumentCandidate::decode_json_bytes_with_limits(bytes, reservation.limits)?
                    .into_composition_source()
                    .map(SourceTree::Scene)
            }
            (nara_scene::SourceDocumentKind::Scene, RecoverySourceEncoding::Ron) => {
                SceneDocumentCandidate::decode_ron_bytes_with_limits(bytes, reservation.limits)?
                    .into_composition_source()
                    .map(SourceTree::Scene)
            }
            (nara_scene::SourceDocumentKind::Prefab, RecoverySourceEncoding::Json) => {
                PrefabDocumentCandidate::decode_json_bytes_with_limits(bytes, reservation.limits)?
                    .into_composition_source()
                    .map(SourceTree::Prefab)
            }
            (nara_scene::SourceDocumentKind::Prefab, RecoverySourceEncoding::Ron) => {
                PrefabDocumentCandidate::decode_ron_bytes_with_limits(bytes, reservation.limits)?
                    .into_composition_source()
                    .map(SourceTree::Prefab)
            }
        }
        .map_err(SourceUpgradeDecodeError::Structure)?;
        let retained = tree.retained_bytes()?;
        budget.reconcile_retained_reservation(reservation.bytes, retained)?;
        budget.charge_source_bytes(0, retained, retained, 0)?;
        Ok(tree)
    })();
    if result.is_err() {
        // Every attempted parser/tree local is gone. Keep cumulative work, not dropped storage.
        let temporary = budget
            .retained_candidate_bytes
            .checked_sub(before)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        budget.reconcile_retained_reservation(temporary, 0)?;
    }
    result
}
