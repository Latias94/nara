use std::time::Duration;

use nara_app::{App, RuntimeInstance};
use nara_input::{InputPointerId, InputSeatId, InputWindowId};
use nara_reflect::{ComponentRegistry, ComponentRegistryPlugin};
use nara_scene::{SceneDocument, SceneEntityId, SceneEntityRecord, SceneEntitySource};
use nara_ui::{
    UiAdapterDiagnostics, UiNode, UiPlugin, UiRoot, UiStyle, UiSurfaceEndpoint, UiSurfaceSlot,
    install_ui_surface, latest_ui_frame,
};
use nara_ui_kernel::{UiLayer, UiSafeAreaInsets, UiSize, UiSurfaceContentTransform, UiSurfaceId};

#[test]
fn independent_scenes_collide_only_when_their_document_local_ui_roots_match() {
    for second_root in ["different-root", "root"] {
        let mut app = App::new();
        app.add_plugins((
            ComponentRegistryPlugin,
            nara_hierarchy::HierarchyPlugin,
            nara_input::InputPlugin,
            UiPlugin::default(),
        ))
        .unwrap();
        let surface = UiSurfaceId::new(7).unwrap();
        let size = UiSize::new(200.0, 100.0).unwrap();
        let slot = UiSurfaceSlot::new(
            surface,
            UiSurfaceEndpoint::new(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            ),
            UiSurfaceContentTransform::new(size, size, UiSafeAreaInsets::default(), 1.0).unwrap(),
        )
        .unwrap();
        let world = app.world_mut().unwrap();
        install_ui_surface(world, slot).unwrap();
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();

        let mut instances = Vec::new();
        for (root, marker) in [("root", "first-document"), (second_root, "second-document")] {
            let root = SceneEntityId::new(root).unwrap();
            let document = SceneDocument::from_ordered_roots([
                SceneEntityRecord::new(root.clone()),
                SceneEntityRecord::new(SceneEntityId::new(marker).unwrap()),
            ]);
            let report = nara_scene::spawn_scene(
                world,
                &registry,
                &nara_scene::ExpandedSceneCandidate::plain(
                    scene_source_key(marker),
                    std::sync::Arc::new((document).clone()),
                ),
                &scene_occurrence(marker),
                None,
            );
            assert!(!report.diagnostics.has_errors());
            let instance = report.instance.unwrap();
            let root_entity = world
                .iter_entities()
                .find(|entity| {
                    entity.get::<SceneEntitySource>().is_some_and(|source| {
                        source.instance_id == instance.instance_id() && source.entity_id == root
                    })
                })
                .unwrap()
                .id();
            world
                .entity_mut(root_entity)
                .insert((UiRoot::new(UiLayer::Base), UiNode::new(UiStyle::fill())));
            instances.push(instance);
        }
        assert_ne!(instances[0].instance_id(), instances[1].instance_id());

        let mut runtime = RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        let frame = latest_ui_frame(runtime.world(), surface).unwrap();
        let diagnostics = runtime
            .world()
            .resource::<UiAdapterDiagnostics>()
            .as_slice();
        if second_root == "root" {
            assert!(frame.is_none());
            assert!(
                diagnostics
                    .iter()
                    .any(|item| item.code() == "root-key-duplicate")
            );
        } else {
            let frame = frame.expect("different local roots must compile in the same World");
            assert_eq!(frame.root_outcomes().len(), 2);
            assert_ne!(
                frame.root_outcomes()[0].key(),
                frame.root_outcomes()[1].key()
            );
            assert!(diagnostics.is_empty());
        }
        runtime
            .finish()
            .expect("the window-free UI runtime must retire");
    }
}

fn scene_source_key(name: &str) -> nara_scene::SourceDocumentKey {
    nara_scene::SourceDocumentKey::named(
        "ae9c4204-2585-4d22-9da7-6400ebd89664".parse().unwrap(),
        name,
        nara_scene::SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn scene_occurrence(slot: &str) -> nara_scene::SceneOccurrenceKey {
    nara_scene::SceneOccurrenceKey::new("nara.scene.tests", slot).unwrap()
}
