//! Exact encoded ownership, independent of the original executable plan and decoded residency.

use std::{borrow::Cow, io::Write};

#[cfg(any(test, feature = "project-workflow"))]
use nara_asset::pack::CapturedPack;
use nara_asset::pack::{self, Pack, PackError};

use crate::fs::{FileCapability, FsError, FsOperation};

use super::{DecodeLease, RuntimeContentBase, RuntimeContentCatalog, RuntimeContentError};

pub(super) enum EncodedBacking {
    #[cfg(any(test, feature = "project-workflow"))]
    Captured(CapturedPack),
    File {
        pack: Pack,
        catalog: Vec<u8>,
    },
}

pub(super) struct EncodedMember<'a> {
    // Rust drops fields in declaration order. Release the bytes before their shared allowance.
    bytes: Cow<'a, [u8]>,
    _work: Option<DecodeLease>,
}

impl AsRef<[u8]> for EncodedMember<'_> {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

impl RuntimeContentBase {
    pub(super) fn encoded_member(
        &self,
        index: usize,
    ) -> Result<EncodedMember<'_>, RuntimeContentError> {
        match &self.inner.backing {
            #[cfg(any(test, feature = "project-workflow"))]
            EncodedBacking::Captured(pack) => Ok(EncodedMember {
                bytes: Cow::Borrowed(
                    pack.member(index)
                        .ok_or(RuntimeContentError::PackExtentMismatch)?,
                ),
                _work: None,
            }),
            EncodedBacking::File { pack, .. } => {
                let digest = self.inner.catalog.members()[index].digest();
                // Catalogue admission already checked this conversion and the per-member cap.
                let length =
                    usize::try_from(digest.length()).expect("admitted member length fits the host");
                let work = self.inner.budget.reserve(0, length)?;
                let bytes = pack.read_member(index, digest).map_err(pack_error)?;
                Ok(EncodedMember {
                    bytes: Cow::Owned(bytes),
                    _work: Some(work),
                })
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
        let catalog = match &self.inner.backing {
            #[cfg(any(test, feature = "project-workflow"))]
            EncodedBacking::Captured(pack) => return pack.write(output).map_err(pack_error),
            EncodedBacking::File { catalog, .. } => catalog,
        };
        pack::write_catalog(output, catalog).map_err(pack_error)?;
        for index in 0..self.inner.catalog.members().len() {
            let encoded = self.encoded_member(index)?;
            output.write_all(encoded.as_ref()).map_err(|source| {
                RuntimeContentError::Host(FsError::Io {
                    operation: FsOperation::Write,
                    source,
                })
            })?;
        }
        Ok(())
    }
}

/// Reads only the independently bounded catalogue. No payload reaches a codec during open.
pub(super) fn read_catalog(
    file: &FileCapability,
    expected_length: usize,
) -> Result<Vec<u8>, RuntimeContentError> {
    pack::read_catalog(file, expected_length).map_err(pack_error)
}

/// Called only after admission of the complete extent-index allocation.
pub(super) fn open_pack(
    file: FileCapability,
    catalog: &RuntimeContentCatalog,
    catalog_bytes: Vec<u8>,
) -> Result<EncodedBacking, RuntimeContentError> {
    Pack::open(
        file,
        catalog_bytes.len(),
        catalog
            .members()
            .iter()
            .map(|member| member.digest().length()),
    )
    .map(|pack| EncodedBacking::File {
        pack,
        catalog: catalog_bytes,
    })
    .map_err(pack_error)
}

pub(super) fn pack_error(error: PackError) -> RuntimeContentError {
    match error {
        PackError::Io(error) => RuntimeContentError::Host(error),
        PackError::InvalidHeader => RuntimeContentError::InvalidPackHeader,
        PackError::ExtentMismatch => RuntimeContentError::PackExtentMismatch,
        PackError::MemberDigestMismatch => RuntimeContentError::MemberDigestMismatch,
    }
}
