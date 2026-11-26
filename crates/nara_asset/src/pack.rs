//! Wire I/O and bounded member acquisition for NARAPACK v1.
//!
//! A pack stores a fixed header, opaque catalog bytes, then concatenated members in catalog order.
//! The caller admits the catalog and supplies executable selection, digests, and limits.

mod backing;
mod budget;

use std::{
    fmt,
    io::{self, Read, Seek, SeekFrom, Write},
    mem::size_of,
    sync::Arc,
};

use nara_core::vfs::capabilities::{ContentDigest, FileCapability, FsError, FsOperation};

pub use backing::{AssetPreparationError, PackBacking, PackBackingError, PackMember};
pub use budget::{DecodeBudget, DecodeBudgetError, DecodeLease};

const MAGIC: &[u8; 8] = b"NARAPACK";
const VERSION: u32 = 1;
const HEADER_BYTES: u64 = 20;

/// Failures while reading or writing the version-one wire layout.
#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error(transparent)]
    Io(#[from] FsError),
    #[error("pack header differs from version one or the expected catalog length")]
    InvalidHeader,
    #[error("pack extent differs from the declared layout")]
    ExtentMismatch,
    #[error("pack member differs from the expected digest")]
    MemberDigestMismatch,
}

/// Immutable catalog and ordered member buffers captured by a version-one pack producer.
///
/// Clones share the original byte allocations. The producer owns catalog and member admission.
#[derive(Clone)]
pub struct CapturedPack(Arc<CapturedPackInner>);

struct CapturedPackInner {
    catalog: Vec<u8>,
    members: Box<[Vec<u8>]>,
    catalog_digest: ContentDigest,
    encoded_bytes: u64,
    retained_bytes: usize,
}

impl fmt::Debug for CapturedPack {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapturedPack")
            .field("catalog_digest", &self.0.catalog_digest)
            .field("members", &self.0.members.len())
            .field("encoded_bytes", &self.0.encoded_bytes)
            .finish()
    }
}

impl CapturedPack {
    /// Takes ownership of the original buffers in catalog order and checks the wire extent.
    pub fn new(catalog: Vec<u8>, members: Vec<Vec<u8>>) -> Result<Self, PackError> {
        let encoded_bytes = encoded_bytes(
            catalog.len(),
            members.iter().map(|bytes| bytes.len() as u64),
        )?;
        let payload_bytes =
            usize::try_from(encoded_bytes - HEADER_BYTES).map_err(|_| PackError::ExtentMismatch)?;
        let retained_bytes = Self::retained_bytes_for(payload_bytes, members.len())
            .ok_or(PackError::ExtentMismatch)?;
        Ok(Self(Arc::new(CapturedPackInner {
            catalog_digest: ContentDigest::of_bytes(&catalog),
            catalog,
            members: members.into_boxed_slice(),
            encoded_bytes,
            retained_bytes,
        })))
    }

    #[must_use]
    pub fn catalog_bytes(&self) -> &[u8] {
        &self.0.catalog
    }

    #[must_use]
    pub fn catalog_digest(&self) -> ContentDigest {
        self.0.catalog_digest
    }

    #[must_use]
    pub fn member(&self, index: usize) -> Option<&[u8]> {
        self.0.members.get(index).map(Vec::as_slice)
    }

    #[must_use]
    pub fn encoded_bytes(&self) -> u64 {
        self.0.encoded_bytes
    }

    /// Logical retained size of the shared allocation, excluding the shallow handle.
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.0.retained_bytes
    }

    /// Computes the same logical size before retaining a catalog and its ordered member table.
    #[must_use]
    pub fn retained_bytes_for(payload_bytes: usize, member_count: usize) -> Option<usize> {
        member_count
            .checked_mul(size_of::<Vec<u8>>())?
            .checked_add(size_of::<CapturedPackInner>() + 2 * size_of::<usize>())?
            .checked_add(payload_bytes)
    }

    /// Writes the original bytes at the current position. A failed write can leave partial bytes.
    pub fn write(&self, output: &mut (impl Write + ?Sized)) -> Result<(), PackError> {
        write_catalog(output, self.catalog_bytes())?;
        for member in &self.0.members {
            output.write_all(member).map_err(write_error)?;
        }
        Ok(())
    }
}

/// One held file capability and its exact ordered member layout.
pub struct Pack {
    file: FileCapability,
    members_start: u64,
    member_ends: Box<[u64]>,
}

impl Pack {
    /// Returns the heap bytes used by the index, excluding the inline size of `Pack`.
    #[must_use]
    pub fn index_bytes(member_count: usize) -> Option<usize> {
        member_count.checked_mul(size_of::<u64>())
    }

    /// Indexes the held file after the caller admits the catalog and its index storage.
    ///
    /// Supply the same file used by [`read_catalog`] and the lengths in catalog order. The file
    /// must match the entire declared extent. The index retains only the ordered end offsets.
    pub fn open(
        file: FileCapability,
        catalog_length: usize,
        member_lengths: impl ExactSizeIterator<Item = u64>,
    ) -> Result<Self, PackError> {
        let members_start = advance(HEADER_BYTES, catalog_length as u64)?;
        let mut end = members_start;
        let mut member_ends = Vec::new();
        member_ends
            .try_reserve_exact(member_lengths.len())
            .map_err(|_| allocation_error("bounded Runtime pack index allocation failed"))?;
        for length in member_lengths {
            end = advance(end, length)?;
            member_ends.push(end);
        }
        let actual_length = file.reader()?.seek(SeekFrom::End(0)).map_err(read_error)?;
        if actual_length != end {
            return Err(PackError::ExtentMismatch);
        }
        Ok(Self {
            file,
            members_start,
            member_ends: member_ends.into_boxed_slice(),
        })
    }

    /// Reads one member from the held file and checks the caller's digest before returning bytes.
    ///
    /// The supplied digest length must equal the indexed member length. Callers admit the
    /// temporary allocation before reading. Each call uses an independent positional cursor.
    pub fn read_member(&self, index: usize, expected: ContentDigest) -> Result<Vec<u8>, PackError> {
        let end = *self
            .member_ends
            .get(index)
            .ok_or(PackError::ExtentMismatch)?;
        let start = if index == 0 {
            self.members_start
        } else {
            self.member_ends[index - 1]
        };
        let length = end - start;
        if expected.length() != length {
            return Err(PackError::ExtentMismatch);
        }
        let length = usize::try_from(length).map_err(|_| PackError::ExtentMismatch)?;
        let mut reader = self.file.reader()?;
        reader.seek(SeekFrom::Start(start)).map_err(read_error)?;
        let bytes = read_exact_bytes(&mut reader, length)?;
        verify_member(&bytes, expected)?;
        Ok(bytes)
    }
}

/// Computes the complete pack extent in bytes without allocating an index.
pub fn encoded_bytes(
    catalog_length: usize,
    member_lengths: impl IntoIterator<Item = u64>,
) -> Result<u64, PackError> {
    member_lengths
        .into_iter()
        .try_fold(advance(HEADER_BYTES, catalog_length as u64)?, advance)
}

/// Reads the header and the independently bounded catalog bytes from the start of the file.
pub fn read_catalog(file: &FileCapability, expected_length: usize) -> Result<Vec<u8>, PackError> {
    let mut reader = file.reader()?;
    let mut header = [0_u8; HEADER_BYTES as usize];
    reader.read_exact(&mut header).map_err(read_error)?;
    if &header[..8] != MAGIC
        || header[8..12] != VERSION.to_le_bytes()
        || header[12..20] != (expected_length as u64).to_le_bytes()
    {
        return Err(PackError::InvalidHeader);
    }
    read_exact_bytes(&mut reader, expected_length)
}

/// Writes the header and opaque catalog at the stream's current position.
///
/// The caller appends the member bytes in catalog order. A failed write can leave partial bytes.
pub fn write_catalog(output: &mut (impl Write + ?Sized), catalog: &[u8]) -> Result<(), PackError> {
    output.write_all(MAGIC).map_err(write_error)?;
    output
        .write_all(&VERSION.to_le_bytes())
        .map_err(write_error)?;
    output
        .write_all(&(catalog.len() as u64).to_le_bytes())
        .map_err(write_error)?;
    output.write_all(catalog).map_err(write_error)?;
    Ok(())
}

fn verify_member(bytes: &[u8], expected: ContentDigest) -> Result<(), PackError> {
    if ContentDigest::of_bytes(bytes) != expected {
        return Err(PackError::MemberDigestMismatch);
    }
    Ok(())
}

fn write_error(source: io::Error) -> PackError {
    FsError::Io {
        operation: FsOperation::Write,
        source,
    }
    .into()
}

fn advance(start: u64, length: u64) -> Result<u64, PackError> {
    start
        .checked_add(length)
        .filter(|end| *end <= i64::MAX as u64)
        .ok_or(PackError::ExtentMismatch)
}

fn read_exact_bytes(reader: &mut impl Read, length: usize) -> Result<Vec<u8>, PackError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| allocation_error("bounded Runtime pack member allocation failed"))?;
    bytes.resize(length, 0);
    reader.read_exact(&mut bytes).map_err(read_error)?;
    Ok(bytes)
}

fn allocation_error(message: &'static str) -> PackError {
    read_error(io::Error::new(io::ErrorKind::OutOfMemory, message))
}

fn read_error(source: io::Error) -> PackError {
    FsError::Io {
        operation: FsOperation::Read,
        source,
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_extent_stops_at_the_signed_native_offset_limit() {
        let maximum = i64::MAX as u64;
        let member_length = maximum - 23;
        assert_eq!(encoded_bytes(3, [member_length, 0]).unwrap(), maximum);
        assert!(matches!(
            encoded_bytes(3, [member_length, 1]),
            Err(PackError::ExtentMismatch)
        ));
        assert!(matches!(
            encoded_bytes(3, [u64::MAX]),
            Err(PackError::ExtentMismatch)
        ));
    }

    #[test]
    fn captured_pack_shares_exact_catalog_and_ordered_member_buffers() {
        let catalog = b"cat".to_vec();
        let members = vec![Vec::new(), b"red".to_vec(), b"blue".to_vec()];
        let catalog_start = catalog.as_ptr();
        let member_start = members[1].as_ptr();
        let digests = [b"".as_slice(), b"red", b"blue"].map(ContentDigest::of_bytes);
        let (backing, pack) = PackBacking::capture(catalog, members, digests.into_iter()).unwrap();
        let shared = pack.clone();
        drop(pack);

        assert_eq!(shared.catalog_bytes().as_ptr(), catalog_start);
        assert_eq!(shared.member(1).unwrap().as_ptr(), member_start);
        assert_eq!(shared.member(0), Some(b"".as_slice()));
        assert!(shared.member(3).is_none());
        assert_eq!(shared.catalog_digest(), ContentDigest::of_bytes(b"cat"));

        let budget = DecodeBudget::new(0, 1, 0);
        let encoded = backing.member(&budget, 1, digests[1]).read().unwrap();
        assert_eq!(encoded.as_ref().as_ptr(), member_start);
        assert!(matches!(
            backing.member(&budget, 1, digests[0]).read(),
            Err(PackBackingError::Pack(PackError::ExtentMismatch))
        ));
        let mut bytes = Vec::new();
        shared.write(&mut bytes).unwrap();
        assert_eq!(bytes, b"NARAPACK\x01\0\0\0\x03\0\0\0\0\0\0\0catredblue");
        assert_eq!(shared.encoded_bytes(), bytes.len() as u64);
        let mut from_backing = Vec::new();
        backing
            .write(&mut from_backing, &budget, digests.into_iter())
            .unwrap();
        assert_eq!(from_backing, bytes);
    }

    #[test]
    #[cfg(any(windows, unix))]
    fn member_reads_authenticate_exact_indexed_extents() {
        use std::{
            fs,
            sync::Barrier,
            thread,
            time::{SystemTime, UNIX_EPOCH},
        };

        use nara_core::vfs::capabilities::TrustMode;

        let mut bytes = Vec::new();
        write_catalog(&mut bytes, b"cat").unwrap();
        bytes.extend_from_slice(b"redblue");
        assert_eq!(bytes, b"NARAPACK\x01\0\0\0\x03\0\0\0\0\0\0\0catredblue");
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("nara_pack_{}_{stamp}", std::process::id()));
        let mut handle = fs::File::create_new(&path).unwrap();
        handle.write_all(&bytes).unwrap();
        let file = FileCapability::from_host_handle(handle, TrustMode::TrustedLocal, 0).unwrap();
        let catalog = read_catalog(&file, 3).unwrap();
        assert_eq!(catalog, b"cat");
        let backing = PackBacking::open(file, catalog, [0, 3, 4, 0].into_iter()).unwrap();
        let budget = DecodeBudget::new(0, 0, 7);
        let read = |index, bytes: &[u8]| {
            backing
                .member(&budget, index, ContentDigest::of_bytes(bytes))
                .read()
        };
        let start = Barrier::new(2);
        let (first, second) = thread::scope(|scope| {
            let first = scope.spawn(|| {
                start.wait();
                read(1, b"red").unwrap()
            });
            start.wait();
            let second = read(2, b"blue").unwrap();
            (first.join().unwrap(), second)
        });
        assert_eq!(first.as_ref(), b"red");
        assert_eq!(second.as_ref(), b"blue");
        assert!(matches!(
            budget.reserve(0, 1),
            Err(DecodeBudgetError::WorkBytes {
                required: 8,
                limit: 7
            })
        ));
        drop(first);
        drop(second);
        assert!(budget.reserve(0, 7).is_ok());

        assert!(read(0, b"").unwrap().as_ref().is_empty());
        assert!(read(3, b"").unwrap().as_ref().is_empty());
        for (index, bytes) in [(1, b"re".as_slice()), (1, b"redblue"), (4, b"")] {
            assert!(matches!(
                read(index, bytes),
                Err(PackBackingError::Pack(PackError::ExtentMismatch))
            ));
        }
        assert!(matches!(
            read(1, b"tan"),
            Err(PackBackingError::Pack(PackError::MemberDigestMismatch))
        ));
        assert!(budget.reserve(0, 7).is_ok());
        drop(backing);
        fs::remove_file(path).unwrap();
    }
}
