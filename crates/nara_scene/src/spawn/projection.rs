//! Whole-domain disposable projection, separate from authoring history and ordinary Scene instances.

use super::*;

#[cfg(test)]
mod tests;

/// Owns one disposable Scene projection and its complete identity-domain epoch.
///
/// Every successful publication closes the predecessor epoch, even when the source retains its
/// entity IDs. The World, assets and other non-identity resources remain in place. A domain with
/// unrelated current or historical identity responsibility is rejected without replacing it.
/// Use ordinary Scene spawning/replacement for independently resident instances in a shared domain.
#[derive(Debug)]
pub struct SceneProjection {
    occurrence: SceneOccurrenceKey,
    current: Option<SpawnedSceneInstance>,
}

impl SceneProjection {
    #[must_use]
    pub const fn new(occurrence: SceneOccurrenceKey) -> Self {
        Self {
            occurrence,
            current: None,
        }
    }

    /// Returns the exact receipt of the last successfully published projection.
    #[must_use]
    pub const fn instance(&self) -> Option<&SpawnedSceneInstance> {
        self.current.as_ref()
    }

    /// Publishes a complete candidate in a fresh domain bound to this same World.
    ///
    /// Rejection preserves the previous projection and receipt. Preparation may allocate scratch
    /// entities; it does not promise an unchanged ECS allocator or run arbitrary lifecycle work.
    pub fn publish(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
        candidate: &ExpandedSceneCandidate,
        asset_resolution: AssetReferenceResolver<'_>,
    ) -> SceneSpawnReport {
        let report = SceneSpawner::new().spawn_with_asset_context_and_product(
            world,
            registry,
            candidate,
            &self.occurrence,
            Some(asset_resolution),
            SceneIdentityCommit::Projection(self.current.as_ref()),
            None,
        );
        if let Some(instance) = &report.instance {
            self.current = Some(instance.clone());
        }
        report
    }

    /// Retires this entire projection epoch while preserving the World's other resources.
    ///
    /// A refused retirement retains the receipt and domain for an explicit retry. Dropping this
    /// value alone is not evidence that the projection or its World has retired.
    pub fn clear(&mut self, world: &mut World) -> Result<usize, DiagnosticReport> {
        let Some(current) = self.current.as_ref() else {
            return Ok(0);
        };
        crate::occurrence::validate_projection_occurrence(world, Some(current)).map_err(
            |error| occurrence_rejection(DiagnosticReport::default(), error).diagnostics,
        )?;
        nara_identity::__private::prepare_scene_projection_epoch_retirement(world, Some(current))
            .map_err(projection_identity_rejection)?;
        let retired = retire_scene_instance_exact(world, current, TombstoneCause::Unloaded)
            .map_err(|error| match error {
                SceneInstanceRetirementTransactionError::Identity(error) => {
                    projection_identity_rejection(error)
                }
                _ => {
                    let mut report = DiagnosticReport::default();
                    report.push(diagnostic_error(
                        "scene.projection-retirement-ineligible",
                        "Scene projection retirement would run unsupported lifecycle work",
                    ));
                    report
                }
            })?;
        world
            .remove_resource::<WorldIdentityDomain>()
            .expect("the exclusively retired Scene projection retains its domain until release");
        self.current = None;
        Ok(retired.len())
    }
}

fn projection_identity_rejection(error: IdentityDomainError) -> DiagnosticReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(crate::diagnostics::with_identity_error(
        diagnostic_error(
            "scene.projection-epoch-ineligible",
            "The complete identity domain does not belong to this disposable Scene projection",
        ),
        &error,
    ));
    diagnostics
}
