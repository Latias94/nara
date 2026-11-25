//! Game-owned finite cooker and captured Player publication through the original Nara owners.

#[path = "cook/content.rs"]
mod content;
#[path = "cook/input.rs"]
mod input;

use std::{
    hash::{BuildHasher, RandomState},
    io::{self, Write},
    path::Path,
    process::ExitCode,
    time::{Duration, Instant},
};

use nara::{
    fs::{
        DirectoryCapability, DirectoryPublicationOutcome, FsError, FsOperation, RelativeComponent,
        RelativePath,
    },
    project::delivery::{CookerReceipt, DELIVERY_RECEIPT_PREFIX, DeliveryOperation},
    project_tooling::{
        cargo_build::{CargoBuildFailure, CargoBuildLimits, CargoBuildRequest, CargoBuildTools},
        host_fs,
        player_package::{
            PLAYER_PACK_FILE, PlayerPackageError, PlayerPackageLimits, PlayerPackageOutcome,
        },
        project_cook::{PreparedProjectCook, ProjectCookError, project_rust_input_inventory},
    },
    project_workflow::{cargo::CargoBuildSelection, cooked_content::PreparedRuntimeContent},
};

fn main() -> ExitCode {
    match input::Options::read().and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("two-level cook: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: input::Options) -> Result<(), String> {
    let project = options
        .project
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let output = options
        .output
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let target = options
        .target_directory
        .as_ref()
        .map(|target| target.canonicalize())
        .transpose()
        .map_err(|error| error.to_string())?;
    if output.starts_with(&project)
        || target
            .as_ref()
            .is_some_and(|path| path.starts_with(&project))
    {
        return Err(
            "output parent and shared Cargo target must be outside the source project".into(),
        );
    }
    let destination = RelativeComponent::new(&options.name).map_err(|error| error.to_string())?;
    let parent = host_fs::writable_directory(&output).map_err(|error| error.to_string())?;
    let root = host_fs::readable_directory(&project).map_err(|error| error.to_string())?;
    let cache_name =
        RelativeComponent::new("two-level-cook-cache").map_err(|error| error.to_string())?;
    let cache = match parent.open_child_directory(&cache_name) {
        Ok(cache) => cache,
        Err(FsError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
            match parent.create_child_directory(&cache_name) {
                Err(FsError::AlreadyExists { .. }) => parent.open_child_directory(&cache_name),
                result => result,
            }
            .map_err(|error| error.to_string())?
        }
        Err(error) => return Err(error.to_string()),
    };
    let prepared = content::prepare_for_player(&root, &cache, &options.target, options.selection)
        .map_err(build_failure)?;
    if options.operation == Some(DeliveryOperation::Cook) {
        publish_pack(prepared, &project, &parent, &output, &destination)?;
        return report_publication(None, None);
    }
    let limits = PlayerPackageLimits::default();
    let mut inputs = project_rust_input_inventory(&project).map_err(build_failure)?;
    if options.selection.runtime() != input::RuntimeKind::Server {
        inputs.push(
            RelativePath::new("assets/materials/region.wgsl").map_err(|error| error.to_string())?,
        );
    }
    let selection = CargoBuildSelection::new(
        "two-level-game",
        options.selection.binary(),
        &options.profile,
        &options.target,
        options.selection.features(),
        false,
    )
    .map_err(|error| error.to_string())?;
    let tools = match options.tools {
        Some((cargo, rustc)) => CargoBuildTools::new(cargo, rustc),
        None => CargoBuildTools::discover_in(&project),
    }
    .map_err(|error| build_failure(ProjectCookError::Cargo(error)))?;
    let request = CargoBuildRequest::new(
        &project,
        RelativePath::new("Cargo.toml").map_err(|error| error.to_string())?,
        selection,
        tools,
        inputs,
        CargoBuildLimits::default(),
    )
    .and_then(|request| match target {
        Some(target) => request.with_target_directory(target),
        None => Ok(request),
    })
    .map_err(|error| build_failure(ProjectCookError::Cargo(error)))?;
    let prepared = prepared
        .build_player(request, limits)
        .map_err(build_failure)?;
    match prepared.publish(&parent, &destination) {
        PlayerPackageOutcome::Published { receipt, error } => {
            let postpublication_error = error.as_ref().map(ToString::to_string);
            if options.operation == Some(DeliveryOperation::Export) {
                report_publication(Some(receipt.files.executable_name), postpublication_error)?;
            } else {
                println!(
                    "TWO_LEVEL_PACKAGE_PUBLISHED {}",
                    serde_json::json!({
                        "directory": output.join(&options.name),
                        "executable": receipt.files.executable_name,
                        "postpublication_error": postpublication_error,
                        "runtime": options.selection.runtime().name(),
                        "controllers": options.selection.controllers().name(),
                        "flow": options.selection.flow().name(),
                    })
                );
            }
            error.map_or(Ok(()), |error| {
                Err(format!("publication completed with error: {error}"))
            })
        }
        PlayerPackageOutcome::BeforeStaging(error) => {
            Err(format!("publication refused before staging: {error}"))
        }
        PlayerPackageOutcome::Rejected { error, publication } => Err(format!(
            "publication rejected with original clean removal: error={error:?}; {publication:?}"
        )),
        PlayerPackageOutcome::Orphaned { error, publication } => {
            report_orphan(&output, &publication);
            Err(format!(
                "publication retained its original recoverable orphan: error={error:?}; {publication:?}"
            ))
        }
        PlayerPackageOutcome::Unresolved(error) => {
            if let Some(orphan) = error.orphan() {
                report_orphan(&output, orphan);
            }
            Err(format!("publication could not begin: {error:?}"))
        }
    }
}

fn publish_pack(
    prepared: PreparedProjectCook,
    project: &Path,
    parent: &DirectoryCapability,
    output: &Path,
    destination: &RelativeComponent,
) -> Result<(), String> {
    let (content, source) = prepared.into_parts();
    source.verify(project).map_err(build_failure)?;
    let nonce = RandomState::new().hash_one(destination);
    let staging_name = RelativeComponent::new(format!(".nara-cook-{nonce:016x}"))
        .map_err(|error| error.to_string())?;
    let transaction = parent
        .begin_directory_publication(destination, &staging_name)
        .map_err(|error| {
            if let Some(orphan) = error.orphan() {
                report_orphan(output, orphan);
            }
            error.to_string()
        })?;
    let error = stage_pack(&transaction.staging_directory(), &content)
        .map_err(|error| error.to_string())
        .and_then(|()| source.verify(project).map_err(build_failure))
        .err();
    let outcome = if error.is_some() {
        transaction.abort()
    } else {
        transaction.publish()
    };
    match outcome {
        DirectoryPublicationOutcome::Published(_) => Ok(()),
        DirectoryPublicationOutcome::RejectedClean(publication) => Err(format!(
            "pack publication rejected: {error:?}; {publication:?}"
        )),
        DirectoryPublicationOutcome::Orphaned(publication) => {
            report_orphan(output, &publication);
            Err(format!(
                "pack publication retained a recoverable orphan: {error:?}; {publication:?}"
            ))
        }
    }
}

fn stage_pack(
    staging: &DirectoryCapability,
    content: &PreparedRuntimeContent,
) -> Result<(), Box<dyn std::error::Error>> {
    let name = RelativeComponent::new(PLAYER_PACK_FILE)?;
    let mut file = staging.create_child_file(&name)?;
    content.base().write_pack(&mut file.writer()?)?;
    file.sync()?;
    drop(file);
    match staging.sync() {
        Ok(_)
        | Err(FsError::Unsupported {
            operation: FsOperation::SyncDirectory,
            ..
        }) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn report_publication(
    executable: Option<String>,
    postpublication_error: Option<String>,
) -> Result<(), String> {
    let receipt = serde_json::to_string(&CookerReceipt {
        executable,
        postpublication_error,
    })
    .map_err(|error| error.to_string())?;
    writeln!(io::stdout().lock(), "{DELIVERY_RECEIPT_PREFIX}{receipt}")
        .map_err(|error| format!("publication completed, but writing its receipt failed: {error}"))
}

fn build_failure(mut error: ProjectCookError) -> String {
    let message = match &error {
        ProjectCookError::Source(source) => format!("source delivery agreement: {source:?}"),
        _ => format!("{error:?}"),
    };
    let build = match &mut error {
        ProjectCookError::Package(PlayerPackageError::Build { source, .. }) => {
            Some(source.as_mut())
        }
        ProjectCookError::Cargo(source) => Some(source),
        _ => None,
    };
    if let Some(CargoBuildFailure::CleanupIncomplete(custody)) = build {
        let notice = Instant::now() + Duration::from_secs(10);
        let mut reported = false;
        loop {
            if custody.retry_cleanup().is_ok() {
                break;
            }
            if !reported && Instant::now() >= notice {
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "two-level cook: original build cleanup remains pending; retaining child and target lease while retrying: pid={:?}",
                    custody.process_id()
                );
                reported = true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    message
}

fn report_orphan(parent: &std::path::Path, orphan: &nara::fs::DirectoryPublicationOrphan) {
    eprintln!(
        "TWO_LEVEL_PACKAGE_ORPHAN {}",
        serde_json::json!({
            "parent": parent,
            "original_staging_name": orphan.staging_name().to_str(),
            "destination": orphan.destination().to_str(),
            "parent_identity": format!("{:?}", orphan.parent_identity()),
            "staging_identity": format!("{:?}", orphan.staging_identity()),
            "cleanup_error": orphan.cleanup_error().to_string(),
            "recovery": "Retain the original candidate for explicit inspection; this diagnostic name does not authorize recursive deletion.",
        })
    );
}
