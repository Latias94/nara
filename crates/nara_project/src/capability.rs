use std::fmt;

use serde::Deserialize;

/// A stable product capability that may be compiled by a host and requested by a project.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[repr(u8)]
pub enum ProductCapability {
    #[serde(rename = "runtime-core")]
    RuntimeCore,
    #[serde(rename = "runtime-2d")]
    Runtime2d,
    #[serde(rename = "runtime-ui")]
    RuntimeUi,
    #[serde(rename = "tooling")]
    Tooling,
    #[serde(rename = "asset-watch")]
    AssetWatch,
    #[serde(rename = "desktop-winit")]
    DesktopWinit,
    #[serde(rename = "render-wgpu")]
    RenderWgpu,
    #[serde(rename = "tooling-egui")]
    ToolingEgui,
}

impl ProductCapability {
    pub const ALL: [Self; 8] = [
        Self::RuntimeCore,
        Self::Runtime2d,
        Self::RuntimeUi,
        Self::Tooling,
        Self::AssetWatch,
        Self::DesktopWinit,
        Self::RenderWgpu,
        Self::ToolingEgui,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RuntimeCore => "runtime-core",
            Self::Runtime2d => "runtime-2d",
            Self::RuntimeUi => "runtime-ui",
            Self::Tooling => "tooling",
            Self::AssetWatch => "asset-watch",
            Self::DesktopWinit => "desktop-winit",
            Self::RenderWgpu => "render-wgpu",
            Self::ToolingEgui => "tooling-egui",
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
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
