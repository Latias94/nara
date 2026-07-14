use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara_fs::{
    CapabilityRights, ConflictProtection, ContentDigest, DirectoryCapability, DirectorySyncTier,
    ExpectedTarget, FileCapability, FsError, HostCapabilityOptions, LockMode,
    ParentAuthorizationTier, ProofStatus, PublicationAtomicity, PublicationIdentityEvidence,
    RelativeComponent, RelativePath, ResolutionTier, StageStatus, TrustMode,
    platform_capability_matrix,
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must follow the Unix epoch")
            .as_nanos();
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nara_fs_test_{}_{}_{}",
            std::process::id(),
            stamp,
            sequence
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap_or_else(|error| {
            panic!("failed to remove test root {}: {error}", self.0.display())
        });
    }
}

fn relative(path: &str) -> RelativePath {
    RelativePath::new(Path::new(path)).unwrap()
}

fn component(value: &str) -> RelativeComponent {
    RelativeComponent::new(value).unwrap()
}

fn host_directory(path: &Path) -> File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE,
        };

        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap()
    }

    #[cfg(unix)]
    {
        File::open(path).unwrap()
    }
}

#[cfg(windows)]
fn create_junction(link: &Path, target: &Path) {
    use std::os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    };
    use std::{io, mem::size_of, ptr};
    use windows_sys::Win32::{
        Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::IO::DeviceIoControl,
    };

    const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00a4;
    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xa000_0003;

    fs::create_dir(link).unwrap();
    let target = target.as_os_str().encode_wide().collect::<Vec<_>>();
    let mut substitute = "\\??\\".encode_utf16().collect::<Vec<_>>();
    substitute.extend_from_slice(&target);
    let print_name = target;
    let path_units = substitute
        .iter()
        .copied()
        .chain(std::iter::once(0))
        .chain(print_name.iter().copied())
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let substitute_bytes = substitute.len() * size_of::<u16>();
    let print_offset = substitute_bytes + size_of::<u16>();
    let print_bytes = print_name.len() * size_of::<u16>();
    let reparse_data_length = 8 + path_units.len() * size_of::<u16>();
    let mut buffer = vec![0_u8; 8 + reparse_data_length];
    buffer[0..4].copy_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    buffer[4..6].copy_from_slice(&(reparse_data_length as u16).to_le_bytes());
    buffer[8..10].copy_from_slice(&0_u16.to_le_bytes());
    buffer[10..12].copy_from_slice(&(substitute_bytes as u16).to_le_bytes());
    buffer[12..14].copy_from_slice(&(print_offset as u16).to_le_bytes());
    buffer[14..16].copy_from_slice(&(print_bytes as u16).to_le_bytes());
    for (index, unit) in path_units.iter().enumerate() {
        let offset = 16 + index * size_of::<u16>();
        buffer[offset..offset + 2].copy_from_slice(&unit.to_le_bytes());
    }

    let mut link_name = link.as_os_str().encode_wide().collect::<Vec<_>>();
    link_name.push(0);
    // SAFETY: `link_name` is null-terminated and every pointer remains valid
    // for the duration of the synchronous handle-open call.
    let raw = unsafe {
        CreateFileW(
            link_name.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        panic!(
            "privileged Windows junction fixture could not be opened: {}",
            io::Error::last_os_error()
        );
    }
    // SAFETY: `CreateFileW` returned unique ownership of a valid handle.
    let file = unsafe { File::from_raw_handle(raw.cast()) };
    let mut returned = 0_u32;
    // SAFETY: the input buffer contains a complete mount-point reparse record;
    // no output buffer or asynchronous OVERLAPPED state is requested.
    let succeeded = unsafe {
        DeviceIoControl(
            file.as_raw_handle().cast(),
            FSCTL_SET_REPARSE_POINT,
            buffer.as_ptr().cast(),
            buffer.len() as u32,
            ptr::null_mut(),
            0,
            &mut returned,
            ptr::null_mut(),
        )
    };
    if succeeded == 0 {
        panic!(
            "privileged Windows junction fixture could not be created: {}",
            io::Error::last_os_error()
        );
    }
}

fn portable_trust() -> TrustMode {
    if cfg!(any(windows, target_os = "linux")) {
        TrustMode::Untrusted
    } else {
        TrustMode::TrustedLocal
    }
}

fn portable_read_only(root: &TestRoot) -> DirectoryCapability {
    DirectoryCapability::from_host_handle(
        host_directory(root.path()),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
    )
    .unwrap()
}

fn portable_read_write(root: &TestRoot) -> DirectoryCapability {
    DirectoryCapability::from_host_handle(
        host_directory(root.path()),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, portable_trust()),
    )
    .unwrap()
}

#[cfg(target_os = "linux")]
fn strict_read_write(root: &TestRoot) -> DirectoryCapability {
    DirectoryCapability::from_host_handle(
        host_directory(root.path()),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .unwrap()
}

fn trusted_read_write(root: &TestRoot) -> DirectoryCapability {
    DirectoryCapability::from_host_handle(
        host_directory(root.path()),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap()
}

#[test]
fn normal_relative_open_returns_a_handle_without_retaining_a_host_path() {
    let root = TestRoot::new();
    fs::create_dir(root.path().join("assets")).unwrap();
    fs::write(root.path().join("assets/hello.txt"), b"hello").unwrap();
    let capability = portable_read_only(&root);

    let file = capability.open_file(&relative("assets/hello.txt")).unwrap();
    let mut reader = file.reader().unwrap();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();

    assert_eq!(bytes, b"hello");
    assert!(!format!("{capability:?}").contains(&root.path().display().to_string()));
}

#[test]
fn identity_is_scoped_to_capability_session_and_generation() {
    let root = TestRoot::new();
    fs::write(root.path().join("value.txt"), b"value").unwrap();
    let capability = DirectoryCapability::from_host_handle(
        host_directory(root.path()),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()).with_generation(7),
    )
    .unwrap();
    let file = capability.open_file(&relative("value.txt")).unwrap();

    assert_eq!(file.identity().session(), capability.identity().session());
    assert_eq!(file.identity().generation().get(), 7);
    assert_eq!(file.resolution_tier(), capability.resolution_tier());
    assert!(format!("{:?}", file.identity()).contains("<sensitive>"));
}

#[test]
fn imported_file_authority_is_limited_to_its_opened_handle() {
    let root = TestRoot::new();
    let path = root.path().join("value.txt");
    fs::write(&path, b"value").unwrap();

    let file =
        FileCapability::from_host_handle(File::open(path).unwrap(), portable_trust(), 3).unwrap();

    assert_eq!(file.resolution_tier(), ResolutionTier::OpenedHandleOnly);
}

#[test]
fn derived_directory_handle_survives_name_replacement_without_escaping() {
    let root = TestRoot::new();
    let original = root.path().join("assets");
    let moved = root.path().join("moved-assets");
    fs::create_dir(&original).unwrap();
    fs::write(original.join("value.txt"), b"old").unwrap();
    let capability = portable_read_only(&root);
    let derived = capability.open_directory(&relative("assets")).unwrap();

    fs::rename(&original, &moved).unwrap();
    fs::create_dir(&original).unwrap();
    fs::write(original.join("value.txt"), b"new").unwrap();

    let file = derived.open_file(&relative("value.txt")).unwrap();
    let mut bytes = Vec::new();
    file.reader().unwrap().read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"old");
}

#[test]
fn root_handle_survives_authority_name_replacement() {
    let outer = TestRoot::new();
    let original = outer.path().join("authority");
    let moved = outer.path().join("moved-authority");
    fs::create_dir(&original).unwrap();
    fs::write(original.join("value.txt"), b"authorized").unwrap();
    let capability = DirectoryCapability::from_host_handle(
        host_directory(&original),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
    )
    .unwrap();

    fs::rename(&original, &moved).unwrap();
    fs::create_dir(&original).unwrap();
    fs::write(original.join("value.txt"), b"replacement").unwrap();

    let mut bytes = Vec::new();
    capability
        .open_file(&relative("value.txt"))
        .unwrap()
        .reader()
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, b"authorized");
}

#[test]
fn opened_leaf_handle_is_stable_after_leaf_name_replacement() {
    let root = TestRoot::new();
    let target = root.path().join("value.txt");
    fs::write(&target, b"old").unwrap();
    let capability = portable_read_only(&root);
    let opened = capability.open_file(&relative("value.txt")).unwrap();

    fs::rename(&target, root.path().join("old-value.txt")).unwrap();
    fs::write(&target, b"new").unwrap();

    let mut bytes = Vec::new();
    opened.reader().unwrap().read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"old");
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn strict_mode_rejects_multi_link_files() {
    let root = TestRoot::new();
    let target = root.path().join("value.txt");
    fs::write(&target, b"value").unwrap();
    fs::hard_link(&target, root.path().join("alias.txt")).unwrap();
    let capability = portable_read_only(&root);

    let error = capability.open_file(&relative("value.txt")).unwrap_err();
    assert!(matches!(error, FsError::MultipleLinks { .. }));
}

#[test]
fn exclusive_temp_collision_is_reported_without_truncation() {
    let root = TestRoot::new();
    fs::write(root.path().join("candidate.tmp"), b"keep").unwrap();
    let capability = portable_read_write(&root);

    let error = capability
        .create_temp(&component("candidate.tmp"))
        .unwrap_err();
    assert!(matches!(error, FsError::AlreadyExists { .. }));
    assert_eq!(
        fs::read(root.path().join("candidate.tmp")).unwrap(),
        b"keep"
    );
}

#[test]
fn same_directory_replace_is_atomic_but_does_not_claim_strong_cas() {
    let root = TestRoot::new();
    fs::write(root.path().join("document.scene"), b"old").unwrap();
    let capability = trusted_read_write(&root);
    let old = capability.open_file(&relative("document.scene")).unwrap();
    let mut temporary = capability.create_temp(&component("document.tmp")).unwrap();
    temporary.write_all(b"new").unwrap();
    temporary.sync().unwrap();

    let receipt = capability
        .replace_temp(
            temporary,
            &component("document.scene"),
            ExpectedTarget::Identity(old.identity()),
        )
        .unwrap();

    assert_eq!(
        receipt.conflict_protection(),
        ConflictProtection::DetectOnly
    );
    assert_eq!(
        receipt.parent_authorization(),
        ParentAuthorizationTier::HandleBound
    );
    assert_eq!(
        receipt.publication_atomicity(),
        PublicationAtomicity::AtomicNameSwitch
    );
    assert!(receipt.naming_is_atomic());
    assert_eq!(
        receipt.published_identity(),
        Some(receipt.candidate_identity())
    );
    if cfg!(windows) {
        assert_eq!(
            receipt.publication_identity_evidence(),
            PublicationIdentityEvidence::HandleBoundCandidate
        );
    } else {
        assert_eq!(
            receipt.publication_identity_evidence(),
            PublicationIdentityEvidence::PostPublishObserved
        );
    }
    assert_eq!(receipt.durability().data_synced(), StageStatus::Achieved);
    assert_eq!(
        receipt.durability().file_metadata_synced(),
        StageStatus::Achieved
    );
    assert_eq!(receipt.durability().name_published(), StageStatus::Achieved);
    assert_eq!(
        fs::read(root.path().join("document.scene")).unwrap(),
        b"new"
    );
}

#[cfg(windows)]
#[test]
fn replacement_sharing_failure_preserves_target_and_discards_candidate() {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let root = TestRoot::new();
    let target = root.path().join("document.scene");
    fs::write(&target, b"old").unwrap();
    let capability = trusted_read_write(&root);
    let old = capability.open_file(&relative("document.scene")).unwrap();
    let _sharing_blocker = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&target)
        .unwrap();
    let mut temporary = capability.create_temp(&component("document.tmp")).unwrap();
    temporary.write_all(b"new").unwrap();

    let error = capability
        .replace_temp(
            temporary,
            &component("document.scene"),
            ExpectedTarget::Identity(old.identity()),
        )
        .unwrap_err();

    assert!(matches!(error, FsError::Io { .. }));
    assert_eq!(fs::read(target).unwrap(), b"old");
    assert!(!root.path().join("document.tmp").exists());
}

#[test]
fn failed_expected_identity_preserves_target_and_reports_cleanup_tier() {
    let root = TestRoot::new();
    fs::write(root.path().join("document.scene"), b"old").unwrap();
    let capability = trusted_read_write(&root);
    let unrelated = capability.open_file(&relative("document.scene")).unwrap();
    fs::write(root.path().join("other.scene"), b"other").unwrap();
    let other = capability.open_file(&relative("other.scene")).unwrap();
    let mut temporary = capability.create_temp(&component("document.tmp")).unwrap();
    temporary.write_all(b"new").unwrap();

    let error = capability
        .replace_temp(
            temporary,
            &component("document.scene"),
            ExpectedTarget::Identity(other.identity()),
        )
        .unwrap_err();

    assert!(matches!(error, FsError::TargetStateMismatch));
    assert_eq!(
        fs::read(root.path().join("document.scene")).unwrap(),
        b"old"
    );
    if cfg!(windows) {
        assert!(!root.path().join("document.tmp").exists());
    } else {
        assert!(root.path().join("document.tmp").exists());
    }
    drop(unrelated);
}

#[cfg(windows)]
#[test]
fn explicit_and_drop_cleanup_remove_the_temporary_name() {
    let root = TestRoot::new();
    let capability = trusted_read_write(&root);

    capability
        .create_temp(&component("explicit.tmp"))
        .unwrap()
        .discard()
        .unwrap();
    assert!(!root.path().join("explicit.tmp").exists());

    {
        let mut temporary = capability.create_temp(&component("drop.tmp")).unwrap();
        temporary.write_all(b"candidate").unwrap();
    }
    assert!(!root.path().join("drop.tmp").exists());
}

#[cfg(unix)]
#[test]
fn unix_cleanup_fails_closed_and_preserves_recoverable_orphans() {
    let root = TestRoot::new();
    let capability = trusted_read_write(&root);

    let error = capability
        .create_temp(&component("explicit.tmp"))
        .unwrap()
        .discard()
        .unwrap_err();
    assert!(matches!(error, FsError::Unsupported { .. }));
    assert!(root.path().join("explicit.tmp").exists());

    {
        let mut temporary = capability.create_temp(&component("drop.tmp")).unwrap();
        temporary.write_all(b"candidate").unwrap();
    }
    assert!(root.path().join("drop.tmp").exists());
}

#[test]
fn read_only_capability_refuses_write_primitives() {
    let root = TestRoot::new();
    let capability = portable_read_only(&root);

    assert!(matches!(
        capability.create_temp(&component("candidate.tmp")),
        Err(FsError::ReadOnlyCapability { .. })
    ));
}

#[test]
fn file_lock_contention_is_structured() {
    let root = TestRoot::new();
    fs::write(root.path().join("workspace.lock"), b"lock").unwrap();
    let capability = portable_read_only(&root);
    let first = capability.open_file(&relative("workspace.lock")).unwrap();
    let second = capability.open_file(&relative("workspace.lock")).unwrap();
    let _held = first.try_lock(LockMode::Exclusive).unwrap();

    let error = second.try_lock(LockMode::Exclusive).unwrap_err();
    assert!(matches!(error, FsError::LockContended));
}

#[test]
fn explicit_lock_release_allows_the_next_owner() {
    let root = TestRoot::new();
    fs::write(root.path().join("workspace.lock"), b"lock").unwrap();
    let capability = portable_read_only(&root);
    let first = capability.open_file(&relative("workspace.lock")).unwrap();
    let second = capability.open_file(&relative("workspace.lock")).unwrap();

    first
        .try_lock(LockMode::Exclusive)
        .unwrap()
        .release()
        .unwrap();
    let _next = second.try_lock(LockMode::Exclusive).unwrap();
}

#[test]
fn dropped_lock_guard_allows_the_next_owner() {
    let root = TestRoot::new();
    fs::write(root.path().join("workspace.lock"), b"lock").unwrap();
    let capability = portable_read_only(&root);
    let first = capability.open_file(&relative("workspace.lock")).unwrap();
    let second = capability.open_file(&relative("workspace.lock")).unwrap();

    let held = first.try_lock(LockMode::Exclusive).unwrap();
    drop(held);

    let _next = second.try_lock(LockMode::Exclusive).unwrap();
}

#[test]
fn streaming_digest_is_bounded_and_mismatch_is_structured() {
    let root = TestRoot::new();
    fs::write(root.path().join("asset.bin"), b"asset bytes").unwrap();
    let capability = portable_read_only(&root);
    let file = capability.open_file(&relative("asset.bin")).unwrap();

    let digest = file.digest(1024).unwrap();
    assert_eq!(digest, ContentDigest::of_bytes(b"asset bytes"));
    assert_eq!(digest.length(), 11);
    assert!(matches!(
        file.verify_digest(ContentDigest::of_bytes(b"different"), 1024),
        Err(FsError::DigestMismatch { .. })
    ));
    assert!(matches!(
        file.digest(4),
        Err(FsError::ByteLimitExceeded { .. })
    ));
}

#[test]
fn bounded_read_accepts_exact_length_and_rejects_one_sentinel_byte() {
    let root = TestRoot::new();
    let payload = (0..=(64 * 1024))
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    fs::write(root.path().join("asset.bin"), &payload).unwrap();
    fs::write(root.path().join("empty.bin"), []).unwrap();
    let capability = portable_read_only(&root);
    let file = capability.open_file(&relative("asset.bin")).unwrap();
    let empty = capability.open_file(&relative("empty.bin")).unwrap();

    assert_eq!(
        file.read_to_end_bounded(payload.len() as u64).unwrap(),
        payload
    );
    assert!(matches!(
        file.read_to_end_bounded((payload.len() - 1) as u64),
        Err(FsError::ByteLimitExceeded { limit }) if limit == (payload.len() - 1) as u64
    ));
    assert_eq!(empty.read_to_end_bounded(0).unwrap(), Vec::<u8>::new());
}

#[test]
fn readers_use_independent_positional_offsets() {
    let root = TestRoot::new();
    fs::write(root.path().join("asset.bin"), b"abcdef").unwrap();
    let capability = portable_read_only(&root);
    let file = capability.open_file(&relative("asset.bin")).unwrap();
    let mut first = file.reader().unwrap();
    let mut second = file.reader().unwrap();
    let mut first_pair = [0_u8; 2];
    let mut second_pair = [0_u8; 2];

    first.read_exact(&mut first_pair).unwrap();
    second.read_exact(&mut second_pair).unwrap();
    assert_eq!(&first_pair, b"ab");
    assert_eq!(&second_pair, b"ab");

    first.read_exact(&mut first_pair).unwrap();
    second.read_exact(&mut second_pair).unwrap();
    assert_eq!(&first_pair, b"cd");
    assert_eq!(&second_pair, b"cd");
}

#[test]
fn sync_and_platform_proof_tiers_are_explicit() {
    let root = TestRoot::new();
    let capability = portable_read_write(&root);
    let matrix = platform_capability_matrix();

    if cfg!(windows) {
        assert_eq!(matrix.strict_relative_open(), ProofStatus::Proven);
    } else {
        assert_eq!(matrix.strict_relative_open(), ProofStatus::Unproven);
    }
    if cfg!(any(windows, target_os = "linux")) {
        assert_eq!(
            capability.resolution_tier(),
            nara_fs::ResolutionTier::HandleBound
        );
    } else {
        assert_eq!(
            capability.resolution_tier(),
            nara_fs::ResolutionTier::Unproven
        );
    }
    match matrix.directory_sync_tier() {
        DirectorySyncTier::Unsupported => {
            assert!(matches!(
                capability.sync(),
                Err(FsError::Unsupported { .. })
            ));
        }
        tier => assert_eq!(capability.sync().unwrap().tier(), tier),
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
#[test]
fn strict_non_linux_unix_capability_is_explicitly_unproven() {
    let root = TestRoot::new();
    let error = DirectoryCapability::from_host_handle(
        host_directory(root.path()),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted),
    )
    .unwrap_err();

    assert!(matches!(error, FsError::Unproven { .. }));
    assert_eq!(
        platform_capability_matrix().strict_relative_open(),
        ProofStatus::Unproven
    );
}

#[test]
fn deferred_public_primitives_fail_explicitly_and_match_the_matrix() {
    let root = TestRoot::new();
    fs::write(root.path().join("value.txt"), b"value").unwrap();
    let capability = portable_read_write(&root);
    let identity = capability
        .open_file(&relative("value.txt"))
        .unwrap()
        .identity();
    let matrix = platform_capability_matrix();

    assert_eq!(matrix.read_directory(), ProofStatus::Unsupported);
    assert_eq!(matrix.relative_unlink(), ProofStatus::Unsupported);
    assert_eq!(matrix.non_overwrite_rename(), ProofStatus::Unsupported);
    assert!(matches!(
        capability.read_directory(&relative("value.txt")),
        Err(FsError::Unsupported { .. })
    ));
    assert!(matches!(
        capability.unlink_file(&relative("value.txt"), identity),
        Err(FsError::Unsupported { .. })
    ));
    assert!(matches!(
        capability.rename_file_no_replace(&component("value.txt"), &component("moved.txt")),
        Err(FsError::Unsupported { .. })
    ));
}

#[cfg(target_os = "linux")]
#[test]
fn strict_unix_replacement_rejects_name_bound_candidate_publication() {
    let root = TestRoot::new();
    fs::write(root.path().join("document.scene"), b"old").unwrap();
    let capability = strict_read_write(&root);
    let old = capability.open_file(&relative("document.scene")).unwrap();
    let mut temporary = capability.create_temp(&component("document.tmp")).unwrap();
    temporary.write_all(b"new").unwrap();

    let error = capability
        .replace_temp(
            temporary,
            &component("document.scene"),
            ExpectedTarget::Identity(old.identity()),
        )
        .unwrap_err();

    assert!(matches!(error, FsError::Unproven { .. }));
    assert_eq!(
        fs::read(root.path().join("document.scene")).unwrap(),
        b"old"
    );
    assert!(root.path().join("document.tmp").exists());
}

#[cfg(windows)]
#[test]
#[ignore = "requires live Windows symlink privileges; the default gate covers this tag through the public open fixture"]
fn privileged_windows_strict_mode_rejects_reparse_leaf() {
    use std::os::windows::fs::symlink_file;

    let root = TestRoot::new();
    fs::write(root.path().join("target.txt"), b"target").unwrap();
    symlink_file(root.path().join("target.txt"), root.path().join("link.txt")).unwrap_or_else(
        |error| panic!("privileged Windows file-symlink fixture could not be created: {error}"),
    );
    let capability = portable_read_only(&root);

    assert!(matches!(
        capability.open_file(&relative("link.txt")),
        Err(FsError::ReparsePoint { .. })
    ));
}

#[cfg(windows)]
#[test]
#[ignore = "requires live Windows symlink privileges; the default gate covers this tag through the public open fixture"]
fn privileged_windows_strict_mode_rejects_reparse_intermediate_escape() {
    use std::os::windows::fs::symlink_dir;

    let root = TestRoot::new();
    let outside = TestRoot::new();
    fs::write(outside.path().join("secret.txt"), b"secret").unwrap();
    symlink_dir(outside.path(), root.path().join("escape")).unwrap_or_else(|error| {
        panic!("privileged Windows directory-symlink fixture could not be created: {error}")
    });
    let capability = portable_read_only(&root);

    assert!(matches!(
        capability.open_file(&relative("escape/secret.txt")),
        Err(FsError::ReparsePoint { .. })
    ));
}

#[cfg(windows)]
#[test]
#[ignore = "requires live Windows junction creation; the default gate covers the mount-point tag through the public open fixture"]
fn privileged_windows_strict_mode_rejects_junction_intermediate_escape() {
    let root = TestRoot::new();
    let outside = TestRoot::new();
    let junction = root.path().join("escape");
    fs::write(outside.path().join("secret.txt"), b"secret").unwrap();
    create_junction(&junction, outside.path());
    let capability = portable_read_only(&root);

    assert!(matches!(
        capability.open_file(&relative("escape/secret.txt")),
        Err(FsError::ReparsePoint { .. })
    ));

    drop(capability);
    fs::remove_dir(junction).unwrap();
}

#[cfg(unix)]
#[test]
fn unix_open_rejects_symlink_leaf() {
    use std::os::unix::fs::symlink;

    let root = TestRoot::new();
    fs::write(root.path().join("target.txt"), b"target").unwrap();
    symlink("target.txt", root.path().join("link.txt")).unwrap();
    let capability = portable_read_only(&root);

    assert!(capability.open_file(&relative("link.txt")).is_err());
}
