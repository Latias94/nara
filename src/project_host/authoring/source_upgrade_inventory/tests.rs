use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use nara_asset::{AssetMeta, AssetMetaIdentity, AssetPath, StableAssetId};
use nara_core::ByteLimit;
use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
use nara_reflect::{AuthoringSchemaBundle, AuthoringSchemaBundleOwner};
use nara_scene::{AuthoringSchemaBasis, PrefabDocument, SceneDocument, SourceDocumentOrigin};
use nara_tasks::{TaskPoolConfig, TaskPools};

use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionHost, AuthoringProjectAdmissionLimits,
    document::{AuthoringDocumentAdmissionBasis, AuthoringDocumentKind},
    source_index::ProjectSourceIndexState,
};

const FIRST_ID: &str = "ad322a15-01a2-4c04-847f-83e4df33f139";
const SECOND_ID: &str = "fda28046-1e3b-4a26-bd80-bc50c7180fc0";
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

mod custody;
mod preparation_inputs;
mod successor;

#[test]
fn includes_default_and_every_profile_even_when_receipt_selected_another_profile() {
    let project = Fixture::new(
        "project_location",
        r#"
[content]
scenes = ["content.scene.json"]
[startup]
default_scene = "startup.scene.json"
[profiles.alpha.paths]
assets = "alpha/assets"
scenes = "alpha/scenes"
prefabs = "alpha/prefabs"
[profiles.beta.paths]
scenes = "beta/scenes"
prefabs = "beta/prefabs"
"#,
    );
    for prefix in ["scenes", "alpha/scenes", "beta/scenes"] {
        for file in ["content.scene.json", "startup.scene.json"] {
            project.scene(&format!("{prefix}/{file}"));
        }
    }
    project.prefab("prefabs/base.prefab.ron");
    project.prefab("alpha/prefabs/alpha.prefab.ron");
    project.prefab("beta/prefabs/beta.prefab.ron");
    project.scene("alpha/assets/catalogued.json");
    project.metadata(
        "alpha/assets/catalogued.json",
        "catalogued.json",
        FIRST_ID,
        AssetSourceKind::Scene,
        0,
    );
    let schema = project.receipt(Some("beta"));
    let inventory = capture(&schema);
    assert_eq!(
        paths(&inventory),
        [
            "alpha/assets/catalogued.json",
            "alpha/prefabs/alpha.prefab.ron",
            "alpha/scenes/content.scene.json",
            "alpha/scenes/startup.scene.json",
            "beta/prefabs/beta.prefab.ron",
            "beta/scenes/content.scene.json",
            "beta/scenes/startup.scene.json",
            "prefabs/base.prefab.ron",
            "scenes/content.scene.json",
            "scenes/startup.scene.json",
        ]
    );
    let mut budget = schema.budget;
    inventory.validate(&schema, &[], &mut budget).unwrap();
}

#[test]
fn catalogue_documents_outside_suffix_listing_use_the_existing_native_identity_policy() {
    let project = Fixture::new("stable_metadata", "");
    project.scene("assets/level.json");
    project.prefab("assets/enemy.ron");
    project.metadata(
        "assets/level.json",
        "level.json",
        FIRST_ID,
        AssetSourceKind::Scene,
        0,
    );
    project.metadata(
        "assets/enemy.ron",
        "enemy.ron",
        SECOND_ID,
        AssetSourceKind::Prefab,
        0,
    );
    let schema = project.receipt(None);
    let inventory = capture(&schema);
    assert_eq!(paths(&inventory), ["assets/enemy.ron", "assets/level.json"]);
    assert_eq!(inventory.metadata().len(), 2);
    for member in inventory.participants() {
        let expected = if member.kind() == SourceDocumentKind::Scene {
            FIRST_ID
        } else {
            SECOND_ID
        };
        assert_eq!(
            member.source_key().origin(),
            &SourceDocumentOrigin::StableAsset {
                stable_id: StableAssetId::parse_str(expected).unwrap(),
            }
        );
        assert_eq!(
            ContentDigest::of_bytes(member.bytes()),
            member.file().digest()
        );
        assert_eq!(member.file().encoded_bytes(), member.bytes().len() as u64);
    }
}

#[test]
fn overlapping_roots_deduplicate_exact_files_but_never_guess_an_ambiguous_kind() {
    let project = Fixture::new(
        "project_location",
        r#"
[paths]
assets = "content"
scenes = "content"
prefabs = "content"
"#,
    );
    project.scene("content/level.scene.json");
    project.prefab("content/enemy.prefab.ron");
    let schema = project.receipt(None);
    assert_eq!(
        paths(&capture(&schema)),
        ["content/enemy.prefab.ron", "content/level.scene.json"]
    );
    project.scene("content/ambiguous.json");
    assert!(matches!(
        try_capture(&schema),
        Err(SourceUpgradeInventoryError::KindConflict)
    ));
    project.metadata(
        "content/ambiguous.json",
        "ambiguous.json",
        FIRST_ID,
        AssetSourceKind::Scene,
        0,
    );
    assert_eq!(capture(&schema).participants().len(), 3);
}

#[test]
fn missing_roots_are_explicit_and_even_an_empty_late_root_invalidates_review() {
    let project = Fixture::new(
        "project_location",
        r#"
[profiles.unused.paths]
scenes = "dormant/scenes"
"#,
    );
    let schema = project.receipt(None);
    let inventory = capture(&schema);
    let root = inventory
        .roots()
        .find(|root| root.path() == "dormant/scenes")
        .unwrap();
    assert!(
        matches!(root.observation(), SourceUpgradeRootObservation::Missing {
        ancestor_path, ancestor_identity,
    } if ancestor_path.is_empty() && *ancestor_identity == schema.root_identity())
    );
    fs::create_dir_all(project.root.join("dormant/scenes")).unwrap();
    let mut budget = schema.budget;
    assert!(matches!(
        inventory.validate(&schema, &[], &mut budget),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
}

#[test]
fn unavailable_root_is_a_refusal_and_cannot_become_an_empty_missing_root() {
    let project = Fixture::new("project_location", "");
    fs::remove_dir(project.root.join("prefabs")).unwrap();
    project.write("prefabs", b"a file is not an available source directory");
    let schema = project.receipt(None);
    assert!(matches!(
        try_capture(&schema),
        Err(SourceUpgradeInventoryError::Host(_))
    ));
    assert!(!is_missing(&FsError::IdentityUnavailable));
}

#[test]
fn late_source_addition_and_removal_refuse_without_a_directory_identity_change() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/original.scene.json");
    let schema = project.receipt(None);
    let original = capture(&schema);
    let directory = schema
        .project_root()
        .open_directory_exact(&RelativePath::new("scenes").unwrap())
        .unwrap();
    project.scene("scenes/late.scene.json");
    assert_eq!(
        directory.identity(),
        schema
            .project_root()
            .open_directory_exact(&RelativePath::new("scenes").unwrap())
            .unwrap()
            .identity()
    );
    let mut budget = schema.budget;
    assert!(matches!(
        original.validate(&schema, &[], &mut budget),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
    let with_late_source = capture(&schema);
    fs::remove_file(project.root.join("scenes/original.scene.json")).unwrap();
    let mut budget = schema.budget;
    assert!(matches!(
        with_late_source.validate(&schema, &[], &mut budget),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
}

#[test]
fn complete_metadata_membership_and_payload_observations_are_reenumerated() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/level.scene.json");
    let schema = project.receipt(None);
    let original = capture(&schema);
    project.metadata(
        "assets/unused.png",
        "unused.png",
        FIRST_ID,
        AssetSourceKind::Image,
        1,
    );
    let mut budget = schema.budget;
    assert!(matches!(
        original.validate(&schema, &[], &mut budget),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
    let with_metadata = capture(&schema);
    project.metadata(
        "assets/unused.png",
        "unused.png",
        FIRST_ID,
        AssetSourceKind::Image,
        2,
    );
    let mut budget = schema.budget;
    assert!(matches!(
        with_metadata.validate(&schema, &[], &mut budget),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
    let with_changed_metadata = capture(&schema);
    fs::remove_file(project.root.join("assets/unused.png.meta")).unwrap();
    let mut budget = schema.budget;
    assert!(matches!(
        with_changed_metadata.validate(&schema, &[], &mut budget),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
}

#[test]
fn conflicts_and_catalogue_encoding_are_refused_specifically() {
    let project = Fixture::new("project_location", "");
    project.scene("assets/one.json");
    project.scene("assets/two.json");
    project.metadata(
        "assets/one.json",
        "one.json",
        FIRST_ID,
        AssetSourceKind::Scene,
        0,
    );
    project.metadata(
        "assets/two.json",
        "two.json",
        FIRST_ID,
        AssetSourceKind::Scene,
        0,
    );
    let schema = project.receipt(None);
    assert!(matches!(
        try_capture(&schema),
        Err(SourceUpgradeInventoryError::Catalogue(
            AssetDatabaseError::DuplicateStableId { .. }
        ))
    ));
    project.metadata(
        "assets/two.json",
        "two.json",
        SECOND_ID,
        AssetSourceKind::Scene,
        0,
    );
    project.scene("scenes/conflict.scene.json");
    project.metadata(
        "scenes/conflict.scene.json",
        "conflict.scene.json",
        "d1ee1ba6-df78-4511-b4d0-2b7cb38fa320",
        AssetSourceKind::Prefab,
        0,
    );
    assert!(matches!(
        try_capture(&schema),
        Err(SourceUpgradeInventoryError::KindConflict)
    ));
    fs::remove_file(project.root.join("scenes/conflict.scene.json.meta")).unwrap();
    project.write("assets/unsupported.data", b"{}");
    project.metadata(
        "assets/unsupported.data",
        "unsupported.data",
        "d1ee1ba6-df78-4511-b4d0-2b7cb38fa320",
        AssetSourceKind::Scene,
        0,
    );
    assert!(matches!(
        try_capture(&schema),
        Err(SourceUpgradeInventoryError::UnsupportedEncoding)
    ));
}

#[test]
fn exact_source_encoding_does_not_fall_back_to_guessing_json() {
    assert_eq!(
        SourceUpgradeEncoding::for_path("a.scene.json").unwrap(),
        SourceUpgradeEncoding::Json
    );
    assert_eq!(
        SourceUpgradeEncoding::for_path("a.prefab.ron").unwrap(),
        SourceUpgradeEncoding::Ron
    );
    for path in ["a.JSON", "a.json.meta", "a.ron.tmp", "a"] {
        assert!(matches!(
            SourceUpgradeEncoding::for_path(path),
            Err(SourceUpgradeInventoryError::UnsupportedEncoding)
        ));
    }
}

#[test]
fn rejected_files_refuse_instead_of_publishing_a_truncated_inventory() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/original.scene.json");
    fs::hard_link(
        project.root.join("scenes/original.scene.json"),
        project.root.join("assets/alias.json"),
    )
    .unwrap();
    let schema = project.receipt(None);
    assert!(matches!(
        try_capture(&schema),
        Err(SourceUpgradeInventoryError::EnumerationIncomplete)
    ));
}

#[test]
fn shared_budget_refuses_before_capture_or_source_body_allocation() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/level.scene.json");
    let schema = project.receipt(None);
    let limits = AuthoringProjectAdmissionLimits::default()
        .with_retained_candidate_bytes(ByteLimit::new(1).unwrap());
    let mut budget = AuthoringProjectAdmissionBudget::new(limits);
    assert!(matches!(
        SourceUpgradeInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget),
        Err(SourceUpgradeInventoryError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
        ))
    ));
    assert_eq!(budget.usage().unique_files(), 0);
    let mut budget = schema.budget;
    budget.limits.retained_candidate_bytes =
        ByteLimit::new(budget.retained_candidate_bytes + 512).unwrap();
    assert!(matches!(
        read_file(
            &schema.project_root(),
            "scenes/level.scene.json",
            None,
            1024,
            &mut budget
        ),
        Err(SourceUpgradeInventoryError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
        ))
    ));
    assert_eq!(
        budget.usage().aggregate_encoded_bytes(),
        schema.usage().aggregate_encoded_bytes()
    );
}

#[test]
fn complete_open_membership_and_original_disk_observations_bind_review() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/inactive.scene.json");
    let schema = project.receipt(None);
    let open = project.open_source(&schema, "scenes/inactive.scene.json");
    let mut budget = schema.budget;
    let inventory = SourceUpgradeInventory::capture(
        &schema,
        &[SourceUpgradeOpenSource::Bound(&open)],
        SceneFileLimits::default(),
        &mut budget,
    )
    .unwrap();
    let mut budget = schema.budget;
    inventory
        .validate(
            &schema,
            &[SourceUpgradeOpenSource::Bound(&open)],
            &mut budget,
        )
        .unwrap();
    let mut budget = schema.budget;
    assert!(matches!(
        inventory.validate(&schema, &[], &mut budget),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
    project.write("scenes/inactive.scene.json", b"external bytes");
    let mut budget = schema.budget;
    assert!(matches!(
        SourceUpgradeInventory::capture(
            &schema,
            &[SourceUpgradeOpenSource::Bound(&open)],
            SceneFileLimits::default(),
            &mut budget
        ),
        Err(SourceUpgradeInventoryError::OpenSourceChanged)
    ));
}

#[test]
fn unbound_open_participants_refuse_explicitly() {
    let project = Fixture::new("project_location", "");
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    assert!(matches!(
        SourceUpgradeInventory::capture(
            &schema,
            &[SourceUpgradeOpenSource::Unbound],
            SceneFileLimits::default(),
            &mut budget
        ),
        Err(SourceUpgradeInventoryError::UnboundOpenSource)
    ));
}

#[test]
fn mandatory_scene_selections_cannot_disappear_into_a_missing_root_observation() {
    for selection in [
        "[startup]\ndefault_scene = 'required.json'",
        "[content]\nscenes = ['required.json']",
    ] {
        let project = Fixture::new("project_location", selection);
        fs::remove_dir(project.root.join("scenes")).unwrap();
        let schema = project.receipt(None);
        assert!(matches!(
            try_capture(&schema),
            Err(SourceUpgradeInventoryError::Host(_))
        ));
    }
}

#[test]
fn unknown_metadata_kind_is_not_guessed_from_a_prefab_root_or_json_encoding() {
    let project = Fixture::new("project_location", "");
    project.scene("prefabs/unknown.json");
    project.metadata(
        "prefabs/unknown.json",
        "unknown.json",
        FIRST_ID,
        AssetSourceKind::Unknown,
        0,
    );
    let schema = project.receipt(None);
    assert!(matches!(
        try_capture(&schema),
        Err(SourceUpgradeInventoryError::KindConflict)
    ));
}

#[test]
fn scripts_inside_assets_cannot_rebind_a_catalogue_local_path() {
    let project = Fixture::new(
        "stable_metadata",
        r#"
[paths]
scripts = "assets/scripts"
"#,
    );
    project.scene("assets/scripts/level.json");
    project.metadata(
        "assets/scripts/level.json",
        "level.json",
        FIRST_ID,
        AssetSourceKind::Scene,
        0,
    );
    let schema = project.receipt(None);
    assert!(matches!(
        try_capture(&schema),
        Err(SourceUpgradeInventoryError::MetadataPathMismatch),
    ));
    // The actual catalogue alias is relative to Assets; Scripts is not a catalogue scope.
    project.metadata(
        "assets/scripts/level.json",
        "scripts/level.json",
        FIRST_ID,
        AssetSourceKind::Scene,
        0,
    );
    assert_eq!(paths(&capture(&schema)), ["assets/scripts/level.json"]);
}

#[test]
fn explicit_open_source_outside_catalogue_roots_keeps_native_scripts_scope() {
    let project = Fixture::new("stable_metadata", "");
    project.scene("scripts/level.scene.json");
    project.metadata(
        "scripts/level.scene.json",
        "level.scene.json",
        FIRST_ID,
        AssetSourceKind::Scene,
        0,
    );
    let schema = project.receipt(None);
    let open = project.open_source(&schema, "scripts/level.scene.json");
    let complete_open = [SourceUpgradeOpenSource::Bound(&open)];
    let mut budget = schema.budget;
    let inventory = SourceUpgradeInventory::capture(
        &schema,
        &complete_open,
        SceneFileLimits::default(),
        &mut budget,
    )
    .unwrap();
    assert_eq!(paths(&inventory), ["scripts/level.scene.json"]);
    assert_eq!(inventory.metadata().len(), 1);
    assert_eq!(
        inventory.participants().next().unwrap().source_key(),
        open.source_key()
    );
    let mut budget = schema.budget;
    inventory
        .validate(&schema, &complete_open, &mut budget)
        .unwrap();
}

#[test]
fn long_root_metadata_decoded_budget_counts_canonical_record() {
    let segment = "a".repeat(100);
    let assets = format!("assets/{}", vec![segment; 8].join("/"));
    let source = format!("{assets}/unused.png");
    let project = Fixture::new("project_location", &format!("[paths]\nassets = '{assets}'"));
    for kind in [
        AssetSourceKind::Image,
        AssetSourceKind::Other("trial.custom".to_owned()),
    ] {
        let kind_bytes = match &kind {
            AssetSourceKind::Other(kind) => kind.len(),
            _ => 0,
        };
        project.metadata(&source, "unused.png", FIRST_ID, kind, 0);
        let encoded_bytes = fs::read(project.root.join(format!("{source}.meta")))
            .unwrap()
            .len();
        let schema = project.receipt(None);
        // Exactly the existing project::asset_record_retained_bytes owner formula.
        let decoded_record_bytes = size_of::<AssetRecord>() + source.len() + kind_bytes;
        assert!(decoded_record_bytes > encoded_bytes);
        // One manifest decode and one default-configuration resolution; no profiles or
        // document participants add other decoded costs in this metadata-only inventory.
        let before_metadata = schema.usage().decoded_bytes() + 2 * schema.manifest_bytes().len();
        let expected = before_metadata + decoded_record_bytes;
        let mut budget = schema.budget;
        budget.limits.aggregate_decoded_bytes = ByteLimit::new(expected - 1).unwrap();
        assert!(matches!(
            SourceUpgradeInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget),
            Err(SourceUpgradeInventoryError::Admission(
                AuthoringProjectAdmissionError::AggregateDecodedBytesExceeded { maximum },
            )) if maximum == expected - 1,
        ));
        // The old encoded-length formula accepted the same insufficient ceiling because
        // encoded_bytes < decoded_record_bytes. The exact owner cost must also suffice.
        let mut budget = schema.budget;
        budget.limits.aggregate_decoded_bytes = ByteLimit::new(expected).unwrap();
        let inventory =
            SourceUpgradeInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget)
                .unwrap();
        assert_eq!(inventory.metadata().len(), 1);
        assert_eq!(budget.usage().decoded_bytes(), expected);
    }
}

#[test]
fn exact_open_scratch_is_admitted_at_its_peak_and_refunded_on_success_or_host_error() {
    let project = Fixture::new("project_location", "");
    let path = "scenes/level.scene.json";
    project.scene(path);
    let schema = project.receipt(None);
    let root = schema.project_root();
    let mut base = schema.budget;
    let (fact, _, _capability, _) = read_file(&root, path, None, 4096, &mut base).unwrap();
    let mut paths_only = base;
    drop(relative_path(path, &mut paths_only).unwrap());
    let retained_path = paths_only.usage().retained_candidate_bytes();
    // Revalidation includes two exact path components, but their native name buffers are
    // sequential. One 64 KiB peak suffices; both 1 KiB and one byte short must refuse.
    for spare in [
        1024,
        NATIVE_EXACT_OPEN_SCRATCH_BYTES - 1,
        NATIVE_EXACT_OPEN_SCRATCH_BYTES,
    ] {
        let mut budget = base;
        let maximum = retained_path + spare;
        budget.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
        let result = revalidate_file(&root, &fact, &mut budget);
        if spare == NATIVE_EXACT_OPEN_SCRATCH_BYTES {
            result.unwrap();
            assert_eq!(budget.usage().retained_candidate_bytes(), retained_path);
        } else {
            assert!(matches!(result, Err(SourceUpgradeInventoryError::Admission(
                AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { maximum: actual },
            )) if actual == maximum));
        }
    }
    let retained = base.usage().retained_candidate_bytes();
    let mut budget = base;
    budget.limits.retained_candidate_bytes =
        ByteLimit::new(retained + NATIVE_EXACT_OPEN_SCRATCH_BYTES).unwrap();
    let missing = RelativePath::new("scenes/missing.scene.json").unwrap();
    assert!(matches!(open_file_exact(&root, &missing, &mut budget),
        Err(SourceUpgradeInventoryError::Host(error)) if is_missing(&error)));
    assert_eq!(budget.usage().retained_candidate_bytes(), retained);
    let directory =
        open_directory_exact(&root, &RelativePath::new("scenes").unwrap(), &mut budget).unwrap();
    assert_eq!(Some(directory.identity()), fact.parent_identity());
    assert_eq!(budget.usage().retained_candidate_bytes(), retained);
}

#[test]
fn cursor_buffer_remains_charged_while_observations_and_exact_open_scratch_overlap() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/level.scene.json");
    let schema = project.receipt(None);
    let directory = schema
        .project_root()
        .open_directory_exact(&RelativePath::new("scenes").unwrap())
        .unwrap();
    let limits = DirectoryEnumerationLimits::new(8, 64, 1);
    let base = schema.budget;
    let retained = base.usage().retained_candidate_bytes();
    let mut too_small = base;
    too_small.limits.retained_candidate_bytes =
        ByteLimit::new(retained + NATIVE_CURSOR_SCRATCH_BYTES - 1).unwrap();
    assert!(matches!(
        InventoryDirectoryCursor::begin(&directory, limits, &mut too_small),
        Err(SourceUpgradeInventoryError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
        ))
    ));
    let observation = 8 * limits.max_name_bytes()
        + size_of::<DirectoryEntryObservation>()
        + size_of::<DirectoryEntryRejectedFact>();
    for exact_spare in [
        NATIVE_EXACT_OPEN_SCRATCH_BYTES - 1,
        NATIVE_EXACT_OPEN_SCRATCH_BYTES,
    ] {
        let mut budget = base;
        budget.limits.retained_candidate_bytes =
            ByteLimit::new(retained + NATIVE_CURSOR_SCRATCH_BYTES + observation + exact_spare)
                .unwrap();
        let mut cursor = InventoryDirectoryCursor::begin(&directory, limits, &mut budget).unwrap();
        assert_eq!(
            budget.usage().retained_candidate_bytes(),
            retained + NATIVE_CURSOR_SCRATCH_BYTES
        );
        reserve(&mut budget, observation).unwrap();
        let batch = cursor.enumeration.next_batch(1).unwrap();
        assert_eq!(batch.entries().len(), 1);
        let result = open_file_exact(
            &directory,
            &RelativePath::new("level.scene.json").unwrap(),
            &mut budget,
        );
        if exact_spare == NATIVE_EXACT_OPEN_SCRATCH_BYTES {
            drop(result.unwrap());
            assert_eq!(
                budget.usage().retained_candidate_bytes(),
                retained + NATIVE_CURSOR_SCRATCH_BYTES + observation
            );
        } else {
            assert!(matches!(
                result,
                Err(SourceUpgradeInventoryError::Admission(
                    AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. },
                ))
            ));
        }
        drop(batch);
        release(&mut budget, observation).unwrap();
        let before_cursor_drop = budget.usage().retained_candidate_bytes();
        cursor.finish(&mut budget).unwrap();
        assert_eq!(
            budget.usage().retained_candidate_bytes(),
            before_cursor_drop - NATIVE_CURSOR_SCRATCH_BYTES
        );
        if exact_spare == NATIVE_EXACT_OPEN_SCRATCH_BYTES {
            assert_eq!(budget.usage().retained_candidate_bytes(), retained);
        }
    }
}

fn paths(inventory: &SourceUpgradeInventory) -> Vec<&str> {
    inventory
        .participants()
        .map(|source| source.file().path())
        .collect()
}

fn capture(schema: &AuthoringSchemaReceipt) -> SourceUpgradeInventory {
    try_capture(schema).unwrap()
}

fn try_capture(
    schema: &AuthoringSchemaReceipt,
) -> Result<SourceUpgradeInventory, SourceUpgradeInventoryError> {
    let mut budget = schema.budget;
    SourceUpgradeInventory::capture(schema, &[], SceneFileLimits::default(), &mut budget)
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(identity: &str, extra_manifest: &str) -> Self {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nara_upgrade_inventory_{}_{}",
            std::process::id(),
            sequence
        ));
        let fixture = Self { root };
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir_all(fixture.root.join(directory)).unwrap();
        }
        fixture.write(
            "nara.toml",
            format!(
                r#"schema_version = 1
[project]
name = "Upgrade inventory"
source_universe = "984edb80-00a9-45fd-9f6c-d2dbf903ed48"
native_document_identity = "{identity}"
[authoring]
state = "ready"
schema_bundle = "schema/authoring.bundle.json"
{extra_manifest}
"#
            )
            .as_bytes(),
        );
        let bundle =
            AuthoringSchemaBundle::new(std::iter::empty::<AuthoringSchemaBundleOwner>()).unwrap();
        fixture.write(
            "schema/authoring.bundle.json",
            bundle.to_json_string().unwrap().as_bytes(),
        );
        fixture
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn scene(&self, path: &str) {
        self.write(
            path,
            SceneDocument::default()
                .to_json_string()
                .unwrap()
                .as_bytes(),
        );
    }

    fn prefab(&self, path: &str) {
        self.write(
            path,
            PrefabDocument::default()
                .to_ron_string()
                .unwrap()
                .as_bytes(),
        );
    }

    fn metadata(
        &self,
        source: &str,
        local: &str,
        stable_id: &str,
        kind: AssetSourceKind,
        settings: u32,
    ) {
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str(stable_id).unwrap(),
                AssetPath::new(local).unwrap(),
                kind,
            ),
            settings,
        );
        self.write(
            &format!("{source}.meta"),
            metadata.to_json_string().unwrap().as_bytes(),
        );
    }

    fn receipt(&self, profile: Option<&str>) -> AuthoringSchemaReceipt {
        let root = DirectoryCapability::from_host_handle(
            host_directory(&self.root).unwrap(),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted),
        )
        .unwrap();
        AuthoringProjectAdmissionHost::default()
            .prepare(root, profile)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish()
    }

    fn open_source(
        &self,
        schema: &AuthoringSchemaReceipt,
        path: &str,
    ) -> AuthoringDocumentSourceFact {
        let pools = TaskPools::try_new(TaskPoolConfig::default()).unwrap();
        let mut state = ProjectSourceIndexState::new(schema);
        state.request_refresh().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while state.has_pending_work() {
            state.advance(&pools);
            assert!(Instant::now() < deadline, "source index did not finish");
            thread::sleep(Duration::from_millis(1));
        }
        let basis = AuthoringSchemaBasis::admit(schema.bundle(), &[]).unwrap();
        let document =
            AuthoringDocumentAdmissionBasis::new(schema, &basis, SceneFileLimits::default())
                .admit(
                    &state.snapshot().unwrap(),
                    path,
                    AuthoringDocumentKind::Scene,
                )
                .unwrap();
        assert!(pools.shutdown_blocking().unwrap().is_complete());
        document.source().clone()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let temporary_root = std::env::temp_dir().canonicalize().unwrap();
        let resolved = self.root.canonicalize().unwrap();
        assert!(resolved.starts_with(temporary_root));
        assert!(
            resolved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara_upgrade_inventory_")
        );
        fs::remove_dir_all(resolved).unwrap();
    }
}

#[cfg(windows)]
fn host_directory(path: &Path) -> io::Result<File> {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
    OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2 | 0x4)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(path)
}

#[cfg(not(windows))]
fn host_directory(path: &Path) -> io::Result<File> {
    File::open(path)
}
