//! Actual raw files and concrete Pending data for physical-only interruption controls.
//!
//! No stored-pair semantic proof is asserted here. Only the normal transport, bundle model
//! owner, native source policies and neutral complete inventory supply their real evidence.

use super::*;

pub(super) const RECOVERY: &str = "recovery/recovered-inventory.json";
pub(super) const BUNDLE: &str = "schema/native/bundle.json";

pub(super) fn raw_pending(stable_metadata: bool, profile: Option<&str>) -> Fixture {
    let fixture = Fixture::with_profiles(
        "mobile/assets",
        "[profiles.dormant.paths]\nassets = 'absent/assets'\nscenes = 'absent/scenes'\n\
         prefabs = 'absent/prefabs'\n[profiles.dormant.startup]\nclear_default_scene = true\n",
    );
    fs::create_dir_all(fixture.root.join("schema/native")).unwrap();
    fs::create_dir_all(fixture.root.join("recovery")).unwrap();
    fs::write(
        fixture.root.join(BUNDLE),
        fs::read(fixture.root.join("schema.json")).unwrap(),
    )
    .unwrap();
    let mut manifest =
        ProjectManifest::parse_toml_bytes(&fs::read(fixture.root.join("nara.toml")).unwrap())
            .manifest
            .unwrap();
    manifest.authoring.schema_bundle = Some(BUNDLE.to_owned());
    if stable_metadata {
        manifest.project.native_document_identity = NativeDocumentIdentity::StableMetadata;
        for (path, local, stable, kind) in [
            (P, ALIAS, P_ID, AssetSourceKind::Prefab),
            (R, ALIAS, R_ID, AssetSourceKind::Prefab),
            (
                SHARED,
                "shared.scene.json",
                "b7827fb9-28fa-4c04-847f-000000000010",
                AssetSourceKind::Scene,
            ),
            (
                CLOSED,
                "closed.scene.json",
                "b7827fb9-28fa-4c04-847f-000000000011",
                AssetSourceKind::Scene,
            ),
            (
                INACTIVE,
                "inactive.scene.json",
                "b7827fb9-28fa-4c04-847f-000000000012",
                AssetSourceKind::Scene,
            ),
            (
                GUARD,
                "guard.prefab.ron",
                "b7827fb9-28fa-4c04-847f-000000000013",
                AssetSourceKind::Prefab,
            ),
        ] {
            fs::write(
                fixture.root.join(format!("{path}.meta")),
                AssetMeta::new(
                    AssetMetaIdentity::new(
                        stable.parse().unwrap(),
                        AssetPath::new(local).unwrap(),
                        kind,
                    ),
                    0_u32,
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap();
        }
    }
    fs::write(
        fixture.root.join("nara.toml"),
        manifest.to_canonical_toml_bytes().unwrap(),
    )
    .unwrap();
    let schema = AuthoringProjectAdmissionHost::default()
        .prepare(
            crate::test_host_fs::writable_directory(&fixture.root).unwrap(),
            profile,
        )
        .unwrap()
        .admit_bundle()
        .unwrap()
        .publish();
    let mut budget = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    let output = actor_prefab("output-leaf").to_json_string().unwrap();
    let originals: Vec<_> = inventory
        .participants()
        .map(|source| RecoveryOriginalSourceInput {
            path: source.file().path(),
            key: source.source_key(),
            encoding: match source.encoding() {
                SourceEncoding::Json => RecoverySourceEncoding::Json,
                SourceEncoding::Ron => RecoverySourceEncoding::Ron,
            },
            explicit_native: source.explicit_native(),
            original_disk: source.bytes(),
            reviewed_working: source.bytes(),
            disposition: if source.file().path() == P {
                RecoverySourceDisposition::ReplaceSource
            } else {
                RecoverySourceDisposition::Guard
            },
            output: (source.file().path() == P).then_some(output.as_bytes()),
        })
        .collect();
    let metadata: Vec<_> = inventory
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
                SourceRootObservation::Missing { ancestor_path, .. } => {
                    Some(ancestor_path.as_str())
                }
            },
        })
        .collect();
    let missing: Vec<_> = inventory.missing_sidecars().collect();
    let q_path = format!(
        "{}/new.prefab.json",
        schema.settings().paths.prefabs.as_str()
    );
    let q_key = if stable_metadata {
        SourceDocumentKey::stable_asset(Q_ID.parse().unwrap(), SourceDocumentKind::Prefab)
    } else {
        SourceDocumentKey::project_location(
            manifest.project.source_universe,
            AssetSourcePath::new(&q_path).unwrap(),
            SourceDocumentKind::Prefab,
        )
    };
    let q = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("branch"))])
        .to_json_string()
        .unwrap();
    let q_sidecar_path = format!("{q_path}.meta");
    let sidecar = AssetMeta::new(super::super::metadata("new.prefab.json", Q_ID), 0_u32)
        .to_json_string()
        .unwrap();
    let recovery = PrefabExtractionRecovery::create(
        PrefabExtractionRecoveryInput {
            old_ready_manifest: schema.manifest_bytes(),
            final_ready_manifest: schema.manifest_bytes(),
            unchanged_bundle: RecoveryBundleInput {
                path: BUNDLE,
                blob: schema.bundle_bytes(),
            },
            source_universe: manifest.project.source_universe,
            selected_profile: profile,
            selection: RecoverySelectionInput {
                source: inventory.participant(P).unwrap().source_key(),
                root: &id("branch"),
            },
            complete_original_sources: &originals,
            q: RecoveryNewSourceInput {
                path: &q_path,
                key: &q_key,
                encoding: RecoverySourceEncoding::Json,
                output: q.as_bytes(),
            },
            q_sidecar: RecoveryOutputInput {
                path: &q_sidecar_path,
                output: sidecar.as_bytes(),
            },
            metadata_guards: &metadata,
            roots: &roots,
            missing_sidecars: &missing,
        },
        Default::default(),
        &mut budget,
    )
    .unwrap();
    fs::write(fixture.root.join(RECOVERY), recovery.encoded_bytes()).unwrap();
    manifest.authoring.state = ProjectAuthoringState::Pending(
        ProjectAuthoringRecovery::new(ProjectPath::new(RECOVERY).unwrap(), recovery.digest())
            .unwrap(),
    );
    fs::write(
        fixture.root.join("nara.toml"),
        manifest.to_canonical_toml_bytes().unwrap(),
    )
    .unwrap();
    fixture
}

pub(super) fn load(
    fixture: &Fixture,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> ObservedPendingRecoveryStorage<PrefabExtractionRecovery> {
    load_pending(
        crate::test_host_fs::writable_directory(&fixture.root).unwrap(),
        Default::default(),
        budget,
    )
    .unwrap()
}

pub(super) fn replace(
    pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    path: &str,
    bytes: &[u8],
) {
    let file = pending
        .root()
        .open_file_exact(&RelativePath::new(path).unwrap())
        .unwrap();
    let maximum = 64 * 1024 * 1024;
    let digest = file.digest(maximum).unwrap();
    let mut publisher =
        FilePublicationHost::bind_existing(pending.root(), path, &file, digest, maximum).unwrap();
    assert!(matches!(
        publisher.publish(bytes),
        FilePublicationOutcome::Published(_)
    ));
}
