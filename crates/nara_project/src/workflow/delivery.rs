use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
    process::ExitStatus,
    thread,
    time::Duration,
};

use thiserror::Error;

use super::ProjectTrust;
use crate::delivery::{
    CookerReceipt, DELIVERY_ARGUMENT, DELIVERY_PROTOCOL_VERSION, DELIVERY_RECEIPT_PREFIX,
    DeliveryOperation,
};
use crate::play::{
    BuildError, BuildEvent, BuildRequest, OutputLine, OutputStream, PlayProcess,
    build_interruptible,
};

pub const COOKER_BINARY: &str = "cook";
const PACK_FILE: &str = "content.pack";
const PACKAGE_FILE: &str = "package.json";

/// A native cooker build and the explicitly selected delivery inputs. The target selects the
/// delivered content and executable. Cargo builds the cooker itself for the host.
#[derive(Debug, Clone, Copy)]
pub struct DeliveryRequest<'paths> {
    pub build: BuildRequest<'paths>,
    pub operation: DeliveryOperation,
    pub recipe: &'paths str,
    pub target: &'paths str,
    pub output_parent: &'paths Path,
    pub name: &'paths str,
    pub cooker_args: &'paths [OsString],
}

/// The published paths and any fault reported after publication. This receipt does not attest
/// native execution of the exported executable.
#[derive(Debug)]
pub struct DeliveryReceipt {
    pub directory: PathBuf,
    pub content_pack: PathBuf,
    pub executable: Option<PathBuf>,
    pub package_manifest: Option<PathBuf>,
    pub postpublication_error: Option<String>,
}

#[derive(Debug, Error)]
pub enum DeliveryError {
    #[error("building and running a cooker requires a trusted project")]
    UntrustedProject,
    #[error("the cooker manifest must be in the selected project directory")]
    ManifestOutsideProject,
    #[error("delivery requires an explicit recipe and target")]
    MissingSelection,
    #[error("a delivery name must be one ordinary filename")]
    InvalidName,
    #[error("the delivery destination already exists at {}", .0.display())]
    DestinationExists(PathBuf),
    #[error("could not access delivery path {}: {source}", .path.display())]
    Path {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Build(#[from] BuildError),
    #[error("could not supervise the cooker: {0}")]
    ProducerIo(#[source] io::Error),
    #[error("the cooker failed ({0})")]
    ProducerFailed(ExitStatus),
    #[error("delivery was interrupted")]
    Interrupted,
    #[error("the cooker did not report a publication receipt")]
    MissingReceipt,
    #[error("the cooker returned more than one publication receipt")]
    DuplicateReceipt,
    #[error("invalid cooker receipt: {0}")]
    InvalidReceipt(#[source] serde_json::Error),
    #[error("cannot confirm publication at {}: {cause}", .directory.display())]
    UnconfirmedPublication {
        directory: PathBuf,
        #[source]
        cause: Box<DeliveryError>,
    },
    #[error("the cooker receipt does not match the requested operation")]
    WrongReceipt,
}

/// Builds and supervises the selected trusted project-owned cooker. Protocol 1 calls the binary
/// with `--nara-delivery 1 <cook|export> --project-root <dir> --recipe <name> --target <triple>
/// --output-parent <dir> --name <name>`, followed by `--` and any project-specific arguments.
/// The cooker publishes the requested child directory without replacing an existing entry and
/// reports the publication once on stdout as [`DELIVERY_RECEIPT_PREFIX`] plus [`CookerReceipt`]
/// JSON. Other output and Cargo diagnostics are sent to `emit`.
///
/// `interrupted` is checked during the Cargo build and while supervising the cooker. A confirmed
/// publication is returned with its fault if the cooker fails or is interrupted after publishing.
/// If the destination exists without a receipt, the error carries its path for inspection.
pub fn deliver_project(
    request: &DeliveryRequest<'_>,
    trust: ProjectTrust,
    mut interrupted: impl FnMut() -> bool,
    mut emit: impl FnMut(BuildEvent),
) -> Result<DeliveryReceipt, DeliveryError> {
    if trust != ProjectTrust::Trusted {
        return Err(DeliveryError::UntrustedProject);
    }
    if [request.recipe, request.target]
        .into_iter()
        .any(|value| value.trim().is_empty() || value.starts_with('-'))
    {
        return Err(DeliveryError::MissingSelection);
    }
    if !ordinary_name(request.name) {
        return Err(DeliveryError::InvalidName);
    }
    let project_dir = canonicalize(request.build.project_dir)?;
    let manifest_path = canonicalize(&project_dir.join(request.build.manifest_path))?;
    if !manifest_path.starts_with(&project_dir) {
        return Err(DeliveryError::ManifestOutsideProject);
    }
    let output_parent = canonicalize(request.output_parent)?;
    require_kind(&output_parent, "an output directory", fs::Metadata::is_dir)?;
    let directory = output_parent.join(request.name);
    match fs::symlink_metadata(&directory) {
        Ok(_) => return Err(DeliveryError::DestinationExists(directory)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(path_error(&directory, error)),
    }

    let build_request = BuildRequest {
        project_dir: &project_dir,
        manifest_path: &manifest_path,
        target: Some("host-tuple"),
        ..request.build
    };
    let executable =
        build_interruptible(&build_request, &mut interrupted, &mut emit).map_err(|error| {
            match error {
                BuildError::Interrupted => DeliveryError::Interrupted,
                error => DeliveryError::Build(error),
            }
        })?;
    if interrupted() {
        return Err(DeliveryError::Interrupted);
    }
    let mut arguments = vec![
        DELIVERY_ARGUMENT.into(),
        DELIVERY_PROTOCOL_VERSION.into(),
        request.operation.as_str().into(),
        "--project-root".into(),
        project_dir.as_os_str().into(),
        "--recipe".into(),
        request.recipe.into(),
        "--target".into(),
        request.target.into(),
        "--output-parent".into(),
        output_parent.as_os_str().into(),
        "--name".into(),
        request.name.into(),
    ];
    if !request.cooker_args.is_empty() {
        arguments.push("--".into());
        arguments.extend_from_slice(request.cooker_args);
    }
    let mut producer = PlayProcess::launch(&project_dir, &executable, &arguments)
        .map_err(DeliveryError::ProducerIo)?;
    let mut receipt = None;
    let mut observe = |line: OutputLine| {
        if line.stream == OutputStream::Stdout
            && let Some(json) = line.text.strip_prefix(DELIVERY_RECEIPT_PREFIX)
        {
            receipt = Some(if receipt.is_some() {
                Err(DeliveryError::DuplicateReceipt)
            } else {
                serde_json::from_str(json).map_err(DeliveryError::InvalidReceipt)
            });
        } else {
            emit(BuildEvent::Output(line));
        }
    };
    let completion = loop {
        if interrupted() {
            break Err(match producer.stop(&mut observe) {
                Ok(_) => DeliveryError::Interrupted,
                Err(error) => DeliveryError::ProducerIo(error),
            });
        }
        match producer.poll(&mut observe) {
            Ok(Some(status)) if status.success() => break Ok(()),
            Ok(Some(status)) => break Err(DeliveryError::ProducerFailed(status)),
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => break Err(DeliveryError::ProducerIo(error)),
        }
    };
    finish_delivery(request.operation, directory, completion, receipt)
}

fn finish_delivery(
    operation: DeliveryOperation,
    directory: PathBuf,
    completion: Result<(), DeliveryError>,
    receipt: Option<Result<CookerReceipt, DeliveryError>>,
) -> Result<DeliveryReceipt, DeliveryError> {
    let receipt = match receipt.unwrap_or(Err(DeliveryError::MissingReceipt)) {
        Ok(receipt) => receipt,
        Err(error) => {
            let cause = if matches!(error, DeliveryError::MissingReceipt) {
                completion.err().unwrap_or(error)
            } else {
                error
            };
            return Err(unconfirmed_publication(directory, cause));
        }
    };
    let executable = match (operation, receipt.executable) {
        (DeliveryOperation::Cook, None) => None,
        (DeliveryOperation::Export, Some(name))
            if ordinary_name(&name)
                && ![PACK_FILE, PACKAGE_FILE]
                    .into_iter()
                    .any(|reserved| name.eq_ignore_ascii_case(reserved)) =>
        {
            Some(directory.join(name))
        }
        _ => {
            return Err(unconfirmed_publication(
                directory,
                DeliveryError::WrongReceipt,
            ));
        }
    };
    require_kind(&directory, "a published directory", fs::Metadata::is_dir)?;
    let content_pack = directory.join(PACK_FILE);
    require_kind(&content_pack, "a cooked pack file", fs::Metadata::is_file)?;
    let package_manifest = if let Some(executable) = &executable {
        let manifest = directory.join(PACKAGE_FILE);
        require_kind(executable, "an executable file", fs::Metadata::is_file)?;
        require_kind(&manifest, "a package manifest file", fs::Metadata::is_file)?;
        Some(manifest)
    } else {
        None
    };
    Ok(DeliveryReceipt {
        directory,
        content_pack,
        executable,
        package_manifest,
        postpublication_error: receipt
            .postpublication_error
            .or_else(|| completion.err().map(|error| error.to_string())),
    })
}

fn unconfirmed_publication(directory: PathBuf, cause: DeliveryError) -> DeliveryError {
    if fs::symlink_metadata(&directory).is_ok() {
        DeliveryError::UnconfirmedPublication {
            directory,
            cause: Box::new(cause),
        }
    } else {
        cause
    }
}

fn ordinary_name(name: &str) -> bool {
    Path::new(name).file_name() == Some(OsStr::new(name))
        && !name.contains(['\\', ':', '\0', '\r', '\n'])
        && !name.ends_with(['.', ' '])
}

fn canonicalize(path: &Path) -> Result<PathBuf, DeliveryError> {
    path.canonicalize()
        .map_err(|source| path_error(path, source))
}

fn require_kind(
    path: &Path,
    expected: &str,
    matches: fn(&fs::Metadata) -> bool,
) -> Result<(), DeliveryError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| path_error(path, source))?;
    if !matches(&metadata) {
        return Err(path_error(
            path,
            io::Error::new(io::ErrorKind::InvalidData, format!("expected {expected}")),
        ));
    }
    Ok(())
}

fn path_error(path: &Path, source: io::Error) -> DeliveryError {
    DeliveryError::Path {
        path: path.to_owned(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmed_publication_preserves_the_cookers_fault() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join(PACK_FILE), b"pack").unwrap();
        let receipt = finish_delivery(
            DeliveryOperation::Cook,
            directory.path().to_owned(),
            Err(DeliveryError::Interrupted),
            Some(Ok(CookerReceipt {
                executable: None,
                postpublication_error: Some("producer fence changed".into()),
            })),
        )
        .unwrap();
        assert_eq!(receipt.content_pack, directory.path().join(PACK_FILE));
        assert_eq!(
            receipt.postpublication_error.as_deref(),
            Some("producer fence changed")
        );
    }

    #[test]
    fn an_existing_destination_without_a_receipt_is_reported_as_unconfirmed() {
        let parent = tempfile::tempdir().unwrap();
        let destination = parent.path().join("export");
        let finish = || {
            finish_delivery(
                DeliveryOperation::Cook,
                destination.clone(),
                Err(DeliveryError::Interrupted),
                None,
            )
        };
        assert!(matches!(finish(), Err(DeliveryError::Interrupted)));
        fs::create_dir(&destination).unwrap();
        let Err(DeliveryError::UnconfirmedPublication { directory, cause }) = finish() else {
            panic!("the cooker did not confirm who published the destination");
        };
        assert_eq!(directory, destination);
        assert!(matches!(*cause, DeliveryError::Interrupted));

        let invalid_export = finish_delivery(
            DeliveryOperation::Export,
            destination.clone(),
            Ok(()),
            Some(Ok(CookerReceipt {
                executable: None,
                postpublication_error: None,
            })),
        );
        let Err(DeliveryError::UnconfirmedPublication { directory, cause }) = invalid_export else {
            panic!("the Cook receipt cannot confirm an Export");
        };
        assert_eq!(directory, destination);
        assert!(matches!(*cause, DeliveryError::WrongReceipt));
    }

    #[test]
    fn export_receipt_requires_the_pack_executable_and_manifest() {
        let directory = tempfile::tempdir().unwrap();
        let finish = || {
            finish_delivery(
                DeliveryOperation::Export,
                directory.path().to_owned(),
                Ok(()),
                Some(Ok(CookerReceipt {
                    executable: Some("player.exe".into()),
                    postpublication_error: None,
                })),
            )
        };
        for name in [PACK_FILE, "player.exe", PACKAGE_FILE] {
            assert!(
                matches!(finish(), Err(DeliveryError::Path { path, .. }) if path.ends_with(name))
            );
            fs::write(directory.path().join(name), b"artifact").unwrap();
        }
        let receipt = finish().unwrap();
        assert_eq!(
            receipt.executable,
            Some(directory.path().join("player.exe"))
        );
        assert_eq!(
            receipt.package_manifest,
            Some(directory.path().join(PACKAGE_FILE))
        );
    }
}
