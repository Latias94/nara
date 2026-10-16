use std::{
    fs,
    path::PathBuf,
    time::{Duration, UNIX_EPOCH},
};

#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
))]
use std::{thread, time::Instant};

use nara::fs::ContentDigest;

use super::{
    EXECUTABLE_NAME, ExecutableGenerationStore, GenerationBuildObservation,
    GenerationManifestInput, GenerationPackageFacts, GenerationSourceFacts, GenerationStoreError,
    GenerationTargetFacts, MAX_SCAVENGE_WORK_DEPTH, SCAVENGE_MIN_AGE, STORE_NAMESPACE,
    measure_valid_store_tree,
};

const EXACT_CWD_PROBE: &str = "exact-generation-cwd-probe";

#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
))]
use crate::supervised_play::process::{
    SupervisedProcessOwner, SupervisedProcessState, SupervisedProductEnvironmentPolicy,
    SupervisedProductProcessCommand, default_log_limits,
};

#[test]
fn published_generation_is_immutable_when_the_cargo_artifact_changes() {
    let fixture = Fixture::new("immutable");
    let artifact = fixture.artifact(b"first executable");
    let mut store = ExecutableGenerationStore::create(&fixture.root).unwrap();
    let first = store
        .publish_candidate(&artifact, manifest_input(b"receipt-a", vec!["desktop"]))
        .unwrap();
    store.activate_candidate(first.id()).unwrap();
    store.admit_ready(first.id()).unwrap();

    fs::write(&artifact, b"second executable").unwrap();
    let second = store
        .publish_candidate(&artifact, manifest_input(b"receipt-b", vec!["desktop"]))
        .unwrap();

    assert_eq!(first.source_receipt(), b"receipt-a");
    assert_eq!(second.source_receipt(), b"receipt-b");
    assert_eq!(fs::read(first.executable()).unwrap(), b"first executable");
    assert_eq!(fs::read(second.executable()).unwrap(), b"second executable");
    assert_ne!(first.executable_digest(), second.executable_digest());
    assert!(first.executable().exists());
}

#[test]
fn unchanged_generation_revalidates_into_an_opaque_launch_authority() {
    let fixture = Fixture::new("launch-revalidation");
    let artifact = fixture.artifact(b"launchable executable");
    let mut store = ExecutableGenerationStore::create(&fixture.root).unwrap();
    let generation = store
        .publish_candidate(&artifact, manifest_input(b"receipt", vec!["desktop"]))
        .unwrap();

    let authority = generation.revalidate_for_launch().unwrap().into_authority();

    assert_eq!(
        generation.product_identity().as_str(),
        "cargo-product/v1/package@0.1.0/desktop"
    );
    assert_eq!(generation.source_receipt(), b"receipt");
    drop(authority);
}

#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
))]
#[test]
fn exact_launch_authority_survives_generation_path_replacement() {
    let fixture = Fixture::new("exact-authority-path-replacement");
    let artifact = fixture.artifact_from_path(&std::env::current_exe().unwrap());
    let mut store = ExecutableGenerationStore::create(&fixture.root).unwrap();
    let generation = store
        .publish_candidate(&artifact, manifest_input(b"receipt", vec!["desktop"]))
        .unwrap();
    let authority = generation.revalidate_for_launch().unwrap().into_authority();

    let original_directory = generation.directory.clone();
    let moved_directory = original_directory.with_file_name(format!(
        "{}.moved",
        original_directory.file_name().unwrap().to_string_lossy()
    ));
    #[cfg(target_os = "linux")]
    {
        fs::rename(&original_directory, &moved_directory).unwrap();
        fs::create_dir(&original_directory).unwrap();
        let replacement = original_directory.join(EXECUTABLE_NAME);
        fs::write(&replacement, b"path replacement must never execute").unwrap();
        make_executable(&replacement);
        fs::write(
            original_directory.join(super::LAUNCH_MANIFEST_NAME),
            b"replacement manifest",
        )
        .unwrap();
    }
    #[cfg(target_os = "windows")]
    assert!(
        fs::rename(&original_directory, &moved_directory).is_err(),
        "the restrictive generation-directory lease must deny replacement while authority lives"
    );

    let command = SupervisedProductProcessCommand::new(
        SupervisedProductEnvironmentPolicy::AllowlistedDesktopSession,
        fixture.root.clone(),
    )
    .arguments([
        "--exact",
        "supervised_play::generation::store::tests::exact_generation_fd_helper",
        "--nocapture",
    ]);
    let mut owner =
        SupervisedProcessOwner::spawn_exact_generation(authority, &command, default_log_limits())
            .unwrap();
    drop(owner.take_bootstrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    while owner.state() != SupervisedProcessState::Complete {
        owner.advance(Instant::now()).unwrap();
        assert!(
            Instant::now() < deadline,
            "exact generation child timed out"
        );
        thread::sleep(Duration::from_millis(5));
    }

    let output = owner
        .logs()
        .iter()
        .map(|entry| entry.text().into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(output.contains("exact-generation-fd-helper"));
    assert!(output.contains(fixture.root.to_string_lossy().as_ref()));
    assert!(fixture.root.join(EXACT_CWD_PROBE).is_file());
    assert!(!original_directory.join(EXACT_CWD_PROBE).exists());
    assert!(!moved_directory.join(EXACT_CWD_PROBE).exists());
    #[cfg(target_os = "linux")]
    {
        assert!(output.contains("path-present=false"));
    }
    assert!(owner.cleanup_receipt().unwrap().exit().success());
}

#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
))]
#[test]
fn exact_generation_fd_helper() {
    if !std::env::args_os().any(|argument| argument == "--exact") {
        return;
    }
    fs::write(EXACT_CWD_PROBE, b"explicit product working directory").unwrap();
    println!(
        "exact-generation-fd-helper cwd={} path-present={}",
        std::env::current_dir().unwrap().display(),
        std::env::var_os("PATH").is_some()
    );
}

#[test]
fn changed_executable_fails_launch_revalidation() {
    let fixture = Fixture::new("changed-launch-executable");
    let artifact = fixture.artifact(b"published executable");
    let mut store = ExecutableGenerationStore::create(&fixture.root).unwrap();
    let generation = store
        .publish_candidate(&artifact, manifest_input(b"receipt", vec!["desktop"]))
        .unwrap();
    fs::write(generation.executable(), b"changed executable").unwrap();

    assert!(generation.revalidate_for_launch().is_err());
}

#[test]
fn replaced_manifest_fails_launch_revalidation_even_with_identical_bytes() {
    let fixture = Fixture::new("replaced-launch-manifest");
    let artifact = fixture.artifact(b"published executable");
    let mut store = ExecutableGenerationStore::create(&fixture.root).unwrap();
    let generation = store
        .publish_candidate(&artifact, manifest_input(b"receipt", vec!["desktop"]))
        .unwrap();
    let manifest = fs::read(generation.launch_manifest()).unwrap();
    fs::remove_file(generation.launch_manifest()).unwrap();
    fs::write(generation.launch_manifest(), manifest).unwrap();

    assert!(matches!(
        generation.revalidate_for_launch(),
        Err(GenerationStoreError::GenerationBindingChanged)
    ));
}

#[test]
fn retention_never_evicts_active_or_last_good() {
    let fixture = Fixture::new("retention");
    let artifact = fixture.artifact(b"one");
    let mut store = ExecutableGenerationStore::create(&fixture.root).unwrap();
    let first = store
        .publish_candidate(&artifact, manifest_input(b"one", vec!["desktop"]))
        .unwrap();
    store.activate_candidate(first.id()).unwrap();
    store.admit_ready(first.id()).unwrap();
    store.finish_active(first.id()).unwrap();

    fs::write(&artifact, b"two").unwrap();
    let second = store
        .publish_candidate(&artifact, manifest_input(b"two", vec!["desktop"]))
        .unwrap();
    store.activate_candidate(second.id()).unwrap();

    fs::write(&artifact, b"three").unwrap();
    let third = store
        .publish_candidate(&artifact, manifest_input(b"three", vec!["desktop"]))
        .unwrap();
    fs::write(&artifact, b"four").unwrap();
    let fourth = store
        .publish_candidate(&artifact, manifest_input(b"four", vec!["desktop"]))
        .unwrap();

    assert!(first.executable().exists());
    assert!(second.executable().exists());
    assert!(!third.executable().exists());
    assert!(fourth.executable().exists());

    store.admit_ready(second.id()).unwrap();
    assert!(!first.executable().exists());
    assert_eq!(store.last_good().unwrap().id(), second.id());
    assert_eq!(store.active().unwrap().id(), second.id());
    assert_eq!(store.candidate().unwrap().id(), fourth.id());
}

#[test]
fn build_publication_does_not_advance_last_good() {
    let fixture = Fixture::new("candidate-only");
    let artifact = fixture.artifact(b"candidate");
    let mut store = ExecutableGenerationStore::create(&fixture.root).unwrap();
    let candidate = store
        .publish_candidate(&artifact, manifest_input(b"receipt", vec!["desktop"]))
        .unwrap();

    assert_eq!(store.candidate().unwrap().id(), candidate.id());
    assert!(store.last_good().is_none());
    assert!(matches!(
        store.activate_last_good(),
        Err(GenerationStoreError::LastGoodUnavailable)
    ));
}

#[test]
fn last_good_reactivation_preserves_the_immutable_generation_sequence() {
    let fixture = Fixture::new("last-good-sequence");
    let artifact = fixture.artifact(b"last good executable");
    let mut store = ExecutableGenerationStore::create(&fixture.root).unwrap();
    let published = store
        .publish_candidate(&artifact, manifest_input(b"receipt", vec!["desktop"]))
        .unwrap();
    let published_sequence = published.sequence();

    store.activate_candidate(published.id()).unwrap();
    store.admit_ready(published.id()).unwrap();
    store.finish_active(published.id()).unwrap();
    let reactivated = store.activate_last_good().unwrap();

    assert_eq!(reactivated.id(), published.id());
    assert_eq!(reactivated.sequence(), published_sequence);
}

#[test]
fn non_executable_artifact_is_rejected_before_publication() {
    let fixture = Fixture::new("non-executable");
    let artifact = fixture.root.join("not-executable.txt");
    fs::write(&artifact, b"not executable").unwrap();
    let artifact = artifact.canonicalize().unwrap();
    let mut store = ExecutableGenerationStore::create(&fixture.root).unwrap();

    assert!(matches!(
        store.publish_candidate(&artifact, manifest_input(b"receipt", vec!["desktop"])),
        Err(GenerationStoreError::ArtifactNotExecutable)
    ));
    assert!(store.candidate().is_none());
}

#[test]
fn generation_publication_never_replaces_an_existing_destination() {
    let fixture = Fixture::new("no-replace");
    let artifact = fixture.artifact(b"immutable destination");
    let mut store = ExecutableGenerationStore::create(&fixture.root).unwrap();
    let generation = store
        .publish_candidate(&artifact, manifest_input(b"receipt", vec!["desktop"]))
        .unwrap();
    store.activate_candidate(generation.id()).unwrap();
    store.next_generation = 1;

    assert!(matches!(
        store.publish_candidate(&artifact, manifest_input(b"receipt", vec!["desktop"])),
        Err(GenerationStoreError::PublicationBegin(_))
    ));
    assert_eq!(
        fs::read(generation.executable()).unwrap(),
        b"immutable destination"
    );
}

#[test]
fn canonical_manifest_normalizes_feature_order_and_duplicates() {
    let fixture = Fixture::new("canonical");
    let artifact = fixture.artifact(b"same executable");
    let mut first_store = ExecutableGenerationStore::create(&fixture.root).unwrap();
    let first = first_store
        .publish_candidate(
            &artifact,
            manifest_input(b"same", vec!["zeta", "alpha", "alpha"]),
        )
        .unwrap();
    let first_bytes = fs::read(first.launch_manifest()).unwrap();

    let mut second_store = ExecutableGenerationStore::create(&fixture.root).unwrap();
    let second = second_store
        .publish_candidate(&artifact, manifest_input(b"same", vec!["alpha", "zeta"]))
        .unwrap();
    let second_bytes = fs::read(second.launch_manifest()).unwrap();

    // Generation identity is intentionally per run. The canonical payload after that fixed
    // identity field must otherwise be identical for an equivalent feature set.
    const GENERATION_ID_OFFSET: usize = 20 + 2;
    const GENERATION_ID_BYTES: usize = 32;
    assert_eq!(
        &first_bytes[GENERATION_ID_OFFSET + GENERATION_ID_BYTES..],
        &second_bytes[GENERATION_ID_OFFSET + GENERATION_ID_BYTES..]
    );
}

#[test]
fn close_requires_active_retirement_and_removes_the_owned_store() {
    let fixture = Fixture::new("close");
    let artifact = fixture.artifact(b"active");
    let mut store = ExecutableGenerationStore::create(&fixture.root).unwrap();
    let store_path = store.store_path.clone();
    let generation = store
        .publish_candidate(&artifact, manifest_input(b"receipt", vec!["desktop"]))
        .unwrap();
    store.activate_candidate(generation.id()).unwrap();
    let work_directory = store.begin_work_directory().unwrap();
    let runtime_log_directory = work_directory.join("NVIDIA Corporation").join("umdlogs");
    fs::create_dir_all(&runtime_log_directory).unwrap();
    fs::write(
        runtime_log_directory.join("runtime.log"),
        b"runtime side effect",
    )
    .unwrap();
    assert!(matches!(
        store.try_close(),
        Err(GenerationStoreError::ActiveGenerationExists)
    ));

    store.admit_ready(generation.id()).unwrap();
    store.finish_active(generation.id()).unwrap();
    assert!(matches!(
        store.try_close(),
        Err(GenerationStoreError::ActiveWorkDirectoryExists)
    ));
    store.finish_work_directory().unwrap();
    assert!(!work_directory.exists());
    let owner_marker = store.owner_marker.as_ref().unwrap();
    assert!(
        owner_marker.named_binding_is_current(),
        "the retained marker handle must still own the named marker"
    );
    assert!(
        measure_valid_store_tree(&store_path).is_some(),
        "the retained store must still match the bounded store grammar"
    );
    let receipt = store.try_close().unwrap();
    assert_eq!(receipt, super::GenerationStoreCloseReceipt);
    assert!(!store_path.exists());
}

#[test]
fn scavenger_skips_live_store_and_removes_old_inactive_store() {
    let fixture = Fixture::new("scavenge");
    let created = UNIX_EPOCH + Duration::from_secs(10);
    let artifact = fixture.artifact(b"inactive executable");
    let mut store = ExecutableGenerationStore::create_at(&fixture.root, created).unwrap();
    let store_path = store.store_path.clone();
    let generation = store
        .publish_candidate(&artifact, manifest_input(b"receipt", vec!["desktop"]))
        .unwrap();
    store.activate_candidate(generation.id()).unwrap();
    let work_directory = store.begin_work_directory().unwrap();
    let runtime_log_directory = work_directory.join("NVIDIA Corporation").join("umdlogs");
    fs::create_dir_all(&runtime_log_directory).unwrap();
    fs::write(
        runtime_log_directory.join("runtime.log"),
        b"runtime side effect",
    )
    .unwrap();
    let now = created + SCAVENGE_MIN_AGE + Duration::from_secs(1);

    let live = ExecutableGenerationStore::scavenge_inactive_at(&fixture.root, now).unwrap();
    assert_eq!(live.skipped_live(), 1);
    assert_eq!(live.removed(), 0);
    drop(store);

    let inactive = ExecutableGenerationStore::scavenge_inactive_at(&fixture.root, now).unwrap();
    assert_eq!(inactive.removed(), 1);
    assert!(!store_path.exists());
}

#[test]
fn scavenger_refuses_an_inactive_work_tree_beyond_the_depth_bound() {
    let fixture = Fixture::new("scavenge-work-depth");
    let created = UNIX_EPOCH + Duration::from_secs(10);
    let artifact = fixture.artifact(b"inactive executable");
    let mut store = ExecutableGenerationStore::create_at(&fixture.root, created).unwrap();
    let store_path = store.store_path.clone();
    let generation = store
        .publish_candidate(&artifact, manifest_input(b"receipt", vec!["desktop"]))
        .unwrap();
    store.activate_candidate(generation.id()).unwrap();
    let mut nested = store.begin_work_directory().unwrap();
    for _ in 0..=MAX_SCAVENGE_WORK_DEPTH {
        nested = nested.join("nested");
        fs::create_dir(&nested).unwrap();
    }
    drop(store);

    let now = created + SCAVENGE_MIN_AGE + Duration::from_secs(1);
    let receipt = ExecutableGenerationStore::scavenge_inactive_at(&fixture.root, now).unwrap();

    assert_eq!(receipt.removed(), 0);
    assert_eq!(receipt.skipped_unverified(), 1);
    assert!(store_path.exists());
}

fn manifest_input(receipt: &[u8], features: Vec<&str>) -> GenerationManifestInput {
    let digest = |value: &[u8]| ContentDigest::of_bytes(value);
    GenerationManifestInput::new(
        GenerationSourceFacts::new(
            "desktop-supervised-play/v1",
            "editor-launch-context/v1",
            "project-identity",
            "desktop",
            receipt.to_vec(),
        ),
        GenerationTargetFacts::new(
            GenerationPackageFacts::new(digest(b"package-id"), "package", "0.1.0", "Cargo.toml"),
            "desktop",
            "x86_64-pc-windows-msvc",
            "dev",
            features.into_iter().map(str::to_owned).collect(),
        ),
        GenerationBuildObservation::new(
            digest(b"manifest"),
            Some(digest(b"lock")),
            digest(b"metadata"),
            digest(b"cargo version"),
            digest(b"rustc -vV"),
            digest(b"build invocation"),
        ),
    )
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let unique = super::NEXT_STORE_NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nara-generation-test-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn artifact(&self, bytes: &[u8]) -> PathBuf {
        let path = self.root.join(EXECUTABLE_NAME);
        fs::write(&path, bytes).unwrap();
        make_executable(&path);
        path.canonicalize().unwrap()
    }

    #[cfg(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64")
    ))]
    fn artifact_from_path(&self, source: &std::path::Path) -> PathBuf {
        let path = self.root.join(EXECUTABLE_NAME);
        fs::copy(source, &path).unwrap();
        make_executable(&path);
        path.canonicalize().unwrap()
    }
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) {}

impl Drop for Fixture {
    fn drop(&mut self) {
        let expected_namespace = self.root.join(STORE_NAMESPACE);
        if expected_namespace.parent() == Some(self.root.as_path()) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
