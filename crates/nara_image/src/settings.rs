use nara_asset::ImportSettingsHash;

use crate::ImageColorSpace;

const SRGB_CANONICAL_SETTINGS: &[u8] = b"nara.image.import-settings.v1\0color_space\0srgb";
const LINEAR_CANONICAL_SETTINGS: &[u8] = b"nara.image.import-settings.v1\0color_space\0linear";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ImageImportSettings {
    color_space: ImageColorSpace,
}

impl ImageImportSettings {
    #[must_use]
    pub const fn new(color_space: ImageColorSpace) -> Self {
        Self { color_space }
    }

    #[must_use]
    pub const fn color_space(self) -> ImageColorSpace {
        self.color_space
    }

    #[must_use]
    pub const fn canonical_bytes(self) -> &'static [u8] {
        match self.color_space {
            ImageColorSpace::Srgb => SRGB_CANONICAL_SETTINGS,
            ImageColorSpace::Linear => LINEAR_CANONICAL_SETTINGS,
        }
    }

    #[must_use]
    pub fn settings_hash(self) -> ImportSettingsHash {
        ImportSettingsHash::from_bytes(self.canonical_bytes())
    }
}

impl Default for ImageImportSettings {
    fn default() -> Self {
        Self::new(ImageColorSpace::Srgb)
    }
}
