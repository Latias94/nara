use std::{fmt, fs::File};

/// Holds an exact directory object against ordinary rename/removal on qualified platforms.
///
/// Child creation, replacement and unlink remain available. This lease does not freeze contents
/// or supply mutation authority. Dropping it releases the namespace restriction.
pub struct DirectoryNamespaceLease {
    _file: File,
}

impl DirectoryNamespaceLease {
    pub(crate) const fn new(file: File) -> Self {
        Self { _file: file }
    }
}

impl fmt::Debug for DirectoryNamespaceLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DirectoryNamespaceLease(..)")
    }
}

/// Opaque handle lease whose platform adapter has re-opened and revalidated the exact live
/// capability object with the restrictive launch contract.
///
/// This type is intentionally separate from a plain duplicated host handle. Consumers that need
/// exact-generation launch authority must receive this lease from a capability owner rather than
/// attempting to impose a sharing contract on an arbitrary foreign handle.
#[doc(hidden)]
pub struct RestrictiveCapabilityHandleLease {
    file: File,
}

impl RestrictiveCapabilityHandleLease {
    pub(crate) const fn new(file: File) -> Self {
        Self { file }
    }
}

impl fmt::Debug for RestrictiveCapabilityHandleLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RestrictiveCapabilityHandleLease(..)")
    }
}

#[cfg(unix)]
impl std::os::fd::AsFd for RestrictiveCapabilityHandleLease {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        std::os::fd::AsFd::as_fd(&self.file)
    }
}

#[cfg(unix)]
impl std::os::fd::AsRawFd for RestrictiveCapabilityHandleLease {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        std::os::fd::AsRawFd::as_raw_fd(&self.file)
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsHandle for RestrictiveCapabilityHandleLease {
    fn as_handle(&self) -> std::os::windows::io::BorrowedHandle<'_> {
        std::os::windows::io::AsHandle::as_handle(&self.file)
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsRawHandle for RestrictiveCapabilityHandleLease {
    fn as_raw_handle(&self) -> std::os::windows::io::RawHandle {
        std::os::windows::io::AsRawHandle::as_raw_handle(&self.file)
    }
}
