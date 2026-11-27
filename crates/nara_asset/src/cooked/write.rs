use std::io::{BufWriter, Cursor, Seek, SeekFrom, Write};

use super::{
    AssetId, HEADER_BYTES, MAGIC, MAX_NPAK_ENTRIES, MAX_NPAK_ENTRY_BYTES, MAX_NPAK_LZ4_BYTES,
    NPAK_VERSION, NpakCompression, NpakEntry, NpakEntryKind, NpakError, TOC_ROW_BYTES,
    resize_bytes,
};

/// One borrowed cooked payload and its requested storage encoding.
#[derive(Debug, Clone, Copy)]
pub struct NpakInput<'a> {
    id: AssetId,
    kind: NpakEntryKind,
    bytes: &'a [u8],
    compression: NpakCompression,
}

impl<'a> NpakInput<'a> {
    #[must_use]
    pub const fn new(
        id: AssetId,
        kind: NpakEntryKind,
        bytes: &'a [u8],
        compression: NpakCompression,
    ) -> Self {
        Self {
            id,
            kind,
            bytes,
            compression,
        }
    }
}

/// Encodes a deterministic archive to memory.
pub fn encode_npak(entries: &[NpakInput<'_>]) -> Result<Vec<u8>, NpakError> {
    let mut writer = Cursor::new(Vec::new());
    write_npak(&mut writer, entries)?;
    Ok(writer.into_inner())
}

/// Streams a deterministic archive into an empty seekable writer.
///
/// UUID order determines both data and directory order. Requested LZ4 is used when it fits
/// the block limit and shrinks the payload. Raw inputs are written from their borrowed slices.
/// Use a staging file when publishing to disk because I/O errors can leave partial output.
pub fn write_npak(
    writer: &mut (impl Write + Seek),
    entries: &[NpakInput<'_>],
) -> Result<(), NpakError> {
    let count = entries.len();
    if count > MAX_NPAK_ENTRIES {
        return Err(NpakError::TooManyEntries { count });
    }
    let mut sorted = Vec::new();
    sorted.try_reserve_exact(count)?;
    for entry in entries {
        if entry.bytes.len() > MAX_NPAK_ENTRY_BYTES {
            return Err(NpakError::EntryTooLarge { id: entry.id });
        }
        sorted.push(entry);
    }
    sorted.sort_unstable_by_key(|entry| entry.id);
    for pair in sorted.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(NpakError::DuplicateId(pair[0].id));
        }
    }
    if writer.seek(SeekFrom::End(0))? != 0 {
        return Err(NpakError::NonEmptyOutput);
    }

    let mut toc = Vec::new();
    toc.try_reserve_exact(count)?;
    let mut scratch = Vec::new();
    let mut writer = BufWriter::new(writer);
    writer.write_all(&[0; HEADER_BYTES])?;
    let mut offset = HEADER_BYTES as u64;
    for input in sorted {
        let (bytes, compression) = encode_payload(input, &mut scratch)?;
        writer.write_all(bytes)?;
        let stored_len = bytes.len() as u64;
        toc.push(NpakEntry {
            id: input.id,
            kind: input.kind,
            offset,
            stored_len,
            uncompressed_len: input.bytes.len() as u64,
            compression,
        });
        offset += stored_len;
    }
    for entry in &toc {
        writer.write_all(&encode_row(entry))?;
    }

    let toc_len = (count * TOC_ROW_BYTES) as u64;
    let mut header = [0; HEADER_BYTES];
    header[..4].copy_from_slice(MAGIC);
    header[4..6].copy_from_slice(&NPAK_VERSION.to_le_bytes());
    header[6..10].copy_from_slice(&(count as u32).to_le_bytes());
    header[10..18].copy_from_slice(&offset.to_le_bytes());
    header[18..26].copy_from_slice(&toc_len.to_le_bytes());
    writer.seek(SeekFrom::Start(0))?;
    writer.write_all(&header)?;
    writer.seek(SeekFrom::Start(offset + toc_len))?;
    Ok(())
}

fn encode_payload<'a>(
    input: &NpakInput<'a>,
    scratch: &'a mut Vec<u8>,
) -> Result<(&'a [u8], NpakCompression), NpakError> {
    if input.compression == NpakCompression::Lz4 && !input.bytes.is_empty() {
        let capacity = lz4_flex::block::get_maximum_output_size(input.bytes.len());
        if scratch.len() < capacity {
            resize_bytes(scratch, capacity)?;
        }
        let len = lz4_flex::block::compress_into(input.bytes, scratch).map_err(|source| {
            NpakError::Compress {
                id: input.id,
                source,
            }
        })?;
        if len < input.bytes.len() && len <= MAX_NPAK_LZ4_BYTES {
            return Ok((&scratch[..len], NpakCompression::Lz4));
        }
    }
    Ok((input.bytes, NpakCompression::None))
}

fn encode_row(entry: &NpakEntry) -> [u8; TOC_ROW_BYTES] {
    let mut row = [0; TOC_ROW_BYTES];
    row[..16].copy_from_slice(entry.id.as_uuid().as_bytes());
    row[16..24].copy_from_slice(&(entry.kind as u64).to_le_bytes());
    row[24..32].copy_from_slice(&entry.offset.to_le_bytes());
    row[32..40].copy_from_slice(&entry.stored_len.to_le_bytes());
    row[40..48].copy_from_slice(&entry.uncompressed_len.to_le_bytes());
    row[48..52].copy_from_slice(&(entry.compression as u32).to_le_bytes());
    row
}
