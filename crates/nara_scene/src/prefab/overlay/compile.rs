//! Instance selection and layered compilation in the existing Scene expansion owner.

use super::{
    PrefabComponentIntent, PrefabComponentSetMode, PrefabEntityOrigin, PrefabEntityTarget,
    PrefabInstanceHop, PrefabOverlay, SceneCompilationFailureKind, layout,
};
use crate::draft::{ExpandedSceneEntity, SceneDraft};
use crate::{
    PrefabDeclarationSource, PrefabInstance, PrefabInstanceLocation, PrefabInstancePath,
    PrefabInstancePathSegment, PrefabOverlaySlot, PrefabProjectionPath, PrefabProjectionStep,
    ResolvedPrefabSource, SceneCompilationFailure, SceneEntityId, SceneEntityPlacement,
    SceneEntityProvenance, SceneEntityRecord, ScenePlacementParent, ScenePlacementSlot,
    SourceDocumentKey, SourceDocumentKind,
    diagnostics::{
        error as diagnostic_error, usize_to_u64, with_asset_ref, with_public_locator,
        with_public_u64,
    },
    prefab::{SceneCompilationBudgetKind, SceneCompilationContext},
    provenance::provenance_entry_bytes,
    validation::validate_source_structure,
};

#[cfg(test)]
#[path = "compile_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "compile/component_program_tests.rs"]
mod component_program_tests;

mod staged;

/// Flat values and source-only default snapshots have different consumers. Only selected
/// source conversion requests the latter; Native admission never receives these declarations.
pub(crate) struct StagedSceneOutput {
    pub(crate) document: SceneDraft,
    pub(crate) source_defaults: Vec<MaterializedSourceDefaults>,
    #[cfg(feature = "serde")]
    pub(crate) targets: std::collections::BTreeMap<PrefabEntityTarget, SceneEntityId>,
    #[cfg(feature = "serde")]
    pub(crate) reference_targets: std::collections::BTreeMap<
        nara_identity::AuthoredEntityReference,
        nara_identity::EntityReference,
    >,
}

pub(crate) struct MaterializedSourceDefaults {
    pub(crate) entity: SceneEntityId,
    pub(crate) component: nara_reflect::ComponentTypeId,
    pub(crate) values:
        std::collections::BTreeMap<nara_reflect::ComponentFieldId, nara_reflect::ComponentValue>,
}

#[cfg(all(test, feature = "serde"))]
#[path = "compile/selected_tests.rs"]
mod selected_tests;

pub(crate) fn expand_source<F, E>(
    context: &mut SceneCompilationContext<'_, F, E>,
    placements: &[SceneEntityPlacement],
    entities: &[SceneEntityRecord],
    inputs: Option<&std::collections::BTreeSet<nara_identity::ReferenceInputId>>,
) -> Result<SceneDraft, SceneCompilationFailure>
where
    F: FnMut(
        &nara_asset::AssetRef,
        &mut crate::prefab::SceneCompilationWorkingSet,
    ) -> Result<Option<ResolvedPrefabSource>, E>,
{
    staged::expand_source(
        context,
        staged::SourceTopology {
            placements,
            entities,
            inputs,
            reference_queries: &[],
        },
    )
}

#[cfg(feature = "serde")]
pub(crate) fn expand_selected_instance<F, E>(
    context: &mut SceneCompilationContext<'_, F, E>,
    source: crate::prefab::SelectedInstanceSource<'_>,
    anchor: &SceneEntityId,
    reference_queries: &[nara_identity::AuthoredEntityReference],
) -> Result<StagedSceneOutput, SceneCompilationFailure>
where
    F: FnMut(
        &nara_asset::AssetRef,
        &mut crate::prefab::SceneCompilationWorkingSet,
    ) -> Result<Option<ResolvedPrefabSource>, E>,
{
    let (placements, entities, inputs) = match source {
        crate::prefab::SelectedInstanceSource::Scene(document) => (
            document.placements.as_slice(),
            document.entities.as_slice(),
            None,
        ),
        crate::prefab::SelectedInstanceSource::Prefab(document) => (
            document.placements.as_slice(),
            document.entities.as_slice(),
            Some(&document.inputs),
        ),
    };
    staged::expand_selected(
        context,
        staged::SourceTopology {
            placements,
            entities,
            inputs,
            reference_queries,
        },
        anchor,
    )
}

impl<'a, F, E> SceneCompilationContext<'a, F, E>
where
    F: FnMut(
        &nara_asset::AssetRef,
        &mut crate::prefab::SceneCompilationWorkingSet,
    ) -> Result<Option<ResolvedPrefabSource>, E>,
{
    pub(crate) fn push_expansion_failure(&mut self, failure: &SceneCompilationFailure) {
        if failure.reason == SceneCompilationFailureKind::WorkLimit {
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
                SceneCompilationFailureKind::SourceChanged => "source-changed",
                SceneCompilationFailureKind::MissingTarget => "missing-target",
                SceneCompilationFailureKind::DuplicateIntent => "duplicate-intent",
                SceneCompilationFailureKind::Placement => "placement",
                SceneCompilationFailureKind::RootBinding => "root-binding",
                SceneCompilationFailureKind::Component => "component",
                SceneCompilationFailureKind::Reference => "reference",
                SceneCompilationFailureKind::OwnerAdmission => "owner-admission",
                SceneCompilationFailureKind::WorkLimit => "work-limit",
            },
        );
        if let Some(location) = &failure.location {
            diagnostic =
                with_public_locator(diagnostic, "entity-id", location.path.anchor.as_str());
        }
        if let Some(entity) = &failure.entity {
            diagnostic = with_public_locator(
                diagnostic,
                if failure.location.is_some() {
                    "cycle-entity-id"
                } else {
                    "entity-id"
                },
                entity.as_str(),
            );
        }
        self.diagnostics.push(diagnostic);
    }
}
