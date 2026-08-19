use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use nara::{
    fs::{ContentDigest, FsError},
    project_workflow::{
        __private::ProjectExportProductAgreement, ProjectExportBinaryTarget,
        ProjectExportBuildError, ProjectExportMemberKind, ProjectExportMemberReceipt,
        ProjectExportPlanReceipt, StartupContentClosureReceipt, StartupContentMemberKind,
    },
};
use serde_json::Value;
use thiserror::Error;

use crate::host_fs::{self, HostFileError};

const PROCESS_DEADLINE: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_OBSERVATION_BYTES: u64 = 4096;
const OBSERVATION_ATTEMPT_ENV: &str = "NARA_STARTER_OBSERVATION_ATTEMPT";
const OBSERVATION_FILE_ENV: &str = "NARA_STARTER_OBSERVATION_FILE";
static NEXT_ACCEPTANCE_DIRECTORY: AtomicU64 = AtomicU64::new(1);
static NEXT_PRODUCT_ATTEMPT: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub(crate) struct ProductProcessObservationReceipt {
    attempt: String,
    target: ProjectExportBinaryTarget,
    executable_digest: ContentDigest,
    manifest_digest: ContentDigest,
    startup_content_digest: ContentDigest,
    startup_closure_plan_digest: ContentDigest,
    authoring_schema_bundle_digest: ContentDigest,
    schema_fingerprint: String,
    observation_digest: ContentDigest,
}

impl ProductProcessObservationReceipt {
    pub(crate) fn attempt(&self) -> &str {
        &self.attempt
    }

    pub(crate) const fn target(&self) -> ProjectExportBinaryTarget {
        self.target
    }

    pub(crate) const fn profile(&self) -> &'static str {
        product_profile(self.target)
    }

    pub(crate) const fn executable_digest(&self) -> ContentDigest {
        self.executable_digest
    }

    pub(crate) const fn manifest_digest(&self) -> ContentDigest {
        self.manifest_digest
    }

    pub(crate) const fn startup_content_digest(&self) -> ContentDigest {
        self.startup_content_digest
    }

    pub(crate) const fn startup_closure_plan_digest(&self) -> ContentDigest {
        self.startup_closure_plan_digest
    }

    pub(crate) const fn authoring_schema_bundle_digest(&self) -> ContentDigest {
        self.authoring_schema_bundle_digest
    }

    pub(crate) fn schema_fingerprint(&self) -> &str {
        &self.schema_fingerprint
    }

    pub(crate) const fn observation_digest(&self) -> ContentDigest {
        self.observation_digest
    }

    pub(super) fn into_product_agreement(
        self,
    ) -> Result<ProjectExportProductAgreement, ProjectExportBuildError> {
        ProjectExportProductAgreement::new(
            self.target,
            self.executable_digest,
            self.manifest_digest,
            self.startup_content_digest,
            self.startup_closure_plan_digest,
            self.authoring_schema_bundle_digest,
            self.schema_fingerprint,
            self.observation_digest,
        )
    }
}

#[derive(Debug)]
pub(crate) struct ExportAcceptanceReceipt {
    products: [ProductProcessObservationReceipt; 2],
}

impl ExportAcceptanceReceipt {
    pub(crate) const fn products(&self) -> &[ProductProcessObservationReceipt; 2] {
        &self.products
    }
}

#[derive(Debug, Error)]
pub(crate) enum ExportAcceptanceError {
    #[error("the export plan is missing its {target:?} executable")]
    MissingExecutable { target: ProjectExportBinaryTarget },
    #[error("the checked startup closure is missing its authoring schema bundle")]
    MissingAuthoringSchemaBundle,
    #[error("failed to open the published {target:?} executable")]
    OpenExecutable {
        target: ProjectExportBinaryTarget,
        #[source]
        source: HostFileError,
    },
    #[error("the published {target:?} executable no longer matches the export plan")]
    ExecutableChanged {
        target: ProjectExportBinaryTarget,
        #[source]
        source: FsError,
    },
    #[error("failed to create the unrelated acceptance working directory")]
    CreateWorkingDirectory(#[source] io::Error),
    #[error("failed to start the published {target:?} executable")]
    Start {
        target: ProjectExportBinaryTarget,
        #[source]
        source: io::Error,
    },
    #[error("failed while waiting for the published {target:?} executable")]
    Wait {
        target: ProjectExportBinaryTarget,
        #[source]
        source: io::Error,
    },
    #[error("the published {target:?} executable exceeded its bounded deadline")]
    TimedOut { target: ProjectExportBinaryTarget },
    #[error("failed to terminate the timed-out published {target:?} executable")]
    TerminateTimedOut {
        target: ProjectExportBinaryTarget,
        #[source]
        source: io::Error,
    },
    #[error("failed to reap the timed-out published {target:?} executable")]
    ReapTimedOut {
        target: ProjectExportBinaryTarget,
        #[source]
        source: io::Error,
    },
    #[error("the published {target:?} executable failed with {status}")]
    Failed {
        target: ProjectExportBinaryTarget,
        status: ExitStatus,
    },
    #[error("failed to read the {target:?} product observation")]
    ObservationIo {
        target: ProjectExportBinaryTarget,
        #[source]
        source: io::Error,
    },
    #[error("the {target:?} product observation exceeded its byte limit")]
    ObservationTooLarge { target: ProjectExportBinaryTarget },
    #[error("the {target:?} product observation is malformed")]
    ObservationJson {
        target: ProjectExportBinaryTarget,
        #[source]
        source: serde_json::Error,
    },
    #[error("the {target:?} product observation does not match the parent attempt")]
    ObservationMismatch { target: ProjectExportBinaryTarget },
}

pub(super) fn accept(
    published_root: &Path,
    plan: &ProjectExportPlanReceipt,
    manifest_digest: ContentDigest,
) -> Result<ExportAcceptanceReceipt, ExportAcceptanceError> {
    let working_directory = AcceptanceWorkingDirectory::new()?;
    let authoring_schema_bundle_digest = export_bundle_digest(plan)?;
    let mut products = Vec::with_capacity(2);
    for (target, arguments) in [
        (ProjectExportBinaryTarget::Headless, &[][..]),
        (
            ProjectExportBinaryTarget::Desktop,
            &["--verify-first-present"][..],
        ),
    ] {
        let member = executable_member(plan, target)?;
        let executable = published_root.join(member.path().as_str());
        products.push(observe_product(
            "export",
            ExpectedProductObservation::new(
                target,
                &executable,
                member.digest(),
                manifest_digest,
                plan.startup_content_digest(),
                plan.startup_closure_plan_digest(),
                authoring_schema_bundle_digest,
            ),
            working_directory.path(),
            arguments,
        )?);
    }
    Ok(ExportAcceptanceReceipt {
        products: products
            .try_into()
            .expect("the closed export acceptance always observes two products"),
    })
}

pub(super) fn observe_built_products(
    attempt_prefix: &str,
    startup_content: &StartupContentClosureReceipt,
    executables: [(ProjectExportBinaryTarget, &Path); 2],
) -> Result<[ProductProcessObservationReceipt; 2], ExportAcceptanceError> {
    let working_directory = AcceptanceWorkingDirectory::new()?;
    let authoring_schema_bundle_digest = startup_bundle_digest(startup_content)?;
    let mut observations = Vec::with_capacity(2);
    for (target, executable) in executables {
        let executable_digest = host_fs::readable_file(executable)
            .map_err(|source| ExportAcceptanceError::OpenExecutable { target, source })?
            .digest(512 * 1024 * 1024)
            .map_err(|source| ExportAcceptanceError::ExecutableChanged { target, source })?;
        let arguments = match target {
            ProjectExportBinaryTarget::Headless => &[][..],
            ProjectExportBinaryTarget::Desktop => &["--verify-first-present"][..],
        };
        observations.push(observe_product(
            attempt_prefix,
            ExpectedProductObservation::new(
                target,
                executable,
                executable_digest,
                startup_content.manifest_digest(),
                startup_content.content_digest(),
                startup_content.closure_plan_digest(),
                authoring_schema_bundle_digest,
            ),
            working_directory.path(),
            arguments,
        )?);
    }
    Ok(observations
        .try_into()
        .expect("the closed export build observes exactly two products"))
}

fn export_bundle_digest(
    plan: &ProjectExportPlanReceipt,
) -> Result<ContentDigest, ExportAcceptanceError> {
    plan.members()
        .iter()
        .find(|member| {
            member.kind()
                == ProjectExportMemberKind::StartupContent(
                    StartupContentMemberKind::AuthoringSchemaBundle,
                )
        })
        .map(ProjectExportMemberReceipt::digest)
        .ok_or(ExportAcceptanceError::MissingAuthoringSchemaBundle)
}

fn startup_bundle_digest(
    startup_content: &StartupContentClosureReceipt,
) -> Result<ContentDigest, ExportAcceptanceError> {
    startup_content
        .members()
        .iter()
        .find(|member| member.kind() == StartupContentMemberKind::AuthoringSchemaBundle)
        .map(nara::project_workflow::StartupContentMemberReceipt::digest)
        .ok_or(ExportAcceptanceError::MissingAuthoringSchemaBundle)
}

fn executable_member(
    plan: &ProjectExportPlanReceipt,
    target: ProjectExportBinaryTarget,
) -> Result<&ProjectExportMemberReceipt, ExportAcceptanceError> {
    let kind = match target {
        ProjectExportBinaryTarget::Headless => ProjectExportMemberKind::HeadlessExecutable,
        ProjectExportBinaryTarget::Desktop => ProjectExportMemberKind::DesktopExecutable,
    };
    plan.members()
        .iter()
        .find(|member| member.kind() == kind)
        .ok_or(ExportAcceptanceError::MissingExecutable { target })
}

pub(super) struct ExpectedProductObservation<'a> {
    target: ProjectExportBinaryTarget,
    executable: &'a Path,
    executable_digest: ContentDigest,
    manifest_digest: ContentDigest,
    startup_content_digest: ContentDigest,
    startup_closure_plan_digest: ContentDigest,
    authoring_schema_bundle_digest: ContentDigest,
}

impl<'a> ExpectedProductObservation<'a> {
    pub(super) const fn new(
        target: ProjectExportBinaryTarget,
        executable: &'a Path,
        executable_digest: ContentDigest,
        manifest_digest: ContentDigest,
        startup_content_digest: ContentDigest,
        startup_closure_plan_digest: ContentDigest,
        authoring_schema_bundle_digest: ContentDigest,
    ) -> Self {
        Self {
            target,
            executable,
            executable_digest,
            manifest_digest,
            startup_content_digest,
            startup_closure_plan_digest,
            authoring_schema_bundle_digest,
        }
    }
}

pub(super) fn observe_product(
    attempt_prefix: &str,
    expected: ExpectedProductObservation<'_>,
    working_directory: &Path,
    arguments: &[&str],
) -> Result<ProductProcessObservationReceipt, ExportAcceptanceError> {
    let mut command = Command::new(expected.executable);
    command.args(arguments);
    observe_product_command(attempt_prefix, expected, working_directory, command)
}

#[cfg(test)]
pub(super) fn observe_prepared_product_command(
    attempt_prefix: &str,
    expected: ExpectedProductObservation<'_>,
    working_directory: &Path,
    command: Command,
) -> Result<ProductProcessObservationReceipt, ExportAcceptanceError> {
    observe_product_command(attempt_prefix, expected, working_directory, command)
}

fn observe_product_command(
    attempt_prefix: &str,
    expected: ExpectedProductObservation<'_>,
    working_directory: &Path,
    mut command: Command,
) -> Result<ProductProcessObservationReceipt, ExportAcceptanceError> {
    let ExpectedProductObservation {
        target,
        executable,
        executable_digest,
        manifest_digest,
        startup_content_digest,
        startup_closure_plan_digest,
        authoring_schema_bundle_digest,
    } = expected;
    host_fs::readable_file(executable)
        .map_err(|source| ExportAcceptanceError::OpenExecutable { target, source })?
        .verify_digest(executable_digest, executable_digest.length())
        .map_err(|source| ExportAcceptanceError::ExecutableChanged { target, source })?;
    let attempt = product_attempt(
        attempt_prefix,
        target,
        manifest_digest,
        startup_content_digest,
    );
    let observation = working_directory.join(format!("{attempt}.json"));
    let mut child = command
        .current_dir(working_directory)
        .env(OBSERVATION_ATTEMPT_ENV, &attempt)
        .env(OBSERVATION_FILE_ENV, &observation)
        .spawn()
        .map_err(|source| ExportAcceptanceError::Start { target, source })?;
    let deadline = Instant::now() + PROCESS_DEADLINE;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|source| ExportAcceptanceError::Wait { target, source })?
        {
            if !status.success() {
                return Err(ExportAcceptanceError::Failed { target, status });
            }
            let bytes = read_observation(target, &observation)?;
            let schema_fingerprint = validate_observation(
                target,
                &attempt,
                executable_digest,
                manifest_digest,
                startup_content_digest,
                startup_closure_plan_digest,
                authoring_schema_bundle_digest,
                &bytes,
            )?;
            return Ok(ProductProcessObservationReceipt {
                attempt,
                target,
                executable_digest,
                manifest_digest,
                startup_content_digest,
                startup_closure_plan_digest,
                authoring_schema_bundle_digest,
                schema_fingerprint,
                observation_digest: ContentDigest::of_bytes(&bytes),
            });
        }
        if Instant::now() >= deadline {
            child
                .kill()
                .map_err(|source| ExportAcceptanceError::TerminateTimedOut { target, source })?;
            child
                .wait()
                .map_err(|source| ExportAcceptanceError::ReapTimedOut { target, source })?;
            return Err(ExportAcceptanceError::TimedOut { target });
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn read_observation(
    target: ProjectExportBinaryTarget,
    path: &Path,
) -> Result<Vec<u8>, ExportAcceptanceError> {
    let file = File::open(path)
        .map_err(|source| ExportAcceptanceError::ObservationIo { target, source })?;
    let mut bytes = Vec::new();
    file.take(MAX_OBSERVATION_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| ExportAcceptanceError::ObservationIo { target, source })?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_OBSERVATION_BYTES {
        return Err(ExportAcceptanceError::ObservationTooLarge { target });
    }
    Ok(bytes)
}

fn validate_observation(
    target: ProjectExportBinaryTarget,
    attempt: &str,
    executable_digest: ContentDigest,
    manifest_digest: ContentDigest,
    startup_content_digest: ContentDigest,
    startup_closure_plan_digest: ContentDigest,
    authoring_schema_bundle_digest: ContentDigest,
    bytes: &[u8],
) -> Result<String, ExportAcceptanceError> {
    let value = serde_json::from_slice::<Value>(bytes)
        .map_err(|source| ExportAcceptanceError::ObservationJson { target, source })?;
    let Some(object) = value.as_object() else {
        return Err(ExportAcceptanceError::ObservationMismatch { target });
    };
    let executable_hash = digest_hex(executable_digest);
    let manifest_hash = digest_hex(manifest_digest);
    let content_hash = digest_hex(startup_content_digest);
    let closure_plan_hash = digest_hex(startup_closure_plan_digest);
    let bundle_hash = digest_hex(authoring_schema_bundle_digest);
    let schema_fingerprint = object.get("schema_fingerprint").and_then(Value::as_str);
    let matches = object.len() == 15
        && object.get("version").and_then(Value::as_u64) == Some(2)
        && object.get("attempt").and_then(Value::as_str) == Some(attempt)
        && object.get("target").and_then(Value::as_str) == Some(target.cargo_target_name())
        && object.get("profile").and_then(Value::as_str) == Some(product_profile(target))
        && object.get("executable_length").and_then(Value::as_u64)
            == Some(executable_digest.length())
        && object.get("executable_blake3").and_then(Value::as_str)
            == Some(executable_hash.as_str())
        && object.get("manifest_length").and_then(Value::as_u64) == Some(manifest_digest.length())
        && object.get("manifest_blake3").and_then(Value::as_str) == Some(manifest_hash.as_str())
        && object.get("content_length").and_then(Value::as_u64)
            == Some(startup_content_digest.length())
        && object.get("content_blake3").and_then(Value::as_str) == Some(content_hash.as_str());
    let matches = matches
        && object.get("closure_plan_length").and_then(Value::as_u64)
            == Some(startup_closure_plan_digest.length())
        && object.get("closure_plan_blake3").and_then(Value::as_str)
            == Some(closure_plan_hash.as_str())
        && object
            .get("authoring_bundle_length")
            .and_then(Value::as_u64)
            == Some(authoring_schema_bundle_digest.length())
        && object
            .get("authoring_bundle_blake3")
            .and_then(Value::as_str)
            == Some(bundle_hash.as_str())
        && schema_fingerprint.is_some_and(valid_schema_fingerprint);
    if matches {
        Ok(schema_fingerprint
            .expect("a matching product observation has a schema fingerprint")
            .to_owned())
    } else {
        Err(ExportAcceptanceError::ObservationMismatch { target })
    }
}

fn valid_schema_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn product_attempt(
    scope: &str,
    target: ProjectExportBinaryTarget,
    manifest_digest: ContentDigest,
    startup_content_digest: ContentDigest,
) -> String {
    let sequence = NEXT_PRODUCT_ATTEMPT.fetch_add(1, Ordering::Relaxed);
    let predecessor = observation_predecessor(manifest_digest, startup_content_digest);
    format!(
        "{scope}-{}-{sequence}-{}-{}",
        std::process::id(),
        target.cargo_target_name(),
        digest_hex(predecessor),
    )
}

fn observation_predecessor(
    manifest_digest: ContentDigest,
    startup_content_digest: ContentDigest,
) -> ContentDigest {
    let mut facts = [0_u8; 80];
    facts[..32].copy_from_slice(manifest_digest.as_bytes());
    facts[32..40].copy_from_slice(&manifest_digest.length().to_le_bytes());
    facts[40..72].copy_from_slice(startup_content_digest.as_bytes());
    facts[72..].copy_from_slice(&startup_content_digest.length().to_le_bytes());
    ContentDigest::of_bytes(&facts)
}

pub(super) const fn product_profile(target: ProjectExportBinaryTarget) -> &'static str {
    match target {
        ProjectExportBinaryTarget::Headless => "default",
        ProjectExportBinaryTarget::Desktop => "desktop",
    }
}

pub(super) fn digest_hex(digest: ContentDigest) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in digest.as_bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

struct AcceptanceWorkingDirectory {
    path: PathBuf,
}

impl AcceptanceWorkingDirectory {
    fn new() -> Result<Self, ExportAcceptanceError> {
        for _ in 0..16 {
            let sequence = NEXT_ACCEPTANCE_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "nara-export-acceptance-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(ExportAcceptanceError::CreateWorkingDirectory(error));
                }
            }
        }
        Err(ExportAcceptanceError::CreateWorkingDirectory(
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "bounded acceptance working-directory names are exhausted",
            ),
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for AcceptanceWorkingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn product_observation_binds_the_exact_startup_closure_plan() {
        let target = ProjectExportBinaryTarget::Headless;
        let attempt = "export-observation-contract";
        let executable = ContentDigest::of_bytes(b"executable");
        let manifest = ContentDigest::of_bytes(b"manifest");
        let content = ContentDigest::of_bytes(b"content");
        let closure_plan = ContentDigest::of_bytes(b"closure-plan");
        let bundle = ContentDigest::of_bytes(b"authoring-bundle");
        let schema_fingerprint = "ab".repeat(32);
        let observation = json!({
            "version": 2,
            "attempt": attempt,
            "target": "headless",
            "profile": "default",
            "executable_length": executable.length(),
            "executable_blake3": digest_hex(executable),
            "manifest_length": manifest.length(),
            "manifest_blake3": digest_hex(manifest),
            "content_length": content.length(),
            "content_blake3": digest_hex(content),
            "closure_plan_length": closure_plan.length(),
            "closure_plan_blake3": digest_hex(closure_plan),
            "schema_fingerprint": schema_fingerprint,
            "authoring_bundle_length": bundle.length(),
            "authoring_bundle_blake3": digest_hex(bundle),
        });
        let bytes = serde_json::to_vec(&observation).unwrap();
        assert_eq!(
            validate_observation(
                target,
                attempt,
                executable,
                manifest,
                content,
                closure_plan,
                bundle,
                &bytes,
            )
            .unwrap(),
            "ab".repeat(32),
        );

        let mut mismatched = observation;
        mismatched["closure_plan_blake3"] = json!("00".repeat(32));
        assert!(matches!(
            validate_observation(
                target,
                attempt,
                executable,
                manifest,
                content,
                closure_plan,
                bundle,
                &serde_json::to_vec(&mismatched).unwrap(),
            ),
            Err(ExportAcceptanceError::ObservationMismatch { target: actual }) if actual == target
        ));
    }
}
