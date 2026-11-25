use nara_scene::runtime_configuration::{
    EffectiveRuntimeConfiguration, RuntimeConfigurationSettings,
};

use crate::validation::{error, with_field_path};
use crate::{
    EffectiveProjectSettings, ProjectAuthoringState, ProjectPath, RuntimeConfigurationError,
};

/// Admitted runtime values and the source input selection that still requires fulfillment.
///
/// The input requirement is relative to the project root. Retaining it does not read or admit
/// the selected file, grant source authority, or establish that an Input owner is installed.
/// Projection requires Ready at that point, but stores no source-readiness authority. The source
/// Host must capture and finally revalidate the manifest; a retained projection cannot certify
/// that the current project is still Ready.
#[derive(Debug, Clone, PartialEq)]
#[must_use = "source input requirements must be fulfilled before runtime startup"]
pub struct SourceRuntimeProjection {
    runtime: EffectiveRuntimeConfiguration,
    input_requirement: Option<ProjectPath>,
}

impl SourceRuntimeProjection {
    /// Returns the independently admitted runtime values with no processed Input selection.
    #[must_use]
    pub const fn runtime(&self) -> &EffectiveRuntimeConfiguration {
        &self.runtime
    }

    /// Returns the resolved source action-map path that the source Host must fulfill.
    #[must_use]
    pub const fn input_requirement(&self) -> Option<&ProjectPath> {
        self.input_requirement.as_ref()
    }
}

impl EffectiveProjectSettings {
    /// Projects the resolved profile into independently admitted runtime values.
    ///
    /// A remaining input action-map path requires source admission and is rejected. A profile
    /// can explicitly clear that path before projection. Public effective fields are validated
    /// again, and values the runtime record cannot represent exactly are rejected. Source Hosts
    /// can retain the selection as an obligation through [`Self::source_runtime_projection`].
    /// Project authoring state is source-only policy and is excluded, including Pending. A
    /// successful projection does not admit source execution or establish source readiness.
    pub fn runtime_configuration(
        &self,
    ) -> Result<EffectiveRuntimeConfiguration, RuntimeConfigurationError> {
        if self.input.action_map.is_some() {
            return Err(projection_error(
                "runtime.configuration.source-input-required",
                "Runtime configuration cannot carry a source input action-map path",
                "input.action_map",
            ));
        }

        self.project_runtime_configuration()
    }

    /// Projects a resolved source profile while retaining its unfulfilled input selection.
    ///
    /// Uses the same runtime validation and exact representation checks as
    /// [`Self::runtime_configuration`]. The source Host must separately admit any selected input
    /// file and its receiving owner before runtime startup. Profile clearing is applied before
    /// projection and leaves no input requirement. Pending authoring state rejects source
    /// composition. The source Host must still capture and finally revalidate the manifest;
    /// this check cannot certify that a retained projection's source is still Ready.
    pub fn source_runtime_projection(
        &self,
    ) -> Result<SourceRuntimeProjection, RuntimeConfigurationError> {
        if matches!(&self.authoring_state, ProjectAuthoringState::Pending(_)) {
            return Err(projection_error(
                "runtime.configuration.source-authoring-pending",
                "Pending project authoring state cannot enter source runtime composition",
                "authoring.state",
            ));
        }
        Ok(SourceRuntimeProjection {
            runtime: self.project_runtime_configuration()?,
            input_requirement: self.input.action_map.clone(),
        })
    }

    fn project_runtime_configuration(
        &self,
    ) -> Result<EffectiveRuntimeConfiguration, RuntimeConfigurationError> {
        RuntimeConfigurationSettings {
            runtime_preset: self.runtime_preset,
            requested_capabilities: self.requested_capabilities,
            runtime: self.runtime,
            tasks: self.tasks,
            render: self.render,
            ui: self.ui,
            window: self.window.clone(),
            diagnostics: self.diagnostics,
        }
        .resolve()
    }
}
fn projection_error(
    code: &'static str,
    summary: &'static str,
    field: &'static str,
) -> RuntimeConfigurationError {
    RuntimeConfigurationError::from_diagnostic(with_field_path(error(code, summary), field))
}

#[cfg(test)]
mod tests;
