//! Data-only verification of a stored authored upgrade pair after process interruption.
//!
//! The caller admits the old/new definitions and the decoded input owners. Classification here
//! is a fresh data observation, never a recovered Session, history or persistence checkpoint.

use super::*;

/// Verifies a stored Scene transition through the same complete-source kernels as live review.
///
/// The Host must first decode the actual stored v1 bytes with the selected wire encoding and
/// file limits and admit the old and new Schema definitions. This verifies the decoded pair
/// and leaves its original bytes available for publication. Out-of-scope unavailable records
/// remain lossless, while affected declarations must be valid on both sides.
///
/// Classification and validation scratch use the supplied cumulative `work`.
/// No classified state escapes; scratch ends only after all temporary owners have dropped.
pub fn validate_authored_upgrade_scene_transition(
    original: &SceneDocument,
    successor: &SceneDocument,
    source: AuthoringDocumentSource,
    upgrade: &AuthoredFieldUpgrade,
    limits: SceneFileLimits,
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<(), ScenePatchReport> {
    work.begin_step()?;
    let result = (|| {
        let state = classify_original(
            original.into(),
            source,
            AuthoringDocumentKind::Scene,
            upgrade,
            limits,
            work,
        )?;
        let next = state.prepare_authored_upgrade(
            original,
            successor,
            upgrade,
            AuthoredUpgradeOutput::DecodedStored,
            &mut |stage, bytes| work.reserve(stage, bytes),
        )?;
        require_successor(&state, &next, upgrade)
    })();
    work.budget.end_step();
    result
}

/// Nominal Prefab counterpart to [`validate_authored_upgrade_scene_transition`].
///
/// Both classifications borrow the original Prefab declarations and inputs and use the file
/// limits supplied for the actual stored documents.
pub fn validate_authored_upgrade_prefab_transition(
    original: &PrefabDocument,
    successor: &PrefabDocument,
    source: AuthoringDocumentSource,
    upgrade: &AuthoredFieldUpgrade,
    limits: SceneFileLimits,
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<(), ScenePatchReport> {
    work.begin_step()?;
    let result = (|| {
        let state = classify_original(
            original.into(),
            source,
            AuthoringDocumentKind::Prefab,
            upgrade,
            limits,
            work,
        )?;
        let next = state.prepare_authored_prefab_upgrade(
            original,
            successor,
            upgrade,
            AuthoredUpgradeOutput::DecodedStored,
            &mut |stage, bytes| work.reserve(stage, bytes),
        )?;
        require_successor(&state, &next, upgrade)
    })();
    work.budget.end_step();
    result
}

fn classify_original(
    original: AuthoringSourceParts<'_>,
    source: AuthoringDocumentSource,
    kind: AuthoringDocumentKind,
    upgrade: &AuthoredFieldUpgrade,
    limits: SceneFileLimits,
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<AuthoringDocumentState, ScenePatchReport> {
    crate::authoring_decode::schema_upgrade::admit_authored_upgrade_document(
        original.placements,
        original.entities,
        limits,
        &mut |stage, bytes| work.reserve(stage, bytes),
    )?;
    work.reserve(
        "classification-source",
        crate::provenance::provenance_asset_ref_bytes(source.as_asset_ref())
            .and_then(|bytes| bytes.checked_add(size_of::<AuthoringDocumentState>())),
    )?;
    AuthoringDocumentState::classify_authored_upgrade_source(
        original,
        source,
        kind,
        upgrade.previous_basis(),
        limits,
        None,
        &mut |stage, bytes| work.reserve(stage, bytes),
    )
}

#[cfg(test)]
mod tests;
