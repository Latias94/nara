use ron::{error::SpannedError, value::RawValue};
use serde::{Deserialize, Serialize};

use crate::{AssetSourceKind, StableAssetId};

pub(crate) const MAX_META_BYTES: usize = 64 * 1024;

pub(crate) const IMAGE_IMPORTER: &str = "nara.image";
pub(crate) const SCENE_IMPORTER: &str = "nara.scene";
pub(crate) const PREFAB_IMPORTER: &str = "nara.prefab";
#[cfg(any(unix, windows))]
pub(crate) const RAW_IMPORTER: &str = "nara.raw";

/// The stable identity and importer settings stored in a source file's `.meta` sidecar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetMeta {
    document: MetaDocument,
    source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename = "AssetMeta", deny_unknown_fields)]
struct MetaDocument {
    id: StableAssetId,
    importer: String,
    settings: Box<RawValue>,
}

impl AssetMeta {
    /// Parses a UTF-8 sidecar up to 64 KiB without interpreting importer-specific settings.
    pub fn from_ron(source: impl Into<String>) -> Result<Self, AssetMetaError> {
        let source = source.into();
        if source.len() > MAX_META_BYTES {
            return Err(AssetMetaError::TooLarge);
        }
        let document: MetaDocument = ron::from_str(&source)?;
        if document.id.as_uuid().is_nil() {
            return Err(AssetMetaError::NilId);
        }
        if document.importer.is_empty()
            || document
                .importer
                .chars()
                .any(|character| character.is_whitespace() || character.is_control())
        {
            return Err(AssetMetaError::InvalidImporter);
        }
        Ok(Self { document, source })
    }

    #[must_use]
    pub const fn id(&self) -> StableAssetId {
        self.document.id
    }

    #[must_use]
    pub fn importer(&self) -> &str {
        &self.document.importer
    }

    /// Returns the exact sidecar text, including comments and whitespace.
    #[must_use]
    pub fn as_ron(&self) -> &str {
        &self.source
    }

    /// Decodes the settings as the importer's chosen type.
    ///
    /// Newly discovered assets start with `()`, which denotes importer defaults.
    pub fn settings<'a, T: Deserialize<'a>>(&'a self) -> Result<T, SpannedError> {
        self.document.settings.into_rust()
    }

    /// Classifies built-in importers. Other IDs are preserved for their owning importer.
    #[must_use]
    pub fn source_kind(&self) -> AssetSourceKind {
        match self.importer() {
            IMAGE_IMPORTER => AssetSourceKind::Image,
            SCENE_IMPORTER => AssetSourceKind::Scene,
            PREFAB_IMPORTER => AssetSourceKind::Prefab,
            importer => AssetSourceKind::Other(importer.to_owned()),
        }
    }

    #[cfg(any(unix, windows))]
    pub(crate) fn generated(id: StableAssetId, importer: &str) -> Result<Self, ron::Error> {
        let document = MetaDocument {
            id,
            importer: importer.to_owned(),
            settings: RawValue::from_rust(&())?,
        };
        let mut source = ron::ser::to_string_pretty(
            &document,
            ron::ser::PrettyConfig::default()
                .struct_names(true)
                .new_line("\n"),
        )?;
        source.push('\n');
        Ok(Self { document, source })
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AssetMetaError {
    #[error("asset sidecar exceeds 64 KiB")]
    TooLarge,
    #[error("asset sidecar is not UTF-8")]
    Utf8(#[source] std::str::Utf8Error),
    #[error("invalid RON: {0}")]
    Ron(#[from] SpannedError),
    #[error("asset id must not be nil")]
    NilId,
    #[error("asset importer must be nonempty and contain no whitespace or control characters")]
    InvalidImporter,
}
