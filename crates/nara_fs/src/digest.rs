use std::{fmt, num::NonZeroU64};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentDigest {
    length: u64,
    hash: [u8; 32],
}

impl ContentDigest {
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self {
            length: bytes.len() as u64,
            hash: *blake3::hash(bytes).as_bytes(),
        }
    }

    #[must_use]
    pub const fn from_parts(length: u64, hash: [u8; 32]) -> Self {
        Self { length, hash }
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.hash
    }

    #[must_use]
    pub const fn length(self) -> u64 {
        self.length
    }
}

impl fmt::Debug for ContentDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ContentDigest(")?;
        write!(formatter, "len={}, ", self.length)?;
        for byte in self.hash {
            write!(formatter, "{byte:02x}")?;
        }
        formatter.write_str(")")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DigestLimit(NonZeroU64);

impl DigestLimit {
    pub const fn new(limit: u64) -> Option<Self> {
        match NonZeroU64::new(limit) {
            Some(limit) => Some(Self(limit)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}
