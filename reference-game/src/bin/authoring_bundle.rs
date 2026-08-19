use std::{
    env,
    error::Error,
    ffi::{OsStr, OsString},
    fmt::{self, Display, Formatter},
    fs::File,
    io::{self, Write},
    path::Path,
    process::{self, ExitCode},
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    fs::{
        CapabilityRights, ContentDigest, DirectoryCapability, ExpectedTarget, FsError,
        FsOperation, HostCapabilityOptions, RelativeComponent, RelativePath, TrustMode,
    },
    project_host::{ingest_project_manifest, resolve_product_recipe},
    reflect::{AuthoringSchemaBundleError, AuthoringSchemaBundleLimits},
};
use nara_reference_game::project_recipe;

fn main() -> ExitCode {
    match run(env::args_os().skip(1)) {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("authoring-bundle: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: impl IntoIterator<Item = OsString>) -> Result<&'static str, BundleToolError> {
    let command = Command::parse(arguments)?;
    run_in_project(Path::new(env!("CARGO_MANIFEST_DIR")), command)
}

fn run_in_project(project_root: &Path, command: Command) -> Result<&'static str, BundleToolError> {
    let rights = match command {
        Command::Check => CapabilityRights::ReadOnly,
        Command::Write => CapabilityRights::ReadWrite,
    };
    let root = DirectoryCapability::from_host_handle(
        host_directory(project_root, rights).map_err(BundleToolError::OpenProjectRoot)?,
        HostCapabilityOptions::new(rights, portable_trust()),
    )
    .map_err(BundleToolError::AuthorizeProjectRoot)?;
    let (bundle_path, expected) = canonical_bundle_bytes(&root)?;

    match command {
        Command::Check => {
            let actual = read_declared_bundle(&root, &bundle_path)?;
            if actual != expected {
                return Err(BundleToolError::Stale);
            }
            Ok("checked the declared authoring schema bundle")
        }
        Command::Write => {
            let (parent, target) = root
                .resolve_file_parent(&bundle_path)
                .map_err(BundleToolError::ResolveBundleParent)?;
            let expected_target = match parent.open_child_file(&target) {
                Ok(current) => {
                    let current_identity = current.identity();
                    if current
                        .read_to_end_bounded(bundle_read_limit())
                        .is_ok_and(|actual| actual == expected)
                    {
                        return Ok("the declared authoring schema bundle is already current");
                    }
                    ExpectedTarget::Identity(current_identity)
                }
                Err(FsError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
                    ExpectedTarget::Missing
                }
                Err(source) => return Err(BundleToolError::ReadBundle(source)),
            };
            let temporary_name = RelativeComponent::new(&format!(
                ".nara-authoring-bundle-{}.tmp",
                process::id()
            ))
            .map_err(|_| BundleToolError::TemporaryName)?;
            let mut temporary = parent
                .create_temp(&temporary_name)
                .map_err(BundleToolError::CreateTemporary)?;
            temporary
                .write_all(&expected)
                .and_then(|()| temporary.flush())
                .map_err(BundleToolError::WriteTemporary)?;
            temporary
                .sync()
                .map_err(BundleToolError::SyncTemporary)?;
            let receipt = parent
                .replace_temp(temporary, &target, expected_target)
                .map_err(BundleToolError::PublishBundle)?;
            let expected_digest = ContentDigest::of_bytes(&expected);
            if !receipt.naming_is_atomic()
                || receipt.published_content_digest() != Some(expected_digest)
            {
                return Err(BundleToolError::PublicationUnverified);
            }
            match parent.sync() {
                Ok(_)
                | Err(FsError::Unsupported {
                    operation: FsOperation::SyncDirectory,
                    ..
                }) => {}
                Err(source) => return Err(BundleToolError::SyncParent(source)),
            }
            Ok("wrote the declared authoring schema bundle")
        }
    }
}

fn canonical_bundle_bytes(
    root: &DirectoryCapability,
) -> Result<(RelativePath, Vec<u8>), BundleToolError> {
    let manifest_path =
        RelativePath::new("nara.toml").map_err(|_| BundleToolError::ParseProjectManifestPath)?;
    let manifest = root
        .open_file(&manifest_path)
        .map_err(BundleToolError::OpenProjectManifest)?;
    let candidate =
        ingest_project_manifest(&manifest, None).map_err(|_| BundleToolError::IngestManifest)?;
    drop(manifest);
    let bundle_path = candidate
        .settings()
        .authoring_schema_bundle
        .as_ref()
        .ok_or(BundleToolError::BundlePathMissing)?;
    let bundle_path = RelativePath::new(bundle_path.as_str())
        .map_err(|_| BundleToolError::ParseBundlePath)?;
    let recipe = project_recipe().map_err(|_| BundleToolError::ComposeRecipe)?;
    let plan = resolve_product_recipe(&candidate, recipe)
        .map_err(|_| BundleToolError::ResolveProductPlan)?;
    let bundle = plan.schema_validation().authoring_schema_bundle();
    let mut encoded = bundle
        .to_json_string()
        .map_err(BundleToolError::Encode)?
        .into_bytes();
    encoded.push(b'\n');
    Ok((bundle_path, encoded))
}

fn read_declared_bundle(
    root: &DirectoryCapability,
    path: &RelativePath,
) -> Result<Vec<u8>, BundleToolError> {
    root.open_file(path)
        .map_err(BundleToolError::ReadBundle)?
        .read_to_end_bounded(bundle_read_limit())
        .map_err(BundleToolError::ReadBundle)
}

fn bundle_read_limit() -> u64 {
    u64::try_from(AuthoringSchemaBundleLimits::default().encoded_bytes().get())
        .unwrap_or(u64::MAX)
        .saturating_add(1)
}

#[cfg(windows)]
fn host_directory(path: &Path, _rights: CapabilityRights) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(unix)]
fn host_directory(path: &Path, _rights: CapabilityRights) -> io::Result<File> {
    File::open(path)
}

fn portable_trust() -> TrustMode {
    if cfg!(any(windows, target_os = "linux")) {
        TrustMode::Untrusted
    } else {
        TrustMode::TrustedLocal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Check,
    Write,
}

impl Command {
    fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Self, BundleToolError> {
        let arguments = arguments.into_iter().collect::<Vec<_>>();
        match arguments.as_slice() {
            [argument] if argument.as_os_str() == OsStr::new("--check") => Ok(Self::Check),
            [argument] if argument.as_os_str() == OsStr::new("--write") => Ok(Self::Write),
            _ => Err(BundleToolError::Usage),
        }
    }
}

#[derive(Debug)]
enum BundleToolError {
    Usage,
    OpenProjectRoot(io::Error),
    AuthorizeProjectRoot(FsError),
    ParseProjectManifestPath,
    OpenProjectManifest(FsError),
    IngestManifest,
    BundlePathMissing,
    ParseBundlePath,
    ComposeRecipe,
    ResolveProductPlan,
    Encode(AuthoringSchemaBundleError),
    ReadBundle(FsError),
    ResolveBundleParent(FsError),
    TemporaryName,
    CreateTemporary(FsError),
    WriteTemporary(io::Error),
    SyncTemporary(FsError),
    PublishBundle(FsError),
    PublicationUnverified,
    SyncParent(FsError),
    Stale,
}

impl Display for BundleToolError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => formatter.write_str("expected exactly one of --check or --write"),
            Self::OpenProjectRoot(_) => formatter.write_str("project root could not be opened"),
            Self::AuthorizeProjectRoot(_) => {
                formatter.write_str("project root capability could not be authorized")
            }
            Self::ParseProjectManifestPath => {
                formatter.write_str("project manifest path is invalid")
            }
            Self::OpenProjectManifest(_) => {
                formatter.write_str("project manifest could not be opened")
            }
            Self::IngestManifest => formatter.write_str("project manifest could not be ingested"),
            Self::BundlePathMissing => {
                formatter.write_str("project manifest does not declare an authoring schema bundle")
            }
            Self::ParseBundlePath => {
                formatter.write_str("declared authoring schema bundle path is invalid")
            }
            Self::ComposeRecipe => formatter.write_str("project recipe could not be composed"),
            Self::ResolveProductPlan => {
                formatter.write_str("project product plan could not be resolved")
            }
            Self::Encode(_) => formatter.write_str("canonical authoring bundle could not encode"),
            Self::ReadBundle(_) => formatter.write_str("declared bundle could not be read"),
            Self::ResolveBundleParent(_) => {
                formatter.write_str("declared bundle parent could not be resolved")
            }
            Self::TemporaryName => formatter.write_str("temporary bundle name is invalid"),
            Self::CreateTemporary(_) => {
                formatter.write_str("temporary bundle could not be created")
            }
            Self::WriteTemporary(_) => {
                formatter.write_str("temporary bundle could not be written")
            }
            Self::SyncTemporary(_) => {
                formatter.write_str("temporary bundle could not be synchronized")
            }
            Self::PublishBundle(_) => {
                formatter.write_str("temporary bundle could not replace the declared target")
            }
            Self::PublicationUnverified => formatter
                .write_str("published bundle identity or content could not be verified atomically"),
            Self::SyncParent(_) => {
                formatter.write_str("bundle parent directory could not be synchronized")
            }
            Self::Stale => formatter.write_str(
                "the declared bundle differs from the trusted provider catalogs; run again with --write",
            ),
        }
    }
}

impl Error for BundleToolError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::OpenProjectRoot(source) => Some(source),
            Self::AuthorizeProjectRoot(source)
            | Self::OpenProjectManifest(source)
            | Self::ReadBundle(source)
            | Self::ResolveBundleParent(source)
            | Self::CreateTemporary(source)
            | Self::SyncTemporary(source)
            | Self::PublishBundle(source)
            | Self::SyncParent(source) => Some(source),
            Self::WriteTemporary(source) => Some(source),
            Self::Encode(source) => Some(source),
            Self::Usage
            | Self::ParseProjectManifestPath
            | Self::IngestManifest
            | Self::BundlePathMissing
            | Self::ParseBundlePath
            | Self::ComposeRecipe
            | Self::ResolveProductPlan
            | Self::TemporaryName
            | Self::PublicationUnverified
            | Self::Stale => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{BundleToolError, Command, run_in_project};

    static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn check_reads_the_bundle_path_declared_by_the_manifest() {
        let root = std::env::temp_dir().join(format!(
            "nara-reference-authoring-bundle-{}-{}",
            std::process::id(),
            NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("schema")).unwrap();
        fs::create_dir_all(root.join("alternate")).unwrap();
        let manifest = include_str!("../../nara.toml").replace(
            "schema_bundle = \"schema/authoring.bundle.json\"",
            "schema_bundle = \"alternate/authoring.bundle.json\"",
        );
        fs::write(root.join("nara.toml"), manifest).unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            include_bytes!("../../schema/authoring.bundle.json"),
        )
        .unwrap();
        fs::write(root.join("alternate/authoring.bundle.json"), b"{}\n").unwrap();

        assert!(matches!(
            run_in_project(&root, Command::Check),
            Err(BundleToolError::Stale)
        ));
        let write = run_in_project(&root, Command::Write);
        assert!(write.is_ok(), "write failed: {write:?}");
        assert!(run_in_project(&root, Command::Check).is_ok());
        assert_eq!(
            fs::read(root.join("alternate/authoring.bundle.json")).unwrap(),
            include_bytes!("../../schema/authoring.bundle.json")
        );

        fs::remove_dir_all(root).unwrap();
    }
}
