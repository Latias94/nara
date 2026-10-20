use std::{ffi::c_int, io, num::NonZeroI32};

unsafe extern "C" {
    fn getpgid(pid: c_int) -> c_int;
}

/// Observes a Linux process group without treating a successful PGID zero as a positive PID.
///
/// Kernel tasks or namespace-hidden groups can have PGID zero. It is an observation, never a
/// group this owner can signal. Only the retained child leader grants positive group authority.
pub(crate) fn query(pid: c_int) -> io::Result<Option<NonZeroI32>> {
    if pid <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "process-group inspection requires a positive process ID",
        ));
    }
    // SAFETY: on the supported Linux x86-64 target pid_t is c_int; getpgid accepts this positive
    // numeric observation and writes no memory. Inspect its signed result before constructing IDs.
    decode_group(unsafe { getpgid(pid) })
}

fn decode_group(raw: c_int) -> io::Result<Option<NonZeroI32>> {
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(NonZeroI32::new(raw))
}

#[cfg(test)]
mod tests {
    use std::{os::unix::process::CommandExt as _, process::Command};

    use super::*;

    #[test]
    fn zero_group_is_an_observation_without_signal_authority() {
        assert_eq!(decode_group(0).unwrap(), None);
        assert_eq!(decode_group(123).unwrap().unwrap().get(), 123);
        assert_eq!(query(0).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        assert_eq!(query(-1).unwrap_err().kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn isolated_live_child_has_its_own_positive_group() {
        struct ChildOwner(std::process::Child);

        impl Drop for ChildOwner {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }

        let mut command = Command::new("/bin/sleep");
        command.arg("30").process_group(0);
        let child = ChildOwner(command.spawn().expect("the isolated child starts"));
        let pid = c_int::try_from(child.0.id()).expect("the child has a positive Linux PID");
        assert_eq!(query(pid).unwrap().unwrap().get(), pid);
    }

    #[test]
    fn absent_process_preserves_the_os_error() {
        // Linux PID_MAX_LIMIT is below i32::MAX, so this cannot select a live unrelated process.
        // The kernel must preserve ESRCH (3), rather than treating it as a successful zero group.
        assert_eq!(query(c_int::MAX).unwrap_err().raw_os_error(), Some(3));
    }
}
