//! NPAK v1 cooked asset archives.
//!
//! All integers are little-endian. The 26-byte header is `NPAK`, version `u16`, entry count
//! `u32`, TOC offset `u64`, and TOC length `u64`. The data region follows immediately. Entries
//! are stored consecutively in lexicographic order of the 16 UUID bytes. The TOC ends at EOF.
//! Each 52-byte row is UUID bytes, kind `u64`, absolute offset `u64`, stored length `u64`,
//! uncompressed length `u64`, and flags `u32`. Flags are zero for raw bytes and one for an
//! independent LZ4 block with no length prefix or dictionary. Zero-length entries are raw.

use std::{
    borrow::Cow,
    collections::TryReserveError,
    fmt,
    io::{self, BufReader, Cursor, Read, Seek, SeekFrom},
    sync::Arc,
};

#[cfg(any(unix, windows))]
use std::{
    fs::{self, File},
    path::Path,
    sync::Mutex,
};

use nara_core::{
    AssetId,
    vfs::{AssetPath, AssetSource},
};
use uuid::Uuid;

use crate::source::{asset_id_path, check_root_dir, lookup_id};

mod write;

pub use crate::NpakEntryKind;
pub use write::{NpakInput, encode_npak, write_npak};

pub const NPAK_VERSION: u16 = 1;
pub const MAX_NPAK_ENTRIES: usize = 262_144;
pub const MAX_NPAK_ENTRY_BYTES: usize = 256 * 1024 * 1024;
/// Bounds block length accumulation and output offsets on 32-bit readers as well.
pub const MAX_NPAK_LZ4_BYTES: usize = 15 * 1024 * 1024;

const MAGIC: &[u8; 4] = b"NPAK";
const HEADER_BYTES: usize = 26;
const TOC_ROW_BYTES: usize = 52;

impl NpakEntryKind {
    fn from_wire(tag: u64) -> Option<Self> {
        match tag {
            1 => Some(Self::Scene),
            2 => Some(Self::Image),
            3 => Some(Self::Shader),
            4 => Some(Self::Material),
            5 => Some(Self::Raw),
            6 => Some(Self::Audio),
            7 => Some(Self::Font),
            8 => Some(Self::TileSet),
            9 => Some(Self::SpriteAnimation),
            10 => Some(Self::PropertyAnimation),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum NpakCompression {
    #[default]
    None = 0,
    Lz4 = 1,
}

/// Checked metadata for one entry in an open archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NpakEntry {
    pub id: AssetId,
    pub kind: NpakEntryKind,
    pub offset: u64,
    pub stored_len: u64,
    pub uncompressed_len: u64,
    pub compression: NpakCompression,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NpakError {
    #[error("NPAK I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("could not allocate NPAK data: {0}")]
    Capacity(#[from] TryReserveError),
    #[error("invalid NPAK header: {0}")]
    InvalidHeader(&'static str),
    #[error("unsupported NPAK version {0}")]
    UnsupportedVersion(u16),
    #[error("NPAK has {count} entries, exceeding the {MAX_NPAK_ENTRIES}-entry limit")]
    TooManyEntries { count: usize },
    #[error("invalid NPAK entry {index}: {reason}")]
    InvalidEntry { index: usize, reason: &'static str },
    #[error("NPAK contains duplicate asset '{0}'")]
    DuplicateId(AssetId),
    #[error("NPAK asset '{id}' exceeds the {MAX_NPAK_ENTRY_BYTES}-byte limit")]
    EntryTooLarge { id: AssetId },
    #[error("NPAK does not contain asset '{0}'")]
    NotFound(AssetId),
    #[error("NPAK asset '{id}' has kind {actual:?}; expected {expected:?}")]
    WrongKind {
        id: AssetId,
        expected: NpakEntryKind,
        actual: NpakEntryKind,
    },
    #[error("could not decompress NPAK asset '{id}': {source}")]
    Decompress {
        id: AssetId,
        source: lz4_flex::block::DecompressError,
    },
    #[error("NPAK asset '{id}' decoded to {actual} bytes; expected {expected}")]
    DecodedLength {
        id: AssetId,
        actual: usize,
        expected: usize,
    },
    #[error("could not compress NPAK asset '{id}': {source}")]
    Compress {
        id: AssetId,
        source: lz4_flex::block::CompressError,
    },
    #[error("NPAK output must be empty before writing")]
    NonEmptyOutput,
}

/// An archive with an eagerly checked TOC and independently readable asset blocks.
pub struct NpakArchive {
    entries: Vec<NpakEntry>,
    backing: Backing,
}

enum Backing {
    Memory(Arc<[u8]>),
    #[cfg(any(unix, windows))]
    File(Mutex<File>),
}

impl fmt::Debug for NpakArchive {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NpakArchive")
            .field("entry_count", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl NpakArchive {
    /// Reads only the header and directory from an existing regular file.
    ///
    /// The archive holds the opened file. Publish updates as a new file and keep the opened file
    /// immutable. Short reads caused by truncation fail at the affected entry.
    #[cfg(any(unix, windows))]
    pub fn open(path: impl AsRef<Path>) -> Result<Self, NpakError> {
        let path = path.as_ref();
        if !fs::metadata(path)?.is_file() {
            return Err(
                io::Error::new(io::ErrorKind::InvalidInput, "expected a regular file").into(),
            );
        }
        let mut file = File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(
                io::Error::new(io::ErrorKind::InvalidInput, "expected a regular file").into(),
            );
        }
        let entries = read_toc(&mut file, metadata.len())?;
        Ok(Self {
            entries,
            backing: Backing::File(Mutex::new(file)),
        })
    }

    /// Opens a shared immutable memory buffer with the same validation as a file.
    pub fn from_bytes(bytes: impl Into<Arc<[u8]>>) -> Result<Self, NpakError> {
        let bytes = bytes.into();
        let entries = read_toc(&mut Cursor::new(bytes.as_ref()), bytes.len() as u64)?;
        Ok(Self {
            entries,
            backing: Backing::Memory(bytes),
        })
    }

    /// Returns the checked directory in UUID order without reading any payloads.
    #[must_use]
    pub fn entries(&self) -> &[NpakEntry] {
        &self.entries
    }

    /// Finds an entry by binary search in the UUID-ordered directory.
    #[must_use]
    pub fn entry(&self, id: AssetId) -> Option<&NpakEntry> {
        let position = self
            .entries
            .binary_search_by_key(&id, |entry| entry.id)
            .ok()?;
        self.entries.get(position)
    }

    pub fn read(&self, id: AssetId) -> Result<Vec<u8>, NpakError> {
        self.read_entry(self.entry(id).ok_or(NpakError::NotFound(id))?)
    }

    /// Checks the expected kind before reading or decompressing bytes.
    pub fn read_kind(&self, id: AssetId, kind: NpakEntryKind) -> Result<Vec<u8>, NpakError> {
        let entry = self.entry(id).ok_or(NpakError::NotFound(id))?;
        if entry.kind != kind {
            return Err(NpakError::WrongKind {
                id,
                expected: kind,
                actual: entry.kind,
            });
        }
        self.read_entry(entry)
    }

    fn read_entry(&self, entry: &NpakEntry) -> Result<Vec<u8>, NpakError> {
        let stored_len = entry.stored_len as usize;
        let encoded = match &self.backing {
            Backing::Memory(bytes) => {
                let start = entry.offset as usize;
                Cow::Borrowed(&bytes[start..start + stored_len])
            }
            #[cfg(any(unix, windows))]
            Backing::File(file) => {
                let mut bytes = Vec::new();
                resize_bytes(&mut bytes, stored_len)?;
                let mut file = file
                    .lock()
                    .map_err(|_| io::Error::other("NPAK file lock poisoned"))?;
                file.seek(SeekFrom::Start(entry.offset))?;
                file.read_exact(&mut bytes)?;
                Cow::Owned(bytes)
            }
        };
        if entry.compression == NpakCompression::None {
            return match encoded {
                Cow::Owned(bytes) => Ok(bytes),
                Cow::Borrowed(bytes) => {
                    let mut output = Vec::new();
                    output.try_reserve_exact(bytes.len())?;
                    output.extend_from_slice(bytes);
                    Ok(output)
                }
            };
        }
        let expected = entry.uncompressed_len as usize;
        let mut decoded = Vec::new();
        resize_bytes(&mut decoded, expected)?;
        let actual =
            lz4_flex::block::decompress_into(&encoded, &mut decoded).map_err(|source| {
                NpakError::Decompress {
                    id: entry.id,
                    source,
                }
            })?;
        if actual != expected {
            return Err(NpakError::DecodedLength {
                id: entry.id,
                actual,
                expected,
            });
        }
        Ok(decoded)
    }
}

impl AssetSource for NpakArchive {
    fn read_bytes(&self, path: &AssetPath) -> io::Result<Vec<u8>> {
        let entry = lookup_id(path, |id| self.entry(id))?;
        self.read_entry(entry).map_err(|error| match error {
            NpakError::Io(error) => error,
            error @ NpakError::Capacity(_) => io::Error::new(io::ErrorKind::OutOfMemory, error),
            error => io::Error::new(io::ErrorKind::InvalidData, error),
        })
    }

    fn exists(&self, path: &AssetPath) -> bool {
        path == &AssetPath::ROOT || lookup_id(path, |id| self.entry(id)).is_ok()
    }

    fn list_dir(&self, dir: &AssetPath) -> io::Result<Vec<AssetPath>> {
        check_root_dir(dir, |id| self.entry(id))?;
        Ok(self
            .entries
            .iter()
            .map(|entry| asset_id_path(entry.id))
            .collect())
    }
}

fn read_toc(reader: &mut (impl Read + Seek), len: u64) -> Result<Vec<NpakEntry>, NpakError> {
    let header = read_array::<HEADER_BYTES>(reader)?;
    if &header[..4] != MAGIC {
        return Err(NpakError::InvalidHeader("incorrect magic"));
    }
    let version = u16::from_le_bytes(field(&header, 4));
    if version != NPAK_VERSION {
        return Err(NpakError::UnsupportedVersion(version));
    }
    let count = u32::from_le_bytes(field(&header, 6)) as usize;
    if count > MAX_NPAK_ENTRIES {
        return Err(NpakError::TooManyEntries { count });
    }
    let toc_offset = u64::from_le_bytes(field(&header, 10));
    let toc_len = u64::from_le_bytes(field(&header, 18));
    if toc_offset < HEADER_BYTES as u64
        || toc_len != (count * TOC_ROW_BYTES) as u64
        || toc_offset.checked_add(toc_len) != Some(len)
    {
        return Err(NpakError::InvalidHeader("incorrect directory extent"));
    }

    reader.seek(SeekFrom::Start(toc_offset))?;
    let mut reader = BufReader::new(reader.take(toc_len));
    let mut entries = Vec::new();
    entries.try_reserve_exact(count)?;
    let mut next_offset = HEADER_BYTES as u64;
    let mut previous_id = None;
    for index in 0..count {
        let row = read_array::<TOC_ROW_BYTES>(&mut reader)?;
        let invalid = |reason| NpakError::InvalidEntry { index, reason };
        let id = AssetId::try_from_uuid(Uuid::from_bytes(field(&row, 0)))
            .map_err(|_| invalid("nil UUID"))?;
        if previous_id.is_some_and(|previous| previous >= id) {
            return Err(invalid("UUIDs must be strictly increasing"));
        }
        let kind = NpakEntryKind::from_wire(u64::from_le_bytes(field(&row, 16)))
            .ok_or_else(|| invalid("unknown entry kind"))?;
        let offset = u64::from_le_bytes(field(&row, 24));
        let stored_len = u64::from_le_bytes(field(&row, 32));
        let uncompressed_len = u64::from_le_bytes(field(&row, 40));
        if stored_len > MAX_NPAK_ENTRY_BYTES as u64
            || uncompressed_len > MAX_NPAK_ENTRY_BYTES as u64
        {
            return Err(NpakError::EntryTooLarge { id });
        }
        let compression = match u32::from_le_bytes(field(&row, 48)) {
            0 if stored_len == uncompressed_len => NpakCompression::None,
            1 if valid_lz4_lengths(stored_len, uncompressed_len) => NpakCompression::Lz4,
            0 | 1 => return Err(invalid("incorrect entry lengths for compression flags")),
            _ => return Err(invalid("unknown entry flags")),
        };
        if offset != next_offset || stored_len > toc_offset - next_offset {
            return Err(invalid(
                "entries must be contiguous and end before the directory",
            ));
        }
        entries.push(NpakEntry {
            id,
            kind,
            offset,
            stored_len,
            uncompressed_len,
            compression,
        });
        next_offset += stored_len;
        previous_id = Some(id);
    }
    if next_offset != toc_offset {
        return Err(NpakError::InvalidHeader("unused bytes in the data region"));
    }
    Ok(entries)
}

fn valid_lz4_lengths(stored: u64, uncompressed: u64) -> bool {
    (1..=MAX_NPAK_LZ4_BYTES as u64).contains(&stored)
        && stored < uncompressed
        && uncompressed <= stored.saturating_mul(255)
}

fn read_array<const LEN: usize>(reader: &mut impl Read) -> io::Result<[u8; LEN]> {
    let mut field = [0; LEN];
    reader.read_exact(&mut field)?;
    Ok(field)
}

fn field<const LEN: usize>(bytes: &[u8], offset: usize) -> [u8; LEN] {
    bytes[offset..offset + LEN]
        .try_into()
        .expect("fixed wire field")
}

fn resize_bytes(bytes: &mut Vec<u8>, len: usize) -> Result<(), TryReserveError> {
    bytes.try_reserve_exact(len.saturating_sub(bytes.len()))?;
    bytes.resize(len, 0);
    Ok(())
}
