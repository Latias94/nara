use std::{borrow::Cow, io::Write, mem::size_of, sync::Arc};

use nara_core::vfs::capabilities::{ContentDigest, FileCapability, FsError};

use crate::{
    AssetDomain, AssetProductRef, AssetReferenceDemand, AssetRuntimeDecodeError,
    AssetRuntimeLimits, RuntimeAssetCodec, decode_runtime_asset,
};

use super::{
    CapturedPack, DecodeBudget, DecodeBudgetError, DecodeLease, Pack, PackError, verify_member,
    write_catalog, write_error,
};

/// Encoded content behind an independently admitted catalog.
pub struct PackBacking(Backing);

enum Backing {
    Captured(CapturedPack),
    File { pack: Pack, catalog: Vec<u8> },
}

/// Failure to read or write an admitted backing within its shared work budget.
#[derive(Debug, thiserror::Error)]
pub enum PackBackingError {
    #[error(transparent)]
    Pack(#[from] PackError),
    #[error(transparent)]
    Budget(#[from] DecodeBudgetError),
}

/// One member of an admitted catalog, using the budget shared with its backing's other readers.
pub struct PackMember<'a> {
    backing: &'a PackBacking,
    budget: &'a Arc<DecodeBudget>,
    index: usize,
    digest: ContentDigest,
}

struct EncodedMember<'a> {
    bytes: Cow<'a, [u8]>,
    _work: Option<DecodeLease>,
}

impl AsRef<[u8]> for EncodedMember<'_> {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

/// Failure to prepare a typed asset from this backing with the selected codec's frozen limits.
#[derive(Debug, thiserror::Error)]
pub enum AssetPreparationError<E> {
    #[error(transparent)]
    Backing(#[from] PackBackingError),
    #[error(transparent)]
    Decode(#[from] AssetRuntimeDecodeError<E>),
    #[error("the encoded asset requires {required} bytes but only {limit} are covered")]
    EncodedBytes { required: usize, limit: usize },
    #[error("the retained asset size exceeds the host integer range")]
    RetainedBytesOverflow,
}

impl PackBacking {
    /// Checks the raw members against the admitted catalog and shares their original buffers
    /// with the returned pack. The caller admits the complete retained storage before capture.
    pub fn capture(
        catalog: Vec<u8>,
        members: Vec<Vec<u8>>,
        member_digests: impl ExactSizeIterator<Item = ContentDigest>,
    ) -> Result<(Self, CapturedPack), PackError> {
        if members.len() != member_digests.len() {
            return Err(PackError::ExtentMismatch);
        }
        for (bytes, digest) in members.iter().zip(member_digests) {
            verify_member(bytes, digest)?;
        }
        let pack = CapturedPack::new(catalog, members)?;
        Ok((Self(Backing::Captured(pack.clone())), pack))
    }

    /// Captures each member from a reader, authenticating it before requesting the next one.
    /// The caller admits retained storage before capture and bounds each read by its digest.
    pub fn capture_from(
        catalog: Vec<u8>,
        member_digests: impl ExactSizeIterator<Item = ContentDigest>,
        mut read: impl FnMut(usize, ContentDigest) -> Result<Vec<u8>, FsError>,
    ) -> Result<Self, PackError> {
        let mut members = Vec::with_capacity(member_digests.len());
        for (index, digest) in member_digests.enumerate() {
            let bytes = read(index, digest)?;
            verify_member(&bytes, digest)?;
            members.push(bytes);
        }
        let pack = CapturedPack::new(catalog, members)?;
        Ok(Self(Backing::Captured(pack)))
    }

    /// Retains the original file, catalog bytes, and indexed extents after catalog and storage
    /// admission. Supply the capability used to read the catalog and its declared member order.
    pub fn open(
        file: FileCapability,
        catalog: Vec<u8>,
        member_lengths: impl ExactSizeIterator<Item = u64>,
    ) -> Result<Self, PackError> {
        let pack = Pack::open(file, catalog.len(), member_lengths)?;
        Ok(Self(Backing::File { pack, catalog }))
    }

    /// Selects an index and its digest from the same admitted catalog used to create the backing.
    #[must_use]
    pub fn member<'a>(
        &'a self,
        budget: &'a Arc<DecodeBudget>,
        index: usize,
        digest: ContentDigest,
    ) -> PackMember<'a> {
        PackMember {
            backing: self,
            budget,
            index,
            digest,
        }
    }

    /// Writes the original catalog and all members at the current stream position. Held-file
    /// members are read and authenticated in admitted digest order under the shared work budget.
    /// The caller owns fresh destination creation and durability. Failure can leave partial bytes.
    pub fn write(
        &self,
        output: &mut (impl Write + ?Sized),
        budget: &Arc<DecodeBudget>,
        member_digests: impl ExactSizeIterator<Item = ContentDigest>,
    ) -> Result<(), PackBackingError> {
        let catalog = match &self.0 {
            Backing::Captured(pack) => return Ok(pack.write(output)?),
            Backing::File { pack, catalog } => {
                if member_digests.len() != pack.member_ends.len() {
                    return Err(PackError::ExtentMismatch.into());
                }
                catalog
            }
        };
        write_catalog(output, catalog)?;
        for (index, digest) in member_digests.enumerate() {
            let encoded = self.member(budget, index, digest).read()?;
            output.write_all(encoded.as_ref()).map_err(write_error)?;
        }
        Ok(())
    }
}

impl<'a> PackMember<'a> {
    /// Borrows captured bytes or authenticates bytes read from the held file. File bytes retain
    /// their temporary work reservation until the returned owner is dropped.
    /// Captured bytes were authenticated at capture, so their digest is used only for extent and
    /// codec limits here. Select the digest from the original admitted catalog.
    pub fn read(&self) -> Result<impl AsRef<[u8]> + use<'a>, PackBackingError> {
        match &self.backing.0 {
            Backing::Captured(pack) => {
                let bytes = pack.member(self.index).ok_or(PackError::ExtentMismatch)?;
                if bytes.len() as u64 != self.digest.length() {
                    return Err(PackError::ExtentMismatch.into());
                }
                Ok(EncodedMember {
                    bytes: Cow::Borrowed(bytes),
                    _work: None,
                })
            }
            Backing::File { pack, .. } => {
                let length =
                    usize::try_from(self.digest.length()).map_err(|_| PackError::ExtentMismatch)?;
                let work = self.budget.reserve(0, length)?;
                let bytes = pack.read_member(self.index, self.digest)?;
                Ok(EncodedMember {
                    bytes: Cow::Owned(bytes),
                    _work: Some(work),
                })
            }
        }
    }

    /// Prepares a selected codec's typed output under this member's shared budget.
    ///
    /// Supply every product mapped to the member, the selected limits, and the caller's enclosing
    /// storage cost excluding the inline product. The inventory is created after reserving the
    /// codec's promises and before reading bytes. Its validation precedes the retention query.
    /// The returned product must remain covered by its lease until its storage is released.
    pub fn prepare_asset<D, C, E, Inventory>(
        &self,
        products: &[AssetProductRef],
        limits: AssetRuntimeLimits,
        output_metadata: usize,
        create_inventory: impl FnOnce() -> Result<Inventory, E>,
        visit_dependency: impl FnMut(
            &mut Inventory,
            AssetProductRef,
            AssetReferenceDemand,
        ) -> Result<(), E>,
        finish_dependencies: impl FnOnce(Inventory) -> Result<(), E>,
    ) -> Result<(D::Product, DecodeLease, usize), AssetPreparationError<E>>
    where
        D: AssetDomain,
        C: RuntimeAssetCodec<D::Product>,
    {
        let limit = limits.encoded_bytes().get();
        let length = usize::try_from(self.digest.length()).map_err(|_| {
            AssetPreparationError::EncodedBytes {
                required: usize::MAX,
                limit,
            }
        })?;
        if length > limit {
            return Err(AssetPreparationError::EncodedBytes {
                required: length,
                limit,
            });
        }
        let retained_limit = limits.retained_bytes();
        let inline = size_of::<D::Product>();
        if inline > retained_limit.get() {
            return Err(AssetRuntimeDecodeError::RetainedBytes {
                required: inline,
                limit: retained_limit.get(),
            }
            .into());
        }
        let promised = retained_limit
            .get()
            .checked_add(output_metadata)
            .ok_or(AssetPreparationError::RetainedBytesOverflow)?;
        let mut lease = self
            .budget
            .reserve(promised, limits.work_bytes().get())
            .map_err(PackBackingError::from)?;
        let inventory = create_inventory().map_err(AssetRuntimeDecodeError::Dependency)?;
        let encoded = self.read()?;
        let (value, product_retained) = decode_runtime_asset::<D, C, _, _>(
            encoded,
            products,
            retained_limit,
            inventory,
            visit_dependency,
            finish_dependencies,
        )?;
        let retained = product_retained
            .checked_add(output_metadata)
            .ok_or(AssetPreparationError::RetainedBytesOverflow)?;
        lease
            .finish_decode(retained)
            .map_err(PackBackingError::from)?;
        Ok((value, lease, retained))
    }
}
