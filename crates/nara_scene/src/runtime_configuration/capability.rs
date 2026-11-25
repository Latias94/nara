use std::fmt;

use nara_core::diagnostic::{
    DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, ReportDiagnostic, SafeSummary,
};
use serde::{Deserialize, Serialize};

use super::EffectiveRuntimeConfiguration;

/// A stable product capability that may be compiled by a host and requested by a project.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[repr(u8)]
pub enum ProductCapability {
    #[serde(rename = "runtime-core")]
    RuntimeCore = 0,
    #[serde(rename = "runtime-2d")]
    Runtime2d = 1,
    #[serde(rename = "runtime-ui")]
    RuntimeUi = 2,
    #[serde(rename = "asset-watch")]
    AssetWatch = 3,
    #[serde(rename = "desktop-winit")]
    DesktopWinit = 4,
    #[serde(rename = "render-wgpu")]
    RenderWgpu = 5,
    #[serde(rename = "presentation-2d")]
    Presentation2d = 6,
    #[serde(rename = "presentation-ui")]
    PresentationUi = 7,
}

impl ProductCapability {
    pub const ALL: [Self; 8] = [
        Self::RuntimeCore,
        Self::Runtime2d,
        Self::RuntimeUi,
        Self::Presentation2d,
        Self::PresentationUi,
        Self::AssetWatch,
        Self::DesktopWinit,
        Self::RenderWgpu,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RuntimeCore => "runtime-core",
            Self::Runtime2d => "runtime-2d",
            Self::RuntimeUi => "runtime-ui",
            Self::Presentation2d => "presentation-2d",
            Self::PresentationUi => "presentation-ui",
            Self::AssetWatch => "asset-watch",
            Self::DesktopWinit => "desktop-winit",
            Self::RenderWgpu => "render-wgpu",
        }
    }

    const fn bit(self) -> u16 {
        1_u16 << self as u8
    }
}

impl fmt::Debug for ProductCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl fmt::Display for ProductCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A deterministic, allocation-free set of product capabilities.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ProductCapabilitySet(u16);

impl ProductCapabilitySet {
    pub const EMPTY: Self = Self(0);

    #[must_use]
    pub const fn new() -> Self {
        Self::EMPTY
    }

    #[must_use]
    pub const fn singleton(capability: ProductCapability) -> Self {
        Self(capability.bit())
    }

    #[must_use]
    pub fn from_capabilities(capabilities: impl IntoIterator<Item = ProductCapability>) -> Self {
        let mut set = Self::new();
        for capability in capabilities {
            set.insert(capability);
        }
        set
    }

    #[must_use]
    pub const fn with(mut self, capability: ProductCapability) -> Self {
        self.0 |= capability.bit();
        self
    }

    pub fn insert(&mut self, capability: ProductCapability) -> bool {
        let previous = self.0;
        self.0 |= capability.bit();
        self.0 != previous
    }

    pub fn remove(&mut self, capability: ProductCapability) -> bool {
        let previous = self.0;
        self.0 &= !capability.bit();
        self.0 != previous
    }

    #[must_use]
    pub const fn contains(self, capability: ProductCapability) -> bool {
        self.0 & capability.bit() != 0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    #[must_use]
    pub const fn is_subset(self, other: Self) -> bool {
        self.0 & !other.0 == 0
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub fn iter(self) -> impl Iterator<Item = ProductCapability> {
        ProductCapability::ALL
            .into_iter()
            .filter(move |capability| self.contains(*capability))
    }
}

impl fmt::Debug for ProductCapabilitySet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_set().entries(self.iter()).finish()
    }
}

/// Runtime execution policy, independent from additive product capabilities.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimePreset {
    #[default]
    Minimal,
    LocalHeadless,
    Server,
}

impl RuntimePreset {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::LocalHeadless => "local-headless",
            Self::Server => "server",
        }
    }
}

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

    /// Returns the bounded report for a rejected capability request.
    pub fn into_diagnostics(self) -> DiagnosticReport {
        self.diagnostics
    }
}

/// The requested capability set after admission against the executable's compiled ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeCapabilityAdmission {
    explicit: ProductCapabilitySet,
    normalized: ProductCapabilitySet,
    compiled: ProductCapabilitySet,
}

impl RuntimeCapabilityAdmission {
    pub const fn explicit(self) -> ProductCapabilitySet {
        self.explicit
    }

    pub const fn normalized(self) -> ProductCapabilitySet {
        self.normalized
    }

    pub const fn required(self) -> ProductCapabilitySet {
        ProductCapabilitySet::singleton(ProductCapability::RuntimeCore)
    }

    pub const fn compiled(self) -> ProductCapabilitySet {
        self.compiled
    }
}

/// Validates the requested and preset capability rules against the caller's compiled features.
/// Plugin-specific requirements are checked when the executable selects its plugin plan.
pub fn admit_runtime_capabilities(
    settings: &EffectiveRuntimeConfiguration,
    compiled: ProductCapabilitySet,
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

    let unavailable = normalized.difference(compiled);
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
