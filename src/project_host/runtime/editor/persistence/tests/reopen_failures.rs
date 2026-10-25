use super::*;
use nara_scene::SceneAuthoringContentState;
use nara_tooling::{
    EditorPersistenceFailureStage, EditorPersistenceRequestKind, EditorWorkspaceModel,
};

#[derive(Debug, PartialEq)]
struct DocumentState {
    workspace: EditorWorkspaceModel,
    bytes: String,
    content_state: SceneAuthoringContentState,
    saved_content_state: SceneAuthoringContentState,
    saved_digest: Option<nara_tooling::EditorDocumentDigest>,
}

fn document_state(editor: &EditorProjectSession, document: EditorDocumentId) -> DocumentState {
    let (bytes, content_state, saved_content_state, saved_digest) =
        match editor.workspace.document_kind(document).unwrap() {
            EditorDocumentKind::Scene => {
                let slot = editor.workspace.scene(document).unwrap();
                (
                    slot.session().document().to_json_string().unwrap(),
                    slot.session().content_state(),
                    slot.saved_content_state(),
                    slot.saved_digest(),
                )
            }
            EditorDocumentKind::Prefab => {
                let slot = editor.workspace.prefab(document).unwrap();
                (
                    slot.session().document().to_json_string().unwrap(),
                    slot.session().content_state(),
                    slot.saved_content_state(),
                    slot.saved_digest(),
                )
            }
            EditorDocumentKind::Extension(_) => unreachable!(),
        };
    DocumentState {
        workspace: editor.workspace.authoring_model(),
        bytes,
        content_state,
        saved_content_state,
        saved_digest,
    }
}

fn dirty_with_history(editor: &mut EditorProjectSession, document: EditorDocumentId) {
    assert!(editor.workspace.activate_document(document).applied);
    edit(editor, document, "working");
    edit(editor, document, "redo-only");
    let base_revision = editor.workspace.document_revision(document).unwrap();
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::Undo {
                document,
                base_revision,
            })
            .applied
    );
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::SelectEntity {
                document,
                entity: Some(SceneEntityId::new("working").unwrap()),
            })
            .applied
    );
    let state = document_state(editor, document);
    let tab = state.workspace.document(document).unwrap();
    assert!(tab.dirty);
    assert_eq!((tab.history.undo_depth, tab.history.redo_depth), (1, 1));
    assert_eq!(
        state.workspace.active_scene.unwrap().selection.top_entity(),
        Some(&SceneEntityId::new("working").unwrap())
    );
    assert!(state.saved_digest.is_some());
}

fn request_reopen(editor: &mut EditorProjectSession, document: EditorDocumentId) {
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document),
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
}

fn assert_successful_retry(editor: &mut EditorProjectSession, document: EditorDocumentId) {
    let before = document_state(editor, document);
    request_reopen(editor, document);
    assert!(matches!(
        settle(editor),
        EditorPersistenceResult::Opened { document: opened, .. } if opened == document
    ));
    let after = document_state(editor, document);
    let old_tab = before.workspace.document(document).unwrap();
    let tab = after.workspace.document(document).unwrap();
    assert_ne!(tab.revision.source_id(), old_tab.revision.source_id());
    assert_eq!(tab.source_binding, old_tab.source_binding);
    assert_eq!(tab.saved_revision, tab.revision);
    assert_eq!((tab.history.undo_depth, tab.history.redo_depth), (0, 0));
    assert!(!tab.dirty);
    assert_eq!(after.content_state, after.saved_content_state);
    acknowledge(editor);
}

#[derive(Clone, Copy, Debug)]
enum FailureCase {
    Deleted,
    Directory,
    Decode,
    Validate,
}

#[test]
fn scene_and_prefab_reopen_preserve_failures_history_and_baseline_then_allow_retry() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        for failure in [
            FailureCase::Deleted,
            FailureCase::Directory,
            FailureCase::Decode,
            FailureCase::Validate,
        ] {
            let fixture = Fixture::new();
            let mut editor = fixture.editor();
            let document = fixture.add(&mut editor, kind, "reopen.json");
            let directory = if kind == EditorDocumentKind::Scene {
                "scenes"
            } else {
                "prefabs"
            };
            let path = fixture.root.join(directory).join("reopen.json");
            let previous = path.with_extension("previous");
            let original = fs::read(&path).unwrap();
            dirty_with_history(&mut editor, document);
            let before = document_state(&editor, document);
            match failure {
                // Keep the old object alive under another name so identity reuse cannot
                // disguise the new target when the original bytes are restored below.
                FailureCase::Deleted => fs::rename(&path, &previous).unwrap(),
                FailureCase::Directory => {
                    fs::rename(&path, &previous).unwrap();
                    fs::create_dir(&path).unwrap();
                }
                FailureCase::Decode => fs::write(&path, b"{ malformed").unwrap(),
                FailureCase::Validate => {
                    let duplicate =
                        SceneEntityRecord::new(SceneEntityId::new("duplicate").unwrap());
                    let bytes = if kind == EditorDocumentKind::Scene {
                        SceneDocument::from_ordered_roots([duplicate.clone(), duplicate])
                            .to_json_string()
                            .unwrap()
                    } else {
                        PrefabDocument::from_ordered_roots([duplicate.clone(), duplicate])
                            .to_json_string()
                            .unwrap()
                    };
                    fs::write(&path, bytes).unwrap();
                }
            }
            request_reopen(&mut editor, document);
            let result = settle(&mut editor);
            match failure {
                FailureCase::Deleted => assert_eq!(
                    result,
                    EditorPersistenceResult::Rejected {
                        request: EditorPersistenceRequestKind::Reopen,
                        document: Some(document),
                        reason: EditorPersistenceRejection::TargetDeleted,
                    }
                ),
                FailureCase::Directory => assert!(matches!(
                    result,
                    EditorPersistenceResult::Failed {
                        request: EditorPersistenceRequestKind::Reopen,
                        document: Some(failed),
                        stage: EditorPersistenceFailureStage::OpenTarget
                            | EditorPersistenceFailureStage::ReadTarget,
                    } if failed == document
                )),
                FailureCase::Decode | FailureCase::Validate => assert_eq!(
                    result,
                    EditorPersistenceResult::Failed {
                        request: EditorPersistenceRequestKind::Reopen,
                        document: Some(document),
                        stage: if matches!(failure, FailureCase::Decode) {
                            EditorPersistenceFailureStage::Decode
                        } else {
                            EditorPersistenceFailureStage::Validate
                        },
                    }
                ),
            }
            assert_eq!(
                document_state(&editor, document),
                before,
                "{kind:?}: {failure:?}"
            );
            acknowledge(&mut editor);
            if matches!(failure, FailureCase::Directory) {
                fs::remove_dir(&path).unwrap();
            }
            fs::write(&path, &original).unwrap();

            // Save still uses the original identity/digest, not the failed Reopen's observations.
            assert!(matches!(
                editor.request_persistence(EditorPersistenceCommand::Save {
                    document: Some(document),
                }),
                EditorPersistenceRequestResult::Accepted { .. }
            ));
            let saved = settle(&mut editor);
            if matches!(failure, FailureCase::Deleted | FailureCase::Directory) {
                assert_eq!(
                    saved,
                    EditorPersistenceResult::Rejected {
                        request: EditorPersistenceRequestKind::Save,
                        document: Some(document),
                        reason: EditorPersistenceRejection::TargetChanged,
                    }
                );
                assert_eq!(document_state(&editor, document), before);
                assert_eq!(fs::read(&path).unwrap(), original);
            } else {
                assert!(matches!(
                    saved,
                    EditorPersistenceResult::Saved { document: saved, .. } if saved == document
                ));
            }
            acknowledge(&mut editor);
            let persisted = fs::read_to_string(&path).unwrap();
            edit(&mut editor, document, "retry-only");
            assert_successful_retry(&mut editor, document);
            assert_eq!(document_state(&editor, document).bytes, persisted);
            assert_eq!(fs::read_to_string(&path).unwrap(), persisted);
            close(&mut editor);
        }
    }
}

#[test]
fn scene_and_prefab_reopen_reject_file_replacement_after_preparation() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let document = fixture.add(&mut editor, kind, "replacement.json");
        let directory = if kind == EditorDocumentKind::Scene {
            "scenes"
        } else {
            "prefabs"
        };
        let path = fixture.root.join(directory).join("replacement.json");
        let original = fs::read(&path).unwrap();
        dirty_with_history(&mut editor, document);
        let before = document_state(&editor, document);
        let gate = editor.hold_next_reopen(ReopenTestPhase::AfterPreparation);
        request_reopen(&mut editor, document);
        editor.advance_persistence();
        assert!(gate.wait_until_started());
        let previous = path.with_extension("previous");
        fs::rename(&path, &previous).unwrap();
        fs::write(&path, &original).unwrap();
        gate.release();
        assert_eq!(
            settle(&mut editor),
            EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                reason: EditorPersistenceRejection::SourceBindingMismatch,
            }
        );
        assert_eq!(document_state(&editor, document), before);
        assert_eq!(fs::read(&path).unwrap(), original);
        acknowledge(&mut editor);
        fs::remove_file(&path).unwrap();
        fs::rename(&previous, &path).unwrap();
        assert_successful_retry(&mut editor, document);
        assert_eq!(
            document_state(&editor, document).bytes.as_bytes(),
            original.as_slice()
        );
        close(&mut editor);
    }
}

#[test]
fn missing_sidecar_rejects_reopen_but_cannot_mask_a_preparation_decode_failure() {
    use crate::project_host::authoring::AuthoringDocumentKind;
    use crate::project_host::runtime::editor::document_open::EditorDocumentOpenRequestResult;
    use nara_asset::{AssetMeta, AssetMetaIdentity, AssetSourceKind, StableAssetId};

    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new();
        let manifest = fixture.root.join("nara.toml");
        fs::write(
            &manifest,
            fs::read_to_string(&manifest)
                .unwrap()
                .replace("project_location", "stable_metadata"),
        )
        .unwrap();
        let (path, logical, source_kind, authoring_kind, original) = match kind {
            EditorDocumentKind::Scene => (
                "scenes/sidecar.scene.json",
                "sidecar.scene.json",
                AssetSourceKind::Scene,
                AuthoringDocumentKind::Scene,
                SceneDocument::default().to_json_string().unwrap(),
            ),
            EditorDocumentKind::Prefab => (
                "prefabs/sidecar.prefab.json",
                "sidecar.prefab.json",
                AssetSourceKind::Prefab,
                AuthoringDocumentKind::Prefab,
                PrefabDocument::default().to_json_string().unwrap(),
            ),
            EditorDocumentKind::Extension(_) => unreachable!(),
        };
        for (path, logical, source_kind, id) in [
            (
                "scenes/startup.scene.json",
                "startup.scene.json",
                AssetSourceKind::Scene,
                "a48d60e2-8ede-4103-b721-f6a0533fb99f",
            ),
            (
                path,
                logical,
                source_kind,
                "be8c37d3-2cd3-4cdf-9479-004f3cd0866d",
            ),
        ] {
            fs::write(
                fixture.root.join(format!("{path}.meta")),
                AssetMeta::new(
                    AssetMetaIdentity::new(
                        StableAssetId::parse_str(id).unwrap(),
                        AssetPath::new(logical).unwrap(),
                        source_kind,
                    ),
                    (),
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap();
        }
        fs::write(fixture.root.join(path), &original).unwrap();
        let mut editor = fixture.authoring_editor();
        let index = editor.project_source_index_snapshot().unwrap();
        let EditorDocumentOpenRequestResult::Opened { document } =
            editor.open_indexed_authoring_document(index, path, authoring_kind)
        else {
            panic!("the stable native document must open");
        };
        dirty_with_history(&mut editor, document);
        let before = document_state(&editor, document);
        let metadata_path = fixture.root.join(format!("{path}.meta"));
        let metadata = fs::read(&metadata_path).unwrap();
        fs::remove_file(&metadata_path).unwrap();
        request_reopen(&mut editor, document);
        assert_eq!(
            settle(&mut editor),
            EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                reason: EditorPersistenceRejection::SourceBindingMismatch,
            }
        );
        assert_eq!(document_state(&editor, document), before);
        acknowledge(&mut editor);
        fs::write(fixture.root.join(path), b"{ malformed").unwrap();
        request_reopen(&mut editor, document);
        assert_eq!(
            settle(&mut editor),
            EditorPersistenceResult::Failed {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                stage: EditorPersistenceFailureStage::Decode,
            }
        );
        assert_eq!(document_state(&editor, document), before);
        acknowledge(&mut editor);
        fs::write(fixture.root.join(path), &original).unwrap();
        fs::write(&metadata_path, metadata).unwrap();
        assert_successful_retry(&mut editor, document);
        assert_eq!(document_state(&editor, document).bytes, original);
        close(&mut editor);
    }
}

#[test]
fn scene_and_prefab_reopen_reject_replacement_before_native_callbacks_then_retry_runs_them() {
    use std::sync::Mutex;
    use std::thread::{ThreadId, current};

    use crate::project_host::runtime::editor::TrustedCompiledEditorProjectIntent;
    use crate::{ProductRecipe, SchemaContribution};
    use nara_app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    };
    use nara_reflect::{
        AuthoringSchemaBundle, ComponentCapability, ComponentFieldId, ComponentFieldPath,
        ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerId,
        ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    };
    use nara_scene::SceneComponentRecord;

    const COMPONENT: &str = "nara.test.ReopenAdmissionProbe";
    const PROVIDER: PluginSchemaProviderId =
        PluginSchemaProviderId::new("nara.test.reopen-admission.schema");
    static CALLBACKS: Mutex<Vec<(&'static str, ThreadId)>> = Mutex::new(Vec::new());

    #[derive(nara_ecs::Component)]
    struct NativeProbe(i64);

    #[derive(Default)]
    struct NativeProbePlugin;

    impl Plugin for NativeProbePlugin {
        fn declaration() -> &'static PluginDeclaration {
            static DECLARATION: PluginDeclaration = PluginDeclaration::new(
                PluginId::new("nara.test.reopen-admission"),
                PluginCategory::Runtime,
            )
            .requires_plugins(&[nara_reflect::COMPONENT_REGISTRY_PLUGIN_ID])
            .provides_schema(&[PROVIDER]);
            &DECLARATION
        }

        fn build(&self, _app: &mut App) -> Result<(), PluginError> {
            Ok(())
        }
    }

    fn schema() -> ComponentSchema {
        ComponentSchema::new(
            ComponentTypeId::new(COMPONENT),
            "Reopen admission probe",
            ComponentSchemaVersion(2),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
    }

    let provider = ComponentSchemaProviderDefinition::new(
        ComponentSchemaOwnerId::new("nara.test.reopen-admission"),
        PROVIDER,
        ComponentSchemaProviderBindingId::new("nara.test.reopen-admission.native", 1),
        || {
            Ok(ComponentSchemaCatalog {
                components: vec![schema()],
                ..Default::default()
            })
        },
        |registry| {
            registry.register_persistent_component_with_codec::<NativeProbe, _, _>(
                schema(),
                |value| {
                    CALLBACKS.lock().unwrap().push(("decode", current().id()));
                    Ok(NativeProbe(value.field_i64("value")?))
                },
                |value| {
                    CALLBACKS.lock().unwrap().push(("encode", current().id()));
                    Ok(ComponentValue::map([(
                        "value",
                        ComponentValue::I64(value.0),
                    )]))
                },
            )?;
            registry.register_component_migration(
                &ComponentTypeId::new(COMPONENT),
                ComponentSchemaVersion::ONE,
                ComponentSchemaVersion(2),
                |value| {
                    CALLBACKS
                        .lock()
                        .unwrap()
                        .push(("migration", current().id()));
                    Ok(value)
                },
            )?;
            Ok(())
        },
    );

    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new();
        let mut providers = built_in_schema_providers();
        providers.push(provider);
        fs::write(
            fixture.root.join("schema/authoring.bundle.json"),
            AuthoringSchemaBundle::from_trusted_provider_definitions(providers)
                .unwrap()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let recipe = ProductRecipe::new()
            .add_contribution(
                SchemaContribution::<NativeProbePlugin>::for_default([provider]).unwrap(),
            )
            .unwrap();
        let mut editor = EditorProjectSession::open_trusted_compiled(
            fixture.capability(),
            TrustedCompiledEditorProjectIntent::new().with_recipe(recipe),
        )
        .unwrap();
        let document = fixture.add(&mut editor, kind, "native-callback.json");
        let directory = if kind == EditorDocumentKind::Scene {
            "scenes"
        } else {
            "prefabs"
        };
        let project_path = RelativePath::new(format!("{directory}/native-callback.json")).unwrap();
        let path = fixture.root.join(directory).join("native-callback.json");
        let component = ComponentTypeId::new(COMPONENT);
        let entity = SceneEntityRecord::new(SceneEntityId::new("native-probe").unwrap())
            .with_component(
                component.clone(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([("value", ComponentValue::I64(7))]),
                ),
            );
        let original = if kind == EditorDocumentKind::Scene {
            SceneDocument::from_ordered_roots([entity])
                .to_json_string()
                .unwrap()
        } else {
            PrefabDocument::from_ordered_roots([entity])
                .to_json_string()
                .unwrap()
        };
        // Only Reopen sees this v1 candidate, exercising both migration and native decode.
        fs::write(&path, &original).unwrap();
        dirty_with_history(&mut editor, document);
        let before = document_state(&editor, document);
        let root = fixture.capability();
        let original_identity = root.open_file(&project_path).unwrap().identity();
        let owner_thread = current().id();
        CALLBACKS.lock().unwrap().clear();

        let gate = editor.hold_next_reopen(ReopenTestPhase::AfterPreparation);
        request_reopen(&mut editor, document);
        editor.advance_persistence();
        assert!(gate.wait_until_started());
        assert!(CALLBACKS.lock().unwrap().is_empty());
        // Retain the old object so identical replacement bytes still have a distinct identity.
        let previous = path.with_extension("previous");
        fs::rename(&path, &previous).unwrap();
        fs::write(&path, &original).unwrap();
        assert_ne!(
            root.open_file(&project_path).unwrap().identity(),
            original_identity
        );
        gate.release();
        assert_eq!(
            settle(&mut editor),
            EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                reason: EditorPersistenceRejection::SourceBindingMismatch,
            }
        );
        assert!(
            CALLBACKS.lock().unwrap().is_empty(),
            "{kind:?}: source rejection must precede native codecs and migrations"
        );
        assert_eq!(document_state(&editor, document), before);
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        acknowledge(&mut editor);

        fs::remove_file(&path).unwrap();
        fs::rename(&previous, &path).unwrap();
        assert_eq!(
            root.open_file(&project_path).unwrap().identity(),
            original_identity
        );
        request_reopen(&mut editor, document);
        assert!(matches!(
            settle(&mut editor),
            EditorPersistenceResult::Opened { document: opened, .. } if opened == document
        ));
        let callbacks = CALLBACKS.lock().unwrap().clone();
        assert!(callbacks.iter().any(|(kind, _)| *kind == "decode"));
        assert!(callbacks.iter().any(|(kind, _)| *kind == "migration"));
        assert!(callbacks.iter().all(|(_, thread)| *thread == owner_thread));
        let reopened = match kind {
            EditorDocumentKind::Scene => {
                &editor
                    .workspace
                    .scene(document)
                    .unwrap()
                    .session()
                    .document()
                    .entities[0]
                    .components[&component]
            }
            EditorDocumentKind::Prefab => {
                &editor
                    .workspace
                    .prefab(document)
                    .unwrap()
                    .session()
                    .document()
                    .entities[0]
                    .components[&component]
            }
            EditorDocumentKind::Extension(_) => unreachable!(),
        };
        assert_eq!(reopened.version, ComponentSchemaVersion(2));
        assert_eq!(reopened.value.field_i64("value").unwrap(), 7);
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        acknowledge(&mut editor);
        close(&mut editor);
    }
}
