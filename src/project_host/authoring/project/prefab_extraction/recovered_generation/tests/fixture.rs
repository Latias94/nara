//! Real output/package/Pending publication, followed by fresh native recovery admission.

use nara_identity::{EntityReference, ReferenceInputId};
use nara_scene::{PrefabReferenceArguments, ScenePatchDocument, ScenePatchOperation};

use super::*;

pub(super) const RECOVERY: &str = "recovery/recovered-generation.json";

pub(super) fn budget() -> AuthoringProjectAdmissionBudget {
    AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default())
        .for_source_operation(
            ItemLimit::new(200_000_000).unwrap(),
            ItemLimit::new(64).unwrap(),
        )
}

pub(super) fn publish_pending(fixture: &Fixture, save_current: Vec<EditorDocumentId>) {
    fs::create_dir_all(fixture.root.join("recovery")).unwrap();
    let prepared = fixture
        .request()
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(save_current)
        .unwrap()
        .prepare_outputs()
        .unwrap()
        .prepare_recovery(Default::default())
        .unwrap();
    let mut storage = prepared
        .prepare_storage(ProjectPath::new(RECOVERY).unwrap())
        .unwrap();
    for _ in 0..8 {
        if storage.phase() == ProjectPrefabExtractionStoragePhase::Confirmed {
            assert!(storage.pending().is_some());
            // No live Review, output owner, compiler corpus or publication token is used by
            // the fresh loader. The unchanged fixture Workspace is only a negative control.
            drop(storage);
            return;
        }
        storage = storage.advance().unwrap();
    }
    panic!("the finite package/Pending publisher did not reach Confirmed");
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

pub(super) fn generation(
    pending: ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    observation: RecoveredPrefabExtractionInventory,
    budget: AuthoringProjectAdmissionBudget,
) -> RecoveredPrefabExtractionGeneration {
    RecoveredPrefabExtractionGeneration::new(
        pending,
        observation,
        budget,
        PrefabExtractionWork::new(SceneCompilationLimits::default()).unwrap(),
    )
}

/// Inventory confirmation parses transient configuration models. Its cumulative decoded-byte
/// cost is the independent control for later semantic retries, which must not decode sources.
pub(super) fn validation_decoded_bytes(
    pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    observation: &RecoveredPrefabExtractionInventory,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> usize {
    let before = budget.usage();
    observation.validate(pending, budget).unwrap();
    let after = budget.usage();
    assert_eq!(
        after.retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert_eq!(
        after.retained_open_handles(),
        before.retained_open_handles()
    );
    after.decoded_bytes() - before.decoded_bytes()
}

pub(super) fn complete(
    mut owner: RecoveredPrefabExtractionGeneration,
) -> RecoveredPrefabExtractionGeneration {
    for phase in [Phase::Decode, Phase::Verify, Phase::Saved] {
        assert_eq!(owner.phase(), phase);
        assert!(owner.verified().is_none());
        owner = owner.advance().unwrap();
    }
    assert_eq!(owner.phase(), Phase::Complete);
    assert!(owner.verified().is_some());
    owner
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

/// Rebuild only the deliberately falsified role; every other input is borrowed from an actual
/// producer package. The normal constructor still proves grammar and all byte commitments.
pub(super) fn rebuild(
    original: &PrefabExtractionRecovery,
    q_output: Option<&[u8]>,
    omit_guard_output: bool,
) -> PrefabExtractionRecovery {
    let sources: Vec<_> = original
        .complete_original_sources()
        .iter()
        .map(|source| {
            let omitted = omit_guard_output && source.path() == GUARD;
            RecoveryOriginalSourceInput {
                path: source.path(),
                key: source.key(),
                encoding: source.encoding(),
                explicit_native: source.explicit_native(),
                original_disk: source.original_disk().bytes(),
                reviewed_working: source.reviewed_working().bytes(),
                disposition: if omitted {
                    RecoverySourceDisposition::Guard
                } else {
                    source.disposition()
                },
                output: if omitted {
                    None
                } else {
                    source.output().map(|blob| blob.bytes())
                },
            }
        })
        .collect();
    let metadata: Vec<_> = original
        .metadata_guards()
        .iter()
        .map(|guard| RecoveryMetadataGuardInput {
            path: guard.path(),
            observed: guard.observed().bytes(),
        })
        .collect();
    let roots: Vec<_> = original
        .roots()
        .iter()
        .map(|root| RecoveryRootInput {
            path: root.path(),
            missing_ancestor: match root.observation() {
                RecoveryRootObservation::Present => None,
                RecoveryRootObservation::Missing { ancestor } => Some(ancestor.as_str()),
            },
        })
        .collect();
    let missing: Vec<_> = original
        .missing_sidecars()
        .iter()
        .map(String::as_str)
        .collect();
    PrefabExtractionRecovery::create(
        PrefabExtractionRecoveryInput {
            old_ready_manifest: original.old_ready_manifest().bytes(),
            final_ready_manifest: original.final_ready_manifest().bytes(),
            unchanged_bundle: RecoveryBundleInput {
                path: original.unchanged_bundle().path(),
                blob: original.unchanged_bundle().blob().bytes(),
            },
            source_universe: original.source_universe(),
            selected_profile: original.selected_profile(),
            selection: RecoverySelectionInput {
                source: original.selection().source(),
                root: original.selection().root(),
            },
            complete_original_sources: &sources,
            q: RecoveryNewSourceInput {
                path: original.q().path(),
                key: original.q().key(),
                encoding: original.q().encoding(),
                output: q_output.unwrap_or_else(|| original.q().output().bytes()),
            },
            q_sidecar: RecoveryOutputInput {
                path: original.q_sidecar().path(),
                output: original.q_sidecar().output().bytes(),
            },
            metadata_guards: &metadata,
            roots: &roots,
            missing_sidecars: &missing,
        },
        Default::default(),
        &mut budget(),
    )
    .unwrap()
}

pub(super) fn install_package(
    pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    replacement: &PrefabExtractionRecovery,
) {
    let mut manifest = ProjectManifest::parse_toml_bytes(pending.manifest_bytes())
        .manifest
        .unwrap();
    manifest.authoring.state = ProjectAuthoringState::Pending(
        ProjectAuthoringRecovery::new(ProjectPath::new(RECOVERY).unwrap(), replacement.digest())
            .unwrap(),
    );
    replace(pending, RECOVERY, replacement.encoded_bytes());
    replace(
        pending,
        "nara.toml",
        &manifest.to_canonical_toml_bytes().unwrap(),
    );
}

pub(super) fn canonical(source: &CapturedExtractionSource, encoding: SceneFileEncoding) -> Vec<u8> {
    match (source, encoding) {
        (CapturedExtractionSource::Scene { document, .. }, SceneFileEncoding::Json) => {
            document.to_json_string().unwrap().into_bytes()
        }
        (CapturedExtractionSource::Scene { document, .. }, SceneFileEncoding::Ron) => {
            document.to_ron_string().unwrap().into_bytes()
        }
        (CapturedExtractionSource::Prefab { source }, SceneFileEncoding::Json) => {
            source.document().to_json_string().unwrap().into_bytes()
        }
        (CapturedExtractionSource::Prefab { source }, SceneFileEncoding::Ron) => {
            source.document().to_ron_string().unwrap().into_bytes()
        }
    }
}

/// Two actual moved instance arguments require distinct stored Q input names, without a
/// test-only schema, value evaluator or replacement identity allocator.
pub(super) fn add_outward_arguments(fixture: &mut Fixture) {
    let path = "desktop/prefabs/argument.prefab.json";
    let key = SourceDocumentKey::project_location(
        UNIVERSE.parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        SourceDocumentKind::Prefab,
    );
    let input = ReferenceInputId::new("target").unwrap();
    let mut dependency = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("child"))]);
    dependency.inputs.insert(input.clone());
    fs::write(
        fixture.root.join(path),
        dependency.to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(
        fixture.root.join(format!("{path}.meta")),
        AssetMeta::new(
            metadata(
                "argument.prefab.json",
                "b7827fb9-28fa-4c04-847f-000000000020",
            ),
            0_u32,
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let document = fixture.document(P);
    let operations = [("branch", "outside"), ("desktop-leaf", "trunk")].map(|(entity, target)| {
        let mut instance = PrefabInstance::forest(AssetRef::path("argument.prefab.json").unwrap());
        instance.arguments = Some(PrefabReferenceArguments {
            base: key.clone(),
            values: [(
                input.clone(),
                EntityReference::SceneLocal { entity: id(target) },
            )]
            .into(),
        });
        ScenePatchOperation::SetPrefabInstance {
            entity: id(entity),
            expected: None,
            value: Some(instance),
        }
    });
    let base_revision = fixture.workspace.document_revision(document).unwrap();
    let report =
        fixture
            .workspace
            .apply_authoring_command(EditorWorkspaceCommand::ApplyPrefabPatch {
                document,
                base_revision,
                patch: ScenePatchDocument::new(operations),
            });
    assert!(report.applied, "{:?}", report.diagnostics);
}
