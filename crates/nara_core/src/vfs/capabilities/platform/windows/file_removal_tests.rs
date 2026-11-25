//! Native qualification probes, not an admitted file-removal capability.
//!
//! Default-stream custody excludes ordinary writers and writable views, but cannot preserve
//! unknown alternate data streams against creation or mutation. Explicit unlink has a separate
//! whole-object authorization contract and does not promise unchanged-content reclamation.

use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::windows::{
        fs::OpenOptionsExt,
        io::{FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use windows_sys::Win32::{
    Foundation::{ERROR_SHARING_VIOLATION, GENERIC_WRITE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{
        CREATE_ALWAYS, CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    },
    System::Memory::{
        CreateFileMappingW, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile,
        PAGE_READWRITE, UnmapViewOfFile,
    },
};

use super::*;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
const EXPECTED: &[u8] = b"value";

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "nara_fs_removal_probe_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("value.txt"), EXPECTED).unwrap();
        Self(path)
    }

    fn parent(&self) -> File {
        OpenOptions::new()
            .read(true)
            .share_mode(SHARE_ALL)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&self.0)
            .unwrap()
    }

    fn leaf(&self) -> PathBuf {
        self.0.join("value.txt")
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn open_default_stream_custody(parent: &File) -> Result<File, FsError> {
    // This is the exact proposed access/share/options combination. There is intentionally
    // no protected-reclamation API until the all-stream preservation gap is resolved.
    nt_open_with_share_mode(
        parent,
        &RelativeComponent::new("value.txt").unwrap(),
        FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE | DELETE,
        FILE_OPEN,
        FILE_NON_DIRECTORY_FILE,
        FsOperation::OpenFile,
        FILE_SHARE_READ,
    )
}

fn open_writer(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(SHARE_ALL)
        .open(path)
}

fn assert_sharing_rejected(result: Result<File, FsError>) {
    match result.unwrap_err() {
        FsError::Io { source, .. } => assert_eq!(
            source.raw_os_error(),
            Some(ERROR_SHARING_VIOLATION as i32),
            "custody acquisition must fail at native sharing, before reading or deleting"
        ),
        other => panic!("expected a native sharing failure, got {other:?}"),
    }
}

fn assert_native_sharing_error(error: io::Error) {
    assert_eq!(error.raw_os_error(), Some(ERROR_SHARING_VIOLATION as i32));
}

fn set_posix_disposition(file: &File) -> io::Result<()> {
    let disposition = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    };
    // SAFETY: this test owns a live file with DELETE access; the disposition buffer is valid.
    let succeeded = unsafe {
        SetFileInformationByHandle(
            raw_handle(file),
            FileDispositionInfoEx,
            ptr::addr_of!(disposition).cast(),
            size_of_val(&disposition) as u32,
        )
    };
    if succeeded != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[test]
fn default_stream_custody_drop_preserves_content_and_exact_handle_disposition_unlinks() {
    let root = TestRoot::new();
    let parent = root.parent();
    let mut independent_reader = OpenOptions::new()
        .read(true)
        .share_mode(SHARE_ALL)
        .open(root.leaf())
        .unwrap();
    let observed = facts(&independent_reader).unwrap();
    let custody = open_default_stream_custody(&parent).unwrap();
    assert_eq!(facts(&custody).unwrap().identity, observed.identity);
    drop(custody);
    assert_eq!(fs::read(root.leaf()).unwrap(), EXPECTED);

    let custody = open_default_stream_custody(&parent).unwrap();
    set_posix_disposition(&custody).unwrap();
    drop(custody);
    assert!(!root.leaf().exists());
    let mut bytes = [0_u8; EXPECTED.len()];
    independent_reader.read_exact(&mut bytes).unwrap();
    assert_eq!(bytes, EXPECTED);

    fs::write(root.leaf(), b"new object").unwrap();
    assert_ne!(
        facts(&File::open(root.leaf()).unwrap()).unwrap().identity,
        observed.identity
    );
    assert_eq!(fs::read(root.leaf()).unwrap(), b"new object");
}

#[test]
fn default_stream_custody_rejects_an_existing_ordinary_writer() {
    let root = TestRoot::new();
    let parent = root.parent();
    let writer = open_writer(&root.leaf()).unwrap();
    assert_sharing_rejected(open_default_stream_custody(&parent));
    assert_eq!(fs::read(root.leaf()).unwrap(), EXPECTED);
    drop(writer);
    drop(open_default_stream_custody(&parent).unwrap());
}

struct WritableView(MEMORY_MAPPED_VIEW_ADDRESS);

impl WritableView {
    fn new(file: &File) -> Self {
        // SAFETY: the fixture is nonempty, the read/write file is live, and no name is provided.
        let mapping = unsafe {
            CreateFileMappingW(
                raw_handle(file),
                ptr::null(),
                PAGE_READWRITE,
                0,
                0,
                ptr::null(),
            )
        };
        assert!(!mapping.is_null(), "{}", io::Error::last_os_error());
        // SAFETY: successful creation transferred unique ownership of the mapping handle.
        let mapping = unsafe { OwnedHandle::from_raw_handle(mapping.cast()) };
        // SAFETY: the mapping covers the fixture. A zero byte count maps its whole nonempty file.
        let view =
            unsafe { MapViewOfFile(mapping.as_raw_handle().cast(), FILE_MAP_WRITE, 0, 0, 0) };
        assert!(!view.Value.is_null(), "{}", io::Error::last_os_error());
        // A view retains its mapping after both the original file and section handles close.
        drop(mapping);
        Self(view)
    }
}

impl Drop for WritableView {
    fn drop(&mut self) {
        // SAFETY: this owner retains the exact live view returned by MapViewOfFile.
        assert_ne!(unsafe { UnmapViewOfFile(self.0) }, 0);
    }
}

#[test]
fn default_stream_custody_rejects_a_writable_view_after_the_writer_handle_closed() {
    let root = TestRoot::new();
    let parent = root.parent();
    let writer = open_writer(&root.leaf()).unwrap();
    let view = WritableView::new(&writer);
    drop(writer);

    // No writer or section HANDLE remains; only the writable view retains write access.
    assert_sharing_rejected(open_default_stream_custody(&parent));
    assert_eq!(fs::read(root.leaf()).unwrap(), EXPECTED);
    drop(view);
    drop(open_default_stream_custody(&parent).unwrap());
}

#[test]
fn default_stream_custody_blocks_later_ordinary_writer_rename_and_delete() {
    let root = TestRoot::new();
    let parent = root.parent();
    let custody = open_default_stream_custody(&parent).unwrap();
    assert_native_sharing_error(open_writer(&root.leaf()).unwrap_err());
    assert_native_sharing_error(fs::rename(root.leaf(), root.0.join("moved.txt")).unwrap_err());
    assert_native_sharing_error(fs::remove_file(root.leaf()).unwrap_err());
    assert_eq!(fs::read(root.leaf()).unwrap(), EXPECTED);
    drop(custody);
    drop(open_writer(&root.leaf()).unwrap());
    fs::rename(root.leaf(), root.0.join("moved.txt")).unwrap();
}

#[test]
fn posix_disposition_honors_readonly_without_ignoring_the_attribute() {
    let root = TestRoot::new();
    let parent = root.parent();
    let custody = open_default_stream_custody(&parent).unwrap();
    let original = fs::metadata(root.leaf()).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(root.leaf(), readonly).unwrap();
    let result = set_posix_disposition(&custody);
    fs::set_permissions(root.leaf(), original).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(root.leaf()).unwrap(), EXPECTED);
    assert_native_sharing_error(open_writer(&root.leaf()).unwrap_err());
    set_posix_disposition(&custody).unwrap();
    drop(custody);
    assert!(!root.leaf().exists());
}

fn alternate_stream_path(root: &TestRoot) -> PathBuf {
    let mut name = root.leaf().into_os_string();
    name.push(":user-note");
    PathBuf::from(name)
}

fn create_alternate_stream(path: &Path) -> File {
    let mut name = path.as_os_str().encode_wide().collect::<Vec<_>>();
    name.push(0);
    // SAFETY: the UTF-16 path is terminated and all optional pointers are null. These are the
    // actual adversarial flags: GENERIC_WRITE, share read/write/delete, CREATE_ALWAYS.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_WRITE,
            SHARE_ALL,
            ptr::null(),
            CREATE_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            ptr::null_mut(),
        )
    };
    assert_ne!(
        handle,
        INVALID_HANDLE_VALUE,
        "{}",
        io::Error::last_os_error()
    );
    // SAFETY: successful CreateFileW transferred unique ownership of this stream handle.
    unsafe { File::from_raw_handle(handle.cast()) }
}

#[test]
fn restrictive_default_stream_custody_does_not_exclude_new_alternate_stream_creation() {
    let root = TestRoot::new();
    let parent = root.parent();
    let custody = open_default_stream_custody(&parent).unwrap();
    let stream_path = alternate_stream_path(&root);
    let mut stream = create_alternate_stream(&stream_path);
    stream.write_all(b"user-owned note").unwrap();
    drop(stream);

    // A default-stream digest would still match although the whole file gained user content.
    assert_eq!(fs::read(root.leaf()).unwrap(), EXPECTED);
    assert_eq!(fs::read(&stream_path).unwrap(), b"user-owned note");
    drop(custody);
    assert_eq!(fs::read(stream_path).unwrap(), b"user-owned note");
    assert_eq!(capability_matrix().relative_unlink(), ProofStatus::Proven);
}

#[test]
fn restrictive_default_stream_custody_does_not_protect_an_existing_alternate_stream() {
    let root = TestRoot::new();
    let parent = root.parent();
    let stream_path = alternate_stream_path(&root);
    let mut stream = create_alternate_stream(&stream_path);
    stream.write_all(b"original note").unwrap();
    drop(stream);

    let custody = open_default_stream_custody(&parent).unwrap();
    let mut stream = create_alternate_stream(&stream_path);
    stream.write_all(b"changed note").unwrap();
    drop(stream);
    assert_eq!(fs::read(root.leaf()).unwrap(), EXPECTED);
    assert_eq!(fs::read(&stream_path).unwrap(), b"changed note");
    drop(custody);
    assert_eq!(fs::read(stream_path).unwrap(), b"changed note");
}
