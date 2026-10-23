#![cfg(feature = "tooling")]

use super::*;

use nara_app::CoreStage;
use nara_core::ItemLimit;
use nara_ecs::bevy_ecs::system::SystemState;
use nara_identity::SpawnedSceneInstance;
use nara_project::ProjectPath;
use nara_scene::advanced::SceneProductTransactionLimits;
use nara_tooling::{EditorPlayCommand, EditorPlayRequestResult, EditorPlayState};

use super::super::{
    ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes, commit_prepared_project_scene,
};

struct TravelConfiguration(mpsc::SyncSender<TravelObservation>);

impl crate::ProductConfiguration for TravelConfiguration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        // The channel observes this fixed test workflow; it is not an input to game behavior.
        output.extend_from_slice(b"editor-project-scenes-a-b-a-v1");
    }
}

struct TravelPlugin(mpsc::SyncSender<TravelObservation>);

#[derive(Resource)]
struct TravelState {
    completed: usize,
    pending: Option<ProjectSceneRequestId>,
    observations: mpsc::SyncSender<TravelObservation>,
}

#[derive(Debug)]
struct TravelObservation {
    path: &'static str,
    expected: SpawnedSceneInstance,
    committed: SpawnedSceneInstance,
    prepared_source: SceneDocument,
    image_allocation: usize,
}

impl Plugin for TravelPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.editor-scene-travel"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(TravelState {
            completed: 0,
            pending: None,
            observations: self.0.clone(),
        })?
        .add_systems(CoreStage::Update, travel_between_declared_scenes)?;
        Ok(())
    }
}

fn travel_between_declared_scenes(world: &mut World) {
    world.resource_scope(|world, mut travel: Mut<TravelState>| {
        let path = match travel.completed {
            0 => "b.scene.json",
            1 => "startup.scene.json",
            _ => return,
        };
        let Some(request) = travel.pending else {
            let mut access = SystemState::<ProjectScenes>::new(world);
            travel.pending = Some(
                access
                    .get_mut(world)
                    .unwrap()
                    .request(ProjectPath::new(path).unwrap())
                    .unwrap(),
            );
            return;
        };
        let target = {
            let mut access = SystemState::<ProjectScenes>::new(world);
            let scenes = access.get_mut(world).unwrap();
            match scenes.state(request) {
                Some(ProjectSceneRequestState::Preparing) => return,
                Some(ProjectSceneRequestState::Ready) => {
                    let prepared = scenes.prepared(request).unwrap();
                    assert_eq!(prepared.selected_scene_path().as_str(), path);
                    (
                        prepared.scene().clone(),
                        prepared.expected_instance().clone(),
                    )
                }
                state => panic!(
                    "Editor game Scene request failed: {state:?}; {:?}",
                    scenes.diagnostics(request)
                ),
            }
        };
        assert_eq!(world.resource::<CurrentSceneOwner>().receipt(), &target.1);
        let report = commit_prepared_project_scene(
            world,
            request,
            SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
            &[],
            |_| {},
        );
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        let receipt = report.instance.unwrap();
        assert_eq!(world.resource::<CurrentSceneOwner>().source(), &target.0);
        assert_eq!(world.resource::<CurrentSceneOwner>().receipt(), &receipt);
        assert_ne!(receipt.instance_id(), target.1.instance_id());
        for id in receipt.entity_ids() {
            assert!(matches!(
                receipt.resolve(world, id),
                EntityLookup::Resolved(_)
            ));
        }
        let image_allocation = {
            let images = world.resource::<Assets<ImageAsset>>();
            assert_eq!(images.len(), 1);
            images.iter().next().unwrap().1.pixels().as_ptr() as usize
        };
        {
            let mut access = SystemState::<ProjectScenes>::new(world);
            let mut scenes = access.get_mut(world).unwrap();
            assert_eq!(
                scenes.state(request),
                Some(ProjectSceneRequestState::Committed)
            );
            assert!(scenes.acknowledge(request));
            assert_eq!(scenes.state(request), None);
        }
        travel
            .observations
            .try_send(TravelObservation {
                path,
                expected: target.1,
                committed: receipt,
                prepared_source: target.0,
                image_allocation,
            })
            .unwrap();
        travel.completed += 1;
        travel.pending = None;
    });
}

#[test]
fn trusted_editor_play_travels_b_then_same_a_without_losing_preview_generation_or_request_authority()
 {
    let project = TestProject::with_image("editor-project-scene-travel");
    let manifest_path = project.root.join(PROJECT_MANIFEST);
    let mut manifest = fs::read_to_string(&manifest_path).unwrap();
    manifest.push_str("\n[content]\nscenes = [\"b.scene.json\"]\n");
    fs::write(manifest_path, manifest).unwrap();
    let a = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        nara_identity::SceneEntityId::new("player").unwrap(),
    )
    .with_component(
        ComponentTypeId::new("nara.transform.Transform2d"),
        identity_transform_record(),
    )
    .with_component(
        ComponentTypeId::new("nara.sprite.Sprite"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            sprite_value(&AssetRef::path("textures/player.png").unwrap()),
        ),
    )]);
    let b = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        nara_identity::SceneEntityId::new("player-b").unwrap(),
    )
    .with_component(
        ComponentTypeId::new("nara.transform.Transform2d"),
        identity_transform_record(),
    )
    .with_component(
        ComponentTypeId::new("nara.sprite.Sprite"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            sprite_value(&AssetRef::path("textures/player.png").unwrap()),
        ),
    )]);
    fs::write(
        project.root.join("scenes/b.scene.json"),
        b.to_json_string().unwrap(),
    )
    .unwrap();
    let (observations, received) = mpsc::sync_channel(2);
    let recipe = crate::ProductRecipe::new()
        .add_configured_plugin(
            TravelConfiguration(observations),
            |configuration: &TravelConfiguration| TravelPlugin(configuration.0.clone()),
        )
        .unwrap();
    let mut editor = super::super::editor::EditorProjectSession::open_trusted_compiled(
        project.capability(),
        super::super::editor::TrustedCompiledEditorProjectIntent::new().with_recipe(recipe),
    )
    .unwrap();
    let budget = editor.test_project_content_budget_host();
    let editor_revision = editor.test_project_content_revision();
    assert_eq!(budget.snapshot().active_generations(), 1);
    let document = editor.workspace().active_document().unwrap();
    assert_eq!(
        editor
            .workspace()
            .scene(document)
            .unwrap()
            .session()
            .document(),
        &a
    );
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut commits = Vec::new();
    while commits.len() != 2 {
        editor.drive_editor_frame(Duration::ZERO);
        assert!(budget.snapshot().active_generations() <= 2);
        assert!(!matches!(
            editor.play_view().state(),
            EditorPlayState::Faulted | EditorPlayState::CloseIncomplete
        ));
        commits.extend(received.try_iter());
        assert!(
            Instant::now() < deadline,
            "Editor game did not complete A -> B -> A: {:?}",
            editor.play_view()
        );
        if commits.len() != 2 {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    assert_eq!(editor.play_view().state(), EditorPlayState::Running);
    assert_eq!(commits[0].path, "b.scene.json");
    // Ready supplies the canonical expanded document, including schema defaults absent from the
    // authored fixture. The plugin compares that complete target to CurrentScene after commit.
    assert_eq!(commits[0].prepared_source.roots, b.roots);
    assert_eq!(commits[0].committed.entity_ids(), b.roots.as_slice());
    assert_eq!(commits[0].prepared_source.entities.len(), 1);
    assert_eq!(commits[0].prepared_source.entities[0].id, b.entities[0].id);
    assert_eq!(commits[1].path, "startup.scene.json");
    assert_eq!(commits[1].prepared_source.roots, a.roots);
    assert_eq!(commits[1].committed.entity_ids(), a.roots.as_slice());
    assert_eq!(commits[1].prepared_source.entities.len(), 1);
    assert_eq!(commits[1].prepared_source.entities[0].id, a.entities[0].id);
    assert_eq!(commits[1].expected, commits[0].committed);
    assert_ne!(
        commits[0].expected.instance_id(),
        commits[0].committed.instance_id()
    );
    assert_ne!(
        commits[0].committed.instance_id(),
        commits[1].committed.instance_id()
    );
    assert_eq!(
        commits[0].expected.domain_id(),
        commits[1].committed.domain_id()
    );
    assert_ne!(commits[0].image_allocation, commits[1].image_allocation);
    assert_eq!(budget.snapshot().generation_high_water(), 2);
    assert_eq!(
        budget.snapshot().active_generations(),
        1,
        "the returned A revision shares the Editor preview's generation instead of keeping B resident"
    );
    assert_eq!(editor.test_project_content_revision(), editor_revision);
    assert_eq!(
        editor
            .workspace()
            .scene(document)
            .unwrap()
            .session()
            .document(),
        &a
    );

    assert_eq!(
        editor.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor.play_view().state() != EditorPlayState::Empty {
        editor.drive_editor_frame(Duration::ZERO);
        assert!(
            Instant::now() < deadline,
            "Editor Play did not finish Stop: {:?}",
            editor.play_view()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        budget.snapshot().active_generations(),
        1,
        "the authoring/preview snapshot remains a legitimate owner after Stop"
    );
    assert!(budget.snapshot().active_reservations() > 0);
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        super::super::editor::EditorProjectCloseProgress::Complete(_)
    ));
    drop(editor);
    let retired = budget.snapshot();
    assert_eq!(retired.active_generations(), 0);
    assert_eq!(retired.active_reservations(), 0);
    for &kind in ProjectContentBudgetKind::ALL {
        assert_eq!(retired.active(kind), 0, "{kind:?}");
    }
}
