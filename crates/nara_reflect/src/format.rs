use std::io;

use nara_core::ByteLimit;

use crate::{ComponentRegistryError, ComponentSchema, ComponentSchemaCatalog};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentCatalogFileLimits {
    encoded_bytes: ByteLimit,
}

impl Default for ComponentCatalogFileLimits {
    fn default() -> Self {
        Self {
            encoded_bytes: ByteLimit::new(8 * 1024 * 1024).expect("catalog limit is non-zero"),
        }
    }
}

impl ComponentCatalogFileLimits {
    #[must_use]
    pub const fn encoded_bytes(self) -> ByteLimit {
        self.encoded_bytes
    }

    #[must_use]
    pub const fn with_encoded_bytes(self, encoded_bytes: ByteLimit) -> Self {
        Self { encoded_bytes }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ComponentCatalogFileError {
    #[error("Schema catalog exceeds {maximum} encoded bytes")]
    EncodedBytesExceeded { maximum: usize },
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Catalog(#[from] ComponentRegistryError),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogPayload {
    components: Vec<ComponentSchema>,
}

impl ComponentSchemaCatalog {
    pub fn from_json_bytes(encoded: &[u8]) -> Result<Self, ComponentCatalogFileError> {
        Self::from_json_bytes_with_limits(encoded, ComponentCatalogFileLimits::default())
    }

    pub fn from_json_bytes_with_limits(
        encoded: &[u8],
        limits: ComponentCatalogFileLimits,
    ) -> Result<Self, ComponentCatalogFileError> {
        let maximum = limits.encoded_bytes().get();
        if encoded.len() > maximum {
            return Err(ComponentCatalogFileError::EncodedBytesExceeded { maximum });
        }
        let CatalogPayload { components } = serde_json::from_slice(encoded)?;
        Ok(Self::from_components(components)?)
    }

    pub fn to_json_string(&self) -> Result<String, ComponentCatalogFileError> {
        let maximum = ComponentCatalogFileLimits::default().encoded_bytes().get();
        let mut writer = CatalogWriter {
            bytes: Vec::new(),
            maximum,
        };
        if let Err(error) = serde_json::to_writer_pretty(&mut writer, self) {
            if error.io_error_kind() == Some(io::ErrorKind::FileTooLarge) {
                return Err(ComponentCatalogFileError::EncodedBytesExceeded { maximum });
            }
            return Err(error.into());
        }
        Ok(String::from_utf8(writer.bytes)?)
    }
}

struct CatalogWriter {
    bytes: Vec<u8>,
    maximum: usize,
}

impl io::Write for CatalogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.maximum - self.bytes.len() {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
