use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(any(windows, target_os = "linux"))]
use std::collections::HashSet;
#[cfg(windows)]
use std::fs::OpenOptions;

use nara_fs::{
    CapabilityRights, ConflictProtection, ContentDigest, DirectoryCapability,
    DirectoryEnumerationLimits, DirectoryPublicationBeginError, DirectorySyncTier, ExpectedTarget,
    FileCapability, FsError, HostCapabilityOptions, LockMode, ParentAuthorizationTier, ProofStatus,
    PublicationAtomicity, PublicationIdentityEvidence, PublicationSourceBinding, RelativeComponent,
    RelativePath, ResolutionTier, StageStatus, TrustMode, platform_capability_matrix,
};
#[cfg(any(windows, target_os = "linux"))]
use nara_fs::{DirectoryEnumerationSession, FileKind};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "linux")]
#[path = "support/unix_file_open.rs"]
mod unix_file_open;

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

#[cfg(any(windows, target_os = "linux"))]
fn assert_spelling_rejected<T: std::fmt::Debug>(result: Result<T, FsError>) {
    match result.unwrap_err() {
        FsError::ExactSpellingMismatch { .. } => {}
        // A directory with native case-sensitive lookup rejects the alias before verification.
        FsError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {}
        error => panic!("expected an exact-spelling rejection, got {error:?}"),
    }
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn exact_spelling_opens_nested_directories_and_the_verified_file() {
    let root = TestRoot::new();
    fs::create_dir_all(root.path().join("Assets/Scenes")).unwrap();
    fs::write(
        root.path().join("Assets/Scenes/startup.scene.json"),
        b"scene",
    )
    .unwrap();

    for trust in [
        TrustMode::TrustedLocal,
        TrustMode::Untrusted,
        TrustMode::Recovery,
    ] {
        let capability = DirectoryCapability::from_host_handle(
            host_directory(root.path()),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, trust).with_generation(17),
        )
        .unwrap();
        let path = relative("Assets/Scenes/startup.scene.json");
        let file = capability.open_file_exact(&path).unwrap();
        assert_eq!(
            file.identity(),
            capability.open_file(&path).unwrap().identity()
        );
        assert_eq!(file.read_to_end_bounded(5).unwrap(), b"scene");
        assert_eq!(file.identity().generation().get(), 17);

        let directories = relative("Assets/Scenes");
        let directory = capability.open_directory_exact(&directories).unwrap();
        assert_eq!(
            directory.identity(),
            capability.open_directory(&directories).unwrap().identity()
        );
        assert_eq!(directory.options().rights(), CapabilityRights::ReadOnly);
        assert_eq!(directory.options().trust(), trust);
        assert_eq!(directory.resolution_tier(), capability.resolution_tier());
        assert_eq!(
            directory
                .open_file_exact(&relative("startup.scene.json"))
                .unwrap()
                .identity(),
            file.identity()
        );
    }
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn exact_spelling_rejects_aliases_in_every_directory_and_filename_component() {
    let root = TestRoot::new();
    fs::create_dir_all(root.path().join("Assets/Scenes")).unwrap();
    fs::write(
        root.path().join("Assets/Scenes/startup.scene.json"),
        b"scene",
    )
    .unwrap();
    let capability = portable_read_only(&root);

    for path in [
        "assets/Scenes/startup.scene.json",
        "Assets/scenes/startup.scene.json",
        "Assets/Scenes/STARTUP.scene.json",
        "ASSETS/SCENES/STARTUP.SCENE.JSON",
    ] {
        assert_spelling_rejected(capability.open_file_exact(&relative(path)));
    }
    for path in ["assets", "assets/Scenes", "Assets/scenes", "ASSETS/SCENES"] {
        assert_spelling_rejected(capability.open_directory_exact(&relative(path)));
    }
    assert!(
        capability
            .open_file_exact(&relative("Assets/Scenes/startup.scene.json"))
            .is_ok()
    );
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn exact_spelling_compares_unicode_without_casefolding_or_normalization() {
    let root = TestRoot::new();
    fs::create_dir_all(root.path().join("\u{00c4}rt/Scenes")).unwrap();
    fs::write(
        root.path().join("\u{00c4}rt/Scenes/caf\u{00e9}.scene.json"),
        b"scene",
    )
    .unwrap();
    let capability = portable_read_only(&root);

    assert!(
        capability
            .open_directory_exact(&relative("\u{00c4}rt/Scenes"))
            .is_ok()
    );
    assert!(
        capability
            .open_file_exact(&relative("\u{00c4}rt/Scenes/caf\u{00e9}.scene.json"))
            .is_ok()
    );
    assert_spelling_rejected(capability.open_directory_exact(&relative("\u{00e4}rt/Scenes")));
    for path in [
        "\u{00e4}rt/Scenes/caf\u{00e9}.scene.json",
        "\u{00c4}rt/Scenes/caf\u{00c9}.scene.json",
        "\u{00c4}rt/Scenes/cafe\u{0301}.scene.json",
    ] {
        assert_spelling_rejected(capability.open_file_exact(&relative(path)));
    }
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn exact_spelling_does_not_relax_strict_single_link_admission() {
    let root = TestRoot::new();
    fs::write(root.path().join("startup.scene.json"), b"scene").unwrap();
    fs::hard_link(
        root.path().join("startup.scene.json"),
        root.path().join("alias.scene.json"),
    )
    .unwrap();
    let capability = portable_read_only(&root);

    for name in ["startup.scene.json", "alias.scene.json"] {
        assert!(matches!(
            capability.open_file_exact(&relative(name)),
            Err(FsError::MultipleLinks { links: 2 })
        ));
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
#[test]
fn exact_spelling_fails_closed_without_a_unix_stored_name_verifier() {
    let root = TestRoot::new();
    fs::create_dir(root.path().join("Assets")).unwrap();
    fs::write(root.path().join("Assets/startup.scene.json"), b"scene").unwrap();
    let capability = DirectoryCapability::from_host_handle(
        host_directory(root.path()),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
    )
    .unwrap();
    let path = relative("Assets/startup.scene.json");
    assert!(capability.open_file(&path).is_ok());
    assert!(matches!(
        capability.open_file_exact(&path),
        Err(FsError::Unsupported {
            operation: nara_fs::FsOperation::OpenFile,
            ..
        })
    ));
    assert!(matches!(
        capability.open_directory_exact(&relative("Assets")),
        Err(FsError::Unsupported {
            operation: nara_fs::FsOperation::OpenDirectory,
            ..
        })
    ));
    assert!(matches!(
        capability.open_file_exact(&relative("missing.scene.json")),
        Err(FsError::Unsupported { .. })
    ));
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

#[cfg(any(windows, target_os = "linux"))]
fn collect_directory_entries(
    mut session: DirectoryEnumerationSession,
    batch_limit: usize,
) -> Vec<nara_fs::DirectoryEntryObservation> {
    let mut entries = Vec::new();
    loop {
        let batch = session.next_batch(batch_limit).unwrap();
        assert!(batch.entries().len() + batch.rejected_entry_facts().len() <= batch_limit);
        entries.extend_from_slice(batch.entries());
        if batch.is_complete() {
            return entries;
        }
    }
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

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn directory_same_object_matches_independent_imports_without_equating_sessions() {
    let root = TestRoot::new();
    let first = portable_read_only(&root);
    let second = portable_read_only(&root);
    let first_identity = first.identity();
    let second_identity = second.identity();

    assert_ne!(first_identity.session(), second_identity.session());
    assert_eq!(first_identity.generation(), second_identity.generation());
    assert_ne!(first_identity, second_identity);
    assert!(first.same_object(&second).unwrap());
    assert!(second.same_object(&first).unwrap());
    assert_eq!(first.identity(), first_identity);
    assert_eq!(second.identity(), second_identity);
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn directory_same_object_rejects_different_live_roots() {
    let first_root = TestRoot::new();
    let second_root = TestRoot::new();
    let first = portable_read_only(&first_root);
    let second = portable_read_only(&second_root);

    assert!(!first.same_object(&second).unwrap());
    assert!(!second.same_object(&first).unwrap());
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn file_same_object_compares_live_objects_across_independent_sessions() {
    let root = TestRoot::new();
    fs::write(root.path().join("original.bin"), b"same bytes").unwrap();
    fs::write(root.path().join("copy.bin"), b"same bytes").unwrap();
    let first_parent = portable_read_only(&root);
    let second_parent = portable_read_only(&root);
    let first = first_parent
        .open_child_file(&component("original.bin"))
        .unwrap();
    let second = second_parent
        .open_child_file(&component("original.bin"))
        .unwrap();
    let copy = second_parent
        .open_child_file(&component("copy.bin"))
        .unwrap();
    let first_identity = first.identity();
    let second_identity = second.identity();

    assert_ne!(first_identity.session(), second_identity.session());
    assert_ne!(first_identity, second_identity);
    assert!(first.same_object(&second).unwrap());
    assert!(second.same_object(&first).unwrap());
    assert!(!first.same_object(&copy).unwrap());
    assert_eq!(first.identity(), first_identity);
    assert_eq!(second.identity(), second_identity);
    assert_eq!(first.rights(), CapabilityRights::ReadOnly);
    assert_eq!(second.rights(), CapabilityRights::ReadOnly);

    fs::rename(
        root.path().join("original.bin"),
        root.path().join("moved.bin"),
    )
    .unwrap();
    fs::write(root.path().join("original.bin"), b"same bytes").unwrap();
    let replacement = second_parent
        .open_child_file(&component("original.bin"))
        .unwrap();
    assert!(first.same_object(&second).unwrap());
    assert!(!first.same_object(&replacement).unwrap());
    assert_eq!(first.identity(), first_identity);
    assert_eq!(second.identity(), second_identity);
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn file_same_object_preserves_independent_rights_trust_and_generation() {
    let root = TestRoot::new();
    let writable_parent = trusted_read_write(&root);
    let mut created = writable_parent
        .create_child_file(&component("value.txt"))
        .unwrap();
    created.writer().unwrap().write_all(b"value").unwrap();
    let independent_parent = DirectoryCapability::from_host_handle(
        host_directory(root.path()),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted)
            .with_generation(7),
    )
    .unwrap();
    let independent = independent_parent
        .open_child_file(&component("value.txt"))
        .unwrap();
    let created_identity = created.identity();
    let independent_identity = independent.identity();
    assert_ne!(created_identity.session(), independent_identity.session());
    assert_ne!(
        created_identity.generation(),
        independent_identity.generation()
    );
    assert!(created.same_object(&independent).unwrap());
    assert!(independent.same_object(&created).unwrap());
    assert_eq!(created.identity(), created_identity);
    assert_eq!(created.rights(), CapabilityRights::ReadWrite);
    assert_eq!(created.trust(), TrustMode::TrustedLocal);
    assert_eq!(independent.identity(), independent_identity);
    assert_eq!(independent.rights(), CapabilityRights::ReadOnly);
    assert_eq!(independent.trust(), TrustMode::Untrusted);
    created.writer().unwrap().write_all(b"!").unwrap();
    assert_eq!(independent.read_to_end_bounded(6).unwrap(), b"value!");
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn directory_same_object_preserves_strict_and_trusted_authority() {
    let root = TestRoot::new();
    fs::write(root.path().join("value.txt"), b"value").unwrap();
    fs::hard_link(root.path().join("value.txt"), root.path().join("alias.txt")).unwrap();
    let trusted = trusted_read_write(&root);
    let trusted_options = trusted.options();
    let trusted_identity = trusted.identity();
    let trusted_resolution = trusted.resolution_tier();

    for trust in [TrustMode::Untrusted, TrustMode::Recovery] {
        let strict_options =
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, trust).with_generation(7);
        let strict =
            DirectoryCapability::from_host_handle(host_directory(root.path()), strict_options)
                .unwrap();
        let strict_identity = strict.identity();
        let strict_resolution = strict.resolution_tier();

        assert_ne!(strict_identity.generation(), trusted_identity.generation());
        assert!(strict.same_object(&trusted).unwrap());
        assert!(trusted.same_object(&strict).unwrap());
        assert_eq!(strict.options(), strict_options);
        assert_eq!(trusted.options(), trusted_options);
        assert_eq!(strict.identity(), strict_identity);
        assert_eq!(trusted.identity(), trusted_identity);
        assert_eq!(strict.resolution_tier(), strict_resolution);
        assert_eq!(trusted.resolution_tier(), trusted_resolution);
        assert!(matches!(
            strict.open_file(&relative("value.txt")),
            Err(FsError::MultipleLinks { links: 2 })
        ));
        assert_eq!(
            trusted
                .open_file(&relative("value.txt"))
                .unwrap()
                .read_to_end_bounded(5)
                .unwrap(),
            b"value"
        );
        assert!(matches!(
            strict.create_child_directory(&component("trusted-child")),
            Err(FsError::ReadOnlyCapability {
                operation: nara_fs::FsOperation::CreateDirectory
            })
        ));
    }

    let child = trusted
        .create_child_directory(&component("trusted-child"))
        .unwrap();
    assert_eq!(child.options(), trusted_options);
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn directory_same_object_uses_kept_handles_after_name_replacement() {
    let root = TestRoot::new();
    let original = root.path().join("authority");
    let moved = root.path().join("moved-authority");
    fs::create_dir(&original).unwrap();
    let options = HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted);
    let first = DirectoryCapability::from_host_handle(host_directory(&original), options).unwrap();
    let second = DirectoryCapability::from_host_handle(host_directory(&original), options).unwrap();
    let first_identity = first.identity();
    let second_identity = second.identity();
    assert!(first.same_object(&second).unwrap());

    fs::rename(&original, &moved).unwrap();

    // The original name is absent while both imported handles remain live.
    assert!(first.same_object(&second).unwrap());
    assert!(second.same_object(&first).unwrap());

    fs::create_dir(&original).unwrap();
    let replacement =
        DirectoryCapability::from_host_handle(host_directory(&original), options).unwrap();

    assert!(first.same_object(&second).unwrap());
    assert!(!first.same_object(&replacement).unwrap());
    assert!(!second.same_object(&replacement).unwrap());
    assert_eq!(first.identity(), first_identity);
    assert_eq!(second.identity(), second_identity);
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
    assert!(matches!(error.error(), FsError::AlreadyExists { .. }));
    assert!(error.cleanup().is_none());
    assert_eq!(
        fs::read(root.path().join("candidate.tmp")).unwrap(),
        b"keep"
    );
}

#[test]
fn direct_child_creation_keeps_write_authority_explicit() {
    let root = TestRoot::new();
    let capability = trusted_read_write(&root);
    let directory = capability
        .create_child_directory(&component("candidate"))
        .unwrap();
    let reopened_directory = capability
        .open_child_directory(&component("candidate"))
        .unwrap();
    assert_eq!(reopened_directory.identity(), directory.identity());
    let mut created = directory
        .create_child_file(&component("member.bin"))
        .unwrap();

    assert_eq!(created.rights(), CapabilityRights::ReadWrite);
    created.writer().unwrap().write_all(b"member").unwrap();
    created.sync().unwrap();
    drop(created);

    let mut reopened = directory.open_child_file(&component("member.bin")).unwrap();
    assert_eq!(reopened.rights(), CapabilityRights::ReadOnly);
    assert!(matches!(
        reopened.writer(),
        Err(FsError::ReadOnlyCapability {
            operation: nara_fs::FsOperation::Write
        })
    ));
    assert_eq!(
        reopened.read_to_end_bounded(64).unwrap(),
        b"member".to_vec()
    );
}

#[test]
fn directory_publication_begin_uses_a_live_parent_proof_or_fails_closed() {
    let root = TestRoot::new();
    let capability = trusted_read_write(&root);
    let result = capability.begin_directory_publication(
        &component("published"),
        &component(".published-staging-53c1"),
    );

    match result {
        Ok(transaction) => {
            assert_eq!(
                transaction.parent_identity().session(),
                transaction.staging_identity().session()
            );
            if cfg!(windows) {
                assert_eq!(
                    transaction.source_binding(),
                    PublicationSourceBinding::HandleBound
                );
            } else {
                assert_eq!(
                    transaction.source_binding(),
                    PublicationSourceBinding::NameBound
                );
            }
        }
        Err(DirectoryPublicationBeginError::BeforeStaging(
            FsError::Unsupported { .. } | FsError::Unproven { .. },
        )) => {
            assert!(!root.path().join(".published-staging-53c1").exists());
        }
        Err(error) => panic!("unexpected directory-publication begin failure: {error}"),
    }
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
        receipt.written_content_digest(),
        ContentDigest::of_bytes(b"new")
    );
    assert_eq!(
        receipt.published_content_digest(),
        Some(ContentDigest::of_bytes(b"new"))
    );
    assert_eq!(
        receipt.verified_content_digest(),
        Some(ContentDigest::of_bytes(b"new"))
    );
    assert_eq!(
        fs::read(root.path().join("document.scene")).unwrap(),
        b"new"
    );
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn missing_target_publication_uses_atomic_no_replace() {
    let root = TestRoot::new();
    let capability = trusted_read_write(&root);
    let mut temporary = capability.create_temp(&component("document.tmp")).unwrap();
    temporary.write_all(b"first").unwrap();
    temporary.sync().unwrap();

    let receipt = capability
        .replace_temp(
            temporary,
            &component("document.scene"),
            ExpectedTarget::Missing,
        )
        .unwrap();

    assert_eq!(receipt.previous_identity(), None);
    assert_eq!(
        receipt.conflict_protection(),
        ConflictProtection::StrongCompareAndSwap
    );
    assert_eq!(
        fs::read(root.path().join("document.scene")).unwrap(),
        b"first"
    );

    let mut collision = capability.create_temp(&component("collision.tmp")).unwrap();
    collision.write_all(b"second").unwrap();
    let error = capability
        .replace_temp(
            collision,
            &component("document.scene"),
            ExpectedTarget::Missing,
        )
        .unwrap_err();

    assert!(matches!(error.error(), FsError::TargetStateMismatch));
    assert!(error.cleanup().is_some());
    assert_eq!(
        fs::read(root.path().join("document.scene")).unwrap(),
        b"first"
    );
}

#[test]
fn replacement_receipt_detects_candidate_bytes_changed_outside_its_capability() {
    let root = TestRoot::new();
    fs::write(root.path().join("document.scene"), b"old").unwrap();
    let capability = trusted_read_write(&root);
    let old = capability.open_file(&relative("document.scene")).unwrap();
    let mut temporary = capability.create_temp(&component("document.tmp")).unwrap();
    temporary.write_all(b"intended").unwrap();
    temporary.sync().unwrap();

    fs::write(root.path().join("document.tmp"), b"tampered").unwrap();
    let receipt = capability
        .replace_temp(
            temporary,
            &component("document.scene"),
            ExpectedTarget::Identity(old.identity()),
        )
        .unwrap();

    assert_eq!(
        receipt.written_content_digest(),
        ContentDigest::of_bytes(b"intended")
    );
    assert_eq!(
        receipt.published_content_digest(),
        Some(ContentDigest::of_bytes(b"tampered"))
    );
    assert_ne!(
        receipt.published_content_digest(),
        Some(receipt.written_content_digest())
    );
    assert_eq!(receipt.verified_content_digest(), None);
}

#[cfg(windows)]
#[test]
fn replacement_sharing_failure_preserves_target_and_candidate_for_explicit_cleanup() {
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
    let candidate_identity = temporary.identity();

    let error = capability
        .replace_temp(
            temporary,
            &component("document.scene"),
            ExpectedTarget::Identity(old.identity()),
        )
        .unwrap_err();

    assert!(matches!(error.error(), FsError::Io { .. }));
    assert_eq!(
        error.cleanup().unwrap().identity(),
        Some(candidate_identity)
    );
    assert_eq!(fs::read(target).unwrap(), b"old");
    assert!(root.path().join("document.tmp").exists());
    drop(_sharing_blocker);
    error.into_parts().1.unwrap().discard().unwrap();
    assert!(!root.path().join("document.tmp").exists());
}

#[test]
fn failed_expected_identity_preserves_target_and_candidate_for_explicit_cleanup() {
    let root = TestRoot::new();
    fs::write(root.path().join("document.scene"), b"old").unwrap();
    let capability = trusted_read_write(&root);
    let unrelated = capability.open_file(&relative("document.scene")).unwrap();
    fs::write(root.path().join("other.scene"), b"other").unwrap();
    let other = capability.open_file(&relative("other.scene")).unwrap();
    let mut temporary = capability.create_temp(&component("document.tmp")).unwrap();
    temporary.write_all(b"new").unwrap();
    let candidate_identity = temporary.identity();

    let error = capability
        .replace_temp(
            temporary,
            &component("document.scene"),
            ExpectedTarget::Identity(other.identity()),
        )
        .unwrap_err();

    assert!(matches!(error.error(), FsError::TargetStateMismatch));
    assert_eq!(
        error.cleanup().unwrap().identity(),
        Some(candidate_identity)
    );
    assert_eq!(
        fs::read(root.path().join("document.scene")).unwrap(),
        b"old"
    );
    assert!(root.path().join("document.tmp").exists());
    #[cfg(windows)]
    {
        error.into_parts().1.unwrap().discard().unwrap();
        assert!(!root.path().join("document.tmp").exists());
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

#[cfg(windows)]
#[test]
fn explicit_cleanup_failure_retains_the_candidate_until_read_only_is_cleared() {
    let root = TestRoot::new();
    let capability = trusted_read_write(&root);
    let temporary = capability.create_temp(&component("candidate.tmp")).unwrap();
    let identity = temporary.identity();
    let path = root.path().join("candidate.tmp");
    let original_permissions = fs::metadata(&path).unwrap().permissions();
    let mut permissions = original_permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();

    let error = temporary.discard().unwrap_err();
    assert!(matches!(error.error(), FsError::Io { .. }));
    assert_eq!(error.cleanup().unwrap().identity(), Some(identity));
    assert!(path.exists());
    fs::set_permissions(&path, original_permissions).unwrap();
    error.into_parts().1.unwrap().discard().unwrap();
    assert!(!path.exists());
}

#[cfg(any(windows, unix))]
#[test]
fn cleanup_of_a_renamed_candidate_does_not_remove_its_name_replacement() {
    let root = TestRoot::new();
    let capability = trusted_read_write(&root);
    let mut temporary = capability.create_temp(&component("candidate.tmp")).unwrap();
    temporary.write_all(b"candidate").unwrap();
    let identity = temporary.identity();
    fs::rename(
        root.path().join("candidate.tmp"),
        root.path().join("moved.tmp"),
    )
    .unwrap();
    fs::write(root.path().join("candidate.tmp"), b"replacement").unwrap();

    let result = temporary.discard();
    assert_eq!(
        fs::read(root.path().join("candidate.tmp")).unwrap(),
        b"replacement"
    );
    #[cfg(windows)]
    {
        result.unwrap();
        assert!(!root.path().join("moved.tmp").exists());
        let _ = identity;
    }
    #[cfg(unix)]
    {
        let error = result.unwrap_err();
        assert!(matches!(error.error(), FsError::Unsupported { .. }));
        assert_eq!(error.cleanup().unwrap().identity(), Some(identity));
        assert_eq!(
            fs::read(root.path().join("moved.tmp")).unwrap(),
            b"candidate"
        );
    }
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
    assert!(matches!(error.error(), FsError::Unsupported { .. }));
    let identity = error.cleanup().unwrap().identity();
    let error = error.into_parts().1.unwrap().discard().unwrap_err();
    assert!(matches!(error.error(), FsError::Unsupported { .. }));
    assert_eq!(error.cleanup().unwrap().identity(), identity);
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
        capability.create_child_directory(&component("candidate")),
        Err(FsError::ReadOnlyCapability {
            operation: nara_fs::FsOperation::CreateDirectory
        })
    ));
    assert!(matches!(
        capability.create_child_file(&component("candidate.bin")),
        Err(FsError::ReadOnlyCapability {
            operation: nara_fs::FsOperation::CreateFile
        })
    ));
    let error = capability
        .create_temp(&component("candidate.tmp"))
        .unwrap_err();
    assert!(matches!(error.error(), FsError::ReadOnlyCapability { .. }));
    assert!(error.cleanup().is_none());
    assert!(matches!(
        capability.preflight_replace(ExpectedTarget::Any),
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

    let second_identity = second.identity();
    let error = second.try_lock(LockMode::Exclusive).unwrap_err();
    assert!(matches!(error.error(), FsError::LockContended));
    assert_eq!(error.file().identity(), second_identity);
    let second = error.into_file();
    drop(_held);
    let held = second.try_lock(LockMode::Exclusive).unwrap();
    assert_eq!(held.identity(), second_identity);
}

#[test]
fn explicit_lock_release_allows_the_next_owner() {
    let root = TestRoot::new();
    fs::write(root.path().join("workspace.lock"), b"lock").unwrap();
    let capability = portable_read_only(&root);
    let first = capability.open_file(&relative("workspace.lock")).unwrap();
    let second = capability.open_file(&relative("workspace.lock")).unwrap();

    let identity = first.identity();
    let first = first
        .try_lock(LockMode::Exclusive)
        .unwrap()
        .release()
        .unwrap();
    assert_eq!(first.identity(), identity);
    let next = second.try_lock(LockMode::Exclusive).unwrap();

    // Closing the released original must not unlock the next owner's independent handle.
    drop(first);
    let contender = capability.open_file(&relative("workspace.lock")).unwrap();
    assert!(matches!(
        contender.try_lock(LockMode::Exclusive).unwrap_err().error(),
        FsError::LockContended
    ));
    drop(next);
}

#[test]
fn dropped_locked_file_allows_the_next_owner() {
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
fn locked_readers_keep_independent_offsets_without_releasing_lock_authority() {
    let root = TestRoot::new();
    fs::write(root.path().join("workspace.lock"), b"abcdef").unwrap();
    let capability = portable_read_only(&root);
    let file = capability.open_file(&relative("workspace.lock")).unwrap();
    let identity = file.identity();
    let held = file.try_lock(LockMode::Exclusive).unwrap();

    assert_eq!(held.identity(), identity);
    assert_eq!(held.parent_identity(), Some(capability.identity()));
    assert_eq!(held.mode(), LockMode::Exclusive);
    assert_eq!(held.rights(), CapabilityRights::ReadOnly);
    assert_eq!(held.read_to_end_bounded(6).unwrap(), b"abcdef");
    held.verify_digest(ContentDigest::of_bytes(b"abcdef"), 6)
        .unwrap();
    {
        let mut first = held.reader().unwrap();
        let mut second = held.reader().unwrap();
        let mut pair = [0_u8; 2];
        first.read_exact(&mut pair).unwrap();
        assert_eq!(&pair, b"ab");
        first.read_exact(&mut pair).unwrap();
        assert_eq!(&pair, b"cd");
        second.read_exact(&mut pair).unwrap();
        assert_eq!(&pair, b"ab");
    }
    let contender = capability.open_file(&relative("workspace.lock")).unwrap();
    let failure = contender.try_lock(LockMode::Exclusive).unwrap_err();
    assert!(matches!(failure.error(), FsError::LockContended));
    let contender = failure.into_file();
    drop(held.release().unwrap());
    let _next = contender.try_lock(LockMode::Exclusive).unwrap();
}

#[cfg(unix)]
#[test]
fn original_same_owner_dual_guard_primitive_releases_both_logical_locks() {
    use rustix::fs::{FlockOperation, flock};

    let root = TestRoot::new();
    let path = root.path().join("workspace.lock");
    fs::write(&path, b"lock").unwrap();
    let original = File::open(&path).unwrap();
    let duplicate = original.try_clone().unwrap();
    let independent = File::open(&path).unwrap();

    // The former borrowed guard API allowed both calls against this one native lock state.
    flock(&original, FlockOperation::NonBlockingLockExclusive).unwrap();
    flock(&duplicate, FlockOperation::NonBlockingLockExclusive).unwrap();
    assert!(flock(&independent, FlockOperation::NonBlockingLockExclusive).is_err());
    flock(&duplicate, FlockOperation::Unlock).unwrap();
    flock(&independent, FlockOperation::NonBlockingLockExclusive).unwrap();
    flock(&independent, FlockOperation::Unlock).unwrap();
}

#[cfg(any(windows, unix))]
#[test]
fn exported_native_file_lease_prevents_later_lock_authority() {
    let root = TestRoot::new();
    fs::write(root.path().join("workspace.lock"), b"lock").unwrap();
    let capability = portable_read_only(&root);
    let file = capability.open_file(&relative("workspace.lock")).unwrap();
    let identity = file.identity();
    let lease = file.duplicate_restrictive_host_handle().unwrap();
    let failure = file.try_lock(LockMode::Exclusive).unwrap_err();
    assert!(matches!(
        failure.error(),
        FsError::Unproven {
            operation: nara_fs::FsOperation::Lock,
            ..
        }
    ));
    assert_eq!(failure.file().identity(), identity);

    // Export is irreversible: the native recipient may have duplicated the lease again.
    drop(lease);
    let file = failure.into_file();
    assert_eq!(file.read_to_end_bounded(4).unwrap(), b"lock");
    assert!(matches!(
        file.try_lock(LockMode::Exclusive).unwrap_err().error(),
        FsError::Unproven {
            operation: nara_fs::FsOperation::Lock,
            ..
        }
    ));
    let independent = capability.open_file(&relative("workspace.lock")).unwrap();
    let _held = independent.try_lock(LockMode::Exclusive).unwrap();
}

#[cfg(unix)]
#[test]
fn exported_executable_lease_cannot_become_a_shared_description_lock_owner() {
    let root = TestRoot::new();
    fs::write(root.path().join("workspace.lock"), b"lock").unwrap();
    let capability = portable_read_only(&root);
    let file = capability.open_file(&relative("workspace.lock")).unwrap();
    let _lease = file.duplicate_restrictive_executable_host_handle().unwrap();
    let failure = file.try_lock(LockMode::Shared).unwrap_err();
    assert!(matches!(
        failure.error(),
        FsError::Unproven {
            operation: nara_fs::FsOperation::Lock,
            ..
        }
    ));
}

#[cfg(unix)]
#[test]
fn reimported_native_lease_cannot_bypass_original_export_rejection() {
    use std::os::fd::AsFd;

    let root = TestRoot::new();
    fs::write(root.path().join("workspace.lock"), b"lock").unwrap();
    let capability = portable_read_only(&root);
    let file = capability.open_file(&relative("workspace.lock")).unwrap();
    let lease = file.duplicate_restrictive_host_handle().unwrap();
    let alias = File::from(lease.as_fd().try_clone_to_owned().unwrap());
    let imported = FileCapability::from_host_handle(alias, TrustMode::TrustedLocal, 1).unwrap();

    for file in [file, imported] {
        let failure = file.try_lock(LockMode::Exclusive).unwrap_err();
        assert!(matches!(
            failure.error(),
            FsError::Unproven {
                operation: nara_fs::FsOperation::Lock,
                ..
            }
        ));
    }
    let independent = capability.open_file(&relative("workspace.lock")).unwrap();
    let _held = independent.try_lock(LockMode::Exclusive).unwrap();
}

#[cfg(any(windows, unix))]
#[test]
fn imported_native_aliases_cannot_create_independent_rust_lock_owners() {
    let root = TestRoot::new();
    let path = root.path().join("workspace.lock");
    fs::write(&path, b"lock").unwrap();
    let host = File::open(&path).unwrap();
    let alias = host.try_clone().unwrap();
    let first = FileCapability::from_host_handle(host, TrustMode::TrustedLocal, 1).unwrap();
    let second = FileCapability::from_host_handle(alias, TrustMode::TrustedLocal, 1).unwrap();

    for file in [first, second] {
        let identity = file.identity();
        let failure = file.try_lock(LockMode::Exclusive).unwrap_err();
        assert!(matches!(
            failure.error(),
            FsError::Unproven {
                operation: nara_fs::FsOperation::Lock,
                ..
            }
        ));
        assert_eq!(failure.file().identity(), identity);
        assert_eq!(failure.into_file().read_to_end_bounded(4).unwrap(), b"lock");
    }
}

#[test]
fn shared_locks_preserve_other_owners_until_every_owner_releases() {
    let root = TestRoot::new();
    fs::write(root.path().join("workspace.lock"), b"lock").unwrap();
    let capability = portable_read_only(&root);
    let first = capability.open_file(&relative("workspace.lock")).unwrap();
    let second = capability.open_file(&relative("workspace.lock")).unwrap();
    let first = first.try_lock(LockMode::Shared).unwrap();
    let second = second.try_lock(LockMode::Shared).unwrap();
    let contender = capability.open_file(&relative("workspace.lock")).unwrap();
    let failure = contender.try_lock(LockMode::Exclusive).unwrap_err();
    assert!(matches!(failure.error(), FsError::LockContended));
    let contender = failure.into_file();

    drop(first.release().unwrap());
    let failure = contender.try_lock(LockMode::Exclusive).unwrap_err();
    assert!(matches!(failure.error(), FsError::LockContended));
    let contender = failure.into_file();
    drop(second);
    let _exclusive = contender.try_lock(LockMode::Exclusive).unwrap();
}

#[test]
fn owned_lock_returns_original_created_file_write_and_sync_authority() {
    let root = TestRoot::new();
    let capability = portable_read_write(&root);
    let mut file = capability
        .create_child_file(&component("workspace.lock"))
        .unwrap();
    let identity = file.identity();
    {
        let mut writer = file.writer().unwrap();
        writer.write_all(b"locked contents").unwrap();
        writer.flush().unwrap();
    }
    let held = file.try_lock(LockMode::Exclusive).unwrap();
    assert_eq!(held.read_to_end_bounded(15).unwrap(), b"locked contents");
    {
        let mut reader = held.reader().unwrap();
        reader.seek(SeekFrom::Start(7)).unwrap();
        let mut pair = [0_u8; 2];
        reader.read_exact(&mut pair).unwrap();
        assert_eq!(&pair, b"co");
    }
    let contender = capability.open_file(&relative("workspace.lock")).unwrap();
    let failure = contender.try_lock(LockMode::Exclusive).unwrap_err();
    assert!(matches!(failure.error(), FsError::LockContended));
    let contender = failure.into_file();
    let mut file = held.release().unwrap();
    assert_eq!(file.identity(), identity);
    assert_eq!(file.rights(), CapabilityRights::ReadWrite);
    file.writer().unwrap().write_all(b"!").unwrap();
    assert_eq!(
        file.sync().unwrap().progress().data_synced(),
        StageStatus::Achieved
    );
    assert_eq!(file.read_to_end_bounded(16).unwrap(), b"locked contents!");
    let _next = contender.try_lock(LockMode::Exclusive).unwrap();
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
fn writer_position_survives_reads_seeks_and_reborrows() {
    let root = TestRoot::new();
    let capability = portable_read_write(&root);
    let mut file = capability
        .create_child_file(&component("asset.bin"))
        .unwrap();
    let identity = file.identity();
    file.writer().unwrap().write_all(b"abcdef").unwrap();
    {
        let mut reader = file.reader().unwrap();
        let mut pair = [0_u8; 2];
        reader.read_exact(&mut pair).unwrap();
        assert_eq!(&pair, b"ab");
        reader.seek(SeekFrom::Start(99)).unwrap();
    }
    {
        let mut writer = file.writer().unwrap();
        assert_eq!(writer.write(&[]).unwrap(), 0);
        writer.write_all(b"gh").unwrap();
        writer.flush().unwrap();
    }
    assert_eq!(file.read_to_end_bounded(8).unwrap(), b"abcdefgh");
    file.verify_digest(ContentDigest::of_bytes(b"abcdefgh"), 8)
        .unwrap();
    {
        let mut first = file.reader().unwrap();
        let mut second = file.reader().unwrap();
        let mut pair = [0_u8; 2];
        first.seek(SeekFrom::End(-2)).unwrap();
        first.read_exact(&mut pair).unwrap();
        assert_eq!(&pair, b"gh");
        first.seek(SeekFrom::Current(-4)).unwrap();
        first.read_exact(&mut pair).unwrap();
        assert_eq!(&pair, b"ef");
        second.read_exact(&mut pair).unwrap();
        assert_eq!(&pair, b"ab");
    }
    file.writer().unwrap().write_all(b"ij").unwrap();
    assert_eq!(file.identity(), identity);
    assert_eq!(file.read_to_end_bounded(10).unwrap(), b"abcdefghij");
}

#[test]
fn writer_keeps_exact_handle_and_position_after_name_replacement_and_external_growth() {
    let root = TestRoot::new();
    let capability = portable_read_write(&root);
    let original = root.path().join("asset.bin");
    let moved = root.path().join("moved.bin");
    let mut file = capability
        .create_child_file(&component("asset.bin"))
        .unwrap();
    let identity = file.identity();
    file.writer().unwrap().write_all(b"abcdef").unwrap();
    fs::rename(&original, &moved).unwrap();
    fs::write(&original, b"replacement").unwrap();
    {
        let mut external = fs::OpenOptions::new().append(true).open(&moved).unwrap();
        external.write_all(b"tail").unwrap();
    }
    assert_eq!(file.read_to_end_bounded(10).unwrap(), b"abcdeftail");
    file.writer().unwrap().write_all(b"gh").unwrap();
    assert_eq!(file.identity(), identity);
    assert_eq!(file.read_to_end_bounded(10).unwrap(), b"abcdefghil");
    assert_eq!(fs::read(&moved).unwrap(), b"abcdefghil");
    assert_eq!(fs::read(&original).unwrap(), b"replacement");
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
fn unavailable_no_replace_publication_is_rejected_before_candidate_creation() {
    let root = TestRoot::new();
    let capability = trusted_read_write(&root);

    capability.preflight_replace(ExpectedTarget::Any).unwrap();
    assert!(matches!(
        capability.preflight_replace(ExpectedTarget::Missing),
        Err(FsError::Unsupported {
            operation: nara_fs::FsOperation::Replace,
            ..
        })
    ));
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
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

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn handle_bound_directory_enumeration_is_bounded_and_cursor_independent() {
    let root = TestRoot::new();
    fs::create_dir(root.path().join("scenes")).unwrap();
    fs::write(root.path().join("scenes").join("nested.scene"), b"nested").unwrap();
    fs::write(root.path().join("alpha.scene"), b"alpha").unwrap();
    fs::write(root.path().join("beta.prefab"), b"beta").unwrap();
    fs::write(root.path().join("notes.txt"), b"notes").unwrap();
    let capability = portable_read_only(&root);
    let limits = DirectoryEnumerationLimits::new(16, 1_024, 4);

    assert_eq!(
        platform_capability_matrix().read_directory(),
        ProofStatus::Proven
    );
    let mut first = capability.begin_child_enumeration(limits).unwrap();
    let first_batch = first.next_batch(2).unwrap();
    assert_eq!(first_batch.entries().len(), 2);
    assert!(first_batch.rejected_entry_facts().is_empty());
    assert!(!first_batch.is_complete());
    assert!(
        first_batch
            .entries()
            .iter()
            .all(|entry| entry.identity().is_some())
    );

    let mut first_entries = first_batch.entries().to_vec();
    first_entries.extend(collect_directory_entries(first, 2));
    let second_entries =
        collect_directory_entries(capability.begin_child_enumeration(limits).unwrap(), 3);

    let expected = [
        component("scenes"),
        component("alpha.scene"),
        component("beta.prefab"),
        component("notes.txt"),
    ]
    .into_iter()
    .collect::<HashSet<_>>();
    let first_names = first_entries
        .iter()
        .map(|entry| entry.name().clone())
        .collect::<HashSet<_>>();
    let second_names = second_entries
        .iter()
        .map(|entry| entry.name().clone())
        .collect::<HashSet<_>>();

    assert_eq!(first_names, expected);
    assert_eq!(second_names, expected);
    assert_eq!(
        first_entries
            .iter()
            .find(|entry| entry.name() == &component("scenes"))
            .unwrap()
            .kind(),
        FileKind::Directory
    );

    let scenes = capability.open_directory(&relative("scenes")).unwrap();
    let child_entries = collect_directory_entries(
        scenes
            .begin_child_enumeration(DirectoryEnumerationLimits::default())
            .unwrap(),
        1,
    );
    assert_eq!(child_entries.len(), 1);
    assert_eq!(child_entries[0].name(), &component("nested.scene"));
    assert_eq!(child_entries[0].kind(), FileKind::Regular);
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn handle_bound_directory_enumeration_stays_on_open_object_after_name_replacement() {
    let outer = TestRoot::new();
    let original = outer.path().join("original");
    let moved = outer.path().join("moved");
    fs::create_dir(&original).unwrap();
    fs::write(original.join("authorized.scene"), b"authorized").unwrap();
    let capability = DirectoryCapability::from_host_handle(
        host_directory(&original),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted),
    )
    .unwrap();
    let session = capability
        .begin_child_enumeration(DirectoryEnumerationLimits::default())
        .unwrap();

    fs::rename(&original, &moved).unwrap();
    fs::create_dir(&original).unwrap();
    fs::write(original.join("replacement.scene"), b"replacement").unwrap();

    let reopened_names = collect_directory_entries(
        capability
            .begin_child_enumeration(DirectoryEnumerationLimits::default())
            .unwrap(),
        1,
    )
    .into_iter()
    .map(|entry| entry.name().clone())
    .collect::<HashSet<_>>();
    assert_eq!(
        reopened_names,
        HashSet::from([component("authorized.scene")])
    );
    let names = collect_directory_entries(session, 1)
        .into_iter()
        .map(|entry| entry.name().clone())
        .collect::<HashSet<_>>();
    assert_eq!(names, HashSet::from([component("authorized.scene")]));
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn directory_enumeration_limit_failure_is_terminal() {
    let root = TestRoot::new();
    for name in ["one.txt", "two.txt", "three.txt"] {
        fs::write(root.path().join(name), name.as_bytes()).unwrap();
    }
    let capability = portable_read_only(&root);
    let mut session = capability
        .begin_child_enumeration(DirectoryEnumerationLimits::new(2, 1_024, 4))
        .unwrap();

    assert!(matches!(
        session.next_batch(8),
        Err(FsError::DirectoryEnumerationEntryLimitExceeded { limit: 2 })
    ));
    assert!(matches!(
        session.next_batch(1),
        Err(FsError::DirectoryEnumerationTerminated)
    ));
}

#[cfg(target_os = "linux")]
#[test]
fn linux_directory_enumeration_isolates_unsafe_entries_without_following_or_opening_them() {
    use nara_fs::DirectoryEntryRejectionReason;
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::symlink},
    };

    let root = TestRoot::new();
    fs::write(root.path().join("safe.txt"), b"safe").unwrap();
    fs::write(root.path().join("linked.txt"), b"multiple links").unwrap();
    fs::hard_link(
        root.path().join("linked.txt"),
        root.path().join("alias.txt"),
    )
    .unwrap();
    fs::write(
        root.path().join(OsString::from_vec(vec![0xff])),
        b"unportable",
    )
    .unwrap();
    symlink("safe.txt", root.path().join("symlink")).unwrap();
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        root.path().join("fifo"),
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    let capability = portable_read_only(&root);
    let mut session = capability
        .begin_child_enumeration(DirectoryEnumerationLimits::new(16, 1_024, 8))
        .unwrap();
    let mut entries = Vec::new();
    let mut rejected = Vec::new();
    loop {
        let batch = session.next_batch(1).unwrap();
        assert!(batch.entries().len() + batch.rejected_entry_facts().len() <= 1);
        entries.extend_from_slice(batch.entries());
        rejected.extend_from_slice(batch.rejected_entry_facts());
        if batch.is_complete() {
            break;
        }
    }
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name(), &component("safe.txt"));
    assert_eq!(entries[0].kind(), FileKind::Regular);
    assert!(entries[0].identity().is_some());
    assert_eq!(rejected.len(), 5);
    for (name, reason) in [
        (
            Some(component("linked.txt")),
            DirectoryEntryRejectionReason::MultipleLinks,
        ),
        (
            Some(component("alias.txt")),
            DirectoryEntryRejectionReason::MultipleLinks,
        ),
        (None, DirectoryEntryRejectionReason::UnportableName),
        (
            Some(component("symlink")),
            DirectoryEntryRejectionReason::ReparsePoint,
        ),
        (
            Some(component("fifo")),
            DirectoryEntryRejectionReason::UnsupportedKind,
        ),
    ] {
        assert!(
            rejected
                .iter()
                .any(|fact| fact.name() == name.as_ref() && fact.reason() == reason)
        );
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
#[test]
fn unsupported_directory_enumeration_matches_the_platform_matrix() {
    let root = TestRoot::new();
    let capability = portable_read_only(&root);

    assert_eq!(
        platform_capability_matrix().read_directory(),
        ProofStatus::Unsupported
    );
    assert!(matches!(
        capability.begin_child_enumeration(DirectoryEnumerationLimits::default()),
        Err(FsError::Unsupported { .. }) | Err(FsError::Unproven { .. })
    ));
}

#[test]
fn reserved_rename_remains_explicitly_unsupported() {
    let root = TestRoot::new();
    fs::write(root.path().join("value.txt"), b"value").unwrap();
    let capability = portable_read_write(&root);
    let matrix = platform_capability_matrix();

    assert_eq!(matrix.non_overwrite_rename(), ProofStatus::Unsupported);
    assert!(matches!(
        capability.rename_file_no_replace(&component("value.txt"), &component("moved.txt")),
        Err(FsError::Unsupported { .. })
    ));
}

#[test]
fn relative_unlink_matrix_reports_exact_object_support_independently_of_content_policy() {
    let expected = if cfg!(all(windows, target_arch = "x86_64")) {
        ProofStatus::Proven
    } else {
        ProofStatus::Unsupported
    };
    assert_eq!(platform_capability_matrix().relative_unlink(), expected);
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn windows_regular_file_open_reports_a_directory_without_losing_native_kind_evidence() {
    let root = TestRoot::new();
    fs::create_dir(root.path().join("directory")).unwrap();
    let capability = portable_read_only(&root);
    assert!(matches!(
        capability.open_file(&relative("directory")),
        Err(FsError::NotRegularFile)
    ));
    assert!(root.path().join("directory").is_dir());
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn explicit_unlink_removes_the_observed_object_while_its_reader_remains_usable() {
    let root = TestRoot::new();
    fs::create_dir(root.path().join("nested")).unwrap();
    fs::write(root.path().join("nested/value.txt"), b"value").unwrap();
    let capability = portable_read_write(&root);
    let independent_parent = portable_read_only(&root);
    let observed = independent_parent
        .open_file(&relative("nested/value.txt"))
        .unwrap();
    let receipt = capability
        .unlink_file(&relative("nested/value.txt"), &observed)
        .unwrap();
    assert_eq!(
        receipt.source_binding(),
        PublicationSourceBinding::HandleBound
    );
    assert_eq!(receipt.name_removed(), StageStatus::Achieved);
    assert_eq!(receipt.parent_directory_synced(), StageStatus::Unsupported);
    assert!(!root.path().join("nested/value.txt").exists());
    assert_eq!(observed.read_to_end_bounded(5).unwrap(), b"value");
    fs::write(root.path().join("nested/value.txt"), b"replacement").unwrap();
    let replacement = capability.open_file(&relative("nested/value.txt")).unwrap();
    assert!(!observed.same_object(&replacement).unwrap());
    assert!(matches!(
        capability.unlink_file(&relative("nested/value.txt"), &observed),
        Err(FsError::IdentityMismatch { .. })
    ));
    assert_eq!(
        fs::read(root.path().join("nested/value.txt")).unwrap(),
        b"replacement"
    );
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn explicit_unlink_rejects_readonly_rights_conflicting_writers_and_multilink_objects() {
    use std::os::windows::fs::OpenOptionsExt;

    let root = TestRoot::new();
    fs::write(root.path().join("value.txt"), b"value").unwrap();
    let writable = portable_read_write(&root);
    let readonly = portable_read_only(&root);
    let observed = readonly.open_file(&relative("value.txt")).unwrap();
    assert!(matches!(
        readonly.unlink_file(&relative("value.txt"), &observed),
        Err(FsError::ReadOnlyCapability {
            operation: nara_fs::FsOperation::Unlink
        })
    ));
    let writer = OpenOptions::new()
        .write(true)
        .share_mode(3)
        .open(root.path().join("value.txt"))
        .unwrap();
    assert!(matches!(
        writable.unlink_file(&relative("value.txt"), &observed),
        Err(FsError::Io { .. })
    ));
    assert_eq!(fs::read(root.path().join("value.txt")).unwrap(), b"value");
    drop(writer);
    let original_permissions = fs::metadata(root.path().join("value.txt"))
        .unwrap()
        .permissions();
    let mut readonly_permissions = original_permissions.clone();
    readonly_permissions.set_readonly(true);
    fs::set_permissions(root.path().join("value.txt"), readonly_permissions).unwrap();
    let readonly_result = writable.unlink_file(&relative("value.txt"), &observed);
    fs::set_permissions(root.path().join("value.txt"), original_permissions).unwrap();
    assert!(matches!(readonly_result, Err(FsError::Io { .. })));
    assert_eq!(fs::read(root.path().join("value.txt")).unwrap(), b"value");
    fs::hard_link(root.path().join("value.txt"), root.path().join("alias.txt")).unwrap();
    assert!(matches!(
        writable.unlink_file(&relative("value.txt"), &observed),
        Err(FsError::MultipleLinks { .. })
    ));
    assert_eq!(fs::read(root.path().join("alias.txt")).unwrap(), b"value");
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn explicit_unlink_authorizes_the_whole_object_without_claiming_unchanged_bytes() {
    use std::os::windows::fs::OpenOptionsExt;

    let root = TestRoot::new();
    fs::write(root.path().join("value.txt"), b"before").unwrap();
    let capability = portable_read_write(&root);
    let observed = capability.open_file(&relative("value.txt")).unwrap();
    // Whole-object deletion is explicitly authorized; this API is not installer reclamation.
    fs::write(root.path().join("value.txt"), b"edited").unwrap();
    fs::write(root.path().join("value.txt:user-note"), b"alternate stream").unwrap();
    let mut writer = OpenOptions::new()
        .write(true)
        .share_mode(7)
        .open(root.path().join("value.txt"))
        .unwrap();
    let receipt = capability
        .unlink_file(&relative("value.txt"), &observed)
        .unwrap();
    assert_eq!(receipt.name_removed(), StageStatus::Achieved);
    assert!(!root.path().join("value.txt").exists());
    assert_eq!(observed.read_to_end_bounded(6).unwrap(), b"edited");
    writer.write_all(b"after!").unwrap();
    assert_eq!(observed.read_to_end_bounded(6).unwrap(), b"after!");
}

#[cfg(not(windows))]
#[test]
fn unlink_without_exact_object_adapter_preserves_the_named_file() {
    let root = TestRoot::new();
    fs::write(root.path().join("value.txt"), b"value").unwrap();
    let capability = portable_read_write(&root);
    let observed = capability.open_file(&relative("value.txt")).unwrap();
    assert!(matches!(
        capability.unlink_file(&relative("value.txt"), &observed),
        Err(FsError::Unsupported { .. })
    ));
    assert_eq!(fs::read(root.path().join("value.txt")).unwrap(), b"value");
}

#[cfg(target_os = "linux")]
#[test]
fn strict_unix_replacement_rejects_name_bound_candidate_publication() {
    let root = TestRoot::new();
    fs::write(root.path().join("document.scene"), b"old").unwrap();
    let capability = strict_read_write(&root);
    assert!(matches!(
        capability.preflight_replace(ExpectedTarget::Any),
        Err(FsError::Unproven { .. })
    ));
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
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

    assert!(matches!(error.error(), FsError::Unproven { .. }));
    assert!(error.cleanup().is_some());
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

    let result = capability.open_file(&relative("link.txt"));
    assert!(result.is_err());

    #[cfg(target_os = "linux")]
    assert!(matches!(result, Err(FsError::SymbolicLinkTraversal)));
}
