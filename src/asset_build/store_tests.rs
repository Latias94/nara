//! Real capability fixtures for the generated-store protocol. Runtime consumers live elsewhere.

use std::{
    error::Error,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nara_asset::{
    AssetProductRef, AssetReferenceDemand, AssetSourcePath, ImportedProductId, StableAssetId,
};
use nara_core::{ByteLimit, FormatKind, FormatVersion, ItemLimit};
use nara_fs::{CapabilityRights, DirectorySyncTier, HostCapabilityOptions, TrustMode};

use super::super::model::{ArtifactBuildDefinition, ArtifactBuildOutput};
use super::*;

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

struct TestRoot(PathBuf);
impl TestRoot {
    fn new() -> Self {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nara-artifact-store-{}-{stamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn capability(&self) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            directory_handle(&self.0),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap()
    }
    fn store(&self) -> ArtifactBuildStore {
        self.with_limits(limits())
    }
    fn with_limits(&self, limits: ArtifactBuildLimits) -> ArtifactBuildStore {
        ArtifactBuildStore::open(self.capability(), limits).unwrap()
    }
    fn child(&self, name: &RelativeComponent) -> PathBuf {
        self.0.join(name.to_str().unwrap())
    }
}
impl Drop for TestRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[cfg(windows)]
fn directory_handle(path: &Path) -> File {
    use std::os::windows::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2 | 0x4)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(path)
        .unwrap()
}
#[cfg(target_os = "linux")]
fn directory_handle(path: &Path) -> File {
    File::open(path).unwrap()
}

fn limits() -> ArtifactBuildLimits {
    ArtifactBuildLimits {
        metadata_bytes: ByteLimit::new(64 * 1024).unwrap(),
        member_bytes: ByteLimit::new(1024).unwrap(),
        group_bytes: ByteLimit::new(4096).unwrap(),
        retained_bytes: ByteLimit::new(1024 * 1024).unwrap(),
        work_bytes: ByteLimit::new(8 * 1024 * 1024).unwrap(),
        members: ItemLimit::new(16).unwrap(),
        observations: ItemLimit::new(16).unwrap(),
        index_entries: ItemLimit::new(16).unwrap(),
    }
}

fn product(index: usize) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(format!("00000000-0000-4000-8000-{index:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn seed() -> ArtifactBuildSeed {
    ArtifactBuildSeed {
        source: product(1),
        location: Some(AssetSourcePath::new("assets/compound.fixture").unwrap()),
        primary: Some(ArtifactContentDigest::of_bytes(b"primary source")),
        definitions: vec![ArtifactBuildDefinition {
            id: "test.compound.importer".to_owned(),
            revision: FormatVersion::ONE,
            implementation: ArtifactContentDigest::of_bytes(b"compiled fixture implementation"),
        }],
        settings: b"normalized settings".to_vec(),
        target: b"actual target facts".to_vec(),
        format: FormatKind::new("fixture_operation").unwrap(),
        format_version: FormatVersion::ONE,
    }
}

fn output(name: &str, product: AssetProductRef, bytes: &[u8]) -> ArtifactBuildOutput {
    ArtifactBuildOutput {
        name: name.to_owned(),
        format: FormatKind::new("fixture_member").unwrap(),
        format_version: FormatVersion::ONE,
        products: vec![product],
        dependencies: Vec::new(),
        bytes: Arc::from(bytes),
    }
}

fn candidate(observation: &[u8], first: &[u8], second: &[u8]) -> ArtifactGroupCandidate {
    ArtifactGroupCandidate {
        seed: seed(),
        observations: vec![ArtifactBuildObservation::Raw {
            path: AssetSourcePath::new("assets/optional.fixture").unwrap(),
            digest: Some(ArtifactContentDigest::of_bytes(observation)),
        }],
        // Deliberately unsorted, with different source Products and an operation format that is
        // independent of member formats.
        outputs: vec![
            output("secondary", product(2), second),
            output("primary", product(1), first),
        ],
    }
}

fn commit(store: &ArtifactBuildStore, candidate: ArtifactGroupCandidate) -> ArtifactCommitReceipt {
    store.commit(candidate, |_| Ok(())).unwrap()
}

fn stages() -> [ArtifactPublicationPoint; 7] {
    use ArtifactPublicationPoint::*;
    [
        BeforeBlob { index: 0 },
        AfterBlob { index: 0 },
        BeforeBlob { index: 1 },
        AfterBlob { index: 1 },
        BeforeManifest,
        BeforeIndex,
        AfterIndex,
    ]
}

#[test]
fn whole_group_publication_and_reopen_capture_canonical_complete_bytes() {
    let root = TestRoot::new();
    let store = root.store();
    assert!(store.lookup_seed(&seed()).unwrap().is_none());
    let mut input = candidate(b"A", b"first", b"second");
    input.outputs[1].dependencies = vec![
        (product(7), AssetReferenceDemand::Deferred),
        (product(3), AssetReferenceDemand::Immediate),
    ];
    let mut seen = Vec::new();
    let receipt = store
        .commit(input, |point| {
            seen.push(point);
            Ok(())
        })
        .unwrap();
    assert_eq!(seen, stages());
    let digest = receipt.group().digest();
    let recipe = receipt.group().recipe_digest();
    assert_eq!(receipt.group().members()[0].name, "primary");
    assert_eq!(
        receipt.group().members()[0].dependencies,
        vec![
            (product(3), AssetReferenceDemand::Immediate),
            (product(7), AssetReferenceDemand::Deferred)
        ]
    );
    assert_eq!(
        receipt.group().member_bytes("primary"),
        Some(b"first".as_slice())
    );
    assert!(receipt.group().member_bytes("absent").is_none());
    assert!(receipt.index_receipt().verified_content_digest().is_some());
    assert!(receipt.index_receipt().naming_is_atomic());
    #[cfg(windows)]
    {
        assert_eq!(
            receipt.directory_sync_tier(),
            DirectorySyncTier::Unsupported
        );
        assert!(receipt.directory_sync().is_none());
    }
    #[cfg(target_os = "linux")]
    assert_eq!(
        receipt.directory_sync().unwrap().tier(),
        DirectorySyncTier::Naming
    );
    let escaped = receipt.into_group();
    drop(store);
    let reopened = root.store();
    let loaded = reopened.lookup_seed(&seed()).unwrap().unwrap();
    assert_eq!(loaded.digest(), digest);
    assert_eq!(loaded.recipe_digest(), recipe);
    assert_eq!(loaded.members(), escaped.members());
    assert_eq!(loaded.member_bytes("secondary"), Some(b"second".as_slice()));
    assert!(!Arc::ptr_eq(&loaded.0, &escaped.0));
}

#[test]
fn every_publication_interruption_keeps_old_index_or_returns_actual_uncertain_receipt() {
    for stop in stages() {
        let root = TestRoot::new();
        let store = root.store();
        let old = commit(&store, candidate(b"A", b"old primary", b"old secondary")).into_group();
        let before_index = fs::read(root.0.join(INDEX_NAME)).unwrap();
        let mut seen = Vec::new();
        let error = store
            .commit(candidate(b"B", b"new primary", b"new secondary"), |point| {
                seen.push(point);
                if point == stop {
                    Err(ArtifactBuildError::Aborted)
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert_eq!(seen.last(), Some(&stop));
        let expected = if stop == ArtifactPublicationPoint::AfterIndex {
            let ArtifactBuildError::Uncertain { receipt, cause } = error else {
                panic!("post-index interruption must retain its receipt");
            };
            assert!(matches!(*cause, ArtifactBuildError::Aborted));
            assert_eq!(
                receipt.group().member_bytes("primary"),
                Some(b"new primary".as_slice())
            );
            assert!(receipt.index_receipt().verified_content_digest().is_some());
            assert!(receipt.directory_sync().is_none());
            receipt.group().digest()
        } else {
            assert!(matches!(error, ArtifactBuildError::Aborted));
            assert_eq!(fs::read(root.0.join(INDEX_NAME)).unwrap(), before_index);
            old.digest()
        };
        drop(store);
        let reopened = root.store();
        assert_eq!(
            reopened.lookup_seed(&seed()).unwrap().unwrap().digest(),
            expected
        );
        assert_eq!(old.member_bytes("primary"), Some(b"old primary".as_slice()));
        // Any pre-index objects are harmless orphans and can be independently verified/reused.
        let retry = commit(&reopened, candidate(b"B", b"new primary", b"new secondary"));
        assert_eq!(
            retry.group().member_bytes("secondary"),
            Some(b"new secondary".as_slice())
        );
    }
}

#[test]
fn callback_panics_have_the_same_visibility_semantics_as_returned_errors() {
    for stop in stages() {
        let root = TestRoot::new();
        let store = root.store();
        let old = commit(&store, candidate(b"A", b"old", b"old second")).into_group();
        let error = store
            .commit(candidate(b"B", b"new", b"new second"), |point| {
                assert_ne!(point, stop, "injected callback panic");
                Ok(())
            })
            .unwrap_err();
        if stop == ArtifactPublicationPoint::AfterIndex {
            let ArtifactBuildError::Uncertain { receipt, cause } = error else {
                panic!("post-index panic hid its publication");
            };
            assert!(matches!(*cause, ArtifactBuildError::Panicked));
            assert_eq!(
                store.lookup_seed(&seed()).unwrap().unwrap().digest(),
                receipt.group().digest()
            );
        } else {
            assert!(matches!(error, ArtifactBuildError::Panicked));
            assert_eq!(
                store.lookup_seed(&seed()).unwrap().unwrap().digest(),
                old.digest()
            );
        }
    }
}

#[test]
fn successful_recipe_history_rejects_a_to_b_to_different_a() {
    let root = TestRoot::new();
    let store = root.store();
    let first = commit(&store, candidate(b"A", b"A bytes", b"A secondary")).into_group();
    let second = commit(&store, candidate(b"B", b"B bytes", b"B secondary")).into_group();
    assert_ne!(first.recipe_digest(), second.recipe_digest());
    assert_ne!(first.digest(), second.digest());
    let before = fs::read(root.0.join(INDEX_NAME)).unwrap();
    assert!(matches!(
        store.commit(
            candidate(b"A", b"different A bytes", b"A secondary"),
            |_| Ok(())
        ),
        Err(ArtifactBuildError::DeterminismConflict)
    ));
    assert_eq!(fs::read(root.0.join(INDEX_NAME)).unwrap(), before);
    assert_eq!(
        store.lookup_seed(&seed()).unwrap().unwrap().digest(),
        second.digest()
    );
    assert_eq!(
        commit(&store, candidate(b"A", b"A bytes", b"A secondary"))
            .group()
            .digest(),
        first.digest()
    );
}

#[test]
fn identical_complete_groups_reuse_original_objects_after_independent_verification() {
    let root = TestRoot::new();
    let store = root.store();
    let first = commit(&store, candidate(b"A", b"one", b"two")).into_group();
    let blob = store
        .root
        .open_child_file(&blob_name(first.members()[0].digest).unwrap())
        .unwrap();
    let group = store
        .root
        .open_child_file(&group_name(first.digest()).unwrap())
        .unwrap();
    let second = commit(&store, candidate(b"A", b"one", b"two")).into_group();
    assert_eq!(first.digest(), second.digest());
    assert_eq!(
        blob.identity(),
        store
            .root
            .open_child_file(&blob_name(first.members()[0].digest).unwrap())
            .unwrap()
            .identity()
    );
    assert_eq!(
        group.identity(),
        store
            .root
            .open_child_file(&group_name(first.digest()).unwrap())
            .unwrap()
            .identity()
    );
    assert!(!Arc::ptr_eq(&first.0, &second.0));
}

#[test]
fn missing_or_corrupt_referenced_objects_are_invalid_and_never_repaired_by_commit() {
    for damage in 0..5 {
        let root = TestRoot::new();
        let store = root.store();
        let group = commit(&store, candidate(b"A", b"one", b"two")).into_group();
        let blob_path = root.child(&blob_name(group.members()[0].digest).unwrap());
        let manifest_path = root.child(&group_name(group.digest()).unwrap());
        match damage {
            0 => fs::remove_file(&blob_path).unwrap(),
            1 => fs::write(&blob_path, b"bad").unwrap(),
            2 => fs::remove_file(&manifest_path).unwrap(),
            3 => fs::write(&manifest_path, b"{}").unwrap(),
            4 => fs::write(root.0.join(INDEX_NAME), b"{").unwrap(),
            _ => unreachable!(),
        }
        assert!(matches!(
            store.lookup_seed(&seed()),
            Err(ArtifactBuildError::InvalidCache(_))
        ));
        assert!(matches!(
            store.commit(candidate(b"A", b"one", b"two"), |_| Ok(())),
            Err(ArtifactBuildError::InvalidCache(_))
        ));
        assert_eq!(group.member_bytes("primary"), Some(b"one".as_slice()));
        if damage == 1 {
            assert_eq!(fs::read(blob_path).unwrap(), b"bad");
        }
        if damage == 3 {
            assert_eq!(fs::read(manifest_path).unwrap(), b"{}");
        }
    }
}

#[test]
fn post_index_corruption_is_uncertain_with_original_captured_bytes() {
    let root = TestRoot::new();
    let store = root.store();
    let blob_path = root.child(&blob_name(ArtifactContentDigest::of_bytes(b"one")).unwrap());
    let error = store
        .commit(candidate(b"A", b"one", b"two"), |point| {
            if point == ArtifactPublicationPoint::AfterIndex {
                fs::write(&blob_path, b"bad").unwrap();
            }
            Ok(())
        })
        .unwrap_err();
    let ArtifactBuildError::Uncertain { receipt, cause } = error else {
        panic!("published group corruption must be uncertain");
    };
    assert!(matches!(*cause, ArtifactBuildError::InvalidCache(_)));
    assert_eq!(
        receipt.group().member_bytes("primary"),
        Some(b"one".as_slice())
    );
    assert!(receipt.index_receipt().verified_content_digest().is_some());
    assert!(matches!(
        store.lookup_seed(&seed()),
        Err(ArtifactBuildError::InvalidCache(_))
    ));
}

#[test]
fn conflicting_members_products_observations_and_runtime_demands_are_rejected() {
    let root = TestRoot::new();
    let store = root.store();
    for conflict in 0..5 {
        let mut input = candidate(b"A", b"one", b"two");
        match conflict {
            0 => input.outputs[1].name = input.outputs[0].name.clone(),
            1 => input.outputs[0].products = vec![product(1)],
            2 => {
                input.outputs[0].dependencies = vec![
                    (product(3), AssetReferenceDemand::Deferred),
                    (product(3), AssetReferenceDemand::Immediate),
                ]
            }
            3 => input.observations.push(ArtifactBuildObservation::Raw {
                path: AssetSourcePath::new("assets/optional.fixture").unwrap(),
                digest: None,
            }),
            4 => input
                .seed
                .definitions
                .push(input.seed.definitions[0].clone()),
            _ => unreachable!(),
        }
        assert!(matches!(
            store.commit(input, |_| Ok(())),
            Err(ArtifactBuildError::InvalidCandidate(_))
        ));
        assert!(!root.0.join(INDEX_NAME).exists());
    }
}

#[test]
fn exact_duplicate_observations_normalize_and_runtime_metadata_participates_in_determinism() {
    let root = TestRoot::new();
    let store = root.store();
    let mut input = candidate(b"A", b"one", b"two");
    input.observations.push(input.observations[0].clone());
    let first = commit(&store, input).into_group();
    assert_eq!(first.observations().len(), 1);
    let mut changed = candidate(b"A", b"one", b"two");
    changed.outputs[0]
        .dependencies
        .push((product(8), AssetReferenceDemand::Deferred));
    assert!(matches!(
        store.commit(changed, |_| Ok(())),
        Err(ArtifactBuildError::DeterminismConflict)
    ));
}

#[test]
fn member_removal_replaces_the_complete_mapping_and_allows_actual_metadata_members() {
    let root = TestRoot::new();
    let store = root.store();
    let previous = commit(&store, candidate(b"A", b"one", b"two")).into_group();
    let mut successor = candidate(b"B", b"one", b"unused");
    successor.outputs.remove(0);
    let mut metadata = output("catalog", product(99), b"consumed metadata");
    metadata.products.clear();
    metadata.format = FormatKind::new("fixture_catalog").unwrap();
    successor.outputs.push(metadata);
    let current = commit(&store, successor).into_group();
    assert!(current.member_bytes("secondary").is_none());
    assert_eq!(
        current.member_bytes("catalog"),
        Some(b"consumed metadata".as_slice())
    );
    assert_eq!(previous.member_bytes("secondary"), Some(b"two".as_slice()));
    assert!(
        current
            .members()
            .iter()
            .all(|member| !member.products.contains(&product(2)))
    );
}

#[test]
fn exact_work_admission_shrink_and_refund_belong_to_each_store_owner() {
    let root = TestRoot::new();
    let first = root.store();
    let second = root.store();
    let maximum = first.limits().work_bytes.get();
    let mut lease = first.reserve_work(maximum).unwrap();
    assert!(matches!(
        first.reserve_work(1),
        Err(ArtifactBuildError::Budget { kind: "work", .. })
    ));
    drop(second.reserve_work(maximum).unwrap());
    assert!(lease.shrink_to(maximum + 1).is_err());
    assert!(first.reserve_work(1).is_err());
    lease.shrink_to(maximum - 1).unwrap();
    let last = first.reserve_work(1).unwrap();
    assert!(first.reserve_work(1).is_err());
    drop(last);
    lease.shrink_to(0).unwrap();
    drop(lease);
    drop(first.reserve_work(maximum).unwrap());
    assert!(second.reserve_work(maximum + 1).is_err());
}

#[test]
fn cloned_groups_share_one_retained_lease_and_separate_loads_charge_independently() {
    let root = TestRoot::new();
    let store = root.store();
    let group = commit(&store, candidate(b"A", b"one", b"two")).into_group();
    let shared = group.clone();
    let loaded = store.lookup_seed(&seed()).unwrap().unwrap();
    let maximum = store.limits().retained_bytes.get();
    let remaining = maximum - group.retained_bytes() - loaded.retained_bytes();
    let all_remaining = store.reserve_retained(remaining).unwrap();
    assert!(store.reserve_retained(1).is_err());
    let charge = group.retained_bytes();
    drop(group);
    assert!(store.reserve_retained(1).is_err());
    drop(shared);
    drop(store.reserve_retained(charge).unwrap());
    drop(loaded);
    drop(all_remaining);
    drop(store.reserve_retained(maximum).unwrap());
}

#[test]
fn escaped_group_preserves_original_bytes_and_budget_after_cache_removal_and_store_drop() {
    let root = TestRoot::new();
    let store = root.store();
    let group = commit(&store, candidate(b"A", b"one", b"two")).into_group();
    let maximum = store.limits().retained_bytes.get();
    let original_budget = Arc::clone(&store.budget);
    fs::remove_file(root.0.join(INDEX_NAME)).unwrap();
    fs::remove_file(root.child(&group_name(group.digest()).unwrap())).unwrap();
    for member in group.members() {
        fs::remove_file(root.child(&blob_name(member.digest).unwrap())).unwrap();
    }
    drop(store);
    let mut successor_limits = limits();
    successor_limits.retained_bytes = ByteLimit::new(maximum * 2).unwrap();
    let reopened = root.with_limits(successor_limits);
    assert!(reopened.lookup_seed(&seed()).unwrap().is_none());
    drop(
        reopened
            .reserve_retained(successor_limits.retained_bytes.get())
            .unwrap(),
    );
    assert!(original_budget.reserve_retained(maximum).is_err());
    let remaining = original_budget
        .reserve_retained(maximum - group.retained_bytes())
        .unwrap();
    assert_eq!(group.member_bytes("primary"), Some(b"one".as_slice()));
    drop(group);
    drop(remaining);
    drop(original_budget.reserve_retained(maximum).unwrap());
}

#[test]
fn exact_member_group_index_limits_and_failed_allocation_refund_precede_publication() {
    let root = TestRoot::new();
    let mut bounds = limits();
    bounds.member_bytes = ByteLimit::new(4).unwrap();
    bounds.group_bytes = ByteLimit::new(8).unwrap();
    bounds.index_entries = ItemLimit::ONE;
    let store = root.with_limits(bounds);
    let first = commit(&store, candidate(b"A", b"1234", b"5678")).into_group();
    assert!(matches!(
        store.commit(candidate(b"A", b"12345", b"5678"), |_| Ok(())),
        Err(ArtifactBuildError::Budget {
            kind: "member-bytes",
            ..
        })
    ));
    assert!(matches!(
        store.commit(candidate(b"B", b"1234", b"5678"), |_| Ok(())),
        Err(ArtifactBuildError::Budget {
            kind: "successful-recipes",
            ..
        })
    ));
    assert_eq!(
        store.lookup_seed(&seed()).unwrap().unwrap().digest(),
        first.digest()
    );
    drop(store.reserve_work(store.limits().work_bytes.get()).unwrap());
    let remaining = store.limits().retained_bytes.get() - first.retained_bytes();
    drop(store.reserve_retained(remaining).unwrap());

    let smaller = TestRoot::new();
    bounds.group_bytes = ByteLimit::new(7).unwrap();
    let smaller_store = smaller.with_limits(bounds);
    assert!(matches!(
        smaller_store.commit(candidate(b"A", b"1234", b"5678"), |_| Ok(())),
        Err(ArtifactBuildError::Budget {
            kind: "group-bytes",
            ..
        })
    ));
    assert!(!smaller.0.join(INDEX_NAME).exists());
}

#[test]
fn exact_retained_and_encoded_thresholds_are_admitted_before_any_generated_publication() {
    let measuring_root = TestRoot::new();
    let measuring_store = measuring_root.store();
    let measured = commit(&measuring_store, candidate(b"A", b"one", b"two"));
    let retained = measured.group().retained_bytes();
    let encoded = usize::try_from(measured.group().digest().length())
        .unwrap()
        .max(usize::try_from(measured.index_receipt().written_content_digest().length()).unwrap());

    let mut exact = limits();
    exact.retained_bytes = ByteLimit::new(retained).unwrap();
    exact.metadata_bytes = ByteLimit::new(encoded).unwrap();
    let exact_root = TestRoot::new();
    let exact_store = exact_root.with_limits(exact);
    let receipt = commit(&exact_store, candidate(b"A", b"one", b"two"));
    assert_eq!(receipt.group().retained_bytes(), retained);
    assert!(exact_store.reserve_retained(1).is_err());
    drop(receipt);
    drop(exact_store.reserve_retained(retained).unwrap());

    let retained_root = TestRoot::new();
    let mut one_short = exact;
    one_short.retained_bytes = ByteLimit::new(retained - 1).unwrap();
    let retained_store = retained_root.with_limits(one_short);
    assert!(matches!(
        retained_store.commit(candidate(b"A", b"one", b"two"), |_| Ok(())),
        Err(ArtifactBuildError::Budget {
            kind: "retained",
            ..
        })
    ));
    assert_eq!(fs::read_dir(&retained_root.0).unwrap().count(), 1);
    drop(retained_store.reserve_retained(retained - 1).unwrap());

    let encoded_root = TestRoot::new();
    one_short = limits();
    one_short.metadata_bytes = ByteLimit::new(encoded - 1).unwrap();
    let encoded_store = encoded_root.with_limits(one_short);
    assert!(matches!(
        encoded_store.commit(candidate(b"A", b"one", b"two"), |_| Ok(())),
        Err(ArtifactBuildError::Budget {
            kind: "encoded-metadata",
            ..
        })
    ));
    assert_eq!(fs::read_dir(&encoded_root.0).unwrap().count(), 1);
    drop(
        encoded_store
            .reserve_work(one_short.work_bytes.get())
            .unwrap(),
    );
}

#[test]
fn independently_opened_writers_use_the_same_real_lock_and_reopen_the_current_index() {
    let root = TestRoot::new();
    let first = root.store();
    let second = root.store();
    let mut contended = false;
    let first_result = first
        .commit(candidate(b"A", b"A bytes", b"A second"), |point| {
            if point == ArtifactPublicationPoint::BeforeIndex {
                assert!(matches!(
                    second.commit(candidate(b"B", b"B bytes", b"B second"), |_| Ok(())),
                    Err(ArtifactBuildError::Lock(_))
                ));
                contended = true;
            }
            Ok(())
        })
        .unwrap();
    assert!(contended);
    let first_digest = first_result.group().digest();
    let second_result = commit(&second, candidate(b"B", b"B bytes", b"B second"));
    assert_ne!(first_digest, second_result.group().digest());
    assert!(matches!(
        first.commit(candidate(b"A", b"changed A bytes", b"A second"), |_| Ok(())),
        Err(ArtifactBuildError::DeterminismConflict)
    ));
    assert_eq!(
        first.lookup_seed(&seed()).unwrap().unwrap().digest(),
        second_result.group().digest()
    );
}

#[test]
fn temporary_publication_failure_and_post_index_uncertainty_retain_cleanup_custody() {
    let root = TestRoot::new();
    let store = root.store();
    let target = component("occupied.fixture").unwrap();
    drop(store.root.create_child_file(&target).unwrap());
    let error = store
        .publish(&target, b"candidate", ExpectedTarget::Missing)
        .unwrap_err();
    let ArtifactBuildError::Temporary(mut failure) = error else {
        panic!("temporary publication lost its failure owner");
    };
    let temporary_name = failure.cleanup().unwrap().name().clone();
    assert!(root.child(&temporary_name).exists());
    assert!(failure.retry_cleanup());
    assert!(!root.child(&temporary_name).exists());

    let temporary = store
        .root
        .create_temp(&component("retained-cleanup.fixture").unwrap())
        .unwrap();
    let mut failure = Some(TemporaryFileFailure::with_cleanup(
        FsError::Io {
            operation: FsOperation::Write,
            source: std::io::Error::other("fixture write"),
        },
        temporary.into_cleanup(),
    ));
    let error = store
        .commit(candidate(b"A", b"one", b"two"), |point| {
            if point == ArtifactPublicationPoint::AfterIndex {
                return Err(ArtifactBuildError::Temporary(failure.take().unwrap()));
            }
            Ok(())
        })
        .unwrap_err();
    let ArtifactBuildError::Uncertain { receipt, cause } = error else {
        panic!("post-index failure lost publication evidence");
    };
    let ArtifactBuildError::Temporary(mut failure) = *cause else {
        panic!("post-index failure lost cleanup custody");
    };
    assert!(failure.cleanup().is_some());
    assert!(failure.retry_cleanup());
    assert_eq!(
        receipt.group().member_bytes("primary"),
        Some(b"one".as_slice())
    );
    assert_eq!(
        store.lookup_seed(&seed()).unwrap().unwrap().digest(),
        receipt.group().digest()
    );
}

#[test]
fn a_live_original_lock_binding_and_real_exclusive_lock_are_required() {
    let root = TestRoot::new();
    let store = root.store();
    let held = store
        .root
        .open_child_file(&component(LOCK_NAME).unwrap())
        .unwrap()
        .try_lock(LockMode::Exclusive)
        .unwrap();
    let error = store
        .commit(candidate(b"A", b"one", b"two"), |_| Ok(()))
        .unwrap_err();
    assert!(matches!(error, ArtifactBuildError::Lock(_)));
    drop(held);
    let other = TestRoot::new();
    fs::rename(root.0.join(LOCK_NAME), other.0.join("original.lock")).unwrap();
    fs::write(root.0.join(LOCK_NAME), b"replacement").unwrap();
    assert!(matches!(
        store.commit(candidate(b"A", b"one", b"two"), |_| Ok(())),
        Err(ArtifactBuildError::Stale)
    ));
    assert!(!root.0.join(INDEX_NAME).exists());
}

#[test]
fn preflight_counts_and_strings_reject_before_payload_construction_and_refund_work() {
    let root = TestRoot::new();
    let store = root.store();
    let excessive = format!(
        "{{\"payload\":{{\"members\":[{}]}}}}",
        vec!["0"; store.limits().members.get() + 1].join(",")
    );
    assert!(matches!(
        format::decode_group(excessive.as_bytes(), &store),
        Err(ArtifactBuildError::InvalidCache(
            "metadata exceeds its structural bounds"
        ))
    ));
    let long_string = format!("{{\"payload\":\"{}\"}}", "x".repeat(4097));
    assert!(matches!(
        format::decode_group(long_string.as_bytes(), &store),
        Err(ArtifactBuildError::InvalidCache(
            "metadata exceeds its structural bounds"
        ))
    ));
    let duplicate = b"{\"payload\":{},\"payload\":{}}";
    assert!(matches!(
        format::decode_group(duplicate, &store),
        Err(ArtifactBuildError::InvalidCache(
            "metadata structure, strings or duplicate keys are invalid"
        ))
    ));
    drop(store.reserve_work(store.limits().work_bytes.get()).unwrap());
}

#[test]
fn filesystem_failures_preserve_their_typed_source_chain() {
    let error = ArtifactBuildError::Host(FsError::Io {
        operation: FsOperation::Read,
        source: std::io::Error::other("fixture I/O"),
    });
    assert!(error.source().is_some());
    assert!(error.source().unwrap().source().is_some());
}
