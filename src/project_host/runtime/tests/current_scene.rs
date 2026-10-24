use super::*;
use crate::startup_scene::{
    SceneSource, replace_current_scene_with_product, restart_current_scene_with_product,
};
use nara_app::CoreStage;
use nara_core::{ByteLimit, ItemLimit};
use nara_scene::advanced::SceneProductTransactionLimits;

#[derive(Debug, Default)]
struct ProjectSceneRestartProbePlugin;

#[derive(Resource)]
struct RestartedCurrentScene;

impl Plugin for ProjectSceneRestartProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.project-scene-restart"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(CoreStage::Update, inspect_project_scene_restart)?;
        Ok(())
    }
}

fn inspect_project_scene_restart(world: &mut World) {
    if world.contains_resource::<RestartedCurrentScene>() {
        return;
    }
    let original = world.resource::<CurrentSceneOwner>().receipt().clone();
    let source_pointer = world.resource::<CurrentSceneOwner>().source() as *const SceneDocument;
    let entity_count = world.entities().len();
    let limits =
        SceneProductTransactionLimits::new(ItemLimit::new(1).unwrap(), ItemLimit::new(1).unwrap());
    let direct = SceneSource::direct(
        nara_scene::ExpandedSceneCandidate::plain(
            nara_scene::SourceDocumentKey::named(
                "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
                "project-scene-restart-fixture",
                nara_scene::SourceDocumentKind::Scene,
            )
            .unwrap(),
            Arc::new(SceneDocument::default()),
        ),
        ByteLimit::new(16 * 1024).unwrap(),
    )
    .unwrap();
    let rejected =
        replace_current_scene_with_product(world, &original, direct, limits, &[], |_| {
            panic!("project residency must reject before product configuration")
        });
    assert!(rejected.instance.is_none());
    assert!(report_has_code(
        &rejected.diagnostics,
        "scene.current.project-residency-required"
    ));
    assert_eq!(world.resource::<CurrentSceneOwner>().receipt(), &original);
    assert_eq!(world.entities().len(), entity_count);

    let restarted = restart_current_scene_with_product(world, &original, limits, &[], |_| {});
    assert!(!restarted.diagnostics.has_errors());
    let receipt = restarted.instance.unwrap();
    assert_ne!(receipt.instance_id(), original.instance_id());
    assert_eq!(world.resource::<CurrentSceneOwner>().receipt(), &receipt);
    assert_eq!(
        world.resource::<CurrentSceneOwner>().source() as *const SceneDocument,
        source_pointer
    );
    assert_eq!(world.entities().len(), entity_count);
    for id in receipt.entity_ids() {
        assert!(matches!(
            receipt.resolve(world, id),
            EntityLookup::Resolved(_)
        ));
    }
    world.insert_resource(RestartedCurrentScene);
}

#[test]
fn hosted_project_restart_updates_current_receipt_without_replacing_resident_source() {
    let project = TestProject::with_image("project-scene-restart");
    let (snapshot, plan) = project.snapshot_and_plan_with_plugin(PluginDefinition::for_default::<
        ProjectSceneRestartProbePlugin,
    >());
    let startup_revision = snapshot.revision();
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    host.complete_start(&mut attempt).unwrap();

    host.drive_running_runtime(Duration::ZERO).unwrap();

    assert!(
        host.with_running_world(|world| world.contains_resource::<RestartedCurrentScene>())
            .unwrap()
    );
    assert_eq!(host.running_content_revision(), Some(startup_revision));
    close_host(&mut host);
}
