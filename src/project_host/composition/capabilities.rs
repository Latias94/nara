use nara_core::diagnostic::{
    DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, ReportDiagnostic, SafeSummary,
};
use nara_scene::runtime_configuration::{
    EffectiveRuntimeConfiguration, ProductCapability, ProductCapabilitySet, RuntimePreset,
};

use super::{CompiledProductCapabilities, compiled_product_capabilities};

/// Why a runtime configuration cannot enter the compiled product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeCapabilityErrorKind {
    #[error("runtime preset conflicts with requested product capabilities")]
    PresetConflict,
    #[error("project product capability closure is incomplete")]
    InvalidCapabilityClosure,
    #[error("project requests unavailable product capabilities")]
    UnavailableCapabilities,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{kind}")]
pub struct RuntimeCapabilityError {
    kind: RuntimeCapabilityErrorKind,
    diagnostics: DiagnosticReport,
}

impl RuntimeCapabilityError {
    #[must_use]
    pub const fn kind(&self) -> RuntimeCapabilityErrorKind {
        self.kind
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    #[cfg(feature = "project-source")]
    pub(in crate::project_host) fn into_diagnostics(self) -> DiagnosticReport {
        self.diagnostics
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::project_host) struct RuntimeCapabilityAdmission {
    pub(in crate::project_host) explicit: ProductCapabilitySet,
    pub(in crate::project_host) normalized: ProductCapabilitySet,
    pub(in crate::project_host) required: ProductCapabilitySet,
    pub(in crate::project_host) compiled: CompiledProductCapabilities,
}

pub(in crate::project_host) fn validate_runtime_capabilities(
    settings: &EffectiveRuntimeConfiguration,
) -> Result<RuntimeCapabilityAdmission, RuntimeCapabilityError> {
    let explicit = settings.requested_capabilities();
    let required = ProductCapabilitySet::singleton(ProductCapability::RuntimeCore);
    let normalized = explicit.union(required);

    for (capability, dependency) in [
        (
            ProductCapability::Presentation2d,
            ProductCapability::Runtime2d,
        ),
        (
            ProductCapability::PresentationUi,
            ProductCapability::RuntimeUi,
        ),
    ] {
        if normalized.contains(capability) && !normalized.contains(dependency) {
            return Err(capability_dependency_error(capability, dependency));
        }
    }

    if settings.runtime_preset() == RuntimePreset::Server {
        let forbidden = ProductCapabilitySet::from_capabilities([
            ProductCapability::Runtime2d,
            ProductCapability::RuntimeUi,
            ProductCapability::Presentation2d,
            ProductCapability::PresentationUi,
            ProductCapability::DesktopWinit,
            ProductCapability::RenderWgpu,
        ]);
        let conflicting = normalized.intersection(forbidden);
        if !conflicting.is_empty() {
            return Err(capability_error(
                RuntimeCapabilityErrorKind::PresetConflict,
                "project.product.preset-conflict",
                "Runtime preset conflicts with requested product capabilities",
                conflicting,
            ));
        }
    }

    let compiled = compiled_product_capabilities();
    let unavailable = normalized.difference(compiled.capabilities());
    if !unavailable.is_empty() {
        return Err(capability_error(
            RuntimeCapabilityErrorKind::UnavailableCapabilities,
            "project.product.unavailable-capability",
            "Project requests a product capability that is not compiled",
            unavailable,
        ));
    }

    Ok(RuntimeCapabilityAdmission {
        explicit,
        normalized,
        required,
        compiled,
    })
}

fn capability_dependency_error(
    capability: ProductCapability,
    dependency: ProductCapability,
) -> RuntimeCapabilityError {
    error(
        RuntimeCapabilityErrorKind::InvalidCapabilityClosure,
        "project.product.incomplete-capability-closure",
        "Product capability requires an explicit content capability",
        [
            capability_field("capability", capability),
            capability_field("required-capability", dependency),
        ],
    )
}

fn capability_error(
    kind: RuntimeCapabilityErrorKind,
    code: &'static str,
    summary: &'static str,
    capabilities: ProductCapabilitySet,
) -> RuntimeCapabilityError {
    let first = capabilities
        .iter()
        .next()
        .expect("capability errors require a non-empty set");
    let count = DiagnosticFieldKey::new("count").expect("static field key");
    error(
        kind,
        code,
        summary,
        [
            capability_field("capability", first),
            DiagnosticField::public_u64(count, capabilities.len() as u64),
        ],
    )
}

fn capability_field(key: &'static str, capability: ProductCapability) -> DiagnosticField {
    DiagnosticField::public_identifier(
        DiagnosticFieldKey::new(key).expect("static field key"),
        PublicDiagnosticIdentifier::new(capability.as_str()).expect("static capability identifier"),
    )
}

fn error(
    kind: RuntimeCapabilityErrorKind,
    code: &'static str,
    summary: &'static str,
    fields: [DiagnosticField; 2],
) -> RuntimeCapabilityError {
    let mut diagnostic = ReportDiagnostic::error(
        DiagnosticCode::new(code).expect("static capability diagnostic code"),
        SafeSummary::new(summary).expect("static capability diagnostic summary"),
    );
    for field in fields {
        diagnostic = diagnostic
            .try_with_field(field)
            .expect("capability diagnostics use unique bounded fields");
    }
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic);
    RuntimeCapabilityError { kind, diagnostics }
}
