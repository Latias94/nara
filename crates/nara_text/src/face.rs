use std::sync::{Arc, OnceLock};

use thiserror::Error;
use ttf_parser::{Face, name_id};

pub const MAX_FONT_CONTAINER_BYTES: usize = 2 * 1024 * 1024;
pub const BUNDLED_FONT_BYTE_LEN: usize = 438_028;
pub const BUNDLED_FONT_SHA256: &str =
    "12b632bf96ea4e760691f345ef7743c549ca387a018681e11094fcf6146244f1";
pub const BUNDLED_FONT_LICENSE_SHA256: &str =
    "99bfb7c1e845b79d1031fcf37e678178eb4a8c7cc74a3f218ddf7223083a6fcd";

const BUNDLED_FONT_BYTES: &[u8] = include_bytes!("../assets/FiraSans-Bold.ttf");

/// Stable identity for one admitted font face.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontFaceId([u8; 32]);

impl FontFaceId {
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Validated single-face font data owned by the text module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFace {
    bytes: Arc<Vec<u8>>,
    face_index: u32,
    family_name: String,
    id: FontFaceId,
}

impl FontFace {
    pub fn from_bytes(bytes: Vec<u8>, face_index: u32) -> Result<Self, FontFaceError> {
        if bytes.len() > MAX_FONT_CONTAINER_BYTES {
            return Err(FontFaceError::ContainerTooLarge {
                actual: bytes.len(),
                maximum: MAX_FONT_CONTAINER_BYTES,
            });
        }
        if bytes.is_empty() {
            return Err(FontFaceError::Empty);
        }
        if face_index != 0 {
            return Err(FontFaceError::UnsupportedFaceIndex {
                requested: face_index,
            });
        }
        if let Some(faces) = ttf_parser::fonts_in_collection(&bytes) {
            return Err(FontFaceError::CollectionUnsupported { faces });
        }

        let parsed = Face::parse(&bytes, face_index).map_err(|_| FontFaceError::Malformed)?;
        let family_name = preferred_family_name(&parsed).ok_or(FontFaceError::MissingFamilyName)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(&bytes);
        hasher.update(&face_index.to_le_bytes());
        let id = FontFaceId(*hasher.finalize().as_bytes());

        Ok(Self {
            bytes: Arc::new(bytes),
            face_index,
            family_name,
            id,
        })
    }

    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    #[must_use]
    pub const fn face_index(&self) -> u32 {
        self.face_index
    }

    #[must_use]
    pub fn family_name(&self) -> &str {
        &self.family_name
    }

    #[must_use]
    pub const fn id(&self) -> FontFaceId {
        self.id
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    pub(crate) fn shared_bytes(&self) -> Arc<Vec<u8>> {
        Arc::clone(&self.bytes)
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum FontFaceError {
    #[error("font data is empty")]
    Empty,
    #[error("font container is malformed")]
    Malformed,
    #[error("font container has no Unicode family name")]
    MissingFamilyName,
    #[error("font face index {requested} is unsupported; only index zero is admitted")]
    UnsupportedFaceIndex { requested: u32 },
    #[error("font collections are unsupported ({faces} faces)")]
    CollectionUnsupported { faces: u32 },
    #[error("font container has {actual} bytes; maximum is {maximum}")]
    ContainerTooLarge { actual: usize, maximum: usize },
}

pub fn bundled_font_face() -> Result<FontFace, FontFaceError> {
    static BUNDLED_FACE: OnceLock<Result<FontFace, FontFaceError>> = OnceLock::new();
    BUNDLED_FACE
        .get_or_init(|| FontFace::from_bytes(BUNDLED_FONT_BYTES.to_vec(), 0))
        .clone()
}

fn preferred_family_name(face: &Face<'_>) -> Option<String> {
    [name_id::TYPOGRAPHIC_FAMILY, name_id::FAMILY]
        .into_iter()
        .find_map(|wanted| {
            face.names()
                .into_iter()
                .filter(|name| name.name_id == wanted)
                .filter_map(|name| name.to_string())
                .find(|name| !name.trim().is_empty())
        })
}
