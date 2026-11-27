use std::fs;

use nara_asset::{AssetScanMode, AssetSourcePath};
use nara_core::vfs::AssetPath;
use nara_editor_core::viewport::{PanelPoint, PanelSize, WorldPoint};
use nara_scene::authoring::SceneAsset;

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

fn asset_id(suffix: u16) -> AssetId {
    format!("0193a100-1111-7000-8000-00000000{suffix:04x}")
        .parse()
        .unwrap()
}

fn document(entities: Vec<SceneEntity>) -> SceneDocument {
    let mut document = SceneDocument::new("3a66a71f-f13a-4d5b-8c0c-b561f7838373".parse().unwrap());
    document.entities = entities;
    document
}

fn sprite_entity(id: EntityUuid, layer: i32, image: Option<AssetId>) -> SceneEntity {
    let mut entity = SceneEntity::new(id, "Sprite");
    entity
        .set(&SpriteSource {
            size: Vec2::splat(20.0),
            layer,
            image,
            ..SpriteSource::default()
        })
        .unwrap();
    entity
}

fn write_sidecar(root: &Path, path: &str, id: AssetId, importer: &str) {
    fs::write(
        root.join(format!("{path}.meta")),
        format!(r#"AssetMeta(id:"{id}",importer:"{importer}",settings:())"#),
    )
    .unwrap();
}

fn write_image(root: &Path, path: &str, id: AssetId, bytes: &[u8]) {
    let image = root.join(path);
    fs::create_dir_all(image.parent().unwrap()).unwrap();
    fs::write(image, bytes).unwrap();
    write_sidecar(root, path, id, "nara.image");
}

fn scan(root: &Path) -> ProjectAssetDatabase {
    ProjectAssetDatabase::scan(
        root,
        &[AssetSourcePath::new("assets").unwrap()],
        AssetScanMode::ReadOnly,
    )
    .unwrap()
}

#[test]
fn offscreen_scene_bounds_are_available_before_the_first_render() {
    let mut sprite = sprite_entity(entity_id(1), 0, None);
    sprite
        .set(&Transform2d::from_xy(12_000.0, -8_000.0))
        .unwrap();
    let mut hidden = sprite_entity(entity_id(2), 0, None);
    hidden.set(&Visibility::Hidden).unwrap();
    let mut scene = PreviewScene::new().unwrap();
    scene
        .set_document(&document(vec![sprite, hidden]), None)
        .unwrap();

    let expected = Some([Vec2::new(11_990.0, -8_010.0), Vec2::new(12_010.0, -7_990.0)]);
    assert_eq!(scene.scene_bounds(&[]), expected);
    assert_eq!(scene.scene_bounds(&[entity_id(1)]), expected);
    assert_eq!(scene.scene_bounds(&[entity_id(2)]), None);
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
    assert!(
        matches!(overlay.primitives.as_slice(), [OverlayPrimitive::Circle { center, radius, .. }]
            if *center == parent_transform.translation && (*radius - 4.0).abs() < 1e-6)
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
fn image_ids_share_residency_and_bad_edits_leave_the_preview_intact() {
    let directory = tempfile::tempdir().unwrap();
    let first_image = asset_id(1);
    let other_image = asset_id(2);
    let missing_image = asset_id(3);
    write_image(directory.path(), "assets/sprite.png", first_image, PNG);
    write_image(
        directory.path(),
        "assets/invalid.png",
        other_image,
        b"not a PNG",
    );
    let source = DirAssetSource::new(directory.path()).unwrap();
    let database = scan(directory.path());
    let assets = Some((&source, &database));
    let first_id = entity_id(1);
    let second_id = entity_id(2);
    let mut document = document(vec![
        sprite_entity(first_id, 0, Some(first_image)),
        sprite_entity(second_id, 0, Some(first_image)),
    ]);
    document.assets.insert(
        "unregistered".into(),
        SceneAsset {
            id: missing_image,
            hint: AssetPath::new("sprite.png").unwrap(),
        },
    );
    let mut scene = PreviewScene::new().unwrap();
    scene.set_document(&document, assets).unwrap();
    let handle = scene.images[&first_image].handle;
    assert_eq!(scene.images.len(), 1);
    let world = scene.runtime.world();
    for id in [first_id, second_id] {
        let sprite = world.get::<Sprite>(scene.entities[&id]).unwrap();
        assert_eq!(sprite.material.image, Some(handle));
    }
    let initial_frame = world.resource::<RealTime>().frame;

    document.entities[0]
        .set(&Transform2d::from_xy(4.0, 7.0))
        .unwrap();
    document.entities[1] = sprite_entity(second_id, 0, Some(missing_image));
    assert!(matches!(
        scene.set_document(&document, assets),
        Err(ViewportError::MissingImageAsset { id }) if id == missing_image
    ));
    document.entities[1] = sprite_entity(second_id, 0, Some(other_image));
    assert!(matches!(
        scene.set_document(&document, assets),
        Err(ViewportError::DecodeImage { path, .. }) if path == "assets/invalid.png"
    ));
    let invalid_path = directory.path().join("assets/invalid.png");
    fs::write(&invalid_path, PNG).unwrap();
    const IMAGE_LIMIT: u64 = 16 * 1024 * 1024;
    fs::OpenOptions::new()
        .write(true)
        .open(&invalid_path)
        .unwrap()
        .set_len(IMAGE_LIMIT + 1)
        .unwrap();
    assert!(matches!(
        scene.set_document(&document, assets),
        Err(ViewportError::SourceTooLarge { path, limit: IMAGE_LIMIT }) if path == "assets/invalid.png"
    ));
    assert_eq!(scene.images[&first_image].handle, handle);
    assert_eq!(
        scene.runtime.world().resource::<RealTime>().frame,
        initial_frame
    );
    assert_eq!(scene.world_affine(first_id), Some(Affine2::IDENTITY));
    assert!(scene.selection_quad(second_id).is_some());

    fs::write(invalid_path, PNG).unwrap();
    scene.set_document(&document, assets).unwrap();
    assert_ne!(scene.images[&other_image].handle, handle);
    assert_eq!(scene.images[&first_image].handle, handle);

    let other_directory = tempfile::tempdir().unwrap();
    write_image(
        other_directory.path(),
        "assets/sprite.png",
        first_image,
        PNG,
    );
    let other_source = DirAssetSource::new(other_directory.path()).unwrap();
    let other_database = scan(other_directory.path());
    document.entities.pop();
    scene
        .set_document(&document, Some((&other_source, &other_database)))
        .unwrap();
    assert_ne!(scene.images[&first_image].handle, handle);
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
fn prepared_material_reloads_use_shader_identity_and_keep_the_last_good_binding() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let image_id = asset_id(1);
    let material_id = asset_id(2);
    let shader_id = asset_id(3);
    let material = "assets/sprite.material.ron";
    let shader = "assets/sprite.wgsl";
    let shader_code = "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return sampled * tint * material_parameters.color; }";
    write_image(root, "assets/sprite.png", image_id, PNG);
    fs::write(
        root.join(material),
        format!("SpriteMaterial(shader: \"{shader_id}\")"),
    )
    .unwrap();
    write_sidecar(root, material, material_id, "nara.material");
    fs::write(root.join(shader), shader_code).unwrap();
    write_sidecar(root, shader, shader_id, "nara.shader");

    let id = entity_id(1);
    let mut sprite = sprite_entity(id, 0, Some(image_id));
    let mut sprite_source = sprite.typed::<SpriteSource>().unwrap().unwrap();
    sprite_source.material = Some(material_id);
    sprite.set(&sprite_source).unwrap();
    let document = document(vec![sprite]);
    let source = DirAssetSource::new(root).unwrap();
    let empty = PreviewAssets::default();
    let original = PreviewAssets::load(&document, &source, &scan(root), &empty).unwrap();
    let mut scene = PreviewScene::new().unwrap();
    scene.install_document(&document, Some(&original)).unwrap();
    let entity = scene.entities[&id];
    let handle = scene.images[&image_id].handle;
    let binding = scene
        .runtime
        .world()
        .get::<Material2dBinding>(entity)
        .unwrap()
        .clone();

    fs::write(root.join(shader), "invalid WGSL").unwrap();
    assert!(
        matches!(PreviewAssets::load(&document, &source, &scan(root), &original),
        Err(ViewportError::Material { path, .. }) if path == shader)
    );
    assert_eq!(
        scene.runtime.world().get::<Material2dBinding>(entity),
        Some(&binding)
    );
    assert_eq!(scene.images[&image_id].handle, handle);

    let moved = "assets/moved.wgsl";
    fs::rename(root.join(shader), root.join(moved)).unwrap();
    fs::rename(
        root.join(format!("{shader}.meta")),
        root.join(format!("{moved}.meta")),
    )
    .unwrap();
    fs::write(root.join(moved), shader_code).unwrap();
    let unchanged = PreviewAssets::load(&document, &source, &scan(root), &original).unwrap();
    assert!(original.same_content(&unchanged));

    fs::write(
        root.join(material),
        format!("SpriteMaterial(shader: \"{shader_id}\", color: (0.4,1.0,1.0,1.0))"),
    )
    .unwrap();
    fs::write(
        root.join(moved),
        shader_code.replace("sampled * tint", "sampled * tint * 0.8"),
    )
    .unwrap();
    fs::write(
        root.join("assets/sprite.png.meta"),
        format!("AssetMeta(id: \"{image_id}\", importer: \"nara.image\", settings: (srgb: false))"),
    )
    .unwrap();
    let updated = PreviewAssets::load(&document, &source, &scan(root), &unchanged).unwrap();
    assert!(!original.same_content(&updated));
    assert_ne!(original.image_hash(image_id), updated.image_hash(image_id));
    scene.install_document(&document, Some(&updated)).unwrap();
    let installed = scene
        .runtime
        .world()
        .get::<Material2dBinding>(entity)
        .unwrap();
    assert_ne!(
        installed.generation().shader().digest(),
        binding.generation().shader().digest()
    );
    assert_eq!(&installed.uniform_bytes()[..4], &0.4_f32.to_le_bytes());
    assert_eq!(scene.images[&image_id].handle, handle);
}

#[test]
fn an_unchanged_scene_tracks_a_moved_image_without_losing_the_last_good_preview() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let id = asset_id(1);
    let old_path = "assets/original.png";
    let new_path = "assets/sprites/moved.png";
    write_image(root, old_path, id, PNG);
    let mut document = document(vec![sprite_entity(entity_id(1), 0, Some(id))]);
    document.assets.insert(
        "sprite".into(),
        SceneAsset {
            id,
            hint: AssetPath::new("original.png").unwrap(),
        },
    );
    let source = DirAssetSource::new(root).unwrap();
    let mut scene = PreviewScene::new().unwrap();
    scene
        .set_document(&document, Some((&source, &scan(root))))
        .unwrap();
    let handle = scene.images[&id].handle;
    let frame = scene.runtime.world().resource::<RealTime>().frame;

    fs::create_dir(root.join("assets/sprites")).unwrap();
    fs::rename(root.join(old_path), root.join(new_path)).unwrap();
    fs::rename(
        root.join(format!("{old_path}.meta")),
        root.join(format!("{new_path}.meta")),
    )
    .unwrap();
    let moved = scan(root);
    assert_eq!(
        moved
            .record_for_stable_id(id.into())
            .unwrap()
            .path()
            .as_str(),
        new_path
    );
    fs::write(root.join(new_path), b"broken at its new location").unwrap();
    assert!(matches!(
        scene.set_document(&document, Some((&source, &moved))),
        Err(ViewportError::DecodeImage { path, .. }) if path == new_path
    ));
    assert_eq!(scene.images[&id].handle, handle);
    let installed = scene.runtime.world().resource::<Assets<ImageAsset>>();
    assert_eq!(installed.get(handle).unwrap().pixels(), [128, 64, 32, 255]);

    fs::write(root.join(new_path), PNG).unwrap();
    scene
        .set_document(&document, Some((&source, &moved)))
        .unwrap();
    assert_eq!(scene.images[&id].handle, handle);
    assert_eq!(scene.runtime.world().resource::<RealTime>().frame, frame);

    write_sidecar(root, new_path, id, "nara.raw");
    let wrong_kind = scan(root);
    assert!(matches!(
        scene.set_document(&document, Some((&source, &wrong_kind))),
        Err(ViewportError::InvalidImageAssetKind { id: actual, path, .. })
            if actual == id && path == new_path
    ));
    assert_eq!(scene.images[&id].handle, handle);
    assert_eq!(scene.runtime.world().resource::<RealTime>().frame, frame);
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
