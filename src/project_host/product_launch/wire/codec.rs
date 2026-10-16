//! Bounded canonical frame codec shared by launch and READY values.

use std::{
    mem::size_of,
    path::{Path, PathBuf},
};

use nara_fs::ContentDigest;

use super::{MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES, SupervisedDesktopWireError};

pub(crate) const FRAME_LENGTH_BYTES: usize = size_of::<u32>();
const MAX_PROJECT_ROOT_BYTES: usize = 32 * 1024;

pub(super) struct FrameEncoder {
    bytes: Vec<u8>,
    limit: usize,
}

impl FrameEncoder {
    pub(super) const fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }

    pub(super) fn write_u8(&mut self, value: u8) -> Result<(), SupervisedDesktopWireError> {
        self.write_raw(&[value])
    }

    pub(super) fn write_u16(&mut self, value: u16) -> Result<(), SupervisedDesktopWireError> {
        self.write_raw(&value.to_le_bytes())
    }

    pub(super) fn write_u32(&mut self, value: u32) -> Result<(), SupervisedDesktopWireError> {
        self.write_raw(&value.to_le_bytes())
    }

    pub(super) fn write_u64(&mut self, value: u64) -> Result<(), SupervisedDesktopWireError> {
        self.write_raw(&value.to_le_bytes())
    }

    pub(super) fn write_digest(
        &mut self,
        digest: ContentDigest,
    ) -> Result<(), SupervisedDesktopWireError> {
        self.write_u64(digest.length())?;
        self.write_raw(digest.as_bytes())
    }

    pub(super) fn write_text(
        &mut self,
        value: &str,
        limit: usize,
        field: &'static str,
    ) -> Result<(), SupervisedDesktopWireError> {
        validate_text(value, limit, field)?;
        self.write_bytes(value.as_bytes(), limit, field)
    }

    pub(super) fn write_bytes(
        &mut self,
        value: &[u8],
        limit: usize,
        field: &'static str,
    ) -> Result<(), SupervisedDesktopWireError> {
        if value.len() > limit {
            return Err(SupervisedDesktopWireError::FieldTooLarge { field, limit });
        }
        let length = u32::try_from(value.len())
            .map_err(|_| SupervisedDesktopWireError::FieldTooLarge { field, limit })?;
        self.write_u32(length)?;
        self.write_raw(value)
    }

    pub(super) fn write_path(&mut self, path: &Path) -> Result<(), SupervisedDesktopWireError> {
        validate_project_root_wire_size(path)?;

        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt as _;

            self.write_u8(1)?;
            let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
            let byte_length = units
                .len()
                .checked_mul(size_of::<u16>())
                .ok_or(SupervisedDesktopWireError::InvalidLength)?;
            self.write_u32(
                u32::try_from(byte_length)
                    .map_err(|_| SupervisedDesktopWireError::InvalidLength)?,
            )?;
            for unit in units {
                self.write_u16(unit)?;
            }
            return Ok(());
        }

        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;

            self.write_u8(2)?;
            return self.write_bytes(
                path.as_os_str().as_bytes(),
                MAX_PROJECT_ROOT_BYTES,
                "project root",
            );
        }

        #[cfg(not(any(windows, unix)))]
        {
            self.write_u8(3)?;
            let value = path
                .to_str()
                .ok_or(SupervisedDesktopWireError::InvalidPathEncoding)?;
            self.write_bytes(value.as_bytes(), MAX_PROJECT_ROOT_BYTES, "project root")
        }
    }

    pub(super) fn write_raw(&mut self, value: &[u8]) -> Result<(), SupervisedDesktopWireError> {
        let next_len = self
            .bytes
            .len()
            .checked_add(value.len())
            .ok_or(SupervisedDesktopWireError::InvalidLength)?;
        if next_len > self.limit {
            return Err(SupervisedDesktopWireError::InvalidLength);
        }
        self.bytes
            .try_reserve(value.len())
            .map_err(|_| SupervisedDesktopWireError::AllocationFailed)?;
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    pub(super) fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

pub(super) struct FrameReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> FrameReader<'a> {
    pub(super) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn read_u8(&mut self) -> Result<u8, SupervisedDesktopWireError> {
        Ok(self.read_array::<1>()?[0])
    }

    pub(super) fn read_u32(&mut self) -> Result<u32, SupervisedDesktopWireError> {
        Ok(u32::from_le_bytes(self.read_array()?))
    }

    pub(super) fn read_u64(&mut self) -> Result<u64, SupervisedDesktopWireError> {
        Ok(u64::from_le_bytes(self.read_array()?))
    }

    pub(super) fn read_array<const SIZE: usize>(
        &mut self,
    ) -> Result<[u8; SIZE], SupervisedDesktopWireError> {
        let bytes = self.read_exact(SIZE)?;
        let mut array = [0; SIZE];
        array.copy_from_slice(bytes);
        Ok(array)
    }

    pub(super) fn read_digest(&mut self) -> Result<ContentDigest, SupervisedDesktopWireError> {
        Ok(ContentDigest::from_parts(
            self.read_u64()?,
            self.read_array()?,
        ))
    }

    pub(super) fn read_text(
        &mut self,
        limit: usize,
        field: &'static str,
    ) -> Result<&'a str, SupervisedDesktopWireError> {
        let bytes = self.read_bytes(limit, field)?;
        std::str::from_utf8(bytes).map_err(|_| SupervisedDesktopWireError::InvalidUtf8(field))
    }

    pub(super) fn read_bytes(
        &mut self,
        limit: usize,
        field: &'static str,
    ) -> Result<&'a [u8], SupervisedDesktopWireError> {
        let length = usize::try_from(self.read_u32()?)
            .map_err(|_| SupervisedDesktopWireError::InvalidLength)?;
        if length > limit {
            return Err(SupervisedDesktopWireError::FieldTooLarge { field, limit });
        }
        self.read_exact(length)
    }

    pub(super) fn read_path(&mut self) -> Result<PathBuf, SupervisedDesktopWireError> {
        let encoding = self.read_u8()?;
        let bytes = self.read_bytes(MAX_PROJECT_ROOT_BYTES, "project root")?;

        #[cfg(windows)]
        {
            use std::{ffi::OsString, os::windows::ffi::OsStringExt as _};

            if encoding != 1 || !bytes.len().is_multiple_of(size_of::<u16>()) {
                return Err(SupervisedDesktopWireError::InvalidPathEncoding);
            }
            let units = bytes
                .chunks_exact(size_of::<u16>())
                .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
                .collect::<Vec<_>>();
            return Ok(PathBuf::from(OsString::from_wide(&units)));
        }

        #[cfg(unix)]
        {
            use std::{ffi::OsString, os::unix::ffi::OsStringExt as _};

            if encoding != 2 {
                return Err(SupervisedDesktopWireError::InvalidPathEncoding);
            }
            return Ok(PathBuf::from(OsString::from_vec(bytes.to_vec())));
        }

        #[cfg(not(any(windows, unix)))]
        {
            if encoding != 3 {
                return Err(SupervisedDesktopWireError::InvalidPathEncoding);
            }
            let value = std::str::from_utf8(bytes)
                .map_err(|_| SupervisedDesktopWireError::InvalidPathEncoding)?;
            Ok(PathBuf::from(value))
        }
    }

    pub(super) fn read_exact(
        &mut self,
        length: usize,
    ) -> Result<&'a [u8], SupervisedDesktopWireError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(SupervisedDesktopWireError::InvalidLength)?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(SupervisedDesktopWireError::IncompleteFrame)?;
        self.offset = end;
        Ok(bytes)
    }

    pub(super) fn finish(self) -> Result<(), SupervisedDesktopWireError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(SupervisedDesktopWireError::TrailingBytes)
        }
    }
}

pub(super) fn encode_frame(
    magic: &[u8],
    version: u16,
    payload: Vec<u8>,
    limit: usize,
) -> Result<Vec<u8>, SupervisedDesktopWireError> {
    let payload_length =
        u32::try_from(payload.len()).map_err(|_| SupervisedDesktopWireError::InvalidLength)?;
    let mut frame = FrameEncoder::new(limit);
    frame.write_raw(magic)?;
    frame.write_u16(version)?;
    frame.write_u32(payload_length)?;
    frame.write_raw(&payload)?;
    Ok(frame.finish())
}

pub(super) fn decode_frame<'a>(
    encoded: &'a [u8],
    magic: &[u8],
    version: u16,
    limit: usize,
) -> Result<&'a [u8], SupervisedDesktopWireError> {
    let Some(length) = frame_length(encoded, magic, version, limit)? else {
        return Err(SupervisedDesktopWireError::IncompleteFrame);
    };
    match encoded.len().cmp(&length) {
        std::cmp::Ordering::Less => Err(SupervisedDesktopWireError::IncompleteFrame),
        std::cmp::Ordering::Greater => Err(SupervisedDesktopWireError::TrailingBytes),
        std::cmp::Ordering::Equal => {
            let payload_offset = magic.len() + size_of::<u16>() + FRAME_LENGTH_BYTES;
            Ok(&encoded[payload_offset..])
        }
    }
}

pub(crate) fn frame_length(
    bytes: &[u8],
    magic: &[u8],
    version: u16,
    limit: usize,
) -> Result<Option<usize>, SupervisedDesktopWireError> {
    let magic_prefix = bytes.len().min(magic.len());
    if bytes[..magic_prefix] != magic[..magic_prefix] {
        return Err(SupervisedDesktopWireError::InvalidMagic);
    }
    if bytes.len() < magic.len() {
        return Ok(None);
    }

    let version_end = magic
        .len()
        .checked_add(size_of::<u16>())
        .ok_or(SupervisedDesktopWireError::InvalidLength)?;
    if bytes.len() < version_end {
        return Ok(None);
    }
    let encoded_version = u16::from_le_bytes(
        bytes[magic.len()..version_end]
            .try_into()
            .map_err(|_| SupervisedDesktopWireError::InvalidLength)?,
    );
    if encoded_version != version {
        return Err(SupervisedDesktopWireError::UnsupportedVersion);
    }

    let length_end = version_end
        .checked_add(FRAME_LENGTH_BYTES)
        .ok_or(SupervisedDesktopWireError::InvalidLength)?;
    if bytes.len() < length_end {
        return Ok(None);
    }
    let payload_length = usize::try_from(u32::from_le_bytes(
        bytes[version_end..length_end]
            .try_into()
            .map_err(|_| SupervisedDesktopWireError::InvalidLength)?,
    ))
    .map_err(|_| SupervisedDesktopWireError::InvalidLength)?;
    let total = length_end
        .checked_add(payload_length)
        .ok_or(SupervisedDesktopWireError::InvalidLength)?;
    if total > limit {
        return Err(SupervisedDesktopWireError::InvalidLength);
    }
    Ok(Some(total))
}

pub(super) fn validate_text(
    value: &str,
    limit: usize,
    field: &'static str,
) -> Result<(), SupervisedDesktopWireError> {
    if value.is_empty()
        || value
            .bytes()
            .any(|byte| byte == 0 || byte.is_ascii_control())
    {
        return Err(SupervisedDesktopWireError::InvalidField(field));
    }
    if value.len() > limit {
        return Err(SupervisedDesktopWireError::FieldTooLarge { field, limit });
    }
    Ok(())
}

pub(super) fn validate_executable_digest(
    digest: ContentDigest,
    field: &'static str,
) -> Result<(), SupervisedDesktopWireError> {
    if digest.length() == 0 {
        return Err(SupervisedDesktopWireError::InvalidField(field));
    }
    if digest.length() > MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES {
        return Err(SupervisedDesktopWireError::FieldTooLarge {
            field,
            limit: MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES as usize,
        });
    }
    Ok(())
}

pub(super) fn validate_project_root_wire_size(
    path: &Path,
) -> Result<(), SupervisedDesktopWireError> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;

        let byte_length = path
            .as_os_str()
            .encode_wide()
            .count()
            .checked_mul(size_of::<u16>())
            .ok_or(SupervisedDesktopWireError::InvalidLength)?;
        if byte_length > MAX_PROJECT_ROOT_BYTES {
            return Err(SupervisedDesktopWireError::FieldTooLarge {
                field: "project root",
                limit: MAX_PROJECT_ROOT_BYTES,
            });
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;

        if path.as_os_str().as_bytes().len() > MAX_PROJECT_ROOT_BYTES {
            return Err(SupervisedDesktopWireError::FieldTooLarge {
                field: "project root",
                limit: MAX_PROJECT_ROOT_BYTES,
            });
        }
    }

    #[cfg(not(any(windows, unix)))]
    {
        let value = path
            .to_str()
            .ok_or(SupervisedDesktopWireError::InvalidPathEncoding)?;
        if value.len() > MAX_PROJECT_ROOT_BYTES {
            return Err(SupervisedDesktopWireError::FieldTooLarge {
                field: "project root",
                limit: MAX_PROJECT_ROOT_BYTES,
            });
        }
    }

    Ok(())
}
