#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use nara::{
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
        PersistentComponentSchemaOwnerId,
    },
    scene::{
        PrefabDocument, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
        ScenePlacement, ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorCloseDecision, EditorDocumentKind,
        EditorDocumentMutation, EditorNavigationAction, EditorPersistenceAction,
        EditorProjectBrowserAction, EditorReopenTarget, PrefabCompositionCommand,
        PrefabCompositionEntityDraft, SceneCompositionCommand, SceneCompositionEntityKind,
        SceneInspectorCommand,
    },
};
use nara_editor::{EditorApplication, EditorApplicationState, EditorLaunchOptions};

const CUSTOM: &str = "studio.composition.Health";
const NO_TEMPLATE: &str = "studio.composition.StoredOnly";
const UNKNOWN: &str = "studio.missing.Reference";
const OWNER: &str = "studio.composition.components";
static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

#[path = "support/recursive_component_values.rs"]
mod recursive_component_values;

#[test]
fn trusted_project_opens_ordinary_prefab_through_source_navigation() {
    let project = CompositionProject::new();
    let bundle = AuthoringSchemaBundle::from_trusted_provider_definitions(
        nara::project_host::advanced::built_in_schema_providers(),
    )
    .unwrap();
    fs::write(
        project.root.join("catalogs/authoring.bundle.json"),
        bundle.to_json_string().unwrap(),
    )
    .unwrap();
    let anchor = SceneEntityId::new("ordinary-anchor").unwrap();
    let mut record = SceneEntityRecord::new(anchor.clone());
    record.prefab = Some(nara::scene::PrefabInstance {
        source: nara::asset::AssetRef::path("ordinary.prefab.json").unwrap(),
        overrides: nara::scene::ScenePatchDocument::default(),
    });
    fs::write(
        project.source(EditorDocumentKind::Scene),
        SceneDocument::from_ordered_roots([record])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let entity = SceneEntityId::new("ordinary").unwrap();
    let component = ComponentTypeId::new("nara.scene.Name");
    fs::write(
        project.source(EditorDocumentKind::Prefab),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(entity.clone())
            .with_component(
                component.clone(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("Before".into()),
                ),
            )])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let mut app = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(&project.root).with_profile("desktop"),
        nara::project_host::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let current = frame(&app);
    let scene = current.workspace().active_scene.as_ref().unwrap();
    let _ = app.submit_and_drive(
        EditorActionBatch::new(
            current.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: scene.document,
                    entity: Some(anchor.clone()),
                }),
                EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                    document: scene.document,
                    base_revision: scene.revision,
                    entity: anchor,
                }),
            ],
        ),
        Duration::ZERO,
    );
    let opened = frame(&app);
    let model = opened.workspace().active_scene.as_ref().unwrap();
    let opened_kind = model.kind;
    let diagnostics = format!("{:?}", opened.diagnostics());
    if opened_kind != EditorDocumentKind::Prefab {
        close(&mut app);
        panic!("ordinary Prefab source navigation failed: {diagnostics}");
    }
    assert!(app.current_ui_authoring_frame().is_none());
    let document = model.document;
    dispatch(
        &mut app,
        Some(EditorAction::Navigate(
            EditorNavigationAction::SelectEntity {
                document,
                entity: Some(entity.clone()),
            },
        )),
    );
    let revision = frame(&app).workspace().document(document).unwrap().revision;
    dispatch(
        &mut app,
        Some(EditorAction::Mutate(
            EditorDocumentMutation::ApplyInspectorCommand {
                document,
                base_revision: revision,
                command: SceneInspectorCommand::SetField {
                    entity: entity.clone(),
                    component: component.clone(),
                    component_version: ComponentSchemaVersion::ONE,
                    field: ComponentFieldId::new("value"),
                    value: ComponentValue::String("After".into()),
                },
            },
        )),
    );
    let edited = frame(&app);
    let edited = edited.workspace().active_scene.as_ref().unwrap();
    let history = edited.editor.inspector.history;
    let revision = edited.revision;
    dispatch(
        &mut app,
        Some(EditorAction::Persistence(EditorPersistenceAction::Save {
            document,
            base_revision: revision,
        })),
    );
    settle_persistence(&mut app);
    let saved = !frame(&app).workspace().document(document).unwrap().dirty;
    let bytes = fs::read(project.source(EditorDocumentKind::Prefab)).unwrap();
    close(&mut app);
    assert_eq!(history.undo_depth, 1);
    assert!(saved);
    assert!(String::from_utf8(bytes).unwrap().contains("After"));
}

#[test]
fn ordinary_prefab_composition_cannot_bypass_specialized_ui_structure_admission() {
    let project = CompositionProject::new();
    let ui_bundle =
        AuthoringSchemaBundle::from_trusted_provider_definitions([nara::ui::UI_SCHEMA_PROVIDER])
            .unwrap();
    let custom_bundle = AuthoringSchemaBundle::from_json_bytes(
        &fs::read(project.root.join("catalogs/authoring.bundle.json")).unwrap(),
    )
    .unwrap();
    let bundle = AuthoringSchemaBundle::new(
        ui_bundle
            .owners()
            .iter()
            .chain(custom_bundle.owners())
            .cloned(),
    )
    .unwrap();
    fs::write(
        project.root.join("catalogs/authoring.bundle.json"),
        bundle.to_json_string().unwrap(),
    )
    .unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/generic-editor/image-import/prefabs/ui.prefab.json"),
        project.source(EditorDocumentKind::Prefab),
    )
    .unwrap();
    let mut app = project.open();
    for _ in 0..128 {
        if !frame(&app).project_browser().has_pending_work() {
            break;
        }
        dispatch(&mut app, None);
    }
    dispatch(
        &mut app,
        Some(EditorAction::ProjectBrowser(
            EditorProjectBrowserAction::Open {
                relative_path: "blueprints/ordinary.prefab.json".into(),
            },
        )),
    );
    let current = frame(&app);
    let model = current.workspace().active_scene.as_ref().unwrap();
    assert_eq!(model.kind, EditorDocumentKind::Prefab);
    assert!(app.current_ui_authoring_frame().is_some());
    let document = model.document;
    let revision = model.revision;
    let history = model.editor.inspector.history;
    let source = fs::read(project.source(EditorDocumentKind::Prefab)).unwrap();
    let commands = [
        PrefabCompositionCommand::CreateEntity {
            draft: PrefabCompositionEntityDraft::new(Default::default()).unwrap(),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        },
        PrefabCompositionCommand::AddComponent {
            entity: SceneEntityId::new("ui-root").unwrap(),
            component: ComponentTypeId::new(CUSTOM),
        },
        PrefabCompositionCommand::RemoveComponent {
            entity: SceneEntityId::new("ui-root").unwrap(),
            component: ComponentTypeId::new("nara.ui.UiRoot"),
        },
    ];
    for command in commands {
        dispatch(
            &mut app,
            Some(EditorAction::Mutate(
                EditorDocumentMutation::ComposePrefab {
                    document,
                    base_revision: revision,
                    command,
                },
            )),
        );
        let current = frame(&app);
        let model = current.workspace().active_scene.as_ref().unwrap();
        assert_eq!(model.revision, revision);
        assert_eq!(model.editor.inspector.history, history);
    }
    dispatch(
        &mut app,
        Some(EditorAction::Mutate(
            EditorDocumentMutation::ApplyPrefabPatch {
                document,
                base_revision: revision,
                patch: nara::scene::ScenePatchDocument::new([
                    nara::scene::ScenePatchOperation::AddEntity {
                        entity: SceneEntityRecord::new(SceneEntityId::new("bypass").unwrap()),
                        placement: ScenePlacement::last(ScenePlacementOwner::Root),
                    },
                ]),
            },
        )),
    );
    let unchanged = frame(&app).workspace().document(document).unwrap().revision == revision;
    let unchanged_history = frame(&app)
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .history
        == history;
    close(&mut app);
    assert!(unchanged && unchanged_history);
    assert_eq!(
        fs::read(project.source(EditorDocumentKind::Prefab)).unwrap(),
        source
    );
}

#[test]
fn empty_documents_offer_catalog_templates_before_any_component_exists() {
    let project = CompositionProject::new();
    fs::write(
        project.source(EditorDocumentKind::Scene),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(
        project.source(EditorDocumentKind::Prefab),
        PrefabDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    let mut app = project.open();
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        if kind == EditorDocumentKind::Prefab {
            for _ in 0..128 {
                if !frame(&app).project_browser().has_pending_work() {
                    break;
                }
                dispatch(&mut app, None);
            }
            dispatch(
                &mut app,
                Some(EditorAction::ProjectBrowser(
                    EditorProjectBrowserAction::Open {
                        relative_path: "blueprints/ordinary.prefab.json".into(),
                    },
                )),
            );
        }
        let current = frame(&app);
        let model = current.workspace().active_scene.as_ref().unwrap();
        assert_eq!(model.kind, kind);
        assert!(model.editor.inspector.entities.is_empty());
        assert!(
            model
                .editor
                .inspector
                .composition
                .components
                .iter()
                .any(|capability| capability.component.as_str() == CUSTOM
                    && capability.add_unavailable_reason.is_none())
        );
        let mutation = match kind {
            EditorDocumentKind::Scene => EditorDocumentMutation::ComposeScene {
                document: model.document,
                base_revision: model.revision,
                command: SceneCompositionCommand::CreateEntity {
                    kind: SceneCompositionEntityKind::Empty,
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                },
            },
            EditorDocumentKind::Prefab => EditorDocumentMutation::ComposePrefab {
                document: model.document,
                base_revision: model.revision,
                command: PrefabCompositionCommand::CreateEntity {
                    draft: PrefabCompositionEntityDraft::new(Default::default()).unwrap(),
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                },
            },
        };
        dispatch(&mut app, Some(EditorAction::Mutate(mutation)));
        let entity = frame(&app)
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .selection
            .top_entity()
            .unwrap()
            .clone();
        compose_component(&mut app, kind, entity, CUSTOM, true);
        assert_eq!(custom_value(&app), Some(ComponentValue::I64(10)));
    }
    close(&mut app);
}

#[test]
fn ordinary_scene_and_prefab_create_custom_components_without_project_execution() {
    let project = CompositionProject::new();
    let mut app = project.open();
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        if kind == EditorDocumentKind::Prefab {
            for _ in 0..128 {
                if !frame(&app).project_browser().has_pending_work() {
                    break;
                }
                dispatch(&mut app, None);
            }
            dispatch(
                &mut app,
                Some(EditorAction::ProjectBrowser(
                    EditorProjectBrowserAction::Open {
                        relative_path: "blueprints/ordinary.prefab.json".into(),
                    },
                )),
            );
        }
        let initial = frame(&app);
        let model = initial.workspace().active_scene.as_ref().unwrap();
        assert_eq!(model.kind, kind);
        assert!(model.editor.inspector.composition.generic_authoring);
        assert!(
            model
                .editor
                .inspector
                .composition
                .components
                .iter()
                .any(|capability| capability.component.as_str() == CUSTOM
                    && capability.add_unavailable_reason.is_none())
        );
        assert!(
            model
                .editor
                .inspector
                .composition
                .components
                .iter()
                .any(|capability| capability.component.as_str() == NO_TEMPLATE
                    && capability.add_unavailable_reason.is_some())
        );
        let document = model.document;
        let original_manifest = app.active_authoring_unavailable_record_manifest().unwrap();
        let mutation = match kind {
            EditorDocumentKind::Scene => EditorDocumentMutation::ComposeScene {
                document,
                base_revision: model.revision,
                command: SceneCompositionCommand::CreateEntity {
                    kind: SceneCompositionEntityKind::Empty,
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                },
            },
            EditorDocumentKind::Prefab => EditorDocumentMutation::ComposePrefab {
                document,
                base_revision: model.revision,
                command: PrefabCompositionCommand::CreateEntity {
                    draft: PrefabCompositionEntityDraft::new(Default::default()).unwrap(),
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                },
            },
        };
        dispatch(&mut app, Some(EditorAction::Mutate(mutation)));
        let created = frame(&app);
        let created = created.workspace().active_scene.as_ref().unwrap();
        assert_eq!(created.editor.inspector.history.undo_depth, 1);
        let entity = created.selection.top_entity().unwrap().clone();
        assert_eq!(
            created.editor.inspector.selected_entity.as_ref(),
            Some(&entity)
        );
        assert_eq!(
            app.active_authoring_unavailable_record_manifest(),
            Some(original_manifest.clone())
        );

        compose_component(&mut app, kind, entity.clone(), CUSTOM, true);
        let added = frame(&app);
        let added = added.workspace().active_scene.as_ref().unwrap();
        assert_eq!(added.editor.inspector.history.undo_depth, 2);
        assert_eq!(custom_value(&app), Some(ComponentValue::I64(10)));
        let stale = added.revision;
        let history = added.editor.inspector.history;
        compose_component(&mut app, kind, entity.clone(), CUSTOM, true);
        compose_component(&mut app, kind, entity.clone(), NO_TEMPLATE, true);
        assert_eq!(
            frame(&app)
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .revision,
            stale
        );
        assert_eq!(
            frame(&app)
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .editor
                .inspector
                .history,
            history
        );

        dispatch(
            &mut app,
            Some(EditorAction::Mutate(
                EditorDocumentMutation::ApplyInspectorCommand {
                    document,
                    base_revision: stale,
                    command: SceneInspectorCommand::SetField {
                        entity: entity.clone(),
                        component: ComponentTypeId::new(CUSTOM),
                        component_version: ComponentSchemaVersion::ONE,
                        field: ComponentFieldId::new("value"),
                        value: ComponentValue::I64(42),
                    },
                },
            )),
        );
        assert_eq!(custom_value(&app), Some(ComponentValue::I64(42)));
        // A document mutation from before the edit cannot target its successor revision.
        dispatch(
            &mut app,
            Some(EditorAction::Mutate(EditorDocumentMutation::Undo {
                document,
                base_revision: stale,
            })),
        );
        assert_eq!(custom_value(&app), Some(ComponentValue::I64(42)));
        compose_component(&mut app, kind, entity.clone(), CUSTOM, false);
        assert_eq!(custom_value(&app), None);
        for redo in [false, true, false] {
            let revision = frame(&app).workspace().document(document).unwrap().revision;
            let mutation = if redo {
                EditorDocumentMutation::Redo {
                    document,
                    base_revision: revision,
                }
            } else {
                EditorDocumentMutation::Undo {
                    document,
                    base_revision: revision,
                }
            };
            dispatch(&mut app, Some(EditorAction::Mutate(mutation)));
            assert_eq!(
                custom_value(&app),
                (!redo).then_some(ComponentValue::I64(42))
            );
            assert_eq!(
                app.active_authoring_unavailable_record_manifest(),
                Some(original_manifest.clone())
            );
        }

        let before = frame(&app);
        let before = before.workspace().active_scene.as_ref().unwrap();
        let revision = before.revision;
        let history = before.editor.inspector.history;
        compose_component(
            &mut app,
            kind,
            SceneEntityId::new("unavailable").unwrap(),
            UNKNOWN,
            false,
        );
        assert_eq!(
            frame(&app)
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .revision,
            revision
        );
        assert_eq!(
            frame(&app)
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .editor
                .inspector
                .history,
            history
        );
        assert_eq!(
            app.active_authoring_unavailable_record_manifest(),
            Some(original_manifest.clone())
        );

        dispatch(
            &mut app,
            Some(EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult,
            )),
        );
        dispatch(
            &mut app,
            Some(EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: revision,
            })),
        );
        settle_persistence(&mut app);
        let saved = frame(&app);
        let dirty = saved.workspace().document(document).unwrap().dirty;
        if dirty {
            let result = saved.persistence().result();
            close(&mut app);
            panic!("{kind:?} save left the source dirty: {result:?}");
        }
        let bytes = fs::read(project.source(kind)).unwrap();
        assert!(String::from_utf8(bytes).unwrap().contains("42"));
        let target = EditorReopenTarget::OpenDocument {
            document,
            base_revision: frame(&app).workspace().document(document).unwrap().revision,
        };
        dispatch(
            &mut app,
            Some(EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult,
            )),
        );
        dispatch(
            &mut app,
            Some(EditorAction::Persistence(EditorPersistenceAction::Reopen {
                target,
            })),
        );
        settle_persistence(&mut app);
        assert_eq!(
            frame(&app).workspace().active_scene.as_ref().unwrap().kind,
            kind
        );
        assert_eq!(
            app.active_authoring_unavailable_record_manifest(),
            Some(original_manifest)
        );
        let document = frame(&app)
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .document;
        dispatch(
            &mut app,
            Some(EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(entity),
                },
            )),
        );
        assert_eq!(custom_value(&app), Some(ComponentValue::I64(42)));
    }
    assert!(
        fs::read_to_string(project.root.join("src/lib.rs"))
            .unwrap()
            .contains("compile_error!")
    );
    close(&mut app);
}

fn settle_persistence(app: &mut EditorApplication) {
    for _ in 0..128 {
        if frame(app).persistence().operation() == nara::tooling::EditorPersistenceOperation::Idle {
            return;
        }
        dispatch(app, None);
    }
    close(app);
    panic!("persistence did not settle within its bounded drive");
}

fn close(app: &mut EditorApplication) {
    if app.request_exit().exit_ready() {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let Some(current) = app.current_frame() else {
            return;
        };
        let action = current.workspace_intent().decision_prompt().map(|prompt| {
            EditorAction::WorkspaceIntent(prompt.resolve(EditorCloseDecision::Discard))
        });
        let update = app.submit_and_drive(
            EditorActionBatch::new(current.revision(), action),
            Duration::ZERO,
        );
        if update.exit_ready() {
            return;
        }
        std::thread::yield_now();
    }
    panic!("the authoring session did not physically close");
}

fn custom_value(app: &EditorApplication) -> Option<ComponentValue> {
    frame(app)
        .workspace()
        .active_scene
        .as_ref()?
        .editor
        .inspector
        .selected_entity_view
        .as_ref()?
        .components
        .iter()
        .find(|component| component.component.as_str() == CUSTOM)?
        .fields
        .iter()
        .find(|field| field.id.as_str() == "value")?
        .value
        .clone()
}

fn compose_component(
    app: &mut EditorApplication,
    kind: EditorDocumentKind,
    entity: SceneEntityId,
    component: &str,
    add: bool,
) {
    let current = frame(app);
    let model = current.workspace().active_scene.as_ref().unwrap();
    let component = ComponentTypeId::new(component);
    let mutation = match kind {
        EditorDocumentKind::Scene => EditorDocumentMutation::ComposeScene {
            document: model.document,
            base_revision: model.revision,
            command: if add {
                SceneCompositionCommand::AddComponent { entity, component }
            } else {
                SceneCompositionCommand::RemoveComponent { entity, component }
            },
        },
        EditorDocumentKind::Prefab => EditorDocumentMutation::ComposePrefab {
            document: model.document,
            base_revision: model.revision,
            command: if add {
                PrefabCompositionCommand::AddComponent { entity, component }
            } else {
                PrefabCompositionCommand::RemoveComponent { entity, component }
            },
        },
    };
    dispatch(app, Some(EditorAction::Mutate(mutation)));
}

fn frame(app: &EditorApplication) -> std::sync::Arc<nara::tooling::EditorFrameModel> {
    app.current_frame().unwrap()
}

fn dispatch(app: &mut EditorApplication, action: Option<EditorAction>) {
    let revision = frame(app).revision();
    let _ = app.submit_and_drive(EditorActionBatch::new(revision, action), Duration::ZERO);
}

struct CompositionProject {
    root: PathBuf,
}

impl CompositionProject {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara-editor-composition-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/generic-editor/known-unbound");
        for directory in [
            "catalogs",
            "worlds",
            "blueprints",
            "media",
            "scripts",
            "src",
        ] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        for file in ["Cargo.toml", "nara.toml", "src/lib.rs"] {
            fs::copy(fixture.join(file), root.join(file)).unwrap();
        }
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
            ComponentSchemaCatalog {
                components: vec![
                    scalar_schema(CUSTOM).with_authoring_initial_value(ComponentValue::map([(
                        "value",
                        ComponentValue::I64(10),
                    )])),
                    scalar_schema(NO_TEMPLATE),
                ],
                ..Default::default()
            },
            None,
        )])
        .unwrap();
        fs::write(
            root.join("catalogs/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        let unavailable = SceneEntityRecord::new(SceneEntityId::new("unavailable").unwrap())
            .with_component(
                ComponentTypeId::new(UNKNOWN),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([
                        (
                            "payload",
                            ComponentValue::String("keep this exact value".into()),
                        ),
                        (
                            "target",
                            ComponentValue::EntityReference(
                                nara::reflect::EntityReference::SceneLocal {
                                    entity: SceneEntityId::new("unavailable").unwrap(),
                                },
                            ),
                        ),
                    ]),
                )
                .with_owner(
                    PersistentComponentSchemaOwnerId::try_new("studio.missing.components").unwrap(),
                ),
            );
        fs::write(
            root.join("worlds/entry.scene.json"),
            SceneDocument::from_ordered_roots([unavailable.clone()])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("blueprints/ordinary.prefab.json"),
            PrefabDocument::from_ordered_roots([unavailable])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        Self { root }
    }

    fn open(&self) -> EditorApplication {
        let mut app =
            EditorApplication::new(EditorLaunchOptions::new(&self.root).with_profile("desktop"));
        let _ = app.native_shell_ready();
        for _ in 0..8 {
            if !matches!(
                app.state(),
                EditorApplicationState::OpeningInitial | EditorApplicationState::OpeningReplacement
            ) {
                break;
            }
            let _ = app.advance_project_open();
        }
        assert!(matches!(
            app.state(),
            EditorApplicationState::AuthoringReady | EditorApplicationState::AuthoringDegraded
        ));
        app
    }

    fn source(&self, kind: EditorDocumentKind) -> PathBuf {
        self.root.join(match kind {
            EditorDocumentKind::Scene => "worlds/entry.scene.json",
            EditorDocumentKind::Prefab => "blueprints/ordinary.prefab.json",
        })
    }
}

impl Drop for CompositionProject {
    fn drop(&mut self) {
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(temporary));
        assert!(
            root.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("nara-editor-composition-")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn scalar_schema(id: &str) -> ComponentSchema {
    ComponentSchema::new(ComponentTypeId::new(id), id, ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
}
