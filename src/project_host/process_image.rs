//! Bounded Windows image observations backed by restrictive capability leases.

use std::{
    ffi::{OsString, c_void},
    io,
    mem::size_of,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsHandle, AsRawHandle, BorrowedHandle},
    },
    path::{Path, PathBuf},
    ptr::null_mut,
};

use nara_fs::RestrictiveCapabilityHandleLease;
use windows_sys::Win32::{
    Foundation::{HANDLE, RtlNtStatusToDosError},
    Globalization::{CSTR_EQUAL, CompareStringOrdinal},
    Storage::FileSystem::{FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW, VOLUME_NAME_DOS},
};

const MAX_FINAL_PATH_UNITS: u32 = 32_767;
const PROCESS_IMAGE_FILE_MAPPING_INFORMATION_CLASS: i32 = 44;

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationProcess(
        process_handle: HANDLE,
        process_information_class: i32,
        process_information: *mut c_void,
        process_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

/// One executable and its immediate directory held by restrictive capability leases.
///
/// This primitive observes the guarded executable's native path and checks a process image
/// against its underlying file stream. It does not own or control the process, prove a runtime
/// dependency closure, or exclude writes through pre-existing writable mappings.
#[doc(hidden)]
pub struct ExactProcessImage {
    executable: RestrictiveCapabilityHandleLease,
    _directory: RestrictiveCapabilityHandleLease,
    path: PathBuf,
}

impl ExactProcessImage {
    /// Retains capability-issued leases and checks their final native parent relationship.
    ///
    /// The caller supplies an executable lease with the execute access required by image
    /// attestation. Each final-path query is bounded to 32,767 UTF-16 code units plus a terminator.
    pub fn from_verified_handles(
        executable: RestrictiveCapabilityHandleLease,
        directory: RestrictiveCapabilityHandleLease,
    ) -> io::Result<Self> {
        let directory_path = final_path(directory.as_handle(), "directory")?;
        let path = final_path(executable.as_handle(), "executable")?;
        let parent = path.parent().ok_or_else(|| {
            io::Error::other("the exact process executable has no parent directory")
        })?;
        if !paths_equal(parent, &directory_path) {
            return Err(io::Error::other(
                "the exact process executable is no longer bound to its verified directory",
            ));
        }
        Ok(Self {
            executable,
            _directory: directory,
            path,
        })
    }

    /// Observes the guarded executable's native path for a native spawn operation.
    ///
    /// This path does not grant standalone authority. Keep this owner alive and verify the
    /// created process image before the launch owner permits execution.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Checks the process image against the retained executable's underlying file stream.
    ///
    /// `ProcessImageFileMapping` compares the image section and candidate handle through the
    /// stream's `SectionObjectPointer`; it does not prove `FileObject` pointer identity. The
    /// caller retains responsibility for process access, suspension, and lifecycle custody.
    pub fn verify_mapping(&self, process: BorrowedHandle<'_>) -> io::Result<()> {
        let mut executable_handle: HANDLE = self.executable.as_raw_handle().cast();
        let mut returned = 0_u32;
        // SAFETY: the borrowed process and retained executable handles remain live. The buffer
        // contains the candidate HANDLE required by ProcessImageFileMapping and has its exact size.
        let status = unsafe {
            NtQueryInformationProcess(
                process.as_raw_handle().cast(),
                PROCESS_IMAGE_FILE_MAPPING_INFORMATION_CLASS,
                (&mut executable_handle as *mut HANDLE).cast(),
                size_of::<HANDLE>() as u32,
                &mut returned,
            )
        };
        if status == 0 {
            return Ok(());
        }
        // SAFETY: the returned NTSTATUS is accepted by the native conversion routine.
        let code = unsafe { RtlNtStatusToDosError(status) };
        if code == 0 {
            return Err(io::Error::other(format!(
                "NtQueryInformationProcess(ProcessImageFileMapping) returned non-success NTSTATUS {status:#x}"
            )));
        }
        Err(with_operation(
            "NtQueryInformationProcess(ProcessImageFileMapping)",
            io::Error::from_raw_os_error(code as i32),
        ))
    }
}

fn final_path(handle: BorrowedHandle<'_>, label: &'static str) -> io::Result<PathBuf> {
    let flags = FILE_NAME_NORMALIZED | VOLUME_NAME_DOS;
    // SAFETY: the borrowed handle is live; a null buffer and zero length query the required size.
    let required =
        unsafe { GetFinalPathNameByHandleW(handle.as_raw_handle().cast(), null_mut(), 0, flags) };
    if required == 0 {
        return Err(with_operation(
            "GetFinalPathNameByHandleW(size query)",
            io::Error::last_os_error(),
        ));
    }
    // The sizing result includes the terminator. Reject oversized results before allocating,
    // and perform only one retrieval rather than retrying an unstable or growing native path.
    if required > MAX_FINAL_PATH_UNITS + 1 {
        return Err(io::Error::other(format!(
            "the Windows {label} path exceeds 32,767 UTF-16 code units"
        )));
    }
    let mut path = vec![0_u16; required as usize];
    // SAFETY: the buffer is writable for exactly `required` units and the handle remains live.
    let written = unsafe {
        GetFinalPathNameByHandleW(
            handle.as_raw_handle().cast(),
            path.as_mut_ptr(),
            required,
            flags,
        )
    };
    if written == 0 {
        return Err(with_operation(
            "GetFinalPathNameByHandleW",
            io::Error::last_os_error(),
        ));
    }
    if written >= required {
        return Err(io::Error::other(format!(
            "the Windows {label} path changed during its bounded handle query"
        )));
    }
    path.truncate(written as usize);
    Ok(PathBuf::from(OsString::from_wide(&path)))
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    let left = left.as_os_str().encode_wide().collect::<Vec<_>>();
    let right = right.as_os_str().encode_wide().collect::<Vec<_>>();
    // SAFETY: both paths come from bounded final-path queries, so their exact UTF-16 lengths
    // fit in i32 and both buffers remain readable throughout the ordinal ignore-case comparison.
    match unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            left.len() as i32,
            right.as_ptr(),
            right.len() as i32,
            1,
        )
    } {
        0 => left == right,
        comparison => comparison == CSTR_EQUAL,
    }
}

fn with_operation(operation: &'static str, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{operation} failed: {error}"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, OpenOptions},
        os::windows::fs::OpenOptionsExt,
        time::{SystemTime, UNIX_EPOCH},
    };

    use nara_fs::{
        CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativeComponent, TrustMode,
    };
    use windows_sys::Win32::{
        Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE,
        },
        System::Threading::GetCurrentProcess,
    };

    use super::*;

    #[test]
    fn current_process_image_matches_its_guarded_executable() {
        let executable = std::env::current_exe().unwrap();
        let image = guarded_image(&executable);
        image.verify_mapping(current_process()).unwrap();
    }

    #[test]
    fn same_byte_copy_does_not_match_the_current_process_image() {
        let root = TestDirectory::new();
        let executable = std::env::current_exe().unwrap();
        let copied = root.0.join("same-image.exe");
        // The destination is new in this test's exclusive directory: copy identical bytes into
        // a distinct file stream rather than creating another name or handle for the same file.
        fs::copy(&executable, &copied).unwrap();
        let image = guarded_image(&copied);
        assert!(image.verify_mapping(current_process()).is_err());
    }

    fn guarded_image(executable: &Path) -> ExactProcessImage {
        let directory = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(executable.parent().unwrap())
            .unwrap();
        let directory = DirectoryCapability::from_host_handle(
            directory,
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted),
        )
        .unwrap();
        let executable = directory
            .open_child_file(&RelativeComponent::new(executable.file_name().unwrap()).unwrap())
            .unwrap();
        ExactProcessImage::from_verified_handles(
            executable
                .duplicate_restrictive_executable_host_handle()
                .unwrap(),
            directory.duplicate_restrictive_host_handle().unwrap(),
        )
        .unwrap()
    }

    fn current_process() -> BorrowedHandle<'static> {
        // SAFETY: the current-process pseudo handle is valid for the process lifetime. It is
        // borrowed only and must never be closed by this test.
        unsafe { BorrowedHandle::borrow_raw(GetCurrentProcess().cast()) }
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("nara-process-image-{}-{stamp}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
