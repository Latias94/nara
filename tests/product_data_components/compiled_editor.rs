//! Compiled execution keeps source authority and actual preview capabilities independent.

use nara::{
    project_host::{
        EditorProjectCloseProgress, EditorProjectSession, TrustedCompiledEditorProjectIntent,
    },
    render::Extent2d,
    scene::{
        __private::{AuthoringPreviewOmissionLocation, AuthoringPreviewOmissionReason},
        PrefabDocumentCandidate, SceneDocumentCandidate,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorAuthoringReadiness,
        EditorNavigationAction, EditorRuntimeAvailability, EditorWorkspaceIntent,
        EditorWorkspaceIntentAction, EditorWorkspaceIntentRequestResult,
    },
};

use super::*;

#[path = "../support/editor_action_driver.rs"]
mod editor_driver;

fn run_editor(
    project: &TestProject,
    recipe: ProductRecipe,
    inspect: impl FnOnce(&mut EditorProjectSession),
) {
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.writable_root_capability(),
        TrustedCompiledEditorProjectIntent::new().with_recipe(recipe),
    )
    .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| inspect(&mut editor)));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let EditorProjectCloseProgress::Complete(receipt) = editor.advance_close() {
            assert!(!receipt.has_cleanup_failure(), "{receipt:?}");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "compiled Editor must physically close"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}

fn dispatch(editor: &mut EditorProjectSession, action: EditorAction) -> EditorActionOutcome {
    let frame = editor.current_frame();
    let mut outcomes = editor
        .dispatch_editor_actions(EditorActionBatch::new(frame.revision(), [action]))
        .unwrap()
        .into_outcomes();
    assert_eq!(outcomes.len(), 1);
    outcomes.pop().unwrap()
}

fn native_record() -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("enabled", ComponentValue::Bool(true))]),
    )
}

fn editor_project(scene_only: bool) -> (TestProject, ProductRecipe) {
    let project = TestProject::with_prefab_startup();
    let owners = data_owners(3);
    let recipe = native_and_data_recipe(&owners);
    let mut prefab = PrefabDocumentCandidate::decode_json_bytes(
        &fs::read(project.path().join("prefabs/enemy.prefab.json")).unwrap(),
    )
    .unwrap()
    .into_composition_source()
    .unwrap();
    prefab.entities[0]
        .components
        .insert(component(NATIVE_VALUE), native_record());
    prefab.entities[0].components.insert(
        component(MOTION),
        record(
            MOTION,
            ComponentValue::map([("amount", ComponentValue::U64(17))]),
        ),
    );
    if scene_only {
        prefab.entities[0]
            .components
            .insert(component(SCENE_ONLY_VALUE), native_record());
    }
    project.write_prefab_source(&prefab);
    let mut scene = SceneDocumentCandidate::decode_json_bytes(
        &fs::read(project.path().join("scenes/startup.scene.json")).unwrap(),
    )
    .unwrap()
    .into_composition_source()
    .unwrap();
    scene.entities[0]
        .components
        .insert(component(NATIVE_VALUE), native_record());
    scene.entities[0].components.insert(
        component(MOTION),
        record(
            MOTION,
            ComponentValue::map([("amount", ComponentValue::U64(23))]),
        ),
    );
    project.write_scene_source(&scene);
    install_bundle(&project, recipe.clone());
    (project, recipe)
}

#[test]
fn compiled_preview_uses_actual_installed_native_coverage_and_preserves_source_authority() {
    let (project, recipe) = editor_project(false);
    let source = fs::read(project.path().join("scenes/startup.scene.json")).unwrap();
    let prefab = fs::read(project.path().join("prefabs/enemy.prefab.json")).unwrap();
    run_editor(&project, recipe, |editor| {
        let initial = editor.current_frame();
        let document = initial.workspace().active_document.unwrap();
        let basis = editor.document_query_basis(document).unwrap();
        let facts = initial.workspace().document(document).unwrap().clone();
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let frame = editor.drive_editor_frame(Duration::ZERO);
        let candidate = editor.take_editor_preview_candidate().expect(
            "valid visible components must publish despite omitted Data and game Native components",
        );
        assert_eq!(candidate.document(), document);
        assert_eq!(candidate.revision(), basis.revision);
        assert_eq!(candidate.sprite_batches().total_instances(), 1);
        assert_eq!(candidate.hits().len(), 1);
        assert_eq!(candidate.hits()[0].entity(), &scene_id("enemy-anchor"));
        let omissions = candidate.preview_omissions();
        assert_eq!(omissions.len(), 4, "{omissions:?}");
        for component in [component(MOTION), component(NATIVE_VALUE)] {
            assert_eq!(omissions.iter().filter(|omission| omission.reason() == AuthoringPreviewOmissionReason::NativeBindingUnavailable && matches!(omission.location(), AuthoringPreviewOmissionLocation::DirectEntityComponent { entity, component: actual } if entity == &scene_id("enemy-anchor") && actual == &component)).count(), 1);
            assert_eq!(omissions.iter().filter(|omission| omission.reason() == AuthoringPreviewOmissionReason::NativeBindingUnavailable && matches!(omission.location(), AuthoringPreviewOmissionLocation::ProjectedEntityComponent { provenance, component: actual, .. } if provenance.local_entity_id() == &scene_id("enemy") && provenance.projection_path().is_some_and(|path| path.root_anchor() == &scene_id("enemy-anchor")) && actual == &component)).count(), 1);
        }
        assert_eq!(frame.workspace().document(document), Some(&facts));
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_eq!(
            fs::read(project.path().join("scenes/startup.scene.json")).unwrap(),
            source
        );
        assert_eq!(
            fs::read(project.path().join("prefabs/enemy.prefab.json")).unwrap(),
            prefab
        );
    });
}

#[test]
fn compiled_readiness_tracks_inactive_degraded_sources_separately_from_trusted_execution() {
    let (project, recipe) = editor_project(true);
    run_editor(&project, recipe, |editor| {
        let initial = editor.current_frame();
        let scene = initial.workspace().active_document.unwrap();
        let base_revision = initial.workspace().document(scene).unwrap().revision;
        assert_eq!(
            editor.project_readiness().authoring(),
            EditorAuthoringReadiness::Complete
        );
        assert!(matches!(
            dispatch(editor, EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                document: scene,
                entity: Some(scene_id("enemy-anchor")),
            })),
            EditorActionOutcome::Workspace(report) if report.applied
        ));
        editor.drive_editor_frame(Duration::ZERO);
        let opened = dispatch(
            editor,
            EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                document: scene,
                base_revision,
                entity: scene_id("enemy-anchor"),
            }),
        );
        let EditorActionOutcome::Workspace(report) = opened else {
            panic!("{opened:?}");
        };
        assert!(report.applied, "{report:?}");
        let prefab = report.opened_document.unwrap();
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            editor.project_readiness().authoring(),
            EditorAuthoringReadiness::Degraded
        );
        assert_eq!(
            editor.project_readiness().runtime(),
            EditorRuntimeAvailability::TrustedInProcess
        );
        assert!(
            matches!(dispatch(editor, EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: scene })), EditorActionOutcome::Workspace(report) if report.applied)
        );
        let frame = editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(frame.workspace().active_document, Some(scene));
        assert_eq!(
            editor.project_readiness().authoring(),
            EditorAuthoringReadiness::Degraded
        );
        assert!(matches!(
            dispatch(
                editor,
                EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
                    EditorWorkspaceIntent::CloseDocument { document: prefab }
                ))
            ),
            EditorActionOutcome::WorkspaceIntent(EditorWorkspaceIntentRequestResult::Accepted)
        ));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let frame = editor.drive_editor_frame(Duration::ZERO);
            if frame.workspace().document(prefab).is_none() {
                break;
            }
            assert!(Instant::now() < deadline, "source Close must complete");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            editor.project_readiness().authoring(),
            EditorAuthoringReadiness::Complete
        );
        assert_eq!(
            editor.project_readiness().runtime(),
            EditorRuntimeAvailability::TrustedInProcess
        );
    });
}

#[derive(Clone)]
struct ChangeSprite {
    swap: bool,
}

impl ProductConfiguration for ChangeSprite {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.push(u8::from(self.swap));
    }
}

impl Plugin for ChangeSprite {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("test.scoped-sprite-change"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let swap = self.swap;
        app.add_systems(CoreStage::Update, move |world: &mut World| {
            let mut query = world.query::<(Entity, &SceneEntitySource, &nara::sprite::Sprite)>();
            let mut player = None;
            let mut image = None;
            for (entity, source, sprite) in query.iter(world) {
                match source.entity_id.as_str() {
                    "player" => player = Some(entity),
                    "alternate" => image = sprite.material.image,
                    _ => {}
                }
            }
            if let Some(mut sprite) =
                player.and_then(|entity| world.get_mut::<nara::sprite::Sprite>(entity))
            {
                sprite.material.tint.r = 0.25;
                if swap {
                    sprite.material.image = image;
                }
            }
        })?;
        Ok(())
    }
}

fn wait_play(editor: &mut EditorProjectSession, state: nara::tooling::EditorPlayState) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let frame = editor.drive_editor_frame(Duration::from_millis(20));
        if frame.play().state() == state {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Play must reach {state:?}: {:?}; diagnostics: {:?}",
            frame.play(),
            frame.diagnostics()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn scoped_sprite_apply_changes_preserves_unchanged_texture_and_round_trips_changed_texture() {
    use nara::tooling::{
        EditorApplyChangesResult, EditorPersistenceCommand, EditorPersistenceRequestResult,
        EditorPersistenceResult, EditorPlayCommand, EditorPlayRequestResult, EditorPlayState,
        SceneApplyChangesRequest,
    };
    for swap in [false, true] {
        let project = TestProject::with_prefab_startup();
        project.select_local_headless_profile();
        fs::write(
            project.path().join("assets/textures/other.png"),
            project_content_fixture::valid_png_bytes(),
        )
        .unwrap();
        fs::write(
            project.path().join("assets/textures/other.png.meta"),
            project_content_fixture::image_meta(
                "b31edb65-dc0b-4b38-9e92-e7d0fb5828bc",
                "textures/other.png",
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        let sprite_id = component("nara.sprite.Sprite");
        project.write_scene_source(&SceneDocument::from_ordered_roots(
            [
                ("player", "textures/player.png"),
                ("alternate", "textures/other.png"),
            ]
            .map(|(entity, image)| {
                SceneEntityRecord::new(scene_id(entity))
                    .with_component(
                        component("nara.transform.Transform2d"),
                        project_content_fixture::identity_transform_record(),
                    )
                    .with_component(
                        sprite_id.clone(),
                        SceneComponentRecord::new(
                            ComponentSchemaVersion::ONE,
                            project_content_fixture::sprite_value(
                                &nara::asset::AssetRef::path(image).unwrap(),
                            ),
                        ),
                    )
            }),
        ));
        let recipe = ProductRecipe::new()
            .add_configured_plugin(ChangeSprite { swap }, Clone::clone)
            .unwrap();
        install_bundle(&project, recipe.clone());
        run_editor(&project, recipe.clone(), |editor| {
            assert_eq!(
                editor_driver::request_play(editor, EditorPlayCommand::Play),
                EditorPlayRequestResult::Accepted
            );
            wait_play(editor, EditorPlayState::Running);
            editor.drive_editor_frame(Duration::from_millis(20));
            assert_eq!(
                editor_driver::request_play(editor, EditorPlayCommand::Pause),
                EditorPlayRequestResult::Accepted
            );
            wait_play(editor, EditorPlayState::Paused);
            assert_eq!(
                editor_driver::request_apply_changes(
                    editor,
                    SceneApplyChangesRequest::new(scene_id("player"), [sprite_id.clone()])
                ),
                Ok(())
            );
            editor.drive_editor_frame(Duration::ZERO);
            let frame = editor.current_frame();
            let Some(EditorApplyChangesResult::Applied(report)) = frame.apply_changes_result()
            else {
                panic!(
                    "scoped Native export must produce a valid source edit: {:?}",
                    frame.apply_changes_result()
                );
            };
            let patch = report.patch.as_ref().unwrap();
            assert_eq!(patch.operations.len(), if swap { 2 } else { 1 });
            assert_eq!(patch.operations.iter().filter(|operation| matches!(operation,
                nara::scene::ScenePatchOperation::SetField { field, .. } if field.as_str() == "material.image")).count(), usize::from(swap));
            assert_eq!(
                editor_driver::request_play(editor, EditorPlayCommand::Stop),
                EditorPlayRequestResult::Accepted
            );
            wait_play(editor, EditorPlayState::Empty);
            let document = editor.current_frame().workspace().active_document.unwrap();
            assert!(matches!(
                editor_driver::request_persistence(
                    editor,
                    EditorPersistenceCommand::Save {
                        document: Some(document)
                    }
                ),
                EditorPersistenceRequestResult::Accepted { .. }
            ));
            let saved = editor_driver::drive_persistence_until_result(editor);
            assert!(matches!(
                saved.persistence().result(),
                Some(EditorPersistenceResult::Saved { .. })
            ));
            let attempt = saved.persistence().attempt().unwrap();
            assert!(matches!(
                editor_driver::request_persistence(
                    editor,
                    EditorPersistenceCommand::AcknowledgeResult { attempt }
                ),
                EditorPersistenceRequestResult::Acknowledged { .. }
            ));
            assert!(matches!(
                editor_driver::request_persistence(
                    editor,
                    EditorPersistenceCommand::Reopen {
                        document: Some(document)
                    }
                ),
                EditorPersistenceRequestResult::Accepted { .. }
            ));
            let reopened = editor_driver::drive_persistence_until_result(editor);
            assert!(matches!(
                reopened.persistence().result(),
                Some(EditorPersistenceResult::Opened { .. })
            ));
            let saved = SceneDocumentCandidate::decode_json_bytes(
                &fs::read(project.path().join("scenes/startup.scene.json")).unwrap(),
            )
            .unwrap()
            .into_composition_source()
            .unwrap();
            let sprite = &saved
                .entities
                .iter()
                .find(|entity| entity.id == scene_id("player"))
                .unwrap()
                .components[&sprite_id]
                .value;
            let image = sprite.get("material").unwrap().get("image").unwrap();
            let alias = if swap {
                "textures/other.png"
            } else {
                "textures/player.png"
            };
            assert_eq!(
                image.get("value"),
                Some(&ComponentValue::String(alias.into()))
            );
            editor.configure_editor_preview(Extent2d::new(320, 180));
            editor.drive_editor_frame(Duration::ZERO);
            assert!(editor.take_editor_preview_candidate().is_some());
        });
        check_project(ProjectCheckRequest::new(project.root_capability(), recipe)).unwrap();
    }
}
