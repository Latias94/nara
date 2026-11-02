use super::*;
use crate::{
    PrefabProjectionPath, authoring_preview::AuthoringPreviewOmission,
    provenance::provenance_string_bytes,
};
use nara_reflect::ComponentTypeId;

#[cfg(test)]
mod tests;

impl AuthoringSceneCompilationReport {
    /// Filters and seals schema-only compilation using its admitted Native Adapter coverage.
    pub fn into_native_preview(mut self) -> Result<AuthoringNativePreview, DiagnosticReport> {
        self.finish_native_preview(None)
    }

    /// Modeled work through registry restriction, before terminal sealing or omission allocation.
    ///
    /// This includes the original compilation and both representations of every new omission.
    /// Terminal sealing admits its own additional work against the same original allowance.
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
        mut self,
        registry: &ComponentRegistrySnapshot,
    ) -> Result<AuthoringNativePreview, DiagnosticReport> {
        self.finish_native_preview(Some(registry))
    }

    fn finish_native_preview(
        &mut self,
        registry: Option<&ComponentRegistrySnapshot>,
    ) -> Result<AuthoringNativePreview, DiagnosticReport> {
        let Some(candidate) = self.candidate.take() else {
            return Err(std::mem::take(&mut self.diagnostics));
        };
        if let Some(registry) = registry {
            let Some((count, usage)) = self.native_preview_restriction(&candidate, registry) else {
                return Err(self.preview_budget_failure(usize::MAX));
            };
            let work = usage.checked_work_bytes();
            if work.is_none_or(|work| work > self.limits.working_bytes().get()) {
                return Err(self.preview_budget_failure(work.unwrap_or(usize::MAX)));
            }

            // All new facts and their later converted copies are admitted before any cloning.
            self.usage = usage;
            self.omissions.reserve_exact(count);
            for entity in candidate.entities() {
                for (component, record) in entity.components() {
                    if self
                        .basis
                        .native_preview_matches_registry(component, record, registry)
                        == Some(false)
                    {
                        // Available Native records are disjoint from the compiler's original
                        // Data/unavailable omissions. No duplicate index or repeated search is needed.
                        self.omissions.push(AuthoringSceneCompilationOmission {
                            entity: entity.id().clone(),
                            component: component.clone(),
                            reason:
                                AuthoringSceneCompilationOmissionReason::NativeBindingUnavailable,
                        });
                    }
                }
            }
        }
        let work = self.checked_work_bytes();
        if work.is_none_or(|work| work > self.limits.working_bytes().get()) {
            return Err(self.preview_budget_failure(work.unwrap_or(usize::MAX)));
        }
        let candidate = candidate.remove_compiler_components(
            self.omissions
                .iter()
                .filter(|fact| {
                    fact.reason == AuthoringSceneCompilationOmissionReason::NativeBindingUnavailable
                })
                .map(|fact| (&fact.entity, &fact.component)),
        );
        let omissions = self
            .omissions
            .iter()
            .map(|fact| AuthoringPreviewOmission::from_compilation(fact, &candidate))
            .collect();
        let (document, provenance) = candidate.into_compiler_parts();
        let mut usage = self.usage;
        let mut reference_work = self.reference_work;
        let limits = self.limits;
        let result = crate::compiled::admit_draft(
            document,
            provenance,
            SchemaAuthority::Authoring(&self.basis),
            limits.materialization_limits(),
            None,
            |nodes, bytes, allocation| {
                admit_prefab_terminal_work(
                    limits,
                    &mut usage,
                    &mut reference_work,
                    nodes,
                    bytes,
                    allocation,
                )
            },
        );
        self.usage = usage;
        self.reference_work = reference_work;
        let candidate = match result {
            Ok(candidate) => candidate,
            Err(CompiledSceneAdmissionError::Admission((kind, observed, maximum))) => {
                if !self.budget_failed {
                    self.budget_failed = true;
                    self.diagnostics.push(prefab_expansion_budget_diagnostic(
                        kind, observed, maximum, None, None,
                    ));
                }
                return Err(std::mem::take(&mut self.diagnostics));
            }
            Err(CompiledSceneAdmissionError::Diagnostics(diagnostics)) => {
                self.diagnostics.extend(diagnostics);
                return Err(std::mem::take(&mut self.diagnostics));
            }
        };
        Ok(AuthoringNativePreview {
            candidate,
            omissions,
            diagnostics: std::mem::take(&mut self.diagnostics),
            failure: self.failure.take(),
            work_bytes: self.usage.checked_work_bytes(),
        })
    }

    fn native_preview_restriction(
        &self,
        candidate: &ExpandedSceneDocument,
        registry: &ComponentRegistrySnapshot,
    ) -> Option<(usize, SceneCompilationUsage)> {
        let (count, bytes) = candidate.entities().iter().try_fold(
            (0_usize, 0_usize),
            |(count, bytes), entity| {
                entity.components().iter().try_fold(
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
                                entity.id(),
                                component,
                                candidate.provenance().get(entity.id()),
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

    fn preview_budget_failure(&mut self, observed: usize) -> DiagnosticReport {
        if !self.budget_failed {
            self.budget_failed = true;
            self.diagnostics.push(prefab_expansion_budget_diagnostic(
                SceneCompilationBudgetKind::WorkingBytes,
                observed,
                self.limits.working_bytes().get(),
                None,
                None,
            ));
        }
        std::mem::take(&mut self.diagnostics)
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
            std::mem::size_of::<AuthoringSceneCompilationOmission>()
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
