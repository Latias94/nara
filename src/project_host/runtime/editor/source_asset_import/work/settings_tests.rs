use std::{cell::Cell, collections::BTreeSet};

use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
use serde::{Deserialize, Serialize};

use super::*;
use crate::project_content::{ProjectContentBudgetKind, ProjectContentLimits};

thread_local! {
    // Typed admissions, successor validations, source preparations.
    static CALLS: Cell<[usize; 3]> = const { Cell::new([0; 3]) };
}

fn called(index: usize) {
    CALLS.with(|calls| {
        let mut counts = calls.get();
        counts[index] += 1;
        calls.set(counts);
    });
}

#[derive(Default, Serialize)]
struct Settings {
    live: BTreeSet<u32>,
    retired: BTreeSet<u32>,
}

impl<'de> Deserialize<'de> for Settings {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            live: BTreeSet<u32>,
            retired: BTreeSet<u32>,
        }

        called(0);
        let fields = Fields::deserialize(deserializer)?;
        Ok(Self {
            live: fields.live,
            retired: fields.retired,
        })
    }
}

fn settings(live: &[u32], retired: &[u32]) -> Settings {
    Settings {
        live: live.iter().copied().collect(),
        retired: retired.iter().copied().collect(),
    }
}

fn invalid_lineage() -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new("test.source-import-lineage").unwrap(),
        SafeSummary::new("The authored product lineage was not preserved").unwrap(),
    )
}

struct LineageDomain;

impl nara_asset::AssetDomain for LineageDomain {
    type Product = ();
}

impl SourceAssetImporter for LineageDomain {
    type Settings = Settings;
    const SOURCE_KIND: &'static str = "test.source-lineage";

    fn limits() -> AssetImportLimits {
        AssetImportLimits::new(
            ByteLimit::new(4096).unwrap(),
            ByteLimit::new(64 * 1024).unwrap(),
            ByteLimit::new(4096).unwrap(),
        )
    }

    fn prepare<E>(source: &mut AssetImportSource<'_, E>) -> Result<(), AssetImportError<E>> {
        called(2);
        let admitted = source
            .metadata()
            .admit::<Settings>()
            .map_err(|_| AssetImportError::domain(invalid_lineage()))?;
        if !admitted
            .settings()
            .live
            .is_disjoint(&admitted.settings().retired)
        {
            return Err(AssetImportError::domain(invalid_lineage()));
        }
        assert_eq!(source.bytes(), b"source import return fixture");
        Ok(())
    }
}

impl EditorSourceAssetDomain for LineageDomain {
    type Domain = Self;

    fn default_settings() -> Settings {
        Settings::default()
    }

    fn validate_settings_successor(
        previous: &Settings,
        candidate: &Settings,
    ) -> Result<(), Diagnostic> {
        called(1);
        if !previous.retired.is_subset(&candidate.retired)
            || !previous.retired.is_disjoint(&candidate.live)
            || previous
                .live
                .difference(&candidate.live)
                .any(|id| !candidate.retired.contains(id))
        {
            return Err(invalid_lineage());
        }
        Ok(())
    }
}

struct PanickingFacet;

impl EditorSourceAssetDomain for PanickingFacet {
    type Domain = LineageDomain;

    fn default_settings() -> Settings {
        Settings::default()
    }

    fn validate_settings_successor(_: &Settings, _: &Settings) -> Result<(), Diagnostic> {
        called(1);
        panic!("private successor panic payload");
    }
}

fn encode(settings: &Settings) -> String {
    AssetMeta::new(
        AssetMetaIdentity::new(
            stable_id(),
            AssetPath::new(SOURCE).unwrap(),
            AssetSourceKind::Other(LineageDomain::SOURCE_KIND.into()),
        ),
        settings,
    )
    .to_json_string()
    .unwrap()
}

fn write_previous(fixture: &Fixture) -> Vec<u8> {
    let mut bytes = encode(&settings(&[1, 2], &[3])).into_bytes();
    bytes.push(b'\n');
    fs::write(fixture.root.join(METADATA), &bytes).unwrap();
    bytes
}

fn queue<D: EditorSourceAssetDomain<Domain = LineageDomain>>(
    fixture: &Fixture,
    pools: &TaskPools,
    candidate: &Settings,
    budget: &ProjectContentBudgetHost,
) -> Result<PendingSourceImport, Rejection> {
    CALLS.with(|calls| calls.set([0; 3]));
    let assets = DirectoryCapability::from_host_handle(
        open_directory(&fixture.root),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap();
    let expected_source = assets
        .open_file_exact(&RelativePath::new(SOURCE).unwrap())
        .unwrap()
        .identity();
    let file = assets
        .open_file_exact(&RelativePath::new(METADATA).unwrap())
        .unwrap();
    let byte_limit = AssetMetaFileLimits::default().encoded_bytes().get() as u64;
    let bytes = file.read_to_end_bounded(byte_limit).unwrap();
    let digest = ContentDigest::of_bytes(&bytes);
    let previous =
        AssetMetaIdentityCandidate::decode_json_bytes_owned(bytes.into_boxed_slice()).unwrap();
    let publication =
        FilePublicationHost::bind_existing(&assets, METADATA, &file, digest, byte_limit).unwrap();
    let encoded = encode(candidate);
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(encoded.as_bytes()).unwrap();
    PendingSourceImport::start(
        pools,
        HostCustody::default()
            .retain_publication(publication)
            .unwrap(),
        SourceImportWork {
            provider: Arc::new(TypedImporter::<D>(PhantomData)),
            assets,
            metadata,
            previous: Some(previous),
            encoded,
            expected_source,
            budget: budget.clone(),
            gate: None,
        },
    )
}

fn run(pools: &TaskPools, pending: &mut PendingSourceImport) -> EditorSourceAssetImportAttempt {
    assert_eq!(pools.run_pending_for_tests().executed, 1);
    pending
        .poll()
        .expect("the source import physically returned")
}

fn assert_failure(outcome: EditorSourceAssetImportAttempt, code: &str) {
    let EditorSourceAssetImportAttempt::Failed { diagnostics } = outcome else {
        panic!("expected a rejected source import");
    };
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics.iter().next().unwrap().code().as_str(), code);
    assert!(!format!("{diagnostics:?}").contains("private successor panic payload"));
}

#[test]
fn rejected_successors_preserve_exact_metadata_without_preparing_or_publishing() {
    // Each candidate has disjoint live/retired sets and passes standalone validation.
    for candidate in [
        settings(&[1], &[2]),
        settings(&[1], &[3]),
        settings(&[1, 3], &[2]),
    ] {
        let fixture = Fixture::new();
        let original = write_previous(&fixture);
        let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
        let budget = ProjectContentBudgetHost::new(Default::default());
        let mut pending = queue::<LineageDomain>(&fixture, &pools, &candidate, &budget).unwrap();
        assert_failure(run(&pools, &mut pending), "test.source-import-lineage");
        assert_eq!(CALLS.with(Cell::get), [2, 1, 0]);
        assert_eq!(
            budget
                .snapshot()
                .high_water(ProjectContentBudgetKind::Files),
            0
        );
        assert!(!pending.result.lock().unwrap().publication_started);
        assert_eq!(fs::read(fixture.root.join(METADATA)).unwrap(), original);
        drop(pending);
        assert_eq!(budget.snapshot().active_reservations(), 0);
        assert!(pools.shutdown_blocking().unwrap().is_complete());
    }
}

#[test]
fn allowed_successor_releases_transition_work_before_source_preparation_and_publishes() {
    let fixture = Fixture::new();
    write_previous(&fixture);
    let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let limits = LineageDomain::limits();
    let peak = limits.work_bytes().get() * 2 + limits.retained_bytes().get();
    let budget = ProjectContentBudgetHost::new(
        ProjectContentLimits::default()
            .with_limit(ProjectContentBudgetKind::WorkBytes, peak)
            .unwrap(),
    );
    let candidate = settings(&[1], &[2, 3]);
    let encoded = encode(&candidate);
    let mut pending = queue::<LineageDomain>(&fixture, &pools, &candidate, &budget).unwrap();
    assert_eq!(
        run(&pools, &mut pending),
        EditorSourceAssetImportAttempt::MetadataPublished {
            stable_id: stable_id()
        }
    );
    assert_eq!(CALLS.with(Cell::get), [3, 1, 1]);
    assert_eq!(
        budget
            .snapshot()
            .high_water(ProjectContentBudgetKind::WorkBytes),
        peak
    );
    assert_eq!(
        fs::read(fixture.root.join(METADATA)).unwrap(),
        encoded.as_bytes()
    );
    drop(pending);
    assert_eq!(budget.snapshot().active_reservations(), 0);
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn transition_panic_is_bounded_and_does_not_prepare_or_publish() {
    let fixture = Fixture::new();
    let original = write_previous(&fixture);
    let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let budget = ProjectContentBudgetHost::new(Default::default());
    let mut pending =
        queue::<PanickingFacet>(&fixture, &pools, &settings(&[1], &[2, 3]), &budget).unwrap();
    assert_failure(run(&pools, &mut pending), "project.content.domain-panicked");
    assert_eq!(CALLS.with(Cell::get), [2, 1, 0]);
    assert!(!pending.result.lock().unwrap().publication_started);
    assert_eq!(fs::read(fixture.root.join(METADATA)).unwrap(), original);
    drop(pending);
    assert_eq!(budget.snapshot().active_reservations(), 0);
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn transition_does_not_bypass_existing_source_or_metadata_object_fences() {
    for replaced in [SOURCE, METADATA] {
        let fixture = Fixture::new();
        let original = write_previous(&fixture);
        let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
        let budget = ProjectContentBudgetHost::new(Default::default());
        let mut pending =
            queue::<LineageDomain>(&fixture, &pools, &settings(&[1], &[2, 3]), &budget).unwrap();
        let target = fixture.root.join(replaced);
        let bytes = fs::read(&target).unwrap();
        fs::rename(&target, fixture.root.join(format!("{replaced}.previous"))).unwrap();
        fs::write(&target, bytes).unwrap();
        let code = if replaced == SOURCE {
            "project.editor.source-import-source-changed"
        } else {
            "project.editor.source-import-publication-failed"
        };
        assert_failure(run(&pools, &mut pending), code);
        assert_eq!(fs::read(fixture.root.join(METADATA)).unwrap(), original);
        assert!(pools.shutdown_blocking().unwrap().is_complete());
    }
}

#[test]
fn transition_work_is_reserved_before_either_typed_settings_admission() {
    let fixture = Fixture::new();
    let original = write_previous(&fixture);
    let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let limits = LineageDomain::limits();
    let peak = limits.work_bytes().get() * 2 + limits.retained_bytes().get();
    let budget = ProjectContentBudgetHost::new(
        ProjectContentLimits::default()
            .with_limit(ProjectContentBudgetKind::WorkBytes, peak - 1)
            .unwrap(),
    );
    let mut pending =
        queue::<LineageDomain>(&fixture, &pools, &settings(&[1], &[2, 3]), &budget).unwrap();
    assert_failure(run(&pools, &mut pending), "project.content.budget-exceeded");
    assert_eq!(CALLS.with(Cell::get), [0; 3]);
    assert!(!pending.result.lock().unwrap().publication_started);
    assert_eq!(fs::read(fixture.root.join(METADATA)).unwrap(), original);
    drop(pending);
    assert_eq!(budget.snapshot().active_reservations(), 0);
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn queued_metadata_budget_rejects_before_spawn_and_survives_cancelled_owner_disposal() {
    let fixture = Fixture::new();
    let original = write_previous(&fixture);
    let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let budget = ProjectContentBudgetHost::new(
        ProjectContentLimits::default()
            .with_limit(ProjectContentBudgetKind::RetainedBytes, 1)
            .unwrap(),
    );
    assert!(matches!(
        queue::<LineageDomain>(&fixture, &pools, &settings(&[1], &[2, 3]), &budget),
        Err(Rejection::Busy)
    ));
    assert_eq!(CALLS.with(Cell::get), [0; 3]);
    assert_eq!(pools.run_pending_for_tests().executed, 0);
    assert_eq!(budget.snapshot().active_reservations(), 0);

    let budget = ProjectContentBudgetHost::new(Default::default());
    let pending =
        queue::<LineageDomain>(&fixture, &pools, &settings(&[1], &[2, 3]), &budget).unwrap();
    let retained = budget
        .snapshot()
        .active(ProjectContentBudgetKind::RetainedBytes);
    assert!(retained > original.len() + encode(&settings(&[1], &[2, 3])).len() * 2);
    pending.cancel();
    drop(pending);
    assert_eq!(
        budget
            .snapshot()
            .active(ProjectContentBudgetKind::RetainedBytes),
        retained
    );
    assert_eq!(pools.run_pending_for_tests().cancelled_before_start, 1);
    assert_eq!(budget.snapshot().active_reservations(), 0);
    assert_eq!(CALLS.with(Cell::get), [0; 3]);
    assert_eq!(fs::read(fixture.root.join(METADATA)).unwrap(), original);
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}
