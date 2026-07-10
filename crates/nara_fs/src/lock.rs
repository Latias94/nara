use std::{
    fmt::{self, Debug, Formatter},
    fs::File,
};

use crate::platform;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockMode {
    Shared,
    Exclusive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockGuarantee {
    AdvisoryCooperative,
    PlatformEnforced,
    Unsupported,
    Unproven,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockScope {
    OpenHandle,
    Process,
}

pub struct FileLock<'a> {
    file: &'a File,
    mode: LockMode,
    active: bool,
}

impl<'a> FileLock<'a> {
    pub(crate) const fn new(file: &'a File, mode: LockMode) -> Self {
        Self {
            file,
            mode,
            active: true,
        }
    }

    #[must_use]
    pub const fn mode(&self) -> LockMode {
        self.mode
    }

    #[must_use]
    pub const fn guarantee(&self) -> LockGuarantee {
        if cfg!(windows) {
            LockGuarantee::PlatformEnforced
        } else if cfg!(unix) {
            LockGuarantee::AdvisoryCooperative
        } else {
            LockGuarantee::Unsupported
        }
    }

    #[must_use]
    pub const fn scope(&self) -> LockScope {
        LockScope::OpenHandle
    }

    pub fn release(mut self) -> Result<(), crate::FsError> {
        platform::unlock(self.file)?;
        self.active = false;
        Ok(())
    }
}

impl Debug for FileLock<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileLock")
            .field("mode", &self.mode)
            .field("guarantee", &self.guarantee())
            .field("scope", &self.scope())
            .finish_non_exhaustive()
    }
}

impl Drop for FileLock<'_> {
    fn drop(&mut self) {
        if self.active {
            let _ = platform::unlock(self.file);
            self.active = false;
        }
    }
}
