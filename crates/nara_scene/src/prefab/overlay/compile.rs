//! Instance selection and layered compilation in the existing Scene expansion owner.

use super::{
    PrefabComponentIntent, PrefabComponentSetMode, PrefabEntityOrigin, PrefabEntityTarget,
    PrefabInstanceHop, PrefabOverlay, PrefabOverlayConflict, PrefabPlacementParent,
    PrefabPlacementSlot, layout,
};
use crate::{
    PrefabInstance, PrefabProjectionPath, PrefabProjectionStep, ResolvedPrefabSource,
    SceneDocument, SceneEntityId, SceneEntityProvenance, SceneEntityRecord, SourceDocumentKey,
    SourceDocumentKind,
    diagnostics::{
        error as diagnostic_error, usize_to_u64, with_asset_ref, with_public_locator,
        with_public_u64,
    },
    prefab::{PrefabExpansionBudgetKind, PrefabExpansionContext},
    provenance::provenance_entry_bytes,
    validation::validate_scene_structure,
};

#[cfg(test)]
#[path = "compile_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "compile/component_program_tests.rs"]
mod component_program_tests;

mod staged;

pub(crate) fn expand_document<F, E>(
    context: &mut PrefabExpansionContext<'_, F, E>,
    document: &SceneDocument,
) -> Result<SceneDocument, PrefabOverlayConflict>
where
    F: FnMut(
        &nara_asset::AssetRef,
        &mut crate::prefab::PrefabExpansionWorkingSet,
    ) -> Result<Option<ResolvedPrefabSource>, E>,
{
    staged::expand(context, document)
}

/// Source declaration identity, independent of clone addresses and lookup aliases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeclarationRoot {
    Scene(SceneEntityId),
    Prefab(SourceDocumentKey, SceneEntityId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeclarationSegment {
    Addition(usize),
    InstanceClaim(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeclarationAddress {
    root: DeclarationRoot,
    segments: Vec<DeclarationSegment>,
}

impl<'a, F, E> PrefabExpansionContext<'a, F, E>
where
    F: FnMut(
        &nara_asset::AssetRef,
        &mut crate::prefab::PrefabExpansionWorkingSet,
    ) -> Result<Option<ResolvedPrefabSource>, E>,
{
    pub(crate) fn push_overlay_conflict(
        &mut self,
        anchor: &SceneEntityId,
        conflict: PrefabOverlayConflict,
    ) {
        if conflict == PrefabOverlayConflict::WorkLimit {
            self.budget_failed = true;
        }
        // Source declarations remain untouched. Diagnostics describe a rejected projection;
        // they are never persisted in place of the original intent.
        if self.diagnostics.has_errors() {
            return;
        }
        self.diagnostics.push(with_public_locator(
            with_public_locator(
                diagnostic_error(
                    "scene.prefab-overlay-conflict",
                    "Prefab intent conflicts with its selected immutable bases",
                ),
                "entity-id",
                anchor.as_str(),
            ),
            "conflict-kind",
            match conflict {
                PrefabOverlayConflict::SourceChanged => "source-changed",
                PrefabOverlayConflict::MissingTarget(_) => "missing-target",
                PrefabOverlayConflict::DuplicateIntent => "duplicate-intent",
                PrefabOverlayConflict::Placement => "placement",
                PrefabOverlayConflict::Component => "component",
                PrefabOverlayConflict::Reference => "reference",
                PrefabOverlayConflict::OwnerAdmission => "owner-admission",
                PrefabOverlayConflict::WorkLimit => "work-limit",
            },
        ));
    }
}
