use super::*;
use std::convert::Infallible;

use crate::image::ImageColorSpace;
use crate::material::{
    Material2d, Material2dAsset, Material2dBinding, Material2dDomain, Material2dSource,
};
use nara_asset::{
    AssetImportSource, AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetProductRef,
    AssetRef, AssetServer, AssetSourceKind, ImportedProductId, LoadState, SourceAssetImporter,
};
use nara_core::ByteLimit;
use nara_ecs::{Query, Res, ResMut};

mod frozen_textures;

const SHADER: &str = "
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let palette = textureLoad(material_texture, vec2<i32>(0, 0), 0);
    let accent = textureLoad(accent_texture, vec2<i32>(0, 0), 0);
    let color = palette.rgb * material_parameters.y + accent.rgb * material_parameters.x;
    return vec4<f32>(color * tint.rgb, sampled.a * palette.a * accent.a * tint.a);
}";

fn product(index: u32) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(&format!("a09ca315-79e5-47db-bef1-{index:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn image_product(index: u32) -> AssetProductRef {
    AssetProductRef::new(
        product(100).source(),
        ImportedProductId::parse_subproduct(&format!("a57907da-4cf4-42b6-9479-{index:012x}"))
            .unwrap(),
    )
    .unwrap()
}

fn declaration(id: &str, version: u32) -> Material2dDeclaration {
    Material2dDeclaration::from_parameters::<PaletteParameters>(
        id,
        version,
        AlphaMode2d::Opaque,
        vec![
            MaterialTextureDeclaration::new("accent", "accent_texture", 3).unwrap(),
            MaterialTextureDeclaration::new("palette", "material_texture", 1).unwrap(),
        ],
    )
    .unwrap()
}

fn parameters(offset: f32) -> PaletteParameters {
    PaletteParameters {
        x: offset,
        y: 1.0 - offset,
        z: 0.0,
        w: 0.0,
    }
}

fn import_material(
    identity: AssetProductRef,
    declaration: Material2dDeclaration,
    palette: AssetRef,
    accent: AssetRef,
    shader: &str,
) -> Material2dAsset {
    let shader_path = AssetPath::new("shaders/authored-material.wgsl").unwrap();
    let source = Material2dSource::new(
        declaration,
        vec![shader_path.clone()],
        parameters(0.0).parameter_values().unwrap(),
        BTreeMap::from([
            ("palette".to_owned(), palette),
            ("accent".to_owned(), accent),
        ]),
    )
    .unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            identity.source(),
            AssetPath::new("materials/authored.material2d").unwrap(),
            AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.to_owned()),
        ),
        (),
    )
    .to_json_string()
    .unwrap();
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(metadata.as_bytes()).unwrap();
    let mut reads = 0;
    let mut reader = |path: &AssetPath, limit: ByteLimit| {
        assert_eq!(path, &shader_path);
        assert!(shader.len() <= limit.get());
        reads += 1;
        Ok::<_, Infallible>(Arc::<[u8]>::from(shader.as_bytes()))
    };
    let imported = {
        let mut input =
            AssetImportSource::new(&metadata, Arc::from(source.encode().unwrap()), &mut reader);
        Material2dDomain::prepare(&mut input).unwrap()
    };
    assert_eq!(reads, 1);
    assert_eq!(imported.product(), identity);
    assert_eq!(imported.generation().inputs().len(), 1);
    assert_eq!(imported.generation().inputs()[0].source(), shader);
    imported
}

#[derive(Default)]
struct TestAssets {
    server: AssetServer,
    materials: Assets<Material2dAsset>,
    images: Assets<ImageAsset>,
    states: AssetStates,
    events: AssetEvents,
}

impl TestAssets {
    fn image(&mut self, product: AssetProductRef) -> Handle<ImageAsset> {
        let handle = self.server.reserve_product(product).unwrap();
        let image = ImageAsset::from_product(
            product,
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            vec![255; 4],
        )
        .unwrap();
        self.images
            .commit_loaded(
                handle,
                image,
                &mut self.states,
                &mut self.events,
                None,
                None,
            )
            .unwrap();
        handle
    }

    fn material(&mut self, asset: Material2dAsset) -> Handle<Material2dAsset> {
        let handle = self.server.reserve_product(asset.product()).unwrap();
        self.materials
            .commit_loaded(
                handle,
                asset,
                &mut self.states,
                &mut self.events,
                None,
                None,
            )
            .unwrap();
        handle
    }

    fn resolve(&self, reference: &AssetRef) -> Option<Handle<ImageAsset>> {
        match reference {
            AssetRef::Product(product) => self.server.get_product_handle(*product),
            AssetRef::Path(_) | AssetRef::StableId(_) => None,
        }
    }

    fn install(self, app: &mut App) {
        let world = app.world_mut().unwrap();
        world.insert_resource(self.server);
        world.insert_resource(self.materials);
        world.insert_resource(self.images);
        world.insert_resource(self.states);
        world.insert_resource(self.events);
    }
}

fn spawn_authored(world: &mut World, authored: Material2d) -> Entity {
    world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
            authored,
        ))
        .id()
}

fn captures(world: &World, entity: Entity) -> Vec<&Material2dBinding> {
    prepared_captures(world, entity)
        .into_iter()
        .map(|capture| &capture.binding)
        .collect()
}

fn prepared_captures(world: &World, entity: Entity) -> Vec<&Arc<PreparedMaterial2d>> {
    world
        .resource::<ExtractedSprites>()
        .as_slice()
        .iter()
        .filter(|draw| draw.entity == entity)
        .map(|draw| draw.material_extension.as_ref().unwrap())
        .collect()
}

fn assert_shared_capture(world: &World, entity: Entity, expected: &Material2dBinding) {
    let captured = prepared_captures(world, entity);
    assert_eq!(captured.len(), 3);
    for capture in &captured {
        assert_eq!(&capture.binding, expected);
        assert!(Arc::ptr_eq(captured[0], capture));
        assert!(std::ptr::eq(
            captured[0].binding.uniform_bytes(),
            capture.binding.uniform_bytes()
        ));
    }
    assert_eq!(world.resource::<SpriteRenderStats>().extracted_sprites, 3);
    assert_eq!(
        world.resource::<SpriteRenderStats>().extracted_tile_cells,
        2
    );
    assert_eq!(world.resource::<SpriteRenderStats>().missing_materials, 0);
}

#[test]
fn authored_sprite_and_tilemap_capture_current_product_and_retain_a_after_b_and_removal() {
    let palette = image_product(1);
    let accent_a = image_product(2);
    let accent_b = image_product(3);
    let asset_a = import_material(
        product(1),
        declaration("test.authored", 1),
        AssetRef::Product(palette),
        AssetRef::path("textures/accent.png").unwrap(),
        SHADER,
    );
    let generation_a = asset_a.generation().clone();
    let asset_a = asset_a
        .resolve_texture_references(|reference| {
            Ok::<_, Infallible>(match reference {
                AssetRef::Product(product) => *product,
                AssetRef::Path(path) => {
                    assert_eq!(path.as_str(), "textures/accent.png");
                    accent_a
                }
                AssetRef::StableId(_) => panic!("the fixture has no stable-only texture"),
            })
        })
        .unwrap();
    assert_eq!(asset_a.generation(), &generation_a);
    let shader_b = SHADER.replace("color * tint.rgb", "color.bgr * tint.rgb");
    let asset_b = import_material(
        product(1),
        declaration("test.authored", 1),
        AssetRef::Product(palette),
        AssetRef::Product(accent_b),
        &shader_b,
    );
    let mut assets = TestAssets::default();
    assets
        .server
        .reserve_product::<String>(product(100))
        .unwrap();
    assets.image(palette);
    let accent_handle_a = assets.image(accent_a);
    let accent_handle_b = assets.image(accent_b);
    let override_a = assets.image(image_product(4));
    let override_b = assets.image(image_product(5));
    let handle = assets.material(asset_a.clone());
    let authored_a = Material2d::from_parameters(
        handle,
        asset_a.generation().declaration(),
        &parameters(0.25),
        BTreeMap::from([("palette".to_owned(), override_a)]),
    )
    .unwrap();
    let authored_b = Material2d::from_parameters(
        handle,
        asset_b.generation().declaration(),
        &parameters(0.75),
        BTreeMap::from([("palette".to_owned(), override_b)]),
    )
    .unwrap();
    let expected_a = authored_a
        .bind(&asset_a, |reference| assets.resolve(reference))
        .unwrap();
    let expected_b = authored_b
        .bind(&asset_b, |reference| assets.resolve(reference))
        .unwrap();
    let mut app = sprite_app();
    let world = app.world_mut().unwrap();
    let authored = spawn_authored(world, authored_a);
    let mut tilemap = Tilemap::new(Vec2::ONE);
    tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(0)));
    tilemap.set_cell(TileCoord::new(1, 0), TileCell::new(TileIndex::new(1)));
    world.entity_mut(authored).insert(tilemap);
    let direct = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
            expected_a.clone(),
        ))
        .id();
    let stock = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
        ))
        .id();
    place_roots(world, &[authored, direct, stock]);
    assets.install(&mut app);
    let mut successor = Some(asset_b);
    app.add_systems(
        CoreStage::Update,
        move |time: Res<nara_app::RealTime>,
              mut materials: ResMut<Assets<Material2dAsset>>,
              mut states: ResMut<AssetStates>,
              mut events: ResMut<AssetEvents>,
              mut authored_materials: Query<&mut Material2d>| {
            match time.frame {
                2 => {
                    materials
                        .commit_loaded(
                            handle,
                            successor.take().unwrap(),
                            &mut states,
                            &mut events,
                            None,
                            None,
                        )
                        .unwrap();
                    *authored_materials.get_mut(authored).unwrap() = authored_b.clone();
                }
                3 => {
                    materials
                        .remove_with_state(handle, &mut states, &mut events)
                        .unwrap();
                }
                _ => {}
            }
        },
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_shared_capture(runtime.world(), authored, &expected_a);
    let retained: Material2dBinding = captures(runtime.world(), authored)[0].clone();
    assert_eq!(retained.textures()["accent"], accent_handle_a);
    assert_eq!(retained.textures()["palette"], override_a);
    assert_eq!(
        retained.uniform_bytes(),
        parameters(0.25).encode_uniform().unwrap()
    );

    runtime.drive(Duration::ZERO).unwrap();
    assert_shared_capture(runtime.world(), authored, &expected_b);
    assert_eq!(captures(runtime.world(), direct), vec![&expected_a]);
    assert_eq!(expected_b.textures()["accent"], accent_handle_b);
    assert_eq!(expected_b.textures()["palette"], override_b);
    assert_ne!(retained.generation(), expected_b.generation());
    assert_ne!(retained.uniform_bytes(), expected_b.uniform_bytes());
    assert_eq!(
        expected_b.generation().shader().user_source(),
        format!("{shader_b}\n")
    );
    assert_eq!(
        runtime
            .world()
            .resource::<Assets<Material2dAsset>>()
            .get(handle)
            .unwrap()
            .generation(),
        expected_b.generation()
    );

    runtime.drive(Duration::ZERO).unwrap();
    assert!(captures(runtime.world(), authored).is_empty());
    assert_eq!(captures(runtime.world(), direct), vec![&expected_a]);
    let extracted = runtime.world().resource::<ExtractedSprites>();
    assert_eq!(extracted.len(), 2);
    assert_eq!(extracted.as_slice()[1].entity, stock);
    assert!(extracted.as_slice()[1].material_extension.is_none());
    assert!(
        runtime
            .world()
            .resource::<Assets<Material2dAsset>>()
            .get(handle)
            .is_none()
    );
    assert_eq!(
        runtime
            .world()
            .resource::<SpriteRenderStats>()
            .missing_materials,
        1
    );
    assert_eq!(
        runtime
            .world()
            .resource::<SpriteRenderStats>()
            .extracted_tile_cells,
        0
    );
    assert_eq!(retained, expected_a);
    assert_eq!(retained.generation(), &generation_a);
    assert_eq!(
        retained.generation().shader().user_source(),
        format!("{SHADER}\n")
    );
    assert_eq!(
        retained.parameters(),
        &parameters(0.25).parameter_values().unwrap()
    );
    assert_eq!(retained.textures()["accent"], accent_handle_a);
    assert_eq!(retained.textures()["palette"], override_a);
    runtime.finish().unwrap();
}

#[test]
fn authored_publication_allows_last_good_and_rejects_invalidated_overwritten_or_removed_slots() {
    let asset = import_material(
        product(1),
        declaration("test.authored", 1),
        AssetRef::Product(image_product(1)),
        AssetRef::Product(image_product(2)),
        SHADER,
    );
    let mut assets = TestAssets::default();
    assets.image(image_product(1));
    assets.image(image_product(2));
    let handle = assets.material(asset.clone());
    let loaded_slot = assets.materials.slot_revision(handle);
    let authored = Material2d::from_parameters(
        handle,
        asset.generation().declaration(),
        &parameters(0.25),
        BTreeMap::new(),
    )
    .unwrap();
    let expected = authored
        .bind(&asset, |reference| assets.resolve(reference))
        .unwrap();
    let mut app = sprite_app();
    let world = app.world_mut().unwrap();
    let entity = spawn_authored(world, authored);
    let mut tilemap = Tilemap::new(Vec2::ONE);
    tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(0)));
    tilemap.set_cell(TileCoord::new(1, 0), TileCell::new(TileIndex::new(1)));
    world.entity_mut(entity).insert(tilemap);
    let direct = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
            expected.clone(),
        ))
        .id();
    let stock = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
        ))
        .id();
    place_roots(world, &[entity, direct, stock]);
    assets.install(&mut app);
    app.add_systems(
        CoreStage::Update,
        move |time: Res<nara_app::RealTime>,
              mut materials: ResMut<Assets<Material2dAsset>>,
              mut states: ResMut<AssetStates>,
              mut events: ResMut<AssetEvents>| {
            match time.frame {
                2 => {
                    materials
                        .mutation_baseline(handle, &states)
                        .unwrap()
                        .record_failure(
                            &mut materials,
                            &mut states,
                            &mut events,
                            "the new material candidate was rejected",
                        )
                        .unwrap();
                }
                3 => states.invalidate_publication(handle.id()),
                4 => {
                    materials
                        .commit_loaded(handle, asset.clone(), &mut states, &mut events, None, None)
                        .unwrap();
                }
                5 => {
                    assert!(materials.insert(handle, asset.clone()).is_some());
                }
                6 => {
                    assert!(
                        materials
                            .remove_with_state(handle, &mut states, &mut events)
                            .unwrap()
                            .is_some()
                    );
                    assert!(materials.insert(handle, asset.clone()).is_none());
                }
                _ => {}
            }
        },
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    for frame in 1..=6 {
        runtime.drive(Duration::ZERO).unwrap();
        let world = runtime.world();
        let materials = world.resource::<Assets<Material2dAsset>>();
        let states = world.resource::<AssetStates>();
        let state = states.state(handle.id()).unwrap();
        assert!(materials.get(handle).is_some());
        assert_eq!(captures(world, direct), vec![&expected]);
        if matches!(frame, 3 | 5 | 6) {
            assert!(captures(world, entity).is_empty());
            let extracted = world.resource::<ExtractedSprites>().as_slice();
            assert_eq!(extracted.len(), 2);
            assert_eq!(extracted[0].entity, direct);
            assert_eq!(extracted[0].source_order, 0);
            assert_eq!(extracted[1].entity, stock);
            assert_eq!(extracted[1].source_order, 1);
            assert!(extracted[1].material_extension.is_none());
            let stats = world.resource::<SpriteRenderStats>();
            assert_eq!(stats.missing_materials, 1);
            assert_eq!(stats.extracted_sprites, 2);
            assert_eq!(stats.extracted_tile_cells, 0);
        } else {
            assert_shared_capture(world, entity, &expected);
            assert!(state.usable_version().is_some());
            assert_eq!(
                states.published_slot_revision(handle.id()),
                Some(&materials.slot_revision(handle))
            );
        }
        match frame {
            2 => {
                assert!(matches!(state.load_state(), LoadState::Failed { .. }));
                assert_eq!(
                    states.published_slot_revision(handle.id()),
                    Some(&loaded_slot)
                );
                assert_eq!(materials.slot_revision(handle), loaded_slot);
            }
            3 | 6 => {
                assert!(state.usable_version().is_none());
                assert!(states.published_slot_revision(handle.id()).is_none());
                if frame == 6 {
                    assert_eq!(state.load_state(), &LoadState::Removed);
                }
            }
            5 => {
                assert!(state.usable_version().is_some());
                assert_ne!(
                    states.published_slot_revision(handle.id()),
                    Some(&materials.slot_revision(handle))
                );
            }
            _ => {}
        }
    }
    runtime.finish().unwrap();
}

#[test]
fn authored_capture_rejects_another_product_published_into_the_selected_material_slot() {
    let asset = import_material(
        product(1),
        declaration("test.authored", 1),
        AssetRef::Product(image_product(1)),
        AssetRef::Product(image_product(2)),
        SHADER,
    );
    let wrong = import_material(
        product(2),
        declaration("test.authored", 1),
        AssetRef::Product(image_product(1)),
        AssetRef::Product(image_product(2)),
        SHADER,
    );
    let mut assets = TestAssets::default();
    assets.image(image_product(1));
    assets.image(image_product(2));
    let handle = assets.material(asset.clone());
    let authored = Material2d::from_parameters(
        handle,
        asset.generation().declaration(),
        &parameters(0.25),
        BTreeMap::new(),
    )
    .unwrap();
    let mut app = sprite_app();
    let entity = spawn_authored(app.world_mut().unwrap(), authored);
    place_roots(app.world_mut().unwrap(), &[entity]);
    assets.install(&mut app);
    app.add_systems(
        CoreStage::Update,
        move |time: Res<nara_app::RealTime>,
              mut materials: ResMut<Assets<Material2dAsset>>,
              mut states: ResMut<AssetStates>,
              mut events: ResMut<AssetEvents>| {
            if time.frame == 2 {
                materials
                    .commit_loaded(handle, wrong.clone(), &mut states, &mut events, None, None)
                    .unwrap();
            }
        },
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(captures(runtime.world(), entity).len(), 1);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(captures(runtime.world(), entity).is_empty());
    assert_eq!(
        runtime
            .world()
            .resource::<SpriteRenderStats>()
            .missing_materials,
        1
    );
    let world = runtime.world();
    assert_eq!(
        world.resource::<AssetServer>().product(handle.id()),
        Some(product(1))
    );
    assert_eq!(
        world
            .resource::<Assets<Material2dAsset>>()
            .get(handle)
            .unwrap()
            .product(),
        product(2)
    );
    assert_eq!(
        world
            .resource::<AssetStates>()
            .published_slot_revision(handle.id()),
        Some(
            &world
                .resource::<Assets<Material2dAsset>>()
                .slot_revision(handle)
        )
    );
    runtime.finish().unwrap();
}

#[test]
fn authored_default_and_override_textures_require_actual_product_and_published_slot() {
    for override_texture in [false, true] {
        let asset = import_material(
            product(1),
            declaration("test.authored", 1),
            AssetRef::Product(image_product(1)),
            AssetRef::Product(image_product(2)),
            SHADER,
        );
        let mut assets = TestAssets::default();
        let default_handle = assets.image(image_product(1));
        assets.image(image_product(2));
        let effective_handle = if override_texture {
            assets.image(image_product(3))
        } else {
            default_handle
        };
        let effective_product = if override_texture {
            image_product(3)
        } else {
            image_product(1)
        };
        let image = |product, value| {
            ImageAsset::from_product(
                product,
                ImageExtent::new(1, 1),
                ImageFormat::Rgba8,
                ImageColorSpace::Srgb,
                vec![value; 4],
            )
            .unwrap()
        };
        let handle = assets.material(asset.clone());
        let overrides = if override_texture {
            BTreeMap::from([("palette".to_owned(), effective_handle)])
        } else {
            BTreeMap::new()
        };
        let authored = Material2d::from_parameters(
            handle,
            asset.generation().declaration(),
            &parameters(0.25),
            overrides,
        )
        .unwrap();
        let mut app = sprite_app();
        let entity = spawn_authored(app.world_mut().unwrap(), authored);
        place_roots(app.world_mut().unwrap(), &[entity]);
        assets.install(&mut app);
        app.add_systems(
            CoreStage::Update,
            move |time: Res<nara_app::RealTime>,
                  mut images: ResMut<Assets<ImageAsset>>,
                  mut states: ResMut<AssetStates>,
                  mut events: ResMut<AssetEvents>| {
                match time.frame {
                    2 => {
                        images
                            .mutation_baseline(effective_handle, &states)
                            .unwrap()
                            .record_failure(
                                &mut images,
                                &mut states,
                                &mut events,
                                "candidate failed; retain original image",
                            )
                            .unwrap();
                    }
                    3 => {
                        images.insert(effective_handle, image(effective_product, 255));
                    }
                    4 | 6 => {
                        images
                            .commit_loaded(
                                effective_handle,
                                image(effective_product, 255),
                                &mut states,
                                &mut events,
                                None,
                                None,
                            )
                            .unwrap();
                    }
                    5 => {
                        images
                            .commit_loaded(
                                effective_handle,
                                image(image_product(9), 128),
                                &mut states,
                                &mut events,
                                None,
                                None,
                            )
                            .unwrap();
                    }
                    _ => {}
                }
            },
        )
        .unwrap();
        let mut runtime = RuntimeInstance::start(app).unwrap();
        for (frame, valid) in [
            (1, true),
            (2, true),
            (3, false),
            (4, true),
            (5, false),
            (6, true),
        ] {
            runtime.drive(Duration::ZERO).unwrap();
            assert_eq!(
                captures(runtime.world(), entity).len(),
                usize::from(valid),
                "override={override_texture}, frame={frame}"
            );
            assert_eq!(
                runtime
                    .world()
                    .resource::<SpriteRenderStats>()
                    .missing_materials,
                u32::from(!valid),
                "override={override_texture}, frame={frame}"
            );
        }
        runtime.finish().unwrap();
    }
}

#[test]
fn value_only_edit_reuses_generation_and_explicit_texture_override_replaces_missing_default() {
    let missing_default = image_product(1);
    let accent = image_product(2);
    let asset = import_material(
        product(1),
        declaration("test.authored", 1),
        AssetRef::Product(missing_default),
        AssetRef::Product(accent),
        SHADER,
    );
    let mut assets = TestAssets::default();
    let accent_handle = assets.image(accent);
    let override_handle = assets.image(image_product(3));
    let handle = assets.material(asset.clone());
    let overrides = BTreeMap::from([("palette".to_owned(), override_handle)]);
    let first = Material2d::from_parameters(
        handle,
        asset.generation().declaration(),
        &parameters(0.25),
        overrides.clone(),
    )
    .unwrap();
    let second = Material2d::from_parameters(
        handle,
        asset.generation().declaration(),
        &parameters(0.75),
        overrides,
    )
    .unwrap();
    let mut app = sprite_app();
    let entity = spawn_authored(app.world_mut().unwrap(), first);
    place_roots(app.world_mut().unwrap(), &[entity]);
    assets.install(&mut app);
    app.add_systems(
        CoreStage::Update,
        move |time: Res<nara_app::RealTime>, mut materials: Query<&mut Material2d>| {
            if time.frame == 2 {
                *materials.get_mut(entity).unwrap() = second.clone();
            }
        },
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let retained: Material2dBinding = captures(runtime.world(), entity)[0].clone();
    runtime.drive(Duration::ZERO).unwrap();
    let later = captures(runtime.world(), entity);
    assert_eq!(later.len(), 1);
    assert_eq!(later[0].generation(), retained.generation());
    assert_eq!(later[0].generation(), asset.generation());
    assert_ne!(later[0].uniform_bytes(), retained.uniform_bytes());
    assert_eq!(
        retained.uniform_bytes(),
        parameters(0.25).encode_uniform().unwrap()
    );
    assert_eq!(
        later[0].uniform_bytes(),
        parameters(0.75).encode_uniform().unwrap()
    );
    assert_eq!(later[0].textures()["accent"], accent_handle);
    assert_eq!(later[0].textures()["palette"], override_handle);
    assert!(
        runtime
            .world()
            .resource::<AssetServer>()
            .get_product_handle::<ImageAsset>(missing_default)
            .is_none()
    );
    assert_eq!(
        runtime
            .world()
            .resource::<SpriteRenderStats>()
            .missing_materials,
        0
    );
    runtime.finish().unwrap();
}

#[derive(MaterialParameters)]
struct IntegerPaletteParameters {
    #[material(id = "palette.offset")]
    x: u32,
    #[material(id = "palette.strength")]
    y: f32,
    #[material(id = "palette.reserved-z")]
    z: f32,
    #[material(id = "palette.reserved-w")]
    w: f32,
}

#[test]
fn invalid_authored_assignments_skip_only_their_entities_without_claiming_or_reserving() {
    let palette = image_product(1);
    let accent = image_product(2);
    let asset = import_material(
        product(1),
        declaration("test.authored", 1),
        AssetRef::Product(palette),
        AssetRef::Product(accent),
        SHADER,
    );
    let mut assets = TestAssets::default();
    assets.image(palette);
    assets.image(accent);
    let handle = assets.material(asset.clone());
    let assignment = |handle, declaration: &Material2dDeclaration| {
        Material2d::from_parameters(handle, declaration, &parameters(0.25), BTreeMap::new())
            .unwrap()
    };
    let valid = assignment(handle, asset.generation().declaration());
    let expected = valid
        .bind(&asset, |reference| assets.resolve(reference))
        .unwrap();
    let missing_material = assets
        .server
        .reserve_product::<Material2dAsset>(product(2))
        .unwrap();
    let mut invalid = vec![
        assignment(missing_material, asset.generation().declaration()),
        assignment(handle, &declaration("test.other", 1)),
        assignment(handle, &declaration("test.authored", 2)),
    ];
    let integer_declaration = Material2dDeclaration::from_parameters::<IntegerPaletteParameters>(
        "test.authored",
        1,
        AlphaMode2d::Opaque,
        asset.generation().declaration().textures().to_vec(),
    )
    .unwrap();
    invalid.push(
        Material2d::from_parameters(
            handle,
            &integer_declaration,
            &IntegerPaletteParameters {
                x: 1,
                y: 0.75,
                z: 0.0,
                w: 0.0,
            },
            BTreeMap::new(),
        )
        .unwrap(),
    );
    let unknown_image = image_product(3);
    let absent_image = image_product(4);
    let wrong_type = image_product(5);
    let absent_handle = assets
        .server
        .reserve_product::<ImageAsset>(absent_image)
        .unwrap();
    let wrong_type_handle = assets.server.reserve_product::<String>(wrong_type).unwrap();
    for (index, image) in [unknown_image, absent_image, wrong_type]
        .into_iter()
        .enumerate()
    {
        let unavailable = import_material(
            product(10 + index as u32),
            declaration("test.authored", 1),
            AssetRef::Product(image),
            AssetRef::Product(accent),
            SHADER,
        );
        let handle = assets.material(unavailable);
        invalid.push(assignment(handle, asset.generation().declaration()));
    }
    invalid.push(
        Material2d::from_parameters(
            handle,
            asset.generation().declaration(),
            &parameters(0.25),
            BTreeMap::from([("palette".to_owned(), absent_handle)]),
        )
        .unwrap(),
    );
    let missing_count = invalid.len() as u32 + 1;
    let mut app = sprite_app();
    let world = app.world_mut().unwrap();
    let mut roots = invalid
        .into_iter()
        .map(|authored| spawn_authored(world, authored))
        .collect::<Vec<_>>();
    let ambiguous = spawn_authored(world, valid.clone());
    let mut tilemap = Tilemap::new(Vec2::ONE);
    tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(0)));
    tilemap.set_cell(TileCoord::new(1, 0), TileCell::new(TileIndex::new(1)));
    world
        .entity_mut(ambiguous)
        .insert((tilemap, expected.clone()));
    roots.push(ambiguous);
    let healthy = spawn_authored(world, valid);
    let stock = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
        ))
        .id();
    roots.extend([healthy, stock]);
    place_roots(world, &roots);
    assets.install(&mut app);
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let extracted = runtime.world().resource::<ExtractedSprites>().as_slice();
    assert_eq!(extracted.len(), 2);
    assert_eq!(extracted[0].entity, healthy);
    assert_eq!(extracted[0].source_order, 0);
    assert_eq!(
        extracted[0]
            .material_extension
            .as_ref()
            .map(|capture| &capture.binding),
        Some(&expected)
    );
    assert_eq!(extracted[1].entity, stock);
    assert_eq!(extracted[1].source_order, 1);
    assert!(extracted[1].material_extension.is_none());
    let stats = runtime.world().resource::<SpriteRenderStats>();
    assert_eq!(stats.missing_materials, missing_count);
    assert_eq!(stats.extracted_sprites, 2);
    assert_eq!(stats.extracted_tile_cells, 0);
    let server = runtime.world().resource::<AssetServer>();
    assert!(
        server
            .get_product_handle::<ImageAsset>(unknown_image)
            .is_none()
    );
    assert_eq!(
        server.get_product_handle::<ImageAsset>(absent_image),
        Some(absent_handle)
    );
    assert_eq!(
        server.get_product_handle::<String>(wrong_type),
        Some(wrong_type_handle)
    );
    assert!(
        server
            .get_product_handle::<ImageAsset>(wrong_type)
            .is_none()
    );
    runtime.finish().unwrap();
}

#[test]
fn noncanonical_default_references_reject_even_when_every_texture_is_overridden() {
    for reference in [
        AssetRef::path("textures/palette.png").unwrap(),
        AssetRef::StableId(product(100).source()),
    ] {
        let asset = import_material(
            product(1),
            declaration("test.authored", 1),
            reference,
            AssetRef::Product(image_product(2)),
            SHADER,
        );
        let mut assets = TestAssets::default();
        let override_handle = assets.image(image_product(1));
        let accent_handle = assets.image(image_product(2));
        let handle = assets.material(asset.clone());
        let authored = Material2d::from_parameters(
            handle,
            asset.generation().declaration(),
            &parameters(0.25),
            BTreeMap::from([
                ("palette".to_owned(), override_handle),
                ("accent".to_owned(), accent_handle),
            ]),
        )
        .unwrap();
        let mut app = sprite_app();
        let entity = spawn_authored(app.world_mut().unwrap(), authored);
        place_roots(app.world_mut().unwrap(), &[entity]);
        assets.install(&mut app);
        let mut runtime = RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        assert!(runtime.world().resource::<ExtractedSprites>().is_empty());
        assert_eq!(
            runtime
                .world()
                .resource::<SpriteRenderStats>()
                .missing_materials,
            1
        );
        runtime.finish().unwrap();
    }
}

#[test]
fn missing_optional_material_resources_preserve_code_first_and_stock_extraction() {
    let asset = import_material(
        product(1),
        declaration("test.authored", 1),
        AssetRef::Product(image_product(1)),
        AssetRef::Product(image_product(2)),
        SHADER,
    );
    for missing_store in [true, false] {
        let mut assets = TestAssets::default();
        assets.image(image_product(1));
        assets.image(image_product(2));
        let handle = assets.material(asset.clone());
        let authored = Material2d::from_parameters(
            handle,
            asset.generation().declaration(),
            &parameters(0.25),
            BTreeMap::new(),
        )
        .unwrap();
        let expected = authored
            .bind(&asset, |reference| assets.resolve(reference))
            .unwrap();
        let mut app = sprite_app();
        let world = app.world_mut().unwrap();
        let unavailable = spawn_authored(world, authored);
        let direct = world
            .spawn((
                Sprite::from_color(Vec2::ONE, Color::WHITE),
                Transform2d::default(),
                expected.clone(),
            ))
            .id();
        let stock = world
            .spawn((
                Sprite::from_color(Vec2::ONE, Color::WHITE),
                Transform2d::default(),
            ))
            .id();
        place_roots(world, &[unavailable, direct, stock]);
        assets.install(&mut app);
        if missing_store {
            app.world_mut()
                .unwrap()
                .remove_resource::<Assets<Material2dAsset>>();
        } else {
            app.world_mut().unwrap().remove_resource::<AssetServer>();
        }
        let mut runtime = RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let extracted = runtime.world().resource::<ExtractedSprites>().as_slice();
        assert_eq!(extracted.len(), 2);
        assert_eq!(extracted[0].entity, direct);
        assert_eq!(
            extracted[0]
                .material_extension
                .as_ref()
                .map(|capture| &capture.binding),
            Some(&expected)
        );
        assert_eq!(extracted[1].entity, stock);
        assert!(extracted[1].material_extension.is_none());
        assert_eq!(
            runtime
                .world()
                .resource::<SpriteRenderStats>()
                .missing_materials,
            1
        );
        runtime.finish().unwrap();
    }
}
