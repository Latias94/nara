use super::*;
mod retirement;
use nara_tooling::{
    EditorProjectContentAction, EditorProjectContentRefreshFailure,
    EditorProjectContentRefreshRequestResult,
};

fn save_entity(editor: &mut EditorProjectSession, document: EditorDocumentId, entity: &str) {
    edit(editor, document, entity);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    assert!(matches!(
        settle(editor),
        EditorPersistenceResult::Saved { .. }
    ));
    acknowledge(editor);
}

fn wait_for_play(editor: &mut EditorProjectSession, state: EditorPlayState) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor.play_view().state() != state {
        assert!(
            Instant::now() < deadline,
            "Play did not reach {state:?}: {:?}",
            editor.play_view()
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn retry_and_runtime_retirement_revalidate_all_sources_after_the_representative_closes() {
    for runtime_retirement in [false, true] {
        for drift_other_source in [false, true] {
            let fixture = Fixture::new();
            let (mut editor, [p, q]) = fixture.two_prefabs();
            if runtime_retirement {
                let scene = editor.scene_preview_document.unwrap();
                assert!(
                    editor
                        .apply_workspace_command(EditorWorkspaceCommand::SetActiveDocument {
                            document: scene
                        })
                        .applied
                );
                assert_eq!(
                    editor.request_play(EditorPlayCommand::Play),
                    EditorPlayRequestResult::Accepted
                );
                wait_for_play(&mut editor, EditorPlayState::Running);
                let _ = editor.request_play(EditorPlayCommand::AcknowledgeResult);
                save_entity(&mut editor, p, "first-generation-p");
                editor.advance_project_content_refresh();
                assert!(editor.project_content.is_current());
                let retirement_deadline = Instant::now() + Duration::from_secs(5);
                while editor.authoring_retirements.is_pending() {
                    assert!(
                        Instant::now() < retirement_deadline,
                        "authoring predecessor did not retire"
                    );
                    editor.advance_authoring_retirement();
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            let last_good = editor.trusted_compiled().unwrap().snapshot.revision();
            save_entity(&mut editor, p, "pending-p");
            save_entity(&mut editor, q, "pending-q");
            let startup = fixture.root.join("scenes/startup.scene.json");
            let original_scene = fs::read(&startup).unwrap();
            if !runtime_retirement {
                fs::write(&startup, b"invalid Scene JSON").unwrap();
            }
            editor.advance_project_content_refresh();
            let failure = if runtime_retirement {
                EditorProjectContentRefreshFailure::RuntimeRetirementRequired
            } else {
                EditorProjectContentRefreshFailure::InvalidContent
            };
            assert!(
                matches!(editor.project_content, EditorProjectContentView::Stale { failure: observed, .. } if observed == failure),
                "{:?}",
                editor.project_content
            );
            if !runtime_retirement {
                fs::write(startup, original_scene).unwrap();
            }
            assert_eq!(
                editor
                    .request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: p }),
                EditorWorkspaceIntentRequestResult::Accepted
            );
            editor.advance_workspace_retirement();
            assert!(editor.workspace.prefab(p).is_none());
            assert!(editor.acknowledge_workspace_intent_result());
            if drift_other_source {
                let path = fixture.root.join("prefabs/q.json");
                let mut bytes = fs::read(&path).unwrap();
                bytes.extend_from_slice(b"\n");
                fs::write(path, bytes).unwrap();
            }
            if runtime_retirement {
                assert_eq!(editor.play_view().state(), EditorPlayState::Running);
                assert_eq!(
                    editor.request_play(EditorPlayCommand::Stop),
                    EditorPlayRequestResult::Accepted
                );
                wait_for_play(&mut editor, EditorPlayState::Empty);
                editor.drive_editor_frame(Duration::ZERO);
            } else {
                assert_eq!(
                    editor
                        .request_project_content_refresh(EditorProjectContentAction::RetryRefresh),
                    EditorProjectContentRefreshRequestResult::Accepted
                );
            }
            editor.advance_project_content_refresh();
            let result = editor.project_content;
            let actual_content = editor.trusted_compiled().unwrap().snapshot.revision();
            let retirement_deadline = Instant::now() + Duration::from_secs(5);
            while editor.authoring_retirements.is_pending() {
                assert!(
                    Instant::now() < retirement_deadline,
                    "authoring predecessor did not retire"
                );
                editor.advance_project_content_refresh();
                std::thread::sleep(Duration::from_millis(1));
            }
            let generations = editor
                .trusted_compiled()
                .unwrap()
                .content_loader
                .budget_snapshot()
                .active_generations();
            close(&mut editor);
            if drift_other_source {
                assert!(
                    matches!(result, EditorProjectContentView::Stale {
                    document,
                    failure: EditorProjectContentRefreshFailure::SavedSourceChanged,
                    ..
                } if document == p),
                    "a closed representative must not hide Q's drift: {result:?}"
                );
                assert_eq!(actual_content, last_good);
            } else {
                assert!(
                    result.is_current(),
                    "all obligations must recover together: {result:?}"
                );
                assert_ne!(actual_content, last_good);
            }
            assert_eq!(
                generations, 1,
                "a retired generation or failed successor leaked"
            );
        }
    }
}

#[cfg(all(feature = "presentation-2d", feature = "render-wgpu"))]
#[test]
fn custom_scene_policy_and_original_picking_batches_survive_content_refresh() {
    use crate::product::wgpu_policy_tests::{TestPolicy, recipe};
    use nara_reflect::ComponentTypeId;
    use nara_render::Extent2d;
    use nara_render_wgpu::WgpuStockPolicy;
    use nara_scene::SceneComponentRecord;

    let fixture = Fixture::new();
    let manifest_path = fixture.root.join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap().replace(
        "requested = [\"runtime-2d\"]",
        "requested = [\"runtime-2d\", \"presentation-2d\", \"render-wgpu\"]",
    );
    fs::write(manifest_path, manifest).unwrap();
    fs::write(
        fixture.root.join("prefabs/p.json"),
        PrefabDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    let registry_snapshot: nara_reflect::ComponentRegistrySnapshot =
        crate::project_host::authoring::first_party_authoring_preview_registry(
            built_in_schema_providers(),
        )
        .unwrap();
    let mut world = nara_ecs::World::new();
    let entity = world
        .spawn((
            nara_transform::Transform2d::default(),
            nara_sprite::Sprite::from_color(
                nara_core::Vec2::new(32.0, 32.0),
                nara_core::Color::WHITE,
            ),
        ))
        .id();
    let mut body = SceneEntityRecord::new(SceneEntityId::new("body").unwrap());
    for name in ["nara.transform.Transform2d", "nara.sprite.Sprite"] {
        let component = ComponentTypeId::new(name);
        body = body.with_component(
            component.clone(),
            SceneComponentRecord::new(
                registry_snapshot.schema(&component).unwrap().version,
                registry_snapshot
                    .encode_component(&component, &world, entity)
                    .unwrap()
                    .unwrap(),
            ),
        );
    }
    let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
    anchor.prefab = Some(nara_scene::PrefabInstance {
        source: nara_asset::AssetRef::path("p.json").unwrap(),
        overrides: Default::default(),
    });
    fs::write(
        fixture.root.join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([body, anchor])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut editor = EditorProjectSession::open_trusted_compiled(
        fixture.capability(),
        super::super::super::TrustedCompiledEditorProjectIntent::new().with_recipe(recipe(19)),
    )
    .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let scene = editor.workspace.active_document().unwrap();
        editor.configure_editor_preview(Extent2d::new(320, 180));
        editor.drive_editor_frame(Duration::ZERO);
        let original = editor.take_editor_preview_candidate().unwrap();
        let original_input = original.render_input().clone();
        let original_batches = original.sprite_batches().clone();
        let original_capture = original_input.captured::<TestPolicy>().unwrap();
        assert_eq!(original_capture.value, 19);
        assert!(original_input.captured::<WgpuStockPolicy>().is_none());
        assert_eq!(original_batches.total_instances(), 1);
        assert!(std::ptr::eq(
            original_batches.as_slice(),
            original_capture.stock.sprite_batches().unwrap().as_slice(),
        ));
        assert_eq!(original.hits()[0].entity().as_str(), "body");
        assert!(original.hits()[0].contains_clip_point([0.0, 0.0]));

        let prefab = fixture.add(&mut editor, EditorDocumentKind::Prefab, "p.json");
        save_entity(&mut editor, prefab, "refreshed-prefab");
        editor.advance_project_content_refresh();
        assert!(
            editor.project_content.is_current(),
            "{:?}",
            editor.project_content,
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while editor.authoring_retirements.is_pending() {
            assert!(
                Instant::now() < deadline,
                "the original Scene producer did not retire",
            );
            editor.advance_authoring_retirement();
            std::thread::yield_now();
        }
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::SetActiveDocument {
                    document: scene,
                })
                .applied
        );
        editor.drive_editor_frame(Duration::ZERO);
        let refreshed = editor.take_editor_preview_candidate().unwrap();
        assert_eq!(
            refreshed
                .render_input()
                .captured::<TestPolicy>()
                .unwrap()
                .value,
            19,
        );
        assert_ne!(
            refreshed.render_input().installation(),
            original_input.installation(),
        );
        assert_ne!(
            refreshed.packet().source_generation(),
            original.packet().source_generation(),
        );
        assert_eq!(refreshed.hits()[0].entity(), original.hits()[0].entity());
        assert!(refreshed.hits()[0].contains_clip_point([0.0, 0.0]));
        assert!(std::ptr::eq(
            original.sprite_batches().as_slice(),
            original_batches.as_slice(),
        ));
        assert!(std::ptr::eq(
            original.render_input().captured::<TestPolicy>().unwrap(),
            original_capture,
        ));

        let mut presentation = super::super::super::presentation::EditorPreviewPresentation::new();
        let (accepted, retained_input) = presentation.accept(original).unwrap();
        assert_eq!(accepted.document(), scene);
        assert_eq!(retained_input.installation(), original_input.installation());
        assert!(std::ptr::eq(
            retained_input.packet(),
            original_input.packet()
        ));
    }));
    close(&mut editor);
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}
