use std::{
    fs::File,
    io::{self, Read},
    num::NonZeroUsize,
    os::windows::io::{AsRawHandle as _, FromRawHandle as _},
};

use windows_sys::Win32::{
    Foundation::{
        ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_BUSY,
        ERROR_PIPE_NOT_CONNECTED, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation,
    },
    Storage::FileSystem::{FILE_TYPE_PIPE, GetFileType, WriteFile},
    System::{
        Console::{GetStdHandle, STD_INPUT_HANDLE},
        Pipes::{
            GetNamedPipeInfo, PIPE_NOWAIT, PIPE_READMODE_BYTE, PeekNamedPipe,
            SetNamedPipeHandleState,
        },
    },
};

use super::InheritedBootstrapRead;

pub(in crate::project_host::product_launch) struct InheritedBootstrapEndpoint {
    file: File,
}

impl InheritedBootstrapEndpoint {
    pub(in crate::project_host::product_launch) fn try_read(
        &mut self,
        bytes: &mut [u8],
    ) -> io::Result<InheritedBootstrapRead> {
        if bytes.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the inherited bootstrap read buffer is empty",
            ));
        }
        let mut available = 0;
        // SAFETY: the file owns a live named-pipe handle and `available` is writable.
        if unsafe {
            PeekNamedPipe(
                self.file.as_raw_handle().cast(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        } == 0
        {
            let error = io::Error::last_os_error();
            return if is_bootstrap_read_eof(&error) {
                Ok(InheritedBootstrapRead::Eof)
            } else {
                Err(error)
            };
        }
        if available == 0 {
            return Ok(InheritedBootstrapRead::Pending);
        }
        let amount = bytes.len().min(available as usize);
        match self.file.read(&mut bytes[..amount]) {
            Ok(0) => Ok(InheritedBootstrapRead::Eof),
            Ok(read) => Ok(InheritedBootstrapRead::Read(
                NonZeroUsize::new(read).expect("a positive named-pipe read is non-zero"),
            )),
            Err(error) if is_bootstrap_read_eof(&error) => Ok(InheritedBootstrapRead::Eof),
            Err(error) => Err(error),
        }
    }

    pub(in crate::project_host::product_launch) fn prepare_ready_write(
        &mut self,
    ) -> io::Result<()> {
        let mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
        // SAFETY: the file owns the live duplex pipe handle and only its client mode changes.
        if unsafe {
            SetNamedPipeHandleState(
                self.file.as_raw_handle().cast(),
                &mode,
                std::ptr::null(),
                std::ptr::null(),
            )
        } == 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub(in crate::project_host::product_launch) fn try_write(
        &mut self,
        bytes: &[u8],
    ) -> io::Result<usize> {
        let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        let mut written = 0;
        // SAFETY: the file owns a live duplex pipe handle and the input buffer remains readable for
        // the synchronous nonblocking call.
        if unsafe {
            WriteFile(
                self.file.as_raw_handle().cast(),
                bytes.as_ptr(),
                length,
                &mut written,
                std::ptr::null_mut(),
            )
        } != 0
        {
            return Ok(written as usize);
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(code) if code == ERROR_NO_DATA as i32 || code == ERROR_PIPE_BUSY as i32 => Ok(0),
            _ => Err(error),
        }
    }
}

fn is_bootstrap_read_eof(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(code)
            if code == ERROR_BROKEN_PIPE as i32
                || code == ERROR_NO_DATA as i32
                || code == ERROR_PIPE_NOT_CONNECTED as i32
    )
}

pub(in crate::project_host::product_launch) fn inherited_bootstrap_endpoint()
-> io::Result<Option<InheritedBootstrapEndpoint>> {
    // SAFETY: querying the process standard-input slot has no preconditions.
    let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Ok(None);
    }
    // SAFETY: `handle` is a live process standard handle and remains borrowed here.
    if unsafe { GetFileType(handle) } != FILE_TYPE_PIPE {
        return Ok(None);
    }

    let mut flags = 0;
    let mut output_buffer = 0;
    let mut input_buffer = 0;
    let mut maximum_instances = 0;
    // SAFETY: every output pointer is writable and `handle` remains live for this query.
    if unsafe {
        GetNamedPipeInfo(
            handle,
            &mut flags,
            &mut output_buffer,
            &mut input_buffer,
            &mut maximum_instances,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let _ = flags;
    if output_buffer == 0 || input_buffer == 0 || maximum_instances != 1 {
        return Ok(None);
    }

    let mut available = 0;
    // SAFETY: this zero-byte peek proves read access without consuming launch-context bytes.
    if unsafe {
        PeekNamedPipe(
            handle,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    } == 0
    {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) {
            return Ok(None);
        }
        return Err(error);
    }
    let _ = available;

    let mut written = 0;
    // SAFETY: a zero-byte write proves write access without adding a protocol record.
    if unsafe {
        WriteFile(
            handle,
            std::ptr::null(),
            0,
            &mut written,
            std::ptr::null_mut(),
        )
    } == 0
    {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) {
            return Ok(None);
        }
        return Err(error);
    }
    debug_assert_eq!(written, 0);

    // SAFETY: the handle is live and only its inheritance bit is cleared before project code runs.
    if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }

    // SAFETY: the Nara child process exclusively owns its inherited duplex bootstrap handle in
    // the standard-input slot, and this owner closes it after the one READY write.
    let file = unsafe { File::from_raw_handle(handle.cast()) };
    Ok(Some(InheritedBootstrapEndpoint { file }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherited_bootstrap_read_recognizes_closed_pipe_errors_as_eof() {
        for code in [ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_NOT_CONNECTED] {
            assert!(is_bootstrap_read_eof(&io::Error::from_raw_os_error(
                code as i32
            )));
        }
        assert!(!is_bootstrap_read_eof(&io::Error::from_raw_os_error(
            ERROR_ACCESS_DENIED as i32
        )));
    }
}
