//! Game-owned finite cooker and captured Player publication through the original Nara owners.

#[path = "cook/content.rs"]
mod content;
#[path = "cook/input.rs"]
mod input;
#[path = "cook/source.rs"]
mod source;

use std::{
    fs,
    io::Write,
    process::ExitCode,
    time::{Duration, Instant},
};

use nara::{
    fs::{RelativeComponent, RelativePath},
    project_tooling::{
        cargo_build::{CargoBuildFailure, CargoBuildLimits, CargoBuildRequest, CargoBuildTools},
        host_fs,
        player_package::{
            GeneratedPlayerBootstrap, PlayerPackageControlFailure, PlayerPackageError,
            PlayerPackageLimits, PlayerPackageOutcome, build_player_package,
        },
    },
    project_workflow::cargo::CargoBuildSelection,
};

const GENERATED: &str = "src/bin/player/generated.rs";

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
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if output.starts_with(&project) || target.starts_with(&project) {
        return Err(
            "output parent and shared Cargo target must be outside the source project".into(),
        );
    }
    let destination = RelativeComponent::new(&options.name).map_err(|error| error.to_string())?;
    let parent = host_fs::writable_directory(&output).map_err(|error| error.to_string())?;
    let mut fence = source::ContentFence::capture(&project)?;
    let cache_name =
        RelativeComponent::new("two-level-cook-cache").map_err(|error| error.to_string())?;
    let cache = match parent.open_child_directory(&cache_name) {
        Ok(cache) => cache,
        Err(_) => parent
            .create_child_directory(&cache_name)
            .map_err(|error| error.to_string())?,
    };
    let (content, observations) =
        content::prepare(fence.root(), &cache, &options.target, options.runtime)?;
    fence.bind_raw_inputs(&observations)?;
    let limits = PlayerPackageLimits::default();
    let bootstrap = GeneratedPlayerBootstrap::new(&content, GENERATED, limits)
        .map_err(|error| error.to_string())?;
    let generated_directory = project.join("src/bin/player");
    let generated = project.join(GENERATED);
    fs::create_dir_all(&generated_directory).map_err(|error| error.to_string())?;
    if generated_directory
        .canonicalize()
        .map_err(|error| error.to_string())?
        != generated_directory
    {
        return Err("generated directory must retain its original source path".into());
    }
    if let Ok(metadata) = fs::symlink_metadata(&generated) {
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("generated bootstrap must be a regular source file".into());
        }
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&generated)
        .map_err(|error| error.to_string())?;
    file.write_all(bootstrap.bytes())
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    drop(file);
    let inputs = source::code_inventory(&project)?;
    fence.bind_code(&inputs)?;
    let selection = CargoBuildSelection::new(
        "two-level-game",
        options.runtime.binary(),
        &options.profile,
        &options.target,
        options.runtime.features(),
        false,
    )
    .map_err(|error| error.to_string())?;
    let tools =
        CargoBuildTools::new(options.cargo, options.rustc).map_err(|error| error.to_string())?;
    let request = CargoBuildRequest::new(
        &project,
        RelativePath::new("Cargo.toml").map_err(|error| error.to_string())?,
        selection,
        tools,
        inputs,
        CargoBuildLimits::default(),
    )
    .and_then(|request| request.with_target_directory(&target))
    .map_err(|error| error.to_string())?;
    let check = || {
        fence
            .verify()
            .map_err(|_| PlayerPackageControlFailure::SelectionChanged)
    };
    let prepared = match build_player_package(content, bootstrap, &request, limits, |_| check()) {
        Ok(prepared) => prepared,
        Err(error) => return Err(build_failure(error)),
    };
    match prepared.publish(&parent, &destination, |_| check()) {
        PlayerPackageOutcome::Published { receipt, error } => {
            println!(
                "TWO_LEVEL_PACKAGE_PUBLISHED {}",
                serde_json::json!({
                    "directory": output.join(&options.name),
                    "executable": receipt.files.executable_name,
                    "postpublication_error": error.as_ref().map(ToString::to_string),
                    "runtime": options.runtime.name(),
                })
            );
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

fn build_failure(mut error: PlayerPackageError) -> String {
    let message = format!("{error:?}");
    if let PlayerPackageError::Build { source, .. } = &mut error {
        if let CargoBuildFailure::CleanupIncomplete(custody) = source.as_mut() {
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
