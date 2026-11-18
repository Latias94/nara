use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant},
};

use nara::{
    asset_build::{ArtifactBuildLimits, ArtifactBuildStore, ArtifactContentDigest},
    fs::{FsError, RelativeComponent, RelativePath},
    project_host::ingest_project_manifest,
    project_tooling::{
        cargo_build::{CargoBuildFailure, CargoBuildLimits, CargoBuildRequest, CargoBuildTools},
        host_fs,
        player_package::{PlayerPackageError, PlayerPackageLimits, PlayerPackageOutcome},
        project_cook::{
            ProjectCookError, ProjectCookRequest, prepare_project_cooked_content,
            project_rust_input_inventory,
        },
    },
    project_workflow::{cargo::CargoBuildSelection, cooked_content::CookedContentLimits},
    runtime_content::RuntimeContentTarget,
};

const COOKER_BYTES: u64 = 512 * 1024 * 1024;

struct Options {
    project: PathBuf,
    output: PathBuf,
    name: String,
    target_directory: PathBuf,
    target: String,
    profile: String,
    cargo: PathBuf,
    rustc: PathBuf,
}

impl Options {
    fn read() -> Result<Self, String> {
        let mut arguments = std::env::args_os().skip(1);
        let mut values = BTreeMap::new();
        while let Some(flag) = arguments.next() {
            let flag = flag.into_string().map_err(|_| "flags require UTF-8")?;
            if ![
                "--project-root",
                "--output-parent",
                "--name",
                "--target-dir",
                "--target",
                "--profile",
                "--cargo",
                "--rustc",
            ]
            .contains(&flag.as_str())
            {
                return Err(format!("unknown argument: {flag}"));
            }
            let value = arguments
                .next()
                .ok_or_else(|| format!("missing value for {flag}"))?;
            if values.insert(flag, value).is_some() {
                return Err("duplicate argument".into());
            }
        }
        let mut take = |flag: &str| values.remove(flag).ok_or_else(|| format!("missing {flag}"));
        let project = PathBuf::from(take("--project-root")?);
        let output = PathBuf::from(take("--output-parent")?);
        let target_directory = PathBuf::from(take("--target-dir")?);
        let cargo = PathBuf::from(take("--cargo")?);
        let rustc = PathBuf::from(take("--rustc")?);
        let name = take("--name")?
            .into_string()
            .map_err(|_| "name requires UTF-8")?;
        let target = take("--target")?
            .into_string()
            .map_err(|_| "target requires UTF-8")?;
        let profile = values.remove("--profile").map_or_else(
            || Ok("release".to_owned()),
            |value| value.into_string().map_err(|_| "profile requires UTF-8"),
        )?;
        for path in [&project, &output, &target_directory, &cargo, &rustc] {
            if !path.is_absolute() {
                return Err(format!(
                    "an explicit absolute path is required: {}",
                    path.display()
                ));
            }
        }
        Ok(Self {
            project,
            output,
            name,
            target_directory,
            target,
            profile,
            cargo,
            rustc,
        })
    }
}

fn main() -> ExitCode {
    let options = match Options::read() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("starter cook: {message}");
            return ExitCode::FAILURE;
        }
    };
    match run(&options) {
        Ok(outcome) => publication_exit(outcome, &options.output, &options.name),
        Err(error) => failure_exit(error),
    }
}

fn run(options: &Options) -> Result<PlayerPackageOutcome, ProjectCookError> {
    let project = options.project.canonicalize().map_err(invalid)?;
    let output = options.output.canonicalize().map_err(invalid)?;
    let root = host_fs::readable_directory(&project).map_err(invalid)?;
    let parent = host_fs::writable_directory(&output).map_err(invalid)?;
    let destination = RelativeComponent::new(&options.name).map_err(invalid)?;
    let manifest = root.open_file(&RelativePath::new("nara.toml").map_err(invalid)?)?;
    let candidate = ingest_project_manifest(&manifest, None)
        .map_err(|error| ProjectCookError::Invalid(format!("project manifest: {error:?}")))?;
    let cache_name = RelativeComponent::new("nara-cook-cache").map_err(invalid)?;
    let cache = match parent.open_child_directory(&cache_name) {
        Ok(cache) => cache,
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            match parent.create_child_directory(&cache_name) {
                Ok(cache) => cache,
                Err(FsError::AlreadyExists { .. }) => parent.open_child_directory(&cache_name)?,
                Err(error) => return Err(error.into()),
            }
        }
        Err(error) => return Err(error.into()),
    };
    let store = ArtifactBuildStore::open(cache, ArtifactBuildLimits::default())?;
    let executable = std::env::current_exe().map_err(invalid)?;
    let executable = executable.canonicalize().map_err(invalid)?;
    let implementation = ArtifactContentDigest::from_content(
        host_fs::readable_file(&executable)
            .map_err(invalid)?
            .digest(COOKER_BYTES)?,
    );
    let content = prepare_project_cooked_content(
        ProjectCookRequest {
            root: &root,
            candidate: &candidate,
            recipe: nara_game::product_recipe().map_err(invalid)?,
            target: RuntimeContentTarget::new(&options.target, "game-shipping").map_err(invalid)?,
            store: &store,
            implementation,
            limits: CookedContentLimits::default(),
        },
        |_| Ok(()),
    )?;
    let selection = CargoBuildSelection::new(
        env!("CARGO_PKG_NAME"),
        "player",
        &options.profile,
        &options.target,
        std::iter::empty::<&str>(),
        false,
    )
    .map_err(invalid)?;
    let request = CargoBuildRequest::new(
        &project,
        RelativePath::new("Cargo.toml").map_err(invalid)?,
        selection,
        CargoBuildTools::new(&options.cargo, &options.rustc)?,
        project_rust_input_inventory(&project)?,
        CargoBuildLimits::default(),
    )?
    .with_target_directory(&options.target_directory)?;
    let prepared = content.build_player(request, PlayerPackageLimits::default())?;
    Ok(prepared.publish(&parent, &destination))
}

fn publication_exit(outcome: PlayerPackageOutcome, parent: &Path, name: &str) -> ExitCode {
    match outcome {
        PlayerPackageOutcome::Published { receipt, error } => {
            println!(
                "STARTER_PACKAGE_PUBLISHED directory={} executable={}",
                parent.join(name).display(),
                receipt.files.executable_name,
            );
            error.map_or(ExitCode::SUCCESS, |error| failure_exit(error.into()))
        }
        PlayerPackageOutcome::BeforeStaging(error) => failure_exit(error.into()),
        PlayerPackageOutcome::Rejected { error, publication } => {
            eprintln!("starter cook: publication rejected with clean removal: {publication:?}");
            error.map_or(ExitCode::FAILURE, |error| failure_exit(error.into()))
        }
        PlayerPackageOutcome::Orphaned { error, publication } => {
            eprintln!(
                "starter cook: publication retained its original recoverable orphan under {}: {publication:?}",
                parent.display(),
            );
            error.map_or(ExitCode::FAILURE, |error| failure_exit(error.into()))
        }
        PlayerPackageOutcome::Unresolved(error) => {
            eprintln!(
                "starter cook: publication could not begin under {}: {error:?}",
                parent.display(),
            );
            ExitCode::FAILURE
        }
    }
}

fn failure_exit(mut error: ProjectCookError) -> ExitCode {
    let build = match &mut error {
        ProjectCookError::Cargo(build) => Some(build),
        ProjectCookError::Package(PlayerPackageError::Build { source, .. }) => {
            Some(source.as_mut())
        }
        _ => None,
    };
    if let Some(CargoBuildFailure::CleanupIncomplete(custody)) = build {
        let notice = Instant::now() + Duration::from_secs(10);
        let mut reported = false;
        while custody.retry_cleanup().is_err() {
            if !reported && Instant::now() >= notice {
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "starter cook: build cleanup remains pending; retaining the original child and target lease while retrying: pid={:?}",
                    custody.process_id(),
                );
                reported = true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    eprintln!("starter cook: {error:?}");
    ExitCode::FAILURE
}

fn invalid(error: impl std::fmt::Display) -> ProjectCookError {
    ProjectCookError::Invalid(error.to_string())
}
