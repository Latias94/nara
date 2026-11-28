use std::{
    fs::File,
    io::{self, Read as _},
    path::Path,
};

use nara_reflect::{ComponentCodecError, Schema};

use super::{SaveError, path_error};

const MAGIC: &[u8; 4] = b"NSAV";
const FORMAT_VERSION: u16 = 1;
const PREFIX_BYTES: usize = 32;
const HEADER_BYTES: usize = 64;
const MAX_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct SaveSchema {
    pub(super) version: u16,
    hash: [u8; 16],
}

impl SaveSchema {
    pub(super) fn of<T: Schema>() -> Result<Self, SaveError> {
        let schema = T::type_schema()?;
        if !schema.supports_postcard() {
            return Err(ComponentCodecError::PostcardUnavailable.into());
        }
        let schema_version = schema.version().get();
        let version = u16::try_from(schema_version)
            .ok()
            .filter(|&version| version != 0)
            .ok_or(SaveError::InvalidSchemaVersion(schema_version))?;
        let mut hash = [0; 16];
        hash.copy_from_slice(&schema.postcard_schema_hash().as_bytes()[..16]);
        Ok(Self { version, hash })
    }
}

pub(super) struct SaveFile {
    pub(super) schema: SaveSchema,
    pub(super) bytes: Vec<u8>,
}

impl SaveFile {
    pub(super) fn read(path: &Path, mut file: File, size: u64) -> Result<Self, SaveError> {
        let corrupt = |reason| SaveError::Corrupt {
            path: path.to_owned(),
            reason,
        };
        let size = usize::try_from(size)
            .ok()
            .filter(|size| (HEADER_BYTES..=HEADER_BYTES + MAX_PAYLOAD_BYTES).contains(size))
            .ok_or_else(|| corrupt("file size is outside the NSAV limits"))?;
        let mut bytes = vec![0; size];
        match file.read_exact(&mut bytes) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(corrupt("file ended during the read"));
            }
            Err(error) => return Err(path_error(path, error)),
        }
        if file
            .read(&mut [0])
            .map_err(|error| path_error(path, error))?
            != 0
        {
            return Err(corrupt("unexpected bytes after the payload"));
        }

        let (header, payload) = bytes.split_at(HEADER_BYTES);
        if &header[..4] != MAGIC {
            return Err(corrupt("invalid NSAV magic"));
        }
        let len = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);
        if len as usize != payload.len() {
            return Err(corrupt("payload length does not match the header"));
        }
        if digest(&header[..PREFIX_BYTES], payload).as_bytes() != &header[PREFIX_BYTES..] {
            return Err(corrupt("BLAKE3 digest mismatch"));
        }
        let format_version = u16::from_le_bytes([header[4], header[5]]);
        if format_version != FORMAT_VERSION {
            return Err(SaveError::UnsupportedFormatVersion(format_version));
        }
        if header[8..12] != [0; 4] {
            return Err(corrupt("nonzero NSAV v1 flags"));
        }
        let version = u16::from_le_bytes([header[6], header[7]]);
        if version == 0 {
            return Err(corrupt("zero schema version"));
        }
        let mut hash = [0; 16];
        hash.copy_from_slice(&header[16..PREFIX_BYTES]);
        Ok(Self {
            schema: SaveSchema { version, hash },
            bytes,
        })
    }

    pub(super) fn payload(&self) -> &[u8] {
        &self.bytes[HEADER_BYTES..]
    }

    pub(super) fn decode<T: Schema>(
        &self,
        path: &Path,
        expected: SaveSchema,
    ) -> Result<T, SaveError> {
        if self.schema.version != expected.version {
            return Err(SaveError::SchemaVersionMismatch {
                expected: expected.version,
                found: self.schema.version,
            });
        }
        if self.schema.hash != expected.hash {
            return Err(SaveError::SchemaHashMismatch);
        }
        T::decode_postcard(self.payload()).map_err(|source| SaveError::Decode {
            path: path.to_owned(),
            source,
        })
    }
}

pub(super) fn encode(schema: SaveSchema, payload: &[u8]) -> Result<Vec<u8>, SaveError> {
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(SaveError::PayloadTooLarge(payload.len()));
    }
    let mut bytes = Vec::with_capacity(HEADER_BYTES + payload.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&schema.version.to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&schema.hash);
    let checksum = digest(&bytes, payload);
    bytes.extend_from_slice(checksum.as_bytes());
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

fn digest(prefix: &[u8], payload: &[u8]) -> blake3::Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(prefix);
    hasher.update(payload);
    hasher.finalize()
}
