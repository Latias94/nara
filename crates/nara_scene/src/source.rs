use std::{error::Error, fmt, sync::Arc};

use nara_asset::{AssetPath, StableAssetId};
use nara_core::SourceUniverseId;

#[cfg(feature = "serde")]
mod wire;

/// The authored kind of one entity-bearing source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceDocumentKind {
    Scene,
    Prefab,
}

/// The semantic origin of one source document.
///
/// File identity and content digests are admission evidence, not members of this value. A path
/// origin is intentionally scoped by an authored universe and is not rename continuity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceDocumentOrigin {
    Named {
        universe: SourceUniverseId,
        name: Arc<str>,
    },
    ProjectLocation {
        universe: SourceUniverseId,
        path: AssetPath,
    },
    StableAsset {
        stable_id: StableAssetId,
    },
}

/// Canonical semantic identity for one admitted Scene or Prefab document.
///
/// This identifies an entity-bearing source document, not an auxiliary imported product.
/// An explicit primary product selector uses its admitted `StableAsset` origin; it does not
/// introduce another source-key origin or establish that a selected domain product exists.
///
/// With the `serde` feature, the representation preserves the document kind and explicit
/// origin. Deserialization validates an identity claim; it does not resolve a source or grant
/// Host admission. Standalone serde is not file admission: the containing Scene format owns
/// input preflight and aggregate bounds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceDocumentKey {
    origin: SourceDocumentOrigin,
    kind: SourceDocumentKind,
}

impl SourceDocumentKey {
    /// Creates an explicit code-authored source identity.
    pub fn named(
        universe: SourceUniverseId,
        name: impl Into<String>,
        kind: SourceDocumentKind,
    ) -> Result<Self, SourceDocumentKeyError> {
        let name = name.into();
        if name.is_empty() {
            return Err(SourceDocumentKeyError::EmptyName);
        }
        if name.contains('\0') {
            return Err(SourceDocumentKeyError::NameContainsNull);
        }
        Ok(Self {
            origin: SourceDocumentOrigin::Named {
                universe,
                name: Arc::from(name),
            },
            kind,
        })
    }

    /// Creates an identity for an intentionally location-authored project document.
    #[must_use]
    pub const fn project_location(
        universe: SourceUniverseId,
        path: AssetPath,
        kind: SourceDocumentKind,
    ) -> Self {
        Self {
            origin: SourceDocumentOrigin::ProjectLocation { universe, path },
            kind,
        }
    }

    /// Creates an identity backed by an admitted stable asset metadata record.
    #[must_use]
    pub const fn stable_asset(stable_id: StableAssetId, kind: SourceDocumentKind) -> Self {
        Self {
            origin: SourceDocumentOrigin::StableAsset { stable_id },
            kind,
        }
    }

    #[must_use]
    pub const fn origin(&self) -> &SourceDocumentOrigin {
        &self.origin
    }

    #[must_use]
    pub const fn kind(&self) -> SourceDocumentKind {
        self.kind
    }

    /// Returns the modeled owned bytes charged by Scene retention accounting.
    ///
    /// Shared names are conservatively charged for every retained key. This models logical
    /// ownership rather than allocator capacity or process resident memory.
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.checked_retained_bytes().unwrap_or(usize::MAX)
    }

    pub(crate) fn checked_retained_bytes(&self) -> Option<usize> {
        let dynamic = match &self.origin {
            SourceDocumentOrigin::Named { name, .. } => {
                crate::provenance::provenance_string_bytes(name)?
            }
            SourceDocumentOrigin::ProjectLocation { path, .. } => {
                crate::provenance::provenance_string_bytes(path.as_str())?
            }
            SourceDocumentOrigin::StableAsset { .. } => 0,
        };
        std::mem::size_of::<Self>().checked_add(dynamic)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceDocumentKeyError {
    EmptyName,
    NameContainsNull,
}

impl fmt::Display for SourceDocumentKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("source document name must not be empty"),
            Self::NameContainsNull => {
                formatter.write_str("source document name must not contain NUL")
            }
        }
    }
}

impl Error for SourceDocumentKeyError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn universe() -> SourceUniverseId {
        "72011426-37e2-48ba-bc06-78841468889a"
            .parse()
            .expect("test source universe is valid")
    }

    #[test]
    fn source_key_keeps_origin_and_document_kind_as_distinct_axes() {
        let path = AssetPath::new("scenes/startup.scene.json").unwrap();
        let scene = SourceDocumentKey::project_location(
            universe(),
            path.clone(),
            SourceDocumentKind::Scene,
        );
        let prefab =
            SourceDocumentKey::project_location(universe(), path, SourceDocumentKind::Prefab);

        assert_ne!(scene, prefab);
        assert_eq!(scene.kind(), SourceDocumentKind::Scene);
        assert!(matches!(
            scene.origin(),
            SourceDocumentOrigin::ProjectLocation { universe: value, path }
                if *value == universe() && path.as_str() == "scenes/startup.scene.json"
        ));
    }

    #[test]
    fn named_source_rejects_empty_and_nul_names_without_fallback() {
        assert_eq!(
            SourceDocumentKey::named(universe(), "", SourceDocumentKind::Scene),
            Err(SourceDocumentKeyError::EmptyName)
        );
        assert_eq!(
            SourceDocumentKey::named(universe(), "bad\0name", SourceDocumentKind::Scene),
            Err(SourceDocumentKeyError::NameContainsNull)
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn source_key_wire_preserves_document_identity_without_product_origins() {
        let key = SourceDocumentKey::stable_asset(
            "00000000-0000-4000-8000-000000000041".parse().unwrap(),
            SourceDocumentKind::Prefab,
        );
        let json = serde_json::to_string(&key).unwrap();
        let ron = ron::to_string(&key).unwrap();
        assert_eq!(
            serde_json::from_str::<SourceDocumentKey>(&json).unwrap(),
            key
        );
        assert_eq!(ron::from_str::<SourceDocumentKey>(&ron).unwrap(), key);
        assert!(
            serde_json::from_value::<SourceDocumentKey>(serde_json::json!({
                "kind": "prefab",
                "origin": {"product": {
                    "source": "00000000-0000-4000-8000-000000000041",
                    "product": {"kind": "primary"}
                }}
            }))
            .is_err()
        );
    }
}
