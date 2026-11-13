//! The explicit Host-owned private directory is the only control endpoint.

use std::{
    ffi::OsString,
    fs,
    io::{self, Read, Write},
    path::PathBuf,
};

use nara::{
    core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape},
    fs::{
        CapabilityRights, DirectoryCapability, ExpectedTarget, HostCapabilityOptions,
        RelativeComponent, TemporaryFileFailure, TrustMode,
    },
};
use serde::Deserialize;

use super::protocol::{
    MAX_IDENTITY_BYTES, MAX_MESSAGE_BYTES, PROTOCOL_VERSION, READY_FILE, REQUEST_FILE, Reply,
    Request, Response,
};

pub(super) struct Arguments {
    pub project_root: PathBuf,
    pub control_dir: PathBuf,
    session: String,
    generation: String,
}

impl Arguments {
    pub fn parse() -> Result<Self, String> {
        let mut project_root = None;
        let mut control_dir = None;
        let mut session = None;
        let mut generation = None;
        let mut arguments = std::env::args_os().skip(1);
        while let Some(flag) = arguments.next() {
            let value = arguments.next().ok_or("every option requires a value")?;
            match flag.to_str() {
                Some("--project-root") if project_root.is_none() => {
                    project_root = Some(PathBuf::from(value));
                }
                Some("--control-dir") if control_dir.is_none() => {
                    control_dir = Some(PathBuf::from(value));
                }
                Some("--session") if session.is_none() => session = Some(identity(value)?),
                Some("--generation") if generation.is_none() => {
                    generation = Some(identity(value)?);
                }
                _ => return Err("unknown or repeated continuity option".into()),
            }
        }
        let project_root = project_root.ok_or("--project-root is required")?;
        let control_dir = control_dir.ok_or("--control-dir is required")?;
        if !project_root.is_absolute() || !control_dir.is_absolute() {
            return Err("project and control directories must be absolute".into());
        }
        if !fs::symlink_metadata(&control_dir)
            .map_err(|error| format!("inspect control directory: {error}"))?
            .file_type()
            .is_dir()
        {
            return Err("the Host must supply an existing non-symlink control directory".into());
        }
        let project_root = fs::canonicalize(project_root)
            .map_err(|error| format!("resolve project directory: {error}"))?;
        let control_dir = fs::canonicalize(control_dir)
            .map_err(|error| format!("resolve control directory: {error}"))?;
        if control_dir.starts_with(&project_root) {
            return Err("the private Host control directory must be outside project files".into());
        }
        Ok(Self {
            project_root,
            control_dir,
            session: session.ok_or("--session is required")?,
            generation: generation.ok_or("--generation is required")?,
        })
    }

    pub fn validate(&self, request: &Request, previous: u64) -> Result<(), String> {
        if request.version != PROTOCOL_VERSION {
            return Err("unsupported continuity protocol version".into());
        }
        if request.session.is_empty()
            || request.session.len() > MAX_IDENTITY_BYTES
            || request.generation.is_empty()
            || request.generation.len() > MAX_IDENTITY_BYTES
            || request.session != self.session
            || request.generation != self.generation
        {
            return Err(
                "request does not identify this original child session and generation".into(),
            );
        }
        if request.request <= previous {
            return Err("request ID must be nonzero and strictly increasing".into());
        }
        Ok(())
    }

    pub fn response(&self, request: u64, result: Result<Reply, String>) -> Response {
        Response {
            version: PROTOCOL_VERSION,
            session: self.session.clone(),
            generation: self.generation.clone(),
            request,
            result,
        }
    }
}

fn identity(value: OsString) -> Result<String, String> {
    let value = value.into_string().map_err(|_| "identity must be UTF-8")?;
    if value.is_empty() || value.len() > MAX_IDENTITY_BYTES {
        return Err("session and generation must each contain 1..=128 UTF-8 bytes".into());
    }
    Ok(value)
}

pub(super) struct ControlDirectory {
    path: PathBuf,
    directory: DirectoryCapability,
    next_temporary: u64,
}

impl ControlDirectory {
    /// The caller, not these packet strings, grants trust in this private temporary directory.
    pub fn open(arguments: &Arguments) -> Result<Self, String> {
        for entry in fs::read_dir(&arguments.control_dir)
            .map_err(|error| format!("enumerate control directory: {error}"))?
        {
            let entry = entry.map_err(|error| format!("inspect control entry: {error}"))?;
            let name = entry.file_name();
            let name = name.to_string_lossy().to_ascii_lowercase();
            if name == READY_FILE
                || name == REQUEST_FILE
                || name.starts_with("response-")
                || name.starts_with(".continuity-")
            {
                return Err(
                    "control directory already contains a reserved protocol filename".into(),
                );
            }
        }
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0x1 | 0x2 | 0x4)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&arguments.control_dir)
        };
        #[cfg(not(windows))]
        let file = fs::File::open(&arguments.control_dir);
        let directory = DirectoryCapability::from_host_handle(
            file.map_err(|error| format!("open control directory: {error}"))?,
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .map_err(|error| format!("admit control directory: {error}"))?;
        directory
            .preflight_replace(ExpectedTarget::Missing)
            .map_err(|error| format!("control publication is unavailable: {error}"))?;
        Ok(Self {
            path: arguments.control_dir.clone(),
            directory,
            next_temporary: 0,
        })
    }

    pub fn receive(&self) -> Result<Option<Vec<u8>>, String> {
        let path = self.path.join(REQUEST_FILE);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("inspect request: {error}")),
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err("request must be a regular non-symlink file".into());
            }
            Ok(_) => {}
        }
        let bytes = {
            let file = self
                .directory
                .open_child_file(&component(REQUEST_FILE)?)
                .map_err(|error| format!("open request: {error}"))?;
            let mut bytes = Vec::new();
            file.reader()
                .map_err(|error| format!("read request: {error}"))?
                .take((MAX_MESSAGE_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|error| format!("read request: {error}"))?;
            bytes
        };
        fs::remove_file(path).map_err(|error| format!("consume request: {error}"))?;
        Ok(Some(bytes))
    }

    pub fn response_available(&self, request: u64) -> Result<bool, String> {
        match fs::symlink_metadata(self.path.join(format!("response-{request}.json"))) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
            Err(error) => Err(format!("inspect reply destination: {error}")),
            Ok(_) => Ok(false),
        }
    }

    pub fn publish(&mut self, name: &str, response: &Response) -> Result<(), String> {
        let bytes = serde_json::to_vec(response)
            .map_err(|error| format!("encode continuity reply: {error}"))?;
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err("continuity reply exceeds MAX_MESSAGE_BYTES".into());
        }
        self.next_temporary += 1;
        let temporary_name = component(&format!(
            ".continuity-{}-{}.tmp",
            std::process::id(),
            self.next_temporary,
        ))?;
        let mut temporary = self
            .directory
            .create_temp(&temporary_name)
            .map_err(publication_failure)?;
        temporary
            .write_all(&bytes)
            .and_then(|()| temporary.flush())
            .map_err(|error| format!("write continuity reply: {error}"))?;
        self.directory
            .replace_temp(temporary, &component(name)?, ExpectedTarget::Missing)
            .map_err(publication_failure)?;
        Ok(())
    }
}

fn component(name: &str) -> Result<RelativeComponent, String> {
    RelativeComponent::new(name).map_err(|error| format!("invalid protocol filename: {error}"))
}

fn publication_failure(mut failure: TemporaryFileFailure) -> String {
    let cleaned = failure.retry_cleanup();
    format!("publish continuity reply: {failure}; temporary cleanup complete={cleaned}")
}

/// Unreadable packet identities use request zero; they can never authorize a Runtime operation.
pub(super) fn decode_request(bytes: &[u8]) -> (u64, Result<Request, String>) {
    if let Err(error) = preflight(bytes) {
        return (0, Err(error));
    }
    #[derive(Deserialize)]
    struct RequestNumber {
        request: u64,
    }
    let number = match serde_json::from_slice::<RequestNumber>(bytes) {
        Ok(number) => number.request,
        Err(error) => return (0, Err(format!("invalid request number: {error}"))),
    };
    (
        number,
        serde_json::from_slice(bytes).map_err(|error| format!("invalid request: {error}")),
    )
}

fn preflight(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("request exceeds MAX_MESSAGE_BYTES".into());
    }
    let limits = SerdeShapeLimits::new(
        DepthLimit::new(20).expect("static nonzero packet depth"),
        ItemLimit::new(8192).expect("static nonzero packet node limit"),
        ItemLimit::new(256).expect("static nonzero packet container limit"),
        ByteLimit::new(MAX_IDENTITY_BYTES).expect("static nonzero packet string limit"),
        ByteLimit::new(64 * 1024).expect("static nonzero packet total string limit"),
    );
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut deserializer, limits)
        .map_err(|error| format!("request shape was refused: {error}"))?;
    deserializer
        .end()
        .map_err(|error| format!("invalid request JSON: {error}"))
}
