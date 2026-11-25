use std::time::Duration;

use crate::sprite::Sprite;
use crate::sprite_render::batching::SpriteBatches;
use crate::{
    Camera2d, Extent2d, ExtractedViews, RenderFrame, RenderFramePacket,
    RenderFrameSourceGeneration, RenderImage2d, RenderImagePacket, RenderTarget, ViewLoadOp,
    ViewportRect, build_image_render_frame_packet,
};
use nara_app::{App, RuntimeInstance};
use nara_core::{Color, Vec2};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_transform::Transform2d;

pub(crate) fn capture_split_sprite_views(
    material: Option<crate::material::Material2dBinding>,
) -> (RenderFramePacket, SpriteBatches) {
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_transform::TransformPlugin,
        crate::RenderComponentsPlugin,
        crate::RenderPlugin::default(),
        crate::image::ImagePreparePlugin,
        crate::sprite_render::SpriteRenderPlugin,
    ))
    .unwrap();
    let image = RenderImage2d::allocate_handle().unwrap();
    let target = RenderTarget::Image(image);
    let descriptor = RenderImagePacket::new(image, Extent2d::new(64, 64).unwrap()).unwrap();
    let generation = RenderFrameSourceGeneration::try_acquire().unwrap();
    let world = app.world_mut().unwrap();
    let mut entities = Vec::new();
    entities.push(
        world
            .spawn((
                Camera2d {
                    target,
                    viewport: Some(ViewportRect::new(32, 0, 32, 64).unwrap()),
                    viewport_height: 64.0,
                    order: 1,
                    ..Default::default()
                },
                Transform2d {
                    translation: Vec2::new(100.0, 0.0),
                    rotation: std::f32::consts::FRAC_PI_2,
                    ..Default::default()
                },
            ))
            .id(),
    );
    entities.push(
        world
            .spawn((
                Camera2d {
                    target,
                    viewport: Some(ViewportRect::new(0, 0, 32, 64).unwrap()),
                    viewport_height: 64.0,
                    ..Default::default()
                },
                Transform2d::default(),
            ))
            .id(),
    );
    for (x, color) in [
        (0.0, Color::rgb(1.0, 0.0, 0.0)),
        (100.0, Color::rgb(0.0, 1.0, 0.0)),
    ] {
        let mut entity = world.spawn((
            Sprite::from_color(Vec2::new(24.0, 8.0), color),
            Transform2d::from_translation(Vec2::new(x, 0.0)),
        ));
        if let Some(material) = &material {
            entity.insert(material.clone());
        }
        entities.push(entity.id());
    }
    HierarchyWriter::new(world)
        .apply_batch(
            &entities
                .into_iter()
                .map(|entity| HierarchyOperation::Place {
                    entity,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let driven = runtime.drive(Duration::ZERO);
    let snapshot = if driven.is_ok() {
        let views = runtime.world().resource::<ExtractedViews>();
        let loads = views
            .as_slice()
            .iter()
            .map(|view| view.load)
            .collect::<Vec<_>>();
        let packet = build_image_render_frame_packet(
            generation,
            runtime.world().resource::<RenderFrame>().index,
            views,
            descriptor,
        );
        let batches = runtime.world().resource::<SpriteBatches>().clone();
        Some((loads, packet, batches))
    } else {
        None
    };
    let mut retirement = runtime.begin_retirement();
    while retirement.retirement_state() != nara_app::RuntimeCandidateRetirementState::Retired {
        retirement.drive_retirement();
    }
    driven.unwrap();
    let (loads, packet, batches) = snapshot.unwrap();
    assert_eq!(loads, [ViewLoadOp::Clear(Color::BLACK), ViewLoadOp::Load]);
    (packet.unwrap().unwrap(), batches)
}
