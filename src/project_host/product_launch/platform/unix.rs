use std::{
    fs::File,
    io::{self, Read, Write},
    num::NonZeroUsize,
    os::{
        fd::{AsFd as _, FromRawFd as _, OwnedFd},
        unix::{fs::FileTypeExt as _, net::UnixStream},
    },
};

use super::InheritedBootstrapRead;

pub(in crate::project_host::product_launch) struct InheritedBootstrapEndpoint {
    stream: UnixStream,
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
        match self.stream.read(bytes) {
            Ok(0) => Ok(InheritedBootstrapRead::Eof),
            Ok(read) => Ok(InheritedBootstrapRead::Read(
                NonZeroUsize::new(read).expect("a positive Unix stream read is non-zero"),
            )),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                Ok(InheritedBootstrapRead::Pending)
            }
            Err(error) => Err(error),
        }
    }

    pub(in crate::project_host::product_launch) fn prepare_ready_write(
        &mut self,
    ) -> io::Result<()> {
        Ok(())
    }

    pub(in crate::project_host::product_launch) fn try_write(
        &mut self,
        bytes: &[u8],
    ) -> io::Result<usize> {
        match self.stream.write(bytes) {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(0),
            result => result,
        }
    }
}

pub(in crate::project_host::product_launch) fn inherited_bootstrap_endpoint()
-> io::Result<Option<InheritedBootstrapEndpoint>> {
    let stdin = io::stdin();
    let duplicate = match rustix::io::fcntl_dupfd_cloexec(stdin.as_fd(), 3) {
        Ok(duplicate) => duplicate,
        Err(error) if error == rustix::io::Errno::BADF => return Ok(None),
        Err(error) => return Err(io::Error::from_raw_os_error(error.raw_os_error())),
    };
    let duplicate_file = File::from(duplicate);
    if !duplicate_file.metadata()?.file_type().is_socket() {
        return Ok(None);
    }
    if rustix::net::sockopt::socket_type(&duplicate_file)? != rustix::net::SocketType::STREAM {
        return Ok(None);
    }

    let duplicate = OwnedFd::from(duplicate_file);
    let stream = UnixStream::from(duplicate);
    match stream.peer_addr() {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotConnected => return Ok(None),
        Err(error) => return Err(error),
    }

    let descriptor_flags = rustix::io::fcntl_getfd(&stdin)? | rustix::io::FdFlags::CLOEXEC;
    rustix::io::fcntl_setfd(&stdin, descriptor_flags)?;
    drop(stream);

    // SAFETY: descriptor zero remains the original live connected stream proven above. This owner
    // closes it after the one READY write; FD_CLOEXEC was set before Runtime or project code can
    // spawn descendants.
    let stream = unsafe { UnixStream::from_raw_fd(0) };
    stream.set_nonblocking(true)?;
    Ok(Some(InheritedBootstrapEndpoint { stream }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherited_bootstrap_read_distinguishes_pending_data_and_eof() {
        let (stream, mut peer) = UnixStream::pair().unwrap();
        stream.set_nonblocking(true).unwrap();
        let mut endpoint = InheritedBootstrapEndpoint { stream };
        let mut buffer = [0_u8; 8];

        assert_eq!(
            endpoint.try_read(&mut buffer).unwrap(),
            InheritedBootstrapRead::Pending
        );

        peer.write_all(b"ready").unwrap();
        assert_eq!(
            endpoint.try_read(&mut buffer).unwrap(),
            InheritedBootstrapRead::Read(NonZeroUsize::new(5).unwrap())
        );
        assert_eq!(&buffer[..5], b"ready");

        drop(peer);
        assert_eq!(
            endpoint.try_read(&mut buffer).unwrap(),
            InheritedBootstrapRead::Eof
        );
    }
}
