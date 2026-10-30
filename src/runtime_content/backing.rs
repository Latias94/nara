//! Exact encoded ownership, independent of the original executable plan and decoded residency.

use std::io::{Read, Seek, SeekFrom, Write};

use nara_fs::{ContentDigest, FileCapability, FsError, FsOperation};

use super::{DecodeLease, RuntimeContentBase, RuntimeContentCatalog, RuntimeContentError};

const MAGIC: &[u8; 8] = b"NARAPACK";
const VERSION: u32 = 1;
const HEADER_BYTES: u64 = 20;

pub(super) enum EncodedBacking {
    Captured(Box<[Vec<u8>]>),
    Pack {
        file: FileCapability,
        offsets: Box<[u64]>,
    },
}

pub(super) enum EncodedMember<'a> {
    Captured(&'a [u8]),
    // Rust drops fields in declaration order. Release the bytes before their shared allowance.
    Read { bytes: Vec<u8>, _work: DecodeLease },
}

impl AsRef<[u8]> for EncodedMember<'_> {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Captured(bytes) => bytes,
            Self::Read { bytes, .. } => bytes,
        }
    }
}

impl RuntimeContentBase {
    pub(super) fn encoded_member(
        &self,
        index: usize,
    ) -> Result<EncodedMember<'_>, RuntimeContentError> {
        match &self.inner.backing {
            EncodedBacking::Captured(members) => Ok(EncodedMember::Captured(&members[index])),
            EncodedBacking::Pack { file, offsets } => {
                let digest = self.inner.catalog.members()[index].digest();
                // Catalogue admission already checked this conversion and the per-member cap.
                let length =
                    usize::try_from(digest.length()).expect("admitted member length fits the host");
                let work = self.inner.budget.reserve(0, length)?;
                let mut reader = file.reader().map_err(RuntimeContentError::Host)?;
                reader
                    .seek(SeekFrom::Start(offsets[index]))
                    .map_err(read_error)?;
                let bytes = read_exact_bytes(&mut reader, length)?;
                if ContentDigest::of_bytes(&bytes) != digest {
                    return Err(RuntimeContentError::MemberDigestMismatch);
                }
                Ok(EncodedMember::Read { bytes, _work: work })
            }
        }
    }

    /// Writes a version-1 local pack from this exact admitted base, without domain decoding.
    ///
    /// The stream starts at its current position. The caller owns fresh destination creation,
    /// publication and durability; a failed write can leave partial bytes. Pack reads authenticate
    /// each member again, and never fall back to a different file or content context.
    pub fn write_pack(
        &self,
        output: &mut (impl Write + ?Sized),
    ) -> Result<(), RuntimeContentError> {
        output.write_all(MAGIC).map_err(write_error)?;
        output
            .write_all(&VERSION.to_le_bytes())
            .map_err(write_error)?;
        output
            .write_all(&(self.inner.catalog_bytes.len() as u64).to_le_bytes())
            .map_err(write_error)?;
        output
            .write_all(&self.inner.catalog_bytes)
            .map_err(write_error)?;
        for index in 0..self.inner.catalog.members().len() {
            let encoded = self.encoded_member(index)?;
            output.write_all(encoded.as_ref()).map_err(write_error)?;
        }
        Ok(())
    }
}

/// Reads only the independently bounded catalogue. No payload reaches a codec during open.
pub(super) fn read_catalog(
    file: &FileCapability,
    expected_length: usize,
) -> Result<Vec<u8>, RuntimeContentError> {
    let mut reader = file.reader().map_err(RuntimeContentError::Host)?;
    let mut header = [0_u8; HEADER_BYTES as usize];
    reader.read_exact(&mut header).map_err(read_error)?;
    if &header[..8] != MAGIC
        || header[8..12] != VERSION.to_le_bytes()
        || header[12..20] != (expected_length as u64).to_le_bytes()
    {
        return Err(RuntimeContentError::InvalidPackHeader);
    }
    read_exact_bytes(&mut reader, expected_length)
}

/// Called only after admission of the complete extent-index allocation.
pub(super) fn open_pack(
    file: FileCapability,
    catalog: &RuntimeContentCatalog,
    catalog_length: usize,
) -> Result<EncodedBacking, RuntimeContentError> {
    let mut offset = HEADER_BYTES
        .checked_add(catalog_length as u64)
        .ok_or(RuntimeContentError::PackExtentMismatch)?;
    let mut offsets = Vec::with_capacity(catalog.members().len());
    for member in catalog.members() {
        offsets.push(offset);
        offset = offset
            .checked_add(member.digest().length())
            .filter(|end| *end <= i64::MAX as u64)
            .ok_or(RuntimeContentError::PackExtentMismatch)?;
    }
    let actual_length = file
        .reader()
        .map_err(RuntimeContentError::Host)?
        .seek(SeekFrom::End(0))
        .map_err(read_error)?;
    if actual_length != offset {
        return Err(RuntimeContentError::PackExtentMismatch);
    }
    Ok(EncodedBacking::Pack {
        file,
        offsets: offsets.into_boxed_slice(),
    })
}

fn read_exact_bytes(reader: &mut impl Read, length: usize) -> Result<Vec<u8>, RuntimeContentError> {
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length).map_err(|_| {
        read_error(std::io::Error::new(
            std::io::ErrorKind::OutOfMemory,
            "bounded Runtime pack member allocation failed",
        ))
    })?;
    bytes.resize(length, 0);
    reader.read_exact(&mut bytes).map_err(read_error)?;
    Ok(bytes)
}

fn read_error(source: std::io::Error) -> RuntimeContentError {
    RuntimeContentError::Host(FsError::Io {
        operation: FsOperation::Read,
        source,
    })
}

fn write_error(source: std::io::Error) -> RuntimeContentError {
    RuntimeContentError::Host(FsError::Io {
        operation: FsOperation::Write,
        source,
    })
}
