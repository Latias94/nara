use std::fs;

use nara_asset::AssetSourceKind;
use nara_core::ItemLimit;
use nara_project::{ProjectAuthoringRecovery, ProjectAuthoringState, ProjectManifest, ProjectPath};

use super::*;
use crate::project_host::{
    authoring::{
        AuthoringProjectAdmissionLimits,
        project::prefab_component_apply::recovery_data::{
            PrefabComponentApplyRecoveryInput, PrefabComponentApplyRecoveryLimits,
            RecoveryMetadataGuardInput, RecoveryOutputInput, RecoveryRootInput,
            RecoverySourceInput,
        },
        recovery_storage::RecoveryStorageHost,
        source_inventory::test_support::{FIRST_ID, Fixture, SECOND_ID},
    },
    file_publication::{FilePublicationHost, FilePublicationOutcome},
};

pub(in super::super) const CONTAINING: &str = "scenes/containing.scene.json";
pub(in super::super) const DESTINATION: &str = "prefabs/target.prefab.ron";
pub(in super::super) const GUARD: &str = "scenes/dependent.scene.json";
const RECOVERY: &str = "recovery/component-apply.json";

pub(in super::super) fn budget() -> AuthoringProjectAdmissionBudget {
    AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default())
        .for_source_operation(
            ItemLimit::new(200_000_000).unwrap(),
            ItemLimit::new(64).unwrap(),
        )
}

pub(in super::super) fn raw_pending() -> Fixture {
    raw_pending_with_outputs(|_, _| {})
}

pub(in super::super) fn raw_pending_with_outputs(
    change: impl FnOnce(&mut Vec<u8>, &mut Vec<u8>),
) -> Fixture {
    let fixture = Fixture::new(
        "project_location",
        "[profiles.selected.paths]\nassets = 'assets'\n\
         [profiles.dormant.paths]\nscenes = 'absent/scenes'\n",
    );
    fixture.scene(CONTAINING);
    fixture.prefab(DESTINATION);
    fixture.scene(GUARD);
    fixture.metadata(
        DESTINATION,
        "target.prefab.ron",
        FIRST_ID,
        AssetSourceKind::Prefab,
        0,
    );
    let schema = fixture.receipt(Some("selected"));
    let mut budget = budget();
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    let originals: Vec<_> = inventory
        .participants()
        .map(|member| RecoverySourceInput {
            path: member.file().path(),
            key: member.source_key(),
            encoding: member.encoding(),
            explicit_native: member.explicit_native(),
            original_disk: member.bytes(),
        })
        .collect();
    let guards: Vec<_> = inventory
        .metadata()
        .map(|metadata| RecoveryMetadataGuardInput {
            path: metadata.file().path(),
            observed: metadata.bytes(),
        })
        .collect();
    let roots: Vec<_> = inventory
        .roots()
        .map(|root| RecoveryRootInput {
            path: root.path(),
            missing_ancestor: match root.observation() {
                SourceRootObservation::Available { .. } => None,
                SourceRootObservation::Missing { ancestor_path, .. } => Some(ancestor_path),
            },
        })
        .collect();
    let missing: Vec<_> = inventory.missing_sidecars().collect();
    let containing = inventory.participant(CONTAINING).unwrap();
    let destination = inventory.participant(DESTINATION).unwrap();
    let mut containing_output = containing.bytes().to_vec();
    containing_output.extend_from_slice(b"\r\n");
    let mut destination_output = destination.bytes().to_vec();
    destination_output.extend_from_slice(b"\r\n\t");
    change(&mut containing_output, &mut destination_output);
    let recovery = PrefabComponentApplyRecoveryData::create(
        PrefabComponentApplyRecoveryInput {
            ready_manifest: schema.manifest_bytes(),
            bundle_path: schema.bundle_path().as_str(),
            bundle_bytes: schema.bundle_bytes(),
            source_universe: schema.settings().project.source_universe,
            selected_profile: schema.settings().profile_name.as_deref(),
            complete_original_sources: &originals,
            containing: RecoveryOutputInput {
                key: containing.source_key(),
                output: &containing_output,
            },
            destination: RecoveryOutputInput {
                key: destination.source_key(),
                output: &destination_output,
            },
            metadata_guards: &guards,
            roots: &roots,
            missing_sidecars: &missing,
        },
        PrefabComponentApplyRecoveryLimits::default(),
        &mut budget,
    )
    .unwrap();
    let mut manifest = ProjectManifest::parse_toml_bytes(schema.manifest_bytes())
        .manifest
        .unwrap();
    drop(inventory);
    drop(schema);
    fixture.write(RECOVERY, recovery.encoded_bytes());
    manifest.authoring.state = ProjectAuthoringState::Pending(
        ProjectAuthoringRecovery::new(ProjectPath::new(RECOVERY).unwrap(), recovery.digest())
            .unwrap(),
    );
    fixture.write("nara.toml", &manifest.to_canonical_toml_bytes().unwrap());
    fixture
}

pub(in super::super) fn load(
    fixture: &Fixture,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> ObservedPendingRecoveryStorage<PrefabComponentApplyRecoveryData> {
    let limits = PrefabComponentApplyRecoveryLimits::default();
    let loaded = RecoveryStorageHost::load_pending(
        crate::test_host_fs::writable_directory(&fixture.root).unwrap(),
        limits.encoded_bytes,
        budget,
    )
    .unwrap();
    let recovery =
        PrefabComponentApplyRecoveryData::decode(loaded.encoded_bytes(), limits, budget).unwrap();
    loaded.bind(recovery, budget).unwrap()
}

pub(in super::super) fn replace(
    pending: &ObservedPendingRecoveryStorage<PrefabComponentApplyRecoveryData>,
    path: &str,
    bytes: &[u8],
) -> FileIdentity {
    let maximum = SceneFileLimits::default().encoded_bytes().get() as u64;
    let relative = nara_fs::RelativePath::new(path).unwrap();
    let file = pending.root().open_file_exact(&relative).unwrap();
    let digest = file.digest(maximum).unwrap();
    let mut publisher =
        FilePublicationHost::bind_existing(pending.root(), path, &file, digest, maximum).unwrap();
    let FilePublicationOutcome::Published(accepted) = publisher.publish(bytes) else {
        panic!("the fixture must install the exact interrupted output");
    };
    accepted.source().identity()
}

fn capture(
    pending: &ObservedPendingRecoveryStorage<PrefabComponentApplyRecoveryData>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<ComponentApplyPhysicalInventory, ComponentApplyPhysicalError> {
    capture_pending_inventory(pending, SceneFileLimits::default(), budget)
}

#[test]
fn real_mixed_files_preserve_pair_order_and_current_native_leaf() {
    let fixture = raw_pending();
    let mut budget = budget();
    let pending = load(&fixture, &mut budget);
    let [(_, _), (destination, output)] = pending.recovery().outputs();
    let destination_identity = replace(&pending, destination.path(), output.bytes());
    let inventory = capture(&pending, &mut budget).unwrap();
    assert_eq!(inventory.output_is_current(), [false, true]);
    assert_eq!(
        inventory.settings().profile_name.as_deref(),
        Some("selected")
    );
    assert_eq!(
        inventory.settings().authoring_state,
        ProjectAuthoringState::Pending(pending.reference().clone())
    );
    assert_eq!(
        inventory
            .inventory()
            .shared_file(DESTINATION)
            .unwrap()
            .identity(),
        destination_identity
    );
    assert_eq!(
        inventory.bundle_file().parent_identity(),
        Some(inventory.bundle_parent().identity())
    );
    assert_eq!(
        inventory.bundle_digest(),
        pending.recovery().unchanged_bundle().blob().digest()
    );
    inventory.validate(&pending, &mut budget).unwrap();
}

#[test]
fn third_bytes_and_other_dependency_drift_refuse_mixed_admission() {
    for drift in [
        "output",
        "guard",
        "metadata",
        "missing_sidecar",
        "root",
        "bundle",
    ] {
        let fixture = raw_pending();
        let mut budget = budget();
        let pending = load(&fixture, &mut budget);
        let [(_, _), (destination, output)] = pending.recovery().outputs();
        replace(&pending, destination.path(), output.bytes());
        match drift {
            "output" => {
                let mut third = output.bytes().to_vec();
                *third.last_mut().unwrap() = b' ';
                replace(&pending, destination.path(), &third);
            }
            "guard" => {
                let mut changed = fs::read(fixture.root.join(GUARD)).unwrap();
                changed.push(b'\n');
                replace(&pending, GUARD, &changed);
            }
            "metadata" => fixture.metadata(
                DESTINATION,
                "target.prefab.ron",
                FIRST_ID,
                AssetSourceKind::Prefab,
                1,
            ),
            "missing_sidecar" => fixture.metadata(
                CONTAINING,
                "containing.scene.json",
                SECOND_ID,
                AssetSourceKind::Scene,
                0,
            ),
            "root" => fs::create_dir_all(fixture.root.join("absent/scenes")).unwrap(),
            "bundle" => {
                let bundle = pending.recovery().unchanged_bundle();
                let mut changed = bundle.blob().bytes().to_vec();
                changed[0] = b' ';
                replace(&pending, bundle.path(), &changed);
            }
            _ => unreachable!(),
        }
        let error = capture(&pending, &mut budget).unwrap_err();
        match drift {
            "output" => assert!(matches!(
                error,
                ComponentApplyPhysicalError::OutputConflict { index: 1 }
            )),
            "bundle" => assert!(matches!(error, ComponentApplyPhysicalError::BundleChanged)),
            _ => assert!(
                matches!(
                    error,
                    ComponentApplyPhysicalError::InventoryChanged
                        | ComponentApplyPhysicalError::Inventory(_)
                ),
                "{drift}: {error:?}"
            ),
        }
        pending.validate(&mut budget).unwrap();
    }
}

#[test]
fn validation_refuses_equal_bytes_at_a_replaced_leaf() {
    let fixture = raw_pending();
    let mut budget = budget();
    let pending = load(&fixture, &mut budget);
    let inventory = capture(&pending, &mut budget).unwrap();
    assert_eq!(inventory.output_is_current(), [false, false]);
    let retained = inventory.inventory().shared_file(GUARD).unwrap();
    let bytes = inventory.inventory().participant(GUARD).unwrap().bytes();
    assert_ne!(retained.identity(), replace(&pending, GUARD, bytes));
    assert!(matches!(
        inventory.validate(&pending, &mut budget),
        Err(ComponentApplyPhysicalError::Inventory(_))
    ));
    assert_eq!(inventory.output_is_current(), [false, false]);
}
