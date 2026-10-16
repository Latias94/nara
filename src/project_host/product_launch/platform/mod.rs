//! Platform proof and I/O for the child-local inherited bootstrap endpoint.

use std::num::NonZeroUsize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InheritedBootstrapRead {
    Pending,
    Read(NonZeroUsize),
    Eof,
}

#[cfg(all(unix, target_os = "linux", target_arch = "x86_64"))]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(all(unix, target_os = "linux", target_arch = "x86_64"))]
pub(super) use unix::{InheritedBootstrapEndpoint, inherited_bootstrap_endpoint};
#[cfg(windows)]
pub(super) use windows::{InheritedBootstrapEndpoint, inherited_bootstrap_endpoint};

#[cfg(not(any(windows, all(unix, target_os = "linux", target_arch = "x86_64"))))]
mod unsupported {
    use std::io;

    use super::InheritedBootstrapRead;

    pub(in crate::project_host::product_launch) struct InheritedBootstrapEndpoint;

    impl InheritedBootstrapEndpoint {
        pub(in crate::project_host::product_launch) fn try_read(
            &mut self,
            bytes: &mut [u8],
        ) -> io::Result<InheritedBootstrapRead> {
            let _ = bytes;
            Err(unsupported())
        }

        pub(in crate::project_host::product_launch) fn prepare_ready_write(
            &mut self,
        ) -> io::Result<()> {
            Err(unsupported())
        }

        pub(in crate::project_host::product_launch) fn try_write(
            &mut self,
            bytes: &[u8],
        ) -> io::Result<usize> {
            let _ = bytes;
            Err(unsupported())
        }
    }

    pub(in crate::project_host::product_launch) fn inherited_bootstrap_endpoint()
    -> io::Result<Option<InheritedBootstrapEndpoint>> {
        Ok(None)
    }

    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "the supervised desktop bootstrap endpoint is unsupported",
        )
    }
}

#[cfg(not(any(windows, all(unix, target_os = "linux", target_arch = "x86_64"))))]
pub(super) use unsupported::{InheritedBootstrapEndpoint, inherited_bootstrap_endpoint};
