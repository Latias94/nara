use std::{
    fmt,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    diagnostic::DiagnosticReport,
    fs::{
        CapabilityRights, DirectoryCapability, FileCapability, FsError, FsOperation,
        HostCapabilityOptions, RelativePath, TrustMode,
    },
    project::{DEFAULT_MANIFEST_BYTE_LIMIT, ProductCapability, RuntimePreset},
    project_host::{ProjectCandidateError, ProjectCandidateErrorKind, ingest_project_manifest},
};
use tracing::{Event, Metadata, Subscriber, field::Visit, span};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestManifest {
    root: PathBuf,
    path: PathBuf,
}

impl TestManifest {
    fn new(bytes: &[u8]) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nara_project_composition_{}_{}_{}",
            std::process::id(),
            stamp,
            sequence
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("nara.toml");
        fs::write(&path, bytes).unwrap();
        Self { root, path }
    }

    fn capability(&self) -> FileCapability {
        FileCapability::from_host_handle(
            File::open(&self.path).unwrap(),
            TrustMode::TrustedLocal,
            1,
        )
        .unwrap()
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for TestManifest {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn minimal_manifest_publishes_an_immutable_runtime_core_candidate() {
    let manifest = TestManifest::new(minimal_manifest().as_bytes());
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();

    assert_eq!(candidate.settings().project.name, "Capability Test");
    assert_eq!(candidate.settings().runtime_preset, RuntimePreset::Minimal);
    assert!(candidate.explicit_capabilities().is_empty());
    assert!(
        candidate
            .implied_capabilities()
            .contains(ProductCapability::RuntimeCore)
    );
    assert!(
        candidate
            .required_capabilities()
            .contains(ProductCapability::RuntimeCore)
    );
    assert_eq!(
        candidate.normalized_capabilities(),
        candidate.required_capabilities()
    );
}

#[test]
fn server_preset_rejects_client_capabilities_before_publication() {
    let manifest = TestManifest::new(
        br#"
schema_version = 1

[project]
name = "Server Conflict"

[runtime]
preset = "server"

[capabilities]
requested = ["runtime-2d", "render-wgpu"]
"#,
    );

    let error = ingest_project_manifest(&manifest.capability(), None).unwrap_err();

    assert_eq!(error.kind(), ProjectCandidateErrorKind::PresetConflict);
    assert!(!format!("{error:?}").contains(manifest.path().to_string_lossy().as_ref()));
}

#[cfg(not(feature = "tooling-egui"))]
#[test]
fn implied_capability_is_rejected_when_the_compiled_ceiling_is_too_small() {
    let manifest = TestManifest::new(
        br#"
schema_version = 1

[project]
name = "Unavailable Tooling"

[capabilities]
requested = ["tooling-egui"]
"#,
    );

    let error = ingest_project_manifest(&manifest.capability(), None).unwrap_err();

    assert_eq!(
        error.kind(),
        ProjectCandidateErrorKind::UnavailableCapabilities
    );
    let rendered = format!("{error:?}");
    assert!(!rendered.contains("Unavailable Tooling"));
    assert!(!rendered.contains(manifest.path().to_string_lossy().as_ref()));
}

#[test]
fn manifest_byte_limit_rejects_the_sentinel_byte() {
    let limit = usize::try_from(DEFAULT_MANIFEST_BYTE_LIMIT).unwrap();
    let manifest = TestManifest::new(&vec![b' '; limit + 1]);

    let error = ingest_project_manifest(&manifest.capability(), None).unwrap_err();

    assert_eq!(error.kind(), ProjectCandidateErrorKind::ManifestTooLarge);
}

#[test]
fn invalid_utf8_and_toml_shape_publish_no_candidate() {
    let invalid_utf8 = TestManifest::new(&[0xff, 0xfe, 0xfd]);
    let invalid_toml = TestManifest::new(
        br#"
schema_version = 1
[project]
name = ["not", "a", "string"]
"#,
    );

    let utf8_error = ingest_project_manifest(&invalid_utf8.capability(), None).unwrap_err();
    let toml_error = ingest_project_manifest(&invalid_toml.capability(), None).unwrap_err();

    assert_eq!(utf8_error.kind(), ProjectCandidateErrorKind::Manifest);
    assert_eq!(toml_error.kind(), ProjectCandidateErrorKind::Manifest);
}

#[test]
fn a_failed_candidate_does_not_prevent_a_later_valid_ingest() {
    let invalid = TestManifest::new(b"not = [valid");
    let valid = TestManifest::new(minimal_manifest().as_bytes());

    assert!(ingest_project_manifest(&invalid.capability(), None).is_err());
    let candidate = ingest_project_manifest(&valid.capability(), None).unwrap();

    assert_eq!(candidate.settings().project.name, "Capability Test");
}

#[test]
fn manifest_authority_errors_lower_to_stable_private_diagnostics() {
    const CAPABILITY_CANARY: &str = "NARA_AUTHORITY_CAPABILITY_CANARY";
    const PROOF_CANARY: &str = "NARA_AUTHORITY_PROOF_CANARY";
    const IO_CANARY: &str = "NARA_AUTHORITY_IO_CANARY";
    let cases = vec![
        (
            FsError::Unsupported {
                operation: FsOperation::OpenFile,
                capability: CAPABILITY_CANARY,
            },
            ProjectCandidateErrorKind::HostAuthorityUnsupported,
            "project.manifest.authority-unsupported",
            vec![CAPABILITY_CANARY],
        ),
        (
            FsError::Unproven {
                operation: FsOperation::InspectHandle,
                proof: PROOF_CANARY,
            },
            ProjectCandidateErrorKind::HostAuthorityUnproven,
            "project.manifest.authority-unproven",
            vec![PROOF_CANARY],
        ),
        (
            FsError::ReparsePoint { tag: 0xdead_beef },
            ProjectCandidateErrorKind::HostAuthorityRejected,
            "project.manifest.authority-rejected",
            vec!["deadbeef", "3735928559"],
        ),
        (
            FsError::SymbolicLinkTraversal,
            ProjectCandidateErrorKind::HostAuthorityRejected,
            "project.manifest.authority-rejected",
            vec![],
        ),
        (
            FsError::MultipleLinks { links: 4_242_424 },
            ProjectCandidateErrorKind::HostAuthorityRejected,
            "project.manifest.authority-rejected",
            vec!["4242424"],
        ),
        (
            FsError::Io {
                operation: FsOperation::OpenFile,
                source: io::Error::new(io::ErrorKind::PermissionDenied, IO_CANARY),
            },
            ProjectCandidateErrorKind::HostIo,
            "project.manifest.host-io",
            vec![IO_CANARY],
        ),
    ];

    for (source, expected_kind, expected_code, hidden) in cases {
        let error = ProjectCandidateError::from_manifest_authority(source);
        assert_eq!(error.kind(), expected_kind);
        let diagnostics = error.diagnostics().iter().collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code().as_str(), expected_code);
        assert_error_sinks_hide(&error, &hidden);
    }
}

#[test]
fn absolute_manifest_path_is_rejected_without_retaining_the_path() {
    const PATH_CANARY: &str = "NARA_ABSOLUTE_MANIFEST_CANARY.toml";
    let absolute = std::env::current_dir().unwrap().join(PATH_CANARY);
    let source = RelativePath::new(&absolute).unwrap_err();

    let error = ProjectCandidateError::from_manifest_authority(source.into());

    assert_eq!(
        error.kind(),
        ProjectCandidateErrorKind::HostAuthorityRejected
    );
    assert_error_sinks_hide(&error, &[PATH_CANARY, absolute.to_string_lossy().as_ref()]);
}

#[test]
fn missing_manifest_open_is_private_and_does_not_poison_later_ingest() {
    const MISSING_CANARY: &str = "NARA_MISSING_MANIFEST_CANARY.toml";
    let manifest = TestManifest::new(minimal_manifest().as_bytes());
    let root = DirectoryCapability::from_host_handle(
        host_directory(manifest.root()),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
    )
    .unwrap();
    let source = root
        .open_file(&RelativePath::new(Path::new(MISSING_CANARY)).unwrap())
        .unwrap_err();

    let error = ProjectCandidateError::from_manifest_authority(source);

    assert_eq!(error.kind(), ProjectCandidateErrorKind::HostIo);
    assert_error_sinks_hide(&error, &[MISSING_CANARY]);
    let candidate = ingest_project_manifest(&manifest.capability(), None).unwrap();
    assert_eq!(candidate.settings().project.name, "Capability Test");
}

fn assert_error_sinks_hide(error: &ProjectCandidateError, hidden: &[&str]) {
    let debug = format!("{error:?}");
    let display = error.to_string();
    let tracing = capture_tracing(error.diagnostics());
    for value in hidden {
        assert!(!debug.contains(value), "Debug leaked {value:?}: {debug}");
        assert!(
            !display.contains(value),
            "Display leaked {value:?}: {display}"
        );
        assert!(
            !tracing.contains(value),
            "tracing leaked {value:?}: {tracing}"
        );
    }

    #[cfg(feature = "serde")]
    {
        let serialized = serde_json::to_string(error.diagnostics()).unwrap();
        for value in hidden {
            assert!(
                !serialized.contains(value),
                "serialization leaked {value:?}: {serialized}"
            );
        }
    }
}

fn capture_tracing(diagnostics: &DiagnosticReport) -> String {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let subscriber = RecordingSubscriber::new(Arc::clone(&captured));
    tracing::subscriber::with_default(subscriber, || diagnostics.emit_to_tracing());
    captured.lock().unwrap().join("\n")
}

#[derive(Clone)]
struct RecordingSubscriber {
    events: Arc<Mutex<Vec<String>>>,
}

impl RecordingSubscriber {
    fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

#[derive(Default)]
struct FieldRecorder {
    output: String,
}

impl Visit for FieldRecorder {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        use fmt::Write as _;
        let _ = write!(self.output, "{}={value:?};", field.name());
    }
}

impl Subscriber for RecordingSubscriber {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }

    fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut recorder = FieldRecorder::default();
        event.record(&mut recorder);
        self.events.lock().unwrap().push(recorder.output);
    }

    fn enter(&self, _span: &span::Id) {}

    fn exit(&self, _span: &span::Id) {}
}

fn host_directory(path: &Path) -> File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap()
    }

    #[cfg(unix)]
    {
        File::open(path).unwrap()
    }
}

fn minimal_manifest() -> &'static str {
    r#"
schema_version = 1

[project]
name = "Capability Test"
"#
}
