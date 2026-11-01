//! Data-only verification of a stored authored upgrade pair after process interruption.
//!
//! The caller admits the old/new definitions and the decoded input owners. Classification here
//! is a fresh data observation, never a recovered Session, history or persistence checkpoint.

use super::*;

/// Verifies a stored Scene transition through the same complete-source kernels as live review.
///
/// The Host must supply the actual source and publication policy, and admit its decoded inputs
/// and old/new Schema definitions before calling. This verifies the stored pair; it executes no
/// semantic conversion and creates no Session, history or saved token. Out-of-scope unavailable
/// records remain lossless, while affected declarations must be valid on both sides.
///
/// All classification, validation and publication scratch uses the supplied cumulative `work`.
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
        admit_original(&original.roots, &original.entities, limits, work)?;
        let state = classify_original(
            original,
            source,
            AuthoringDocumentKind::Scene,
            upgrade,
            limits,
            work,
        )?;
        let next =
            state.prepare_authored_upgrade(original, successor, upgrade, &mut |stage, bytes| {
                work.reserve(stage, bytes)
            })?;
        require_successor(&state, &next, upgrade)
    })();
    work.budget.end_step();
    result
}

/// Nominal Prefab counterpart to [`validate_authored_upgrade_scene_transition`].
///
/// Both classifications retain Prefab kind and the actual source/policy. The incumbent
/// classifier needs a Scene-shaped input: its complete projection is admitted before copying,
/// and its lifetime remains inside this operation's scratch. Live successor preparation owns
/// the same pre-allocation admission for its projection. No projection becomes a source owner.
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
        admit_original(&original.roots, &original.entities, limits, work)?;
        work.reserve(
            "authored-upgrade-recovery-projection",
            Some(size_of::<SceneDocument>()),
        )?;
        let projection = original.instantiate();
        let state = classify_original(
            &projection,
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
            &mut |stage, bytes| work.reserve(stage, bytes),
        )?;
        require_successor(&state, &next, upgrade)
    })();
    work.budget.end_step();
    result
}

fn admit_original(
    roots: &[SceneEntityId],
    entities: &[SceneEntityRecord],
    limits: SceneFileLimits,
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<(), ScenePatchReport> {
    // Bound the complete dormant/raw tree before the recursive classifier. This also admits
    // the full dynamic copy for a Prefab projection before instantiate allocates anything.
    crate::authoring_decode::schema_upgrade::admit_authored_upgrade_document(
        roots,
        entities,
        limits,
        &mut |stage, bytes| work.reserve(stage, bytes),
    )
}

fn classify_original(
    original: &SceneDocument,
    source: AuthoringDocumentSource,
    kind: AuthoringDocumentKind,
    upgrade: &AuthoredFieldUpgrade,
    limits: SceneFileLimits,
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<AuthoringDocumentState, ScenePatchReport> {
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
        &mut |stage, bytes| work.reserve(stage, bytes),
    )
}

#[cfg(test)]
mod tests;
