#[cfg(test)]
use std::cell::RefCell;
#[cfg(unix)]
use std::io;
use std::{ffi::OsString, fs::File};

use crate::{
    DirectorySyncTier, FileFacts, FileKind, FileSyncTier, FsError, LockMode,
    PlatformCapabilityMatrix, PublicationSourceBinding, PublicationTopologyEvidence,
    RelativeComponent, ResolutionTier, TrustMode,
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
    static TEST_DIRECTORY_PUBLICATION_PLATFORM_IDENTITY_OVERRIDE: RefCell<Option<u64>> = const { RefCell::new(None) };
    static TEST_DIRECTORY_PUBLICATION_TOPOLOGY_ERROR_OVERRIDE: RefCell<Option<FsError>> = const { RefCell::new(None) };
    static TEST_DIRECTORY_CREATION_POST_MUTATION_ERROR_OVERRIDE: RefCell<Option<FsError>> = const { RefCell::new(None) };
}

pub(crate) enum DirectoryCreation {
    Opened(File),
    #[cfg_attr(not(unix), allow(dead_code))]
    CreatedButUnopened(FsError),
}

#[derive(Clone, Copy)]
pub(crate) enum FileCreateMode {
    Regular,
    Executable,
}

pub(crate) struct NativeDirectoryEntry {
    pub(crate) name: OsString,
    pub(crate) name_bytes: usize,
    pub(crate) kind: FileKind,
    pub(crate) reparse: bool,
}

pub(crate) struct DirectoryEnumerationCursor {
    inner: imp::DirectoryEnumerationCursor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirectoryPublicationSupport {
    Proven(PublicationSourceBinding),
    Unsupported,
    Unproven,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirectoryPublicationBinding {
    HandleBound,
    NameBound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DirectoryPublicationProof {
    source_binding: DirectoryPublicationBinding,
    topology: PublicationTopologyEvidence,
    platform_identity: u64,
}

impl DirectoryPublicationProof {
    pub(crate) const fn source_binding(self) -> PublicationSourceBinding {
        match self.source_binding {
            DirectoryPublicationBinding::HandleBound => PublicationSourceBinding::HandleBound,
            DirectoryPublicationBinding::NameBound => PublicationSourceBinding::NameBound,
        }
    }

    pub(crate) const fn binding(self) -> DirectoryPublicationBinding {
        self.source_binding
    }

    pub(crate) const fn topology(self) -> PublicationTopologyEvidence {
        self.topology
    }
}

pub(crate) fn capability_matrix() -> PlatformCapabilityMatrix {
    imp::capability_matrix()
}

pub(crate) fn preflight_temporary_publication(
    expected: crate::ExpectedTarget,
) -> Result<(), FsError> {
    #[cfg(all(unix, not(target_os = "linux")))]
    if matches!(expected, crate::ExpectedTarget::Missing) {
        return Err(FsError::Unsupported {
            operation: crate::FsOperation::Replace,
            capability: "same-parent no-replace temporary-file publication",
        });
    }
    let _ = expected;
    Ok(())
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

pub(crate) fn begin_directory_enumeration(
    directory: &File,
) -> Result<DirectoryEnumerationCursor, FsError> {
    imp::begin_directory_enumeration(directory).map(|inner| DirectoryEnumerationCursor { inner })
}

pub(crate) fn next_directory_entry(
    cursor: &mut DirectoryEnumerationCursor,
) -> Result<Option<NativeDirectoryEntry>, FsError> {
    imp::next_directory_entry(&mut cursor.inner)
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

pub(crate) fn preflight_exact_spelling(operation: crate::FsOperation) -> Result<(), FsError> {
    #[cfg(any(windows, target_os = "linux"))]
    {
        let _ = operation;
        Ok(())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(FsError::Unsupported {
            operation,
            capability: "handle-bound exact stored spelling verification",
        })
    }
}

pub(crate) fn verify_exact_spelling(
    parent: &File,
    file: &File,
    component: &RelativeComponent,
    operation: crate::FsOperation,
) -> Result<(), FsError> {
    #[cfg(windows)]
    {
        let _ = parent;
        imp::verify_exact_spelling(file, component, operation)
    }
    #[cfg(target_os = "linux")]
    {
        imp::verify_exact_spelling(parent, file, component, operation)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (parent, file, component);
        preflight_exact_spelling(operation)
    }
}

pub(crate) fn child_exists(
    parent: &File,
    component: &RelativeComponent,
    operation: crate::FsOperation,
) -> Result<bool, FsError> {
    match imp::open_child(parent, component, operation) {
        Ok(_) => Ok(true),
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn directory_publication_proof(
    parent: &File,
    trust: TrustMode,
    operation: crate::FsOperation,
) -> Result<DirectoryPublicationProof, FsError> {
    #[cfg(test)]
    let support = TEST_DIRECTORY_PUBLICATION_SUPPORT_OVERRIDE
        .with(|slot| slot.borrow_mut().take())
        .unwrap_or_else(|| imp::directory_publication_support(parent));
    #[cfg(not(test))]
    let support = imp::directory_publication_support(parent);

    let source_binding = match support {
        DirectoryPublicationSupport::Proven(PublicationSourceBinding::HandleBound) => {
            DirectoryPublicationBinding::HandleBound
        }
        DirectoryPublicationSupport::Proven(PublicationSourceBinding::NameBound)
            if trust == TrustMode::TrustedLocal =>
        {
            DirectoryPublicationBinding::NameBound
        }
        DirectoryPublicationSupport::Proven(PublicationSourceBinding::NameBound) => {
            return Err(FsError::Unproven {
                operation,
                proof: "name-bound directory publication is admitted only for trusted cooperative local writers",
            });
        }
        DirectoryPublicationSupport::Proven(PublicationSourceBinding::Unsupported) => {
            return Err(FsError::Unproven {
                operation,
                proof: "the platform adapter reported an unsupported source binding as proven",
            });
        }
        DirectoryPublicationSupport::Unsupported => {
            return Err(FsError::Unsupported {
                operation,
                capability: "same-parent directory publication on the exact supported filesystem tier",
            });
        }
        DirectoryPublicationSupport::Unproven => {
            return Err(FsError::Unproven {
                operation,
                proof: "the parent handle cannot prove the required runtime filesystem and publication tier",
            });
        }
    };

    #[cfg(test)]
    let platform_identity = TEST_DIRECTORY_PUBLICATION_PLATFORM_IDENTITY_OVERRIDE
        .with(|slot| slot.borrow_mut().take())
        .map_or_else(
            || imp::directory_publication_platform_identity(parent, operation),
            Ok,
        )?;
    #[cfg(not(test))]
    let platform_identity = imp::directory_publication_platform_identity(parent, operation)?;

    let topology = if cfg!(windows) {
        PublicationTopologyEvidence::SameVolume
    } else {
        PublicationTopologyEvidence::SameMount
    };
    Ok(DirectoryPublicationProof {
        source_binding,
        topology,
        platform_identity,
    })
}

pub(crate) fn validate_directory_publication_topology(
    file: &File,
    proof: DirectoryPublicationProof,
    operation: crate::FsOperation,
) -> Result<(), FsError> {
    #[cfg(test)]
    if let Some(error) =
        TEST_DIRECTORY_PUBLICATION_TOPOLOGY_ERROR_OVERRIDE.with(|slot| slot.borrow_mut().take())
    {
        return Err(error);
    }

    let current = imp::directory_publication_platform_identity(file, operation)?;
    if current != proof.platform_identity {
        return Err(FsError::Unproven {
            operation,
            proof: "the live directory-publication volume or mount identity changed",
        });
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn with_test_directory_publication_topology_error<T>(
    error: FsError,
    operation: impl FnOnce() -> T,
) -> T {
    struct OverrideGuard;

    impl Drop for OverrideGuard {
        fn drop(&mut self) {
            TEST_DIRECTORY_PUBLICATION_TOPOLOGY_ERROR_OVERRIDE.with(|slot| {
                slot.borrow_mut().take();
            });
        }
    }

    TEST_DIRECTORY_PUBLICATION_TOPOLOGY_ERROR_OVERRIDE.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(
            slot.is_none(),
            "nested directory-publication topology-error overrides are forbidden"
        );
        *slot = Some(error);
    });
    let guard = OverrideGuard;
    let result = operation();
    TEST_DIRECTORY_PUBLICATION_TOPOLOGY_ERROR_OVERRIDE.with(|slot| {
        assert!(
            slot.borrow().is_none(),
            "the public directory-publication operation did not consume the topology-error override"
        );
    });
    drop(guard);
    result
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
            "the public directory-publication operation did not consume the support override"
        );
    });
    drop(guard);
    result
}

#[cfg(test)]
pub(crate) fn with_test_directory_publication_platform_identity_override<T>(
    identity: u64,
    operation: impl FnOnce() -> T,
) -> T {
    struct OverrideGuard;

    impl Drop for OverrideGuard {
        fn drop(&mut self) {
            TEST_DIRECTORY_PUBLICATION_PLATFORM_IDENTITY_OVERRIDE.with(|slot| {
                slot.borrow_mut().take();
            });
        }
    }

    TEST_DIRECTORY_PUBLICATION_PLATFORM_IDENTITY_OVERRIDE.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(
            slot.is_none(),
            "nested directory-publication platform-identity overrides are forbidden"
        );
        *slot = Some(identity);
    });
    let guard = OverrideGuard;
    let result = operation();
    TEST_DIRECTORY_PUBLICATION_PLATFORM_IDENTITY_OVERRIDE.with(|slot| {
        assert!(
            slot.borrow().is_none(),
            "the public directory-publication operation did not consume the platform-identity override"
        );
    });
    drop(guard);
    result
}

pub(crate) fn create_directory_exclusive(
    parent: &File,
    component: &RelativeComponent,
    trust: TrustMode,
) -> Result<DirectoryCreation, FsError> {
    let creation = imp::create_directory_exclusive(parent, component, trust)?;
    #[cfg(test)]
    if let Some(error) =
        TEST_DIRECTORY_CREATION_POST_MUTATION_ERROR_OVERRIDE.with(|slot| slot.borrow_mut().take())
    {
        return Ok(DirectoryCreation::CreatedButUnopened(error));
    }
    Ok(creation)
}

#[cfg(test)]
pub(crate) fn with_test_directory_creation_post_mutation_error<T>(
    error: FsError,
    operation: impl FnOnce() -> T,
) -> T {
    struct OverrideGuard;

    impl Drop for OverrideGuard {
        fn drop(&mut self) {
            TEST_DIRECTORY_CREATION_POST_MUTATION_ERROR_OVERRIDE.with(|slot| {
                slot.borrow_mut().take();
            });
        }
    }

    TEST_DIRECTORY_CREATION_POST_MUTATION_ERROR_OVERRIDE.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(
            slot.is_none(),
            "nested post-mutation directory-creation overrides are forbidden"
        );
        *slot = Some(error);
    });
    let guard = OverrideGuard;
    let result = operation();
    TEST_DIRECTORY_CREATION_POST_MUTATION_ERROR_OVERRIDE.with(|slot| {
        assert!(
            slot.borrow().is_none(),
            "the directory-creation operation did not consume the post-mutation override"
        );
    });
    drop(guard);
    result
}

pub(crate) fn create_file_exclusive(
    parent: &File,
    component: &RelativeComponent,
    operation: crate::FsOperation,
    mode: FileCreateMode,
) -> Result<File, FsError> {
    imp::create_file_exclusive(parent, component, operation, mode)
}

pub(crate) fn publish_directory_no_replace(
    parent: &File,
    source: &File,
    source_name: &RelativeComponent,
    destination: &RelativeComponent,
) -> Result<(), FsError> {
    imp::publish_directory_no_replace(parent, source, source_name, destination)
}

pub(crate) fn discard_directory_exact(directory: &File) -> Result<(), FsError> {
    imp::discard_directory_exact(directory)
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

pub(crate) fn publish_temporary_no_replace(
    parent: &File,
    temporary: &File,
    source_name: &RelativeComponent,
    target_name: &RelativeComponent,
) -> Result<(), FsError> {
    imp::publish_temporary_no_replace(parent, temporary, source_name, target_name)
}

pub(crate) fn sync_file(file: &File) -> Result<FileSyncTier, FsError> {
    imp::sync_file(file)
}

pub(crate) fn write_at(file: &File, buffer: &[u8], offset: u64) -> std::io::Result<usize> {
    imp::write_at(file, buffer, offset)
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

/// Reopens a directory through the capability handle with the restrictive exact-self contract
/// required by an immutable-generation launch.
pub(crate) fn restrictive_clone_directory(file: &File) -> Result<File, FsError> {
    imp::restrictive_clone_directory(file)
}

/// Reopens a regular file through the capability handle with the restrictive exact-self contract
/// required by an immutable-generation launch.
pub(crate) fn restrictive_clone_file(file: &File) -> Result<File, FsError> {
    imp::restrictive_clone_file(file)
}

/// Reopens an executable regular file with the additional execute access needed by the native
/// image-mapping attestation.
pub(crate) fn restrictive_clone_executable(file: &File) -> Result<File, FsError> {
    imp::restrictive_clone_executable(file)
}

#[cfg(unix)]
pub(crate) fn rustix_io(operation: crate::FsOperation, error: rustix::io::Errno) -> FsError {
    FsError::io(
        operation,
        io::Error::from_raw_os_error(error.raw_os_error()),
    )
}
