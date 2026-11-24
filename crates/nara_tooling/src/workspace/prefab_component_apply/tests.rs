use std::{
    convert::Infallible,
    fs::{self, OpenOptions},
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_asset::{AssetRef, AssetSourcePath};
use nara_core::{ByteLimit, DocumentHistoryStatus};
use nara_fs::{FileCapability, TrustMode};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::{
        AuthoringAdapterReceipt, PrefabComponentApplyRequest, PrefabInspectionInput,
        try_inspect_authoring_prefab_instance_with_loader,
        try_prepare_prefab_component_apply_with_loader,
    },
    AuthoringDocumentSource, AuthoringFieldEdit, AuthoringSchemaBasis, PrefabComponentIntent,
    PrefabComponentSetMode, PrefabDocument, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance,
    PrefabInstanceHop, PrefabOverlay, ResolvedPrefabSource, SceneAuthoringSession,
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, SourceDocumentKind,
};

use super::*;
use crate::{EditorExternalReloadState, EditorWorkspaceCommand};

const DESTINATION: &str = "prefabs/base.prefab.ron";

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.component-apply.Counter")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.component-apply").unwrap()
}

fn value(number: i64) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("value", ComponentValue::I64(number))]),
    )
    .with_owner(owner())
}

fn schema() -> AuthoringSchemaBasis {
    let capability = ComponentCapability::SCENE_AUTHORING;
    let catalog = ComponentSchemaCatalog {
        components: vec![
            ComponentSchema::new(component(), "Counter", ComponentSchemaVersion::ONE)
                .with_capabilities(capability)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("value"),
                    "Value",
                    ComponentFieldPath::from_fields(["value"]),
                    ComponentValueKind::I64,
                )
                .with_capabilities(capability)]),
        ],
        ..Default::default()
    };
    let bundle =
        AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(owner(), catalog, None)])
            .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn key(path: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "e5d4639d-99fd-431a-9731-64eb2fb61a95".parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        kind,
    )
}

struct Fixture {
    workspace: EditorWorkspace,
    authority: EditorPersistenceAuthority,
    containing: EditorDocumentId,
    destination: EditorDocumentId,
    containing_key: SourceDocumentKey,
    destination_key: SourceDocumentKey,
    schema: AuthoringSchemaBasis,
}

const WORKING_LIMIT: usize = 64 * 1024;
const DESTINATION_LIMIT: usize = 128 * 1024;

fn limits(length: usize) -> SceneFileLimits {
    SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(length).unwrap())
}

impl Fixture {
    fn new(containing_kind: SourceDocumentKind) -> Self {
        let schema = schema();
        let (mut workspace, authority) = EditorWorkspace::new_hosted();
        let destination_key = key(DESTINATION, SourceDocumentKind::Prefab);
        let destination_doc =
            PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("base-root")).with_component(component(), value(3))
            ]);
        let destination_source = PrefabDocumentCandidate::decode_ron_bytes_with_limits(
            destination_doc.to_ron_string().unwrap().as_bytes(),
            limits(DESTINATION_LIMIT),
        )
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path(DESTINATION).unwrap()),
            &schema,
        )
        .unwrap()
        .publish();
        let destination = workspace
            .open_prefab_session(
                "Destination",
                EditorDocumentSourceBinding::new(DESTINATION),
                PrefabAuthoringSession::from_published_authoring_document(destination_source),
            )
            .unwrap()
            .opened_document
            .unwrap();
        let mut instance = PrefabInstance::root(
            AssetRef::path(DESTINATION).unwrap(),
            destination_key.clone(),
            PrefabEntityTarget::source(id("base-root")),
        );
        let mut overlay = PrefabOverlay::new(destination_key.clone());
        overlay.components.push(PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Replace,
            target: PrefabEntityTarget::source(id("base-root")),
            component: component(),
            record: value(7),
        });
        instance.overrides = Some(overlay);
        let mut mount = SceneEntityRecord::new(id("mount"));
        mount.prefab = Some(instance);
        let roots = [
            mount,
            SceneEntityRecord::new(id("marker")).with_component(component(), value(11)),
        ];
        let (containing, path) = if containing_kind == SourceDocumentKind::Scene {
            let path = "scenes/containing.scene.json";
            let source = SceneDocument::from_ordered_roots(roots);
            let published = SceneDocumentCandidate::decode_json_bytes_with_limits(
                source.to_json_string().unwrap().as_bytes(),
                limits(WORKING_LIMIT),
            )
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path(path).unwrap()),
                &schema,
            )
            .unwrap()
            .publish();
            let report = workspace
                .open_scene_session_with_source_binding(
                    "Containing",
                    Some(EditorDocumentSourceBinding::new(path)),
                    SceneAuthoringSession::from_published_authoring_document(published),
                )
                .unwrap();
            (report.opened_document.unwrap(), path)
        } else {
            let path = "prefabs/containing.prefab.json";
            let source = PrefabDocument::from_ordered_roots(roots);
            let published = PrefabDocumentCandidate::decode_json_bytes_with_limits(
                source.to_json_string().unwrap().as_bytes(),
                limits(WORKING_LIMIT),
            )
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path(path).unwrap()),
                &schema,
            )
            .unwrap()
            .publish();
            let report = workspace
                .open_prefab_session(
                    "Containing",
                    EditorDocumentSourceBinding::new(path),
                    PrefabAuthoringSession::from_published_authoring_document(published),
                )
                .unwrap();
            (report.opened_document.unwrap(), path)
        };
        let edit = AuthoringFieldEdit::single(
            id("marker"),
            component(),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new("value"),
            ComponentValue::I64(13),
        )
        .unwrap();
        let query = workspace.document_query_basis(containing).unwrap();
        let prepared = workspace
            .prepare_authoring_edit(&query, &edit.to_patch())
            .unwrap();
        assert!(workspace.commit_authoring_edit(prepared).applied);
        for (document, entity) in [(containing, "marker"), (destination, "base-root")] {
            let report = workspace.apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
                document,
                entity: Some(id(entity)),
            });
            assert!(!report.diagnostics.has_errors());
        }
        workspace.apply_authoring_command(EditorWorkspaceCommand::SetActiveDocument {
            document: destination,
        });
        Self {
            workspace,
            authority,
            containing,
            destination,
            containing_key: key(path, containing_kind),
            destination_key,
            schema,
        }
    }

    fn capture(&self) -> EditorWorkspacePrefabComponentApplyInput {
        self.authority
            .capture_prefab_component_apply(
                &self.workspace,
                &self
                    .workspace
                    .document_query_basis(self.containing)
                    .unwrap(),
                &self.containing_key,
                &self
                    .workspace
                    .document_query_basis(self.destination)
                    .unwrap(),
                &self.destination_key,
                |bytes| {
                    assert!(bytes.is_some());
                    Ok(())
                },
            )
            .unwrap()
    }

    fn pair(
        &self,
        input: &EditorWorkspacePrefabComponentApplyInput,
    ) -> PreparedPrefabComponentApply {
        let sources = input.source_pair();
        let scene = sources.shared_containing_scene();
        let prefab = sources.shared_containing_prefab();
        let revision = sources.containing_revision();
        let inspection_input = match (&scene, &prefab) {
            (Some(document), None) => PrefabInspectionInput::WorkingScene(document, revision),
            (None, Some(document)) => PrefabInspectionInput::WorkingPrefab(document, revision),
            _ => panic!("Expected one captured containing source"),
        };
        let mut load = |asset: &AssetRef, _: &mut nara_scene::SceneCompilationWorkingSet| {
            assert_eq!(asset, &AssetRef::path(DESTINATION).unwrap());
            Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring_source(
                sources.shared_destination(),
                self.schema.clone(),
                self.destination_key.clone(),
            )))
        };
        let inspection = try_inspect_authoring_prefab_instance_with_loader(
            inspection_input,
            &id("mount"),
            &self.schema,
            Default::default(),
            &mut load,
        )
        .unwrap();
        let address = PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("mount")),
                expected_base: self.destination_key.clone(),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id("base-root")),
        };
        try_prepare_prefab_component_apply_with_loader(
            PrefabComponentApplyRequest {
                sources,
                inspection: &inspection,
                selected: &address,
                destination_route: &address,
                component: &component(),
                declaration_limits: Default::default(),
                compilation: Default::default(),
            },
            load,
        )
        .unwrap()
    }

    fn prepare(
        &self,
    ) -> (
        PreparedEditorWorkspacePrefabComponentApply,
        [EncodedEditorPersistenceCheckpoint; 2],
    ) {
        let input = self.capture();
        let pair = self.pair(&input);
        input.prepare_persistence(pair, |_| Ok(())).unwrap()
    }
}

fn observed_commit(encoded: EncodedEditorPersistenceCheckpoint) -> EditorPersistenceCommit {
    static TEST_FILE: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "nara-component-apply-{}-{}",
        std::process::id(),
        TEST_FILE.fetch_add(1, Ordering::Relaxed),
    ));
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    file.write_all(encoded.bytes()).unwrap();
    file.sync_all().unwrap();
    let file = FileCapability::from_host_handle(file, TrustMode::TrustedLocal, 0).unwrap();
    let result = EditorPersistenceCommit::from_observed_file(encoded, &file);
    drop(file);
    fs::remove_file(path).unwrap();
    result.unwrap()
}

#[test]
fn paired_bind_is_reversible_and_installs_exact_saved_histories() {
    for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
        let mut fixture = Fixture::new(kind);
        let documents = [fixture.containing, fixture.destination];
        let before = fixture.workspace.authoring_model();
        let selection =
            documents.map(|document| fixture.workspace.documents[&document].selection().clone());
        assert_eq!(before.active_document, Some(fixture.destination));
        assert!(before.document(fixture.containing).unwrap().dirty);
        assert_eq!(
            before
                .document(fixture.containing)
                .unwrap()
                .history
                .undo_depth,
            1,
        );
        let (abandoned, abandoned_outputs) = fixture.prepare();
        drop(
            fixture
                .authority
                .bind_prefab_component_apply(
                    &mut fixture.workspace,
                    abandoned,
                    abandoned_outputs.map(observed_commit),
                )
                .unwrap(),
        );
        assert_eq!(fixture.workspace.authoring_model(), before);
        let (prepared, [containing_output, destination_output]) = fixture.prepare();
        let pair = prepared.scene_pair();
        let future_scene = pair.shared_containing_scene();
        let future_prefab = pair.shared_containing_prefab();
        let future_destination = pair.shared_destination();
        let expected_containing = if let Some(scene) = &future_scene {
            scene.to_json_string().unwrap()
        } else {
            future_prefab.as_ref().unwrap().to_json_string().unwrap()
        };
        let expected_destination = future_destination.to_ron_string().unwrap();
        assert_eq!(containing_output.bytes(), expected_containing.as_bytes());
        assert_eq!(destination_output.bytes(), expected_destination.as_bytes());
        assert_eq!(
            prepared.file_limits(fixture.containing),
            Some(limits(WORKING_LIMIT)),
        );
        assert_eq!(
            prepared.file_limits(fixture.destination),
            Some(limits(DESTINATION_LIMIT)),
        );
        let revisions = [
            pair.containing_future_revision(),
            pair.destination_future_revision(),
        ];
        let containing_commit = observed_commit(containing_output);
        let destination_commit = observed_commit(destination_output);
        let digests = [containing_commit.digest(), destination_commit.digest()];
        let refused = fixture
            .authority
            .bind_prefab_component_apply(
                &mut fixture.workspace,
                prepared,
                [destination_commit, containing_commit],
            )
            .unwrap_err();
        let (prepared, [destination_commit, containing_commit], report) = refused.into_parts();
        assert_eq!(report.document, Some(fixture.containing));
        assert_eq!(fixture.workspace.authoring_model(), before);
        let (prepared, commits) = fixture
            .authority
            .bind_prefab_component_apply(
                &mut fixture.workspace,
                prepared,
                [containing_commit, destination_commit],
            )
            .unwrap()
            .into_prepared();
        assert_eq!(fixture.workspace.authoring_model(), before);
        let commits = fixture
            .authority
            .bind_prefab_component_apply(&mut fixture.workspace, prepared, commits)
            .unwrap();
        commits.commit();
        assert_eq!(fixture.workspace.active_document(), before.active_document);
        match &fixture.workspace.documents[&fixture.containing] {
            EditorDocumentSlot::Scene(slot) => {
                assert!(Arc::ptr_eq(
                    &slot.session.shared_document(),
                    &future_scene.unwrap()
                ));
            }
            EditorDocumentSlot::Prefab(slot) => {
                assert!(Arc::ptr_eq(
                    &slot.session.shared_document(),
                    &future_prefab.unwrap()
                ));
            }
            EditorDocumentSlot::Extension(_) => unreachable!(),
        }
        let destination_slot = fixture.workspace.prefab(fixture.destination).unwrap();
        assert!(Arc::ptr_eq(
            &destination_slot.session().shared_document(),
            &future_destination,
        ));
        for (index, document) in documents.into_iter().enumerate() {
            let slot = &fixture.workspace.documents[&document];
            assert_eq!(slot.revision(), revisions[index]);
            assert_eq!(slot.saved_revision(), revisions[index]);
            assert_ne!(
                slot.revision().source_id(),
                before.document(document).unwrap().revision.source_id(),
            );
            assert_eq!(slot.history_status(), DocumentHistoryStatus::default());
            assert_eq!(slot.selection(), &selection[index]);
            assert!(!slot.is_dirty());
            assert_eq!(slot.external_reload(), EditorExternalReloadState::Clean);
            let digest = match slot {
                EditorDocumentSlot::Scene(slot) => slot.saved_digest(),
                EditorDocumentSlot::Prefab(slot) => slot.saved_digest(),
                EditorDocumentSlot::Extension(_) => unreachable!(),
            };
            assert_eq!(digest, Some(digests[index]));
        }
    }
}

#[test]
fn exact_checkpoint_rejects_a_different_encoding_of_the_same_capture() {
    let mut fixture = Fixture::new(SourceDocumentKind::Scene);
    let before = fixture.workspace.authoring_model();
    let (prepared, [containing, destination]) = fixture.prepare();
    let expected_digest = containing.digest();
    let alternate = containing
        .into_checkpoint()
        .encode(SceneFileEncoding::Ron, limits(WORKING_LIMIT))
        .unwrap();
    assert_ne!(alternate.digest(), expected_digest);
    let failure = fixture
        .authority
        .bind_prefab_component_apply(
            &mut fixture.workspace,
            prepared,
            [observed_commit(alternate), observed_commit(destination)],
        )
        .unwrap_err();
    assert_eq!(failure.report().document, Some(fixture.containing));
    assert_eq!(fixture.workspace.authoring_model(), before);
}

#[test]
fn preparation_and_late_bind_require_the_original_capture_and_saved_bases() {
    let mut fixture = Fixture::new(SourceDocumentKind::Scene);
    let input = fixture.capture();
    let pair = fixture.pair(&input);
    let other_input = fixture.capture();
    let failure = other_input
        .prepare_persistence(pair, |_| Ok(()))
        .unwrap_err();
    let (_, pair, report) = failure.into_parts();
    assert_eq!(report.document, Some(fixture.containing));
    let failure = input
        .prepare_persistence(pair, |_| {
            Err(ScenePatchReport {
                applied: false,
                inverse: None,
                diagnostics: Default::default(),
            })
        })
        .unwrap_err();
    let (input, pair, report) = failure.into_parts();
    assert!(report.patch_report.is_some());
    assert!(input.source_pair().owns_prepared(&pair));
    let (prepared, outputs) = input.prepare_persistence(pair, |_| Ok(())).unwrap();
    let previous = fixture
        .workspace
        .document_revision(fixture.destination)
        .unwrap();
    let (foreign, _) = EditorWorkspace::new_hosted();
    assert!(prepared.validate(&foreign).is_err());
    let changed = fixture.authority.bind_opened_source_digest(
        &mut fixture.workspace,
        fixture.destination,
        EditorDocumentDigest::new(12, [7; 32]),
    );
    assert!(changed.applied);
    assert_eq!(
        fixture.workspace.document_revision(fixture.destination),
        Some(previous),
    );
    let failure = fixture
        .authority
        .bind_prefab_component_apply(
            &mut fixture.workspace,
            prepared,
            outputs.map(observed_commit),
        )
        .unwrap_err();
    let (prepared, commits, report) = failure.into_parts();
    assert_eq!(report.document, Some(fixture.destination));
    assert_eq!(commits[0].document(), fixture.containing);
    assert_eq!(commits[1].document(), fixture.destination);
    assert_eq!(prepared.scene_pair().destination_revision(), previous);
    let input = fixture.capture();
    fixture
        .workspace
        .apply_authoring_command(EditorWorkspaceCommand::MarkExternalChanged {
            document: fixture.destination,
        });
    assert_eq!(
        input.validate(&fixture.workspace).unwrap_err().document,
        Some(fixture.destination),
    );
}
