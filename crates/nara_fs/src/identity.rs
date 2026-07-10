use std::fmt::{self, Debug, Formatter};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CapabilitySessionId(pub(crate) u64);

impl CapabilitySessionId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CapabilityGeneration(pub(crate) u64);

impl CapabilityGeneration {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Live-object identity scoped to one capability session and generation.
///
/// This value is suitable for expected-object checks while its capability
/// session remains alive. It is not a persistent project or trust identity.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    session: CapabilitySessionId,
    generation: CapabilityGeneration,
    native: NativeFileIdentity,
}

impl FileIdentity {
    pub(crate) const fn new(
        session: CapabilitySessionId,
        generation: CapabilityGeneration,
        native: NativeFileIdentity,
    ) -> Self {
        Self {
            session,
            generation,
            native,
        }
    }

    #[must_use]
    pub const fn session(self) -> CapabilitySessionId {
        self.session
    }

    #[must_use]
    pub const fn generation(self) -> CapabilityGeneration {
        self.generation
    }

    pub(crate) const fn native(self) -> NativeFileIdentity {
        self.native
    }
}

impl Debug for FileIdentity {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileIdentity")
            .field("session", &self.session)
            .field("generation", &self.generation)
            .field("native", &"<sensitive>")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum NativeFileIdentity {
    #[cfg(windows)]
    Windows {
        volume_serial: u64,
        file_id: [u8; 16],
    },
    #[cfg(unix)]
    Unix { device: u64, inode: u64 },
    #[cfg(not(any(unix, windows)))]
    #[allow(dead_code)]
    // Keeps cross-target cleanup code reachable without granting identity proof.
    Unsupported,
}

impl NativeFileIdentity {
    #[allow(unreachable_patterns)]
    pub(crate) const fn same_volume(self, other: Self) -> bool {
        match (self, other) {
            #[cfg(windows)]
            (
                Self::Windows {
                    volume_serial: left,
                    ..
                },
                Self::Windows {
                    volume_serial: right,
                    ..
                },
            ) => left == right,
            #[cfg(unix)]
            (Self::Unix { device: left, .. }, Self::Unix { device: right, .. }) => left == right,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Directory,
    Regular,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileFacts {
    pub identity: NativeFileIdentity,
    pub kind: FileKind,
    pub link_count: u64,
    pub reparse_tag: Option<u32>,
    pub identity_proven: bool,
}
