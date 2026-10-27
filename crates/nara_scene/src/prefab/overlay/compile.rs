//! Instance selection and layered compilation in the existing Scene expansion owner.

use super::{
    PrefabComponentIntent, PrefabComponentSetMode, PrefabEntityOrigin, PrefabEntityTarget,
    PrefabExpansionFailureKind, PrefabInstanceHop, PrefabOverlay, PrefabPlacementParent,
    PrefabPlacementSlot, layout,
};
use crate::{
    PrefabDeclarationSource, PrefabExpansionFailure, PrefabInstance, PrefabInstanceLocation,
    PrefabInstancePath, PrefabInstancePathSegment, PrefabOverlaySlot, PrefabProjectionPath,
    PrefabProjectionStep, ResolvedPrefabSource, SceneDocument, SceneEntityId,
    SceneEntityProvenance, SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
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
) -> Result<SceneDocument, PrefabExpansionFailure>
where
    F: FnMut(
        &nara_asset::AssetRef,
        &mut crate::prefab::PrefabExpansionWorkingSet,
    ) -> Result<Option<ResolvedPrefabSource>, E>,
{
    staged::expand(context, document)
}

impl<'a, F, E> PrefabExpansionContext<'a, F, E>
where
    F: FnMut(
        &nara_asset::AssetRef,
        &mut crate::prefab::PrefabExpansionWorkingSet,
    ) -> Result<Option<ResolvedPrefabSource>, E>,
{
    pub(crate) fn push_expansion_failure(&mut self, failure: &PrefabExpansionFailure) {
        if failure.reason == PrefabExpansionFailureKind::WorkLimit {
            self.budget_failed = true;
        }
        // Source declarations remain untouched. Diagnostics describe a rejected projection;
        // they are never persisted in place of the original intent.
        if self.diagnostics.has_errors() {
            return;
        }
        let mut diagnostic = with_public_locator(
            diagnostic_error(
                "scene.prefab-compilation-failed",
                "Prefab composition was rejected before publication",
            ),
            "failure-kind",
            match failure.reason {
                PrefabExpansionFailureKind::SourceChanged => "source-changed",
                PrefabExpansionFailureKind::MissingTarget => "missing-target",
                PrefabExpansionFailureKind::DuplicateIntent => "duplicate-intent",
                PrefabExpansionFailureKind::Placement => "placement",
                PrefabExpansionFailureKind::Component => "component",
                PrefabExpansionFailureKind::Reference => "reference",
                PrefabExpansionFailureKind::OwnerAdmission => "owner-admission",
                PrefabExpansionFailureKind::WorkLimit => "work-limit",
            },
        );
        if let Some(location) = &failure.location {
            diagnostic =
                with_public_locator(diagnostic, "entity-id", location.path.anchor.as_str());
        }
        self.diagnostics.push(diagnostic);
    }
}
