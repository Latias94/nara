use std::fs;

use nara_core::DocumentId;
use nara_editor_core::viewport::{PanelPoint, PanelSize, WorldPoint};

use super::*;

const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x68, 0x70, 0x50, 0xf8,
    0x0f, 0x00, 0x04, 0x04, 0x01, 0xe0, 0xf2, 0xeb, 0x80, 0xf9, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn entity_id(suffix: u16) -> EntityUuid {
    format!("80b87da8-b82a-4a5f-90d0-ab6ea377{suffix:04x}")
        .parse()
        .unwrap()
}

fn document(entities: Vec<SceneEntity>) -> SceneDocument {
    SceneDocument {
        id: "3a66a71f-f13a-4d5b-8c0c-b561f7838373"
            .parse::<DocumentId>()
            .unwrap(),
        entities,
    }
}

fn sprite_entity(id: EntityUuid, layer: i32, image: Option<&str>) -> SceneEntity {
    let mut entity = SceneEntity::new(id, "Sprite");
    entity
        .set(&SpriteSource {
            size: Vec2::splat(20.0),
            layer,
            image: image.map(str::to_owned),
            ..SpriteSource::default()
        })
        .unwrap();
    entity
}

#[test]
fn picks_follow_the_renderer_and_reparenting_preserves_surviving_entities() {
    let parent_id = entity_id(1);
    let front_id = entity_id(2);
    let back_id = entity_id(3);
    let hidden_id = entity_id(4);
    let mut parent = SceneEntity::new(parent_id, "Hidden parent");
    let parent_transform = Transform2d {
        translation: Vec2::new(3.0, 2.0),
        rotation: 0.3,
        scale: Vec2::new(2.0, 1.0),
        z: 4.0,
    };
    parent.set(&parent_transform).unwrap();
    parent.set(&Visibility::Hidden).unwrap();
    let mut front = sprite_entity(front_id, 0, None);
    front.parent = Some(parent_id);
    front.set(&Visibility::Visible).unwrap();
    front
        .set(&ColliderSource {
            radius: Some(2.0),
            ..ColliderSource::default()
        })
        .unwrap();
    let back = sprite_entity(back_id, 1, None);
    let mut hidden = sprite_entity(hidden_id, 10, None);
    hidden.parent = Some(parent_id);
    hidden.set(&ColliderSource::default()).unwrap();
    let mut document = document(vec![front, parent, back, hidden]);
    let mut scene = PreviewScene::new().unwrap();
    scene.set_document(&document, None).unwrap();
    assert_eq!(
        scene.world_affine(front_id),
        Some(parent_transform.affine())
    );
    assert_eq!(
        scene.parent_affine(front_id),
        Some(parent_transform.affine())
    );
    assert_eq!(scene.selection_quad(hidden_id), None);

    let mut overlay = OverlayFrame::default();
    let stroke = OverlayStroke::new(Color::WHITE, 1.0);
    scene.append_colliders(&mut overlay, stroke);
    assert_eq!(overlay.primitives.len(), CIRCLE_SEGMENTS);
    let start = parent_transform
        .affine()
        .transform_point2(Vec2::new(2.0, 0.0));
    assert!(
        matches!(&overlay.primitives[0], OverlayPrimitive::Line { from, .. } if *from == start)
    );
    assert!(
        matches!(overlay.primitives.last(), Some(OverlayPrimitive::Line { to, .. }) if *to == start)
    );

    let camera = ViewportCamera2D::new(
        PanelSize::new(80.0, 60.0).unwrap(),
        WorldPoint(Vec2::ZERO),
        1.0,
    )
    .unwrap();
    let extent = Extent2d::new(80, 60).unwrap();
    scene.render_input(camera, extent).unwrap();
    let mut picks = Vec::new();
    scene.collect_picks(&mut picks);
    assert_eq!(
        picks.iter().map(|pick| pick.entity()).collect::<Vec<_>>(),
        [back_id, front_id]
    );
    let center = PanelPoint(Vec2::new(40.0, 30.0));
    assert_eq!(camera.pick(center, &picks), Some(front_id));

    let front_entity = scene.entities[&front_id];
    document
        .entities
        .retain(|row| row.id != parent_id && row.id != hidden_id);
    document.entities[0].parent = None;
    scene.set_document(&document, None).unwrap();
    assert_eq!(scene.entities[&front_id], front_entity);
    assert_eq!(scene.parent_affine(front_id), Some(Affine2::IDENTITY));
    assert_eq!(scene.world_affine(front_id), Some(Affine2::IDENTITY));
    assert_eq!(scene.world_affine(hidden_id), None);
    scene.collect_picks(&mut picks);
    assert_eq!(camera.pick(center, &picks), Some(back_id));
}

#[test]
fn png_paths_share_residency_and_bad_edits_leave_the_preview_intact() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("sprite.png"), PNG).unwrap();
    fs::write(directory.path().join("invalid.png"), b"not a PNG").unwrap();
    let source = DirAssetSource::new(directory.path()).unwrap();
    let path = AssetPath::new("sprite.png").unwrap();
    let first_id = entity_id(1);
    let second_id = entity_id(2);
    let mut document = document(vec![
        sprite_entity(first_id, 0, Some("sprite.png")),
        sprite_entity(second_id, 0, Some("sprite.png")),
    ]);
    let mut scene = PreviewScene::new().unwrap();
    scene.set_document(&document, Some(&source)).unwrap();
    let handle = scene.images[&path];
    let images = scene.runtime.world().resource::<Assets<ImageAsset>>();
    assert_eq!(images.get(handle).unwrap().pixels(), [128, 64, 32, 255]);
    assert_eq!(scene.images.len(), 1);
    let initial_frame = scene.runtime.world().resource::<RealTime>().frame;
    scene.set_document(&document, Some(&source)).unwrap();
    assert_eq!(
        scene.runtime.world().resource::<RealTime>().frame,
        initial_frame
    );

    document.entities[0]
        .set(&Transform2d::from_xy(4.0, 7.0))
        .unwrap();
    scene.set_document(&document, Some(&source)).unwrap();
    assert_eq!(scene.images[&path], handle);
    document.entities[1] = sprite_entity(second_id, 0, Some("invalid.png"));
    assert!(matches!(
        scene.set_document(&document, Some(&source)),
        Err(ViewportError::DecodeImage { .. })
    ));
    assert_eq!(scene.images[&path], handle);
    assert_eq!(
        scene.world_affine(first_id).unwrap().translation,
        Vec2::new(4.0, 7.0)
    );
    assert!(scene.selection_quad(second_id).is_some());

    let other_directory = tempfile::tempdir().unwrap();
    fs::write(other_directory.path().join("sprite.png"), PNG).unwrap();
    let other_source = DirAssetSource::new(other_directory.path()).unwrap();
    document.entities.pop();
    scene.set_document(&document, Some(&other_source)).unwrap();
    assert_ne!(scene.images[&path], handle);
    assert!(
        scene
            .runtime
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .is_none()
    );
}

#[test]
fn extracted_camera_matches_logical_picking_after_dpi_rounding() {
    let camera = ViewportCamera2D::new(
        PanelSize::new(501.25, 263.75).unwrap(),
        WorldPoint(Vec2::new(7.0, -23.0)),
        1.7,
    )
    .unwrap();
    let mut scene = PreviewScene::new().unwrap();
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let extent = crate::physical_extent(&camera, scale).unwrap();
        let input = scene.render_input(camera, extent).unwrap();
        let view = &input.packet().views()[0];
        let physical = Vec2::new(extent.width as f32, extent.height as f32);
        for point in [Vec2::ZERO, Vec2::new(45.0, 29.0), Vec2::new(-80.0, -52.0)] {
            let expected = camera.world_to_panel(WorldPoint(point)).unwrap().0;
            let actual = view.world_to_viewport(point) / physical * camera.panel_size().as_vec2();
            assert!(
                (expected - actual).length() < 0.0002,
                "{scale}: {expected} != {actual}"
            );
            assert!(
                (view
                    .viewport_to_world(view.world_to_viewport(point))
                    .unwrap()
                    - point)
                    .length()
                    < 0.0002
            );
        }
    }
}
