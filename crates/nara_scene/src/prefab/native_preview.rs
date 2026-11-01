use super::*;
use crate::{
    PrefabProjectionPath, authoring_preview::AuthoringPreviewOmission,
    provenance::provenance_string_bytes,
};
use nara_reflect::ComponentTypeId;

#[cfg(test)]
mod tests;

impl AuthoringPrefabExpansionReport {
    /// Consumes schema-only compilation using its already admitted Native Adapter coverage.
    pub fn into_native_preview(self) -> Result<AuthoringNativePreview, DiagnosticReport> {
        self.finish_native_preview(None)
    }

    /// Complete modeled work after restricting to this registry, without allocating omissions.
    ///
    /// This includes the original compilation and both representations of every new omission.
    /// Overflow or a missing compilation candidate returns `None`. The consuming operation still
    /// enforces the original working cap; this observation does not enlarge that allowance.
    pub fn checked_native_preview_work_bytes_for(
        &self,
        registry: &ComponentRegistrySnapshot,
    ) -> Option<usize> {
        self.native_preview_restriction(self.candidate.as_ref()?, registry)?
            .1
            .checked_work_bytes()
    }

    /// Further restricts presentation to the receiving App's actual Native registry.
    ///
    /// Source classification and existing omissions are unchanged. The original working cap admits
    /// new omission and typed-provenance conversion storage before allocation. Only the original
    /// Schema basis retained by this compilation can establish source evidence. The report's
    /// borrowed observations cannot replace its candidate, omissions or working allowance.
    pub fn into_native_preview_for(
        self,
        registry: &ComponentRegistrySnapshot,
    ) -> Result<AuthoringNativePreview, DiagnosticReport> {
        self.finish_native_preview(Some(registry))
    }

    fn finish_native_preview(
        mut self,
        registry: Option<&ComponentRegistrySnapshot>,
    ) -> Result<AuthoringNativePreview, DiagnosticReport> {
        let Some(candidate) = self.candidate.take() else {
            return Err(self.diagnostics);
        };
        if let Some(registry) = registry {
            let Some((count, usage)) = self.native_preview_restriction(&candidate, registry) else {
                return Err(self.preview_budget_failure(usize::MAX));
            };
            let work = usage.checked_work_bytes();
            if work.is_none_or(|work| work > self.working_limit.get()) {
                return Err(self.preview_budget_failure(work.unwrap_or(usize::MAX)));
            }

            // All new facts and their later converted copies are admitted before any cloning.
            self.usage = usage;
            self.omissions.reserve_exact(count);
            for entity in &candidate.document().entities {
                for (component, record) in &entity.components {
                    if self
                        .basis
                        .native_preview_matches_registry(component, record, registry)
                        == Some(false)
                    {
                        // Available Native records are disjoint from the compiler's original
                        // Data/unavailable omissions. No duplicate index or repeated search is needed.
                        self.omissions.push(AuthoringPrefabOmission {
                            entity: entity.id.clone(),
                            component: component.clone(),
                            reason: AuthoringPrefabOmissionReason::NativeBindingUnavailable,
                        });
                    }
                }
            }
        }
        if self.checked_work_bytes().is_none() {
            return Err(self.preview_budget_failure(usize::MAX));
        }
        let candidate = candidate.remove_compiler_components(
            self.omissions
                .iter()
                .filter(|fact| {
                    fact.reason == AuthoringPrefabOmissionReason::NativeBindingUnavailable
                })
                .map(|fact| (&fact.entity, &fact.component)),
        );
        let omissions = self
            .omissions
            .iter()
            .map(|fact| AuthoringPreviewOmission::from_expansion(fact, &candidate))
            .collect();
        Ok(AuthoringNativePreview {
            candidate,
            omissions,
            diagnostics: self.diagnostics,
            failure: self.failure,
        })
    }

    fn native_preview_restriction(
        &self,
        candidate: &ExpandedSceneDocument,
        registry: &ComponentRegistrySnapshot,
    ) -> Option<(usize, PrefabExpansionUsage)> {
        let (count, bytes) = candidate.document().entities.iter().try_fold(
            (0_usize, 0_usize),
            |(count, bytes), entity| {
                entity.components.iter().try_fold(
                    (count, bytes),
                    |(count, bytes), (component, record)| {
                        if self
                            .basis
                            .native_preview_matches_registry(component, record, registry)
                            != Some(false)
                        {
                            return Some((count, bytes));
                        }
                        Some((
                            count.checked_add(1)?,
                            bytes.checked_add(omission_work_bytes(
                                &entity.id,
                                component,
                                candidate.provenance().get(&entity.id),
                            )?)?,
                        ))
                    },
                )
            },
        )?;
        let mut usage = self.usage;
        usage.scratch_bytes = usage.scratch_bytes.checked_add(bytes)?;
        Some((count, usage))
    }

    fn preview_budget_failure(mut self, observed: usize) -> DiagnosticReport {
        if !self.budget_failed {
            self.diagnostics.push(prefab_expansion_budget_diagnostic(
                PrefabExpansionBudgetKind::WorkingBytes,
                observed,
                self.working_limit.get(),
                None,
                None,
            ));
        }
        self.diagnostics
    }
}

/// The compiler fact and its later native-preview observation coexist. Reserve their owned
/// locators and the complete converted ancestry before either result can copy it.
pub(super) fn omission_work_bytes(
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    origin: Option<&SceneEntityProvenance>,
) -> Option<usize> {
    let bytes = provenance_string_bytes(entity.as_str())?
        .checked_add(provenance_string_bytes(component.as_str())?)?
        .checked_mul(2)?
        .checked_add(
            std::mem::size_of::<AuthoringPrefabOmission>()
                + std::mem::size_of::<AuthoringPreviewOmission>(),
        )?;
    let origin = origin?;
    let Some(path) = origin.projection_path() else {
        return Some(bytes);
    };
    bytes
        .checked_add(std::mem::size_of::<SceneEntityProvenance>())?
        .checked_add(provenance_string_bytes(origin.local_entity_id().as_str())?)?
        .checked_add(PrefabProjectionPath::extended_allocation_bytes(
            Some(path),
            path.root_anchor(),
            0,
        )?)
}
