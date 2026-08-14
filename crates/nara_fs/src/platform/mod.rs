#[cfg(test)]
use std::cell::RefCell;
use std::fs::File;
#[cfg(unix)]
use std::io;

use crate::{
    DirectorySyncTier, FileFacts, FileSyncTier, FsError, LockMode, PlatformCapabilityMatrix,
    PublicationSourceBinding, RelativeComponent, ResolutionTier, TrustMode,
};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as imp;

#[cfg(not(any(unix, windows)))]
mod unsupported;
#[cfg(not(any(unix, windows)))]
use unsupported as imp;

#[cfg(test)]
thread_local! {
    static TEST_FACTS_OVERRIDE: RefCell<Option<FileFacts>> = const { RefCell::new(None) };
    static TEST_DIRECTORY_PUBLICATION_SUPPORT_OVERRIDE: RefCell<Option<DirectoryPublicationSupport>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirectoryPublicationSupport {
    Proven(PublicationSourceBinding),
    Unsupported,
    Unproven,
}

pub(crate) fn capability_matrix() -> PlatformCapabilityMatrix {
    imp::capability_matrix()
}

pub(crate) fn facts(file: &File) -> Result<FileFacts, FsError> {
    #[cfg(test)]
    if let Some(facts) = TEST_FACTS_OVERRIDE.with(|slot| slot.borrow_mut().take()) {
        return Ok(facts);
    }

    imp::facts(file)
}

#[cfg(test)]
pub(crate) fn with_test_facts_override<T>(facts: FileFacts, operation: impl FnOnce() -> T) -> T {
    struct OverrideGuard;

    impl Drop for OverrideGuard {
        fn drop(&mut self) {
            TEST_FACTS_OVERRIDE.with(|slot| {
                slot.borrow_mut().take();
            });
        }
    }

    TEST_FACTS_OVERRIDE.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(
            slot.is_none(),
            "nested filesystem facts overrides are forbidden"
        );
        *slot = Some(facts);
    });
    let guard = OverrideGuard;
    let result = operation();
    TEST_FACTS_OVERRIDE.with(|slot| {
        assert!(
            slot.borrow().is_none(),
            "the public capability operation did not consume the filesystem facts override"
        );
    });
    drop(guard);
    result
}

pub(crate) fn resolution_tier(file: &File) -> ResolutionTier {
    imp::resolution_tier(file)
}

pub(crate) fn open_directory(
    parent: &File,
    component: &RelativeComponent,
    strict: bool,
) -> Result<File, FsError> {
    imp::open_directory(parent, component, strict)
}

pub(crate) fn open_file(
    parent: &File,
    component: &RelativeComponent,
    strict: bool,
) -> Result<File, FsError> {
    imp::open_file(parent, component, strict)
}

pub(crate) fn child_exists(parent: &File, component: &RelativeComponent) -> Result<bool, FsError> {
    match imp::open_child(parent, component) {
        Ok(_) => Ok(true),
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn directory_publication_source_binding(
    parent: &File,
    trust: TrustMode,
) -> Result<PublicationSourceBinding, FsError> {
    #[cfg(test)]
    let support = TEST_DIRECTORY_PUBLICATION_SUPPORT_OVERRIDE
        .with(|slot| slot.borrow_mut().take())
        .unwrap_or_else(|| imp::directory_publication_support(parent));
    #[cfg(not(test))]
    let support = imp::directory_publication_support(parent);

    match support {
        DirectoryPublicationSupport::Proven(PublicationSourceBinding::HandleBound) => {
            Ok(PublicationSourceBinding::HandleBound)
        }
        DirectoryPublicationSupport::Proven(PublicationSourceBinding::NameBound)
            if trust == TrustMode::TrustedLocal =>
        {
            Ok(PublicationSourceBinding::NameBound)
        }
        DirectoryPublicationSupport::Proven(PublicationSourceBinding::NameBound) => {
            Err(FsError::Unproven {
                operation: crate::FsOperation::BeginDirectoryPublication,
                proof: "name-bound directory publication is admitted only for trusted cooperative local writers",
            })
        }
        DirectoryPublicationSupport::Proven(PublicationSourceBinding::Unsupported) => {
            Err(FsError::Unproven {
                operation: crate::FsOperation::BeginDirectoryPublication,
                proof: "the platform adapter reported an unsupported source binding as proven",
            })
        }
        DirectoryPublicationSupport::Unsupported => Err(FsError::Unsupported {
            operation: crate::FsOperation::BeginDirectoryPublication,
            capability: "same-parent directory publication on the exact supported filesystem tier",
        }),
        DirectoryPublicationSupport::Unproven => Err(FsError::Unproven {
            operation: crate::FsOperation::BeginDirectoryPublication,
            proof: "the parent handle cannot prove the required runtime filesystem and publication tier",
        }),
    }
}

#[cfg(test)]
pub(crate) fn with_test_directory_publication_support_override<T>(
    support: DirectoryPublicationSupport,
    operation: impl FnOnce() -> T,
) -> T {
    struct OverrideGuard;

    impl Drop for OverrideGuard {
        fn drop(&mut self) {
            TEST_DIRECTORY_PUBLICATION_SUPPORT_OVERRIDE.with(|slot| {
                slot.borrow_mut().take();
            });
        }
    }

    TEST_DIRECTORY_PUBLICATION_SUPPORT_OVERRIDE.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(
            slot.is_none(),
            "nested directory-publication support overrides are forbidden"
        );
        *slot = Some(support);
    });
    let guard = OverrideGuard;
    let result = operation();
    TEST_DIRECTORY_PUBLICATION_SUPPORT_OVERRIDE.with(|slot| {
        assert!(
            slot.borrow().is_none(),
            "the public begin operation did not consume the directory-publication support override"
        );
    });
    drop(guard);
    result
}

pub(crate) fn create_directory_exclusive(
    parent: &File,
    component: &RelativeComponent,
    trust: TrustMode,
) -> Result<File, FsError> {
    imp::create_directory_exclusive(parent, component, trust)
}

pub(crate) fn create_file_exclusive(
    parent: &File,
    component: &RelativeComponent,
    operation: crate::FsOperation,
) -> Result<File, FsError> {
    imp::create_file_exclusive(parent, component, operation)
}

pub(crate) fn discard_temporary(
    parent: &File,
    file: &File,
    name: &RelativeComponent,
    expected: crate::NativeFileIdentity,
) -> Result<(), FsError> {
    imp::discard_temporary(parent, file, name, expected)
}

pub(crate) fn replace_temporary(
    parent: &File,
    temporary: &File,
    source_name: &RelativeComponent,
    target_name: &RelativeComponent,
) -> Result<(), FsError> {
    imp::replace_temporary(parent, temporary, source_name, target_name)
}

pub(crate) fn sync_file(file: &File) -> Result<FileSyncTier, FsError> {
    imp::sync_file(file)
}

pub(crate) fn sync_directory(file: &File) -> Result<DirectorySyncTier, FsError> {
    imp::sync_directory(file)
}

pub(crate) fn try_lock(file: &File, mode: LockMode) -> Result<(), FsError> {
    imp::try_lock(file, mode)
}

pub(crate) fn unlock(file: &File) -> Result<(), FsError> {
    imp::unlock(file)
}

pub(crate) fn clone_file(file: &File) -> Result<File, FsError> {
    file.try_clone()
        .map_err(|source| FsError::io(crate::FsOperation::CloneHandle, source))
}

#[cfg(unix)]
pub(crate) fn rustix_io(operation: crate::FsOperation, error: rustix::io::Errno) -> FsError {
    FsError::io(
        operation,
        io::Error::from_raw_os_error(error.raw_os_error()),
    )
}
