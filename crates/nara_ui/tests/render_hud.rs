use std::{sync::Arc, time::Duration};

use nara_app::{App, CoreStage, RuntimeInstance};
use nara_ecs::World;
use nara_input::{InputPointerId, InputSeatId, InputTimeline, InputWindowId};
use nara_reflect::ComponentRegistryPlugin;
use nara_scene::hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_ui::kernel::{UiLayer, UiPaintPrimitive};
use nara_ui::{
    UiNode, UiRoot, UiRuntimeKey, UiSize, UiStyle, UiSurfaceContentTransform, UiSurfaceEndpoint,
    UiSurfaceId, UiSurfaceSlot, UiText, install_ui_surface, ui_candidate_frame,
};

const TEST_DOMAIN: &str = "nara.test.ui";

fn surface_id() -> UiSurfaceId {
    UiSurfaceId::new(7).unwrap()
}

#[test]
fn custom_font_uses_ordinary_adapter_and_configured_presentation() {
    fn compile_packets(world: &mut World) {
        let frames = nara_ui::ui_candidate_frames(world).unwrap();
        let stats = nara_render::hud::compile_ui_paint_packets(world, &frames).unwrap();
        assert_eq!(stats.admitted_packets, 1);
        assert_eq!(stats.rejected_packets, 0);
    }

    let font = nara_ui::text::FontFace::from_bytes(
        include_bytes!("text/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap();
    let bundled = nara_ui::text::bundled_font_face().unwrap();
    assert_ne!(font.id(), bundled.id());
    assert_eq!(
        nara_ui::plugin(font.clone()).key(),
        nara_ui::plugin(font.clone()).key()
    );
    assert_ne!(
        nara_ui::plugin(font.clone()).key(),
        nara_ui::plugin(bundled.clone()).key()
    );
    assert_eq!(
        nara_render::hud::plugin(font.clone()).key(),
        nara_render::hud::plugin(font.clone()).key()
    );
    assert_ne!(
        nara_render::hud::plugin(font.clone()).key(),
        nara_render::hud::plugin(bundled).key()
    );

    let mut configuration = App::new();
    configuration
        .add_plugins((
            ComponentRegistryPlugin,
            nara_scene::hierarchy::HierarchyPlugin,
            nara_scene::SpatialPlugin,
            nara_input::InputPlugin,
            (
                nara_render::RenderComponentsPlugin,
                nara_render::RenderPlugin::default(),
                nara_render::image::ImagePreparePlugin,
            ),
            nara_ui::plugin(font.clone()),
            nara_render::hud::plugin(font.clone()),
        ))
        .unwrap();
    configuration
        .add_systems(CoreStage::Queue, compile_packets)
        .unwrap();
    let world = configuration.world_mut().unwrap();
    {
        let mut input = world.resource_mut::<InputTimeline>();
        input
            .connect_endpoint(InputWindowId::new(11), InputSeatId::PRIMARY)
            .unwrap();
        input
            .connect_pointer(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            )
            .unwrap();
    }
    install_ui_surface(
        world,
        UiSurfaceSlot::new(
            surface_id(),
            UiSurfaceEndpoint::new(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            ),
            UiSurfaceContentTransform::direct(UiSize::new(200.0, 100.0).unwrap(), 1.0).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    nara_render::hud::presentation::replace_ui_presentation_routes(
        world,
        [nara_render::hud::presentation::UiPresentationRoute::new(
            surface_id(),
            nara_window::WindowId::PRIMARY,
        )
        .unwrap()],
    )
    .unwrap();
    let root = world
        .spawn((
            UiRoot::new(UiLayer::Base),
            UiNode::new(UiStyle::fill()),
            UiRuntimeKey::new(TEST_DOMAIN, "custom-font-root").unwrap(),
        ))
        .id();
    let label = world
        .spawn((
            UiNode::new(UiStyle::absolute(8.0, 6.0, 180.0, 40.0)),
            UiText::new("Nara mono", 18.0).unwrap(),
            UiRuntimeKey::new(TEST_DOMAIN, "custom-font-label").unwrap(),
        ))
        .id();
    HierarchyWriter::new(world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: root,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            },
            HierarchyOperation::Place {
                entity: label,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                    parent: root,
                }),
            },
        ])
        .unwrap();

    let mut app = RuntimeInstance::start(configuration).unwrap();
    app.drive(Duration::ZERO).unwrap();
    let frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    let glyphs = frame
        .paint()
        .iter()
        .filter_map(|primitive| match primitive {
            UiPaintPrimitive::Text { layout, .. } => Some(layout.glyphs()),
            _ => None,
        })
        .flatten()
        .collect::<Vec<_>>();
    assert!(!glyphs.is_empty());
    assert!(
        glyphs
            .iter()
            .all(|glyph| glyph.key().face_id() == font.id())
    );
    let (packets, _) = nara_render::hud::presentation::snapshot_ui_paint_packets(app.world())
        .unwrap()
        .into_parts();
    assert_eq!(packets.len(), 1);
    assert!(Arc::ptr_eq(&packets[0].frame(), &frame));
    assert!(
        packets[0]
            .glyph_pages()
            .iter()
            .any(|page| page.pixels().iter().any(|coverage| *coverage != 0))
    );
    app.finish().unwrap();
}
