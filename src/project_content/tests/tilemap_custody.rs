//! Original large Scene/Prefab allocations and their shared content-budget lifetime.

use nara_asset::{AssetPathScope, AssetServer};
use nara_core::{Color, Vec2};
use nara_ecs::World;
use nara_reflect::{ComponentEncodeContext, ComponentRegistrySnapshot};
use nara_scene::CompiledSceneDocument;
use nara_tilemap::{
    TileAtlasLayout, TileCell, TileCoord, TileIndex, TileSet, TileSetSource, Tilemap,
};

use crate::project_content::ProjectContentBudgetSnapshot;

use super::*;

const SIDE: i32 = 64;
const TILEMAP: &str = "nara.tilemap.Tilemap";
const IMAGE_ID: &str = "706fd198-2412-4ccb-a398-f9f826fbe208";
const TILESET_ID: &str = "55885649-084d-462e-b252-dc8be3a34922";

fn tilemap_record(registry: &ComponentRegistrySnapshot, offset: i32) -> SceneComponentRecord {
    let mut server = AssetServer::new();
    let tileset = server
        .reserve::<TileSet>("assets/terrain.tileset.ron")
        .unwrap();
    let mut tilemap = Tilemap::new(Vec2::splat(1.0)).with_tileset(tileset);
    for y in 0..SIDE {
        for x in 0..SIDE {
            tilemap.set_cell(
                TileCoord::new(x + offset, y - SIDE / 2),
                TileCell::new(TileIndex::new(0)).with_color(Color::rgba(
                    x as f32 / SIDE as f32,
                    y as f32 / SIDE as f32,
                    if offset == 0 { 0.25 } else { 0.75 },
                    1.0,
                )),
            );
        }
    }
    assert_eq!(tilemap.cell_count(), 4_096);
    let mut world = World::new();
    world.insert_resource(server);
    let entity = world.spawn(tilemap).id();
    let component = ComponentTypeId::new(TILEMAP);
    let scope = AssetPathScope::under(AssetSourcePath::new("assets").unwrap());
    let context = ComponentEncodeContext::new().with_asset_path_scope(&scope);
    let value = registry
        .encode_component_with_context(&component, &world, entity, &context)
        .unwrap()
        .unwrap();
    SceneComponentRecord::new(registry.schema(&component).unwrap().version, value)
}

fn assert_tilemaps(
    document: &CompiledSceneDocument,
    direct: &SceneComponentRecord,
    inherited: &SceneComponentRecord,
    atlas: TileAtlasLayout,
) {
    assert_eq!(
        document.roots(),
        [
            SceneEntityId::new("direct-map").unwrap(),
            SceneEntityId::new("bound-map").unwrap(),
        ]
    );
    assert_eq!(document.entities().len(), 2);
    for (id, expected) in [("direct-map", direct), ("bound-map", inherited)] {
        let entity = document
            .entities()
            .iter()
            .find(|entity| entity.id().as_str() == id)
            .unwrap();
        assert!(entity.children().is_empty());
        let component = entity
            .components()
            .get(&ComponentTypeId::new(TILEMAP))
            .unwrap();
        assert_eq!(component.version(), expected.version);
        assert_eq!(component.value(), expected.value());
        let Some(cells) = component.value().field("cells").unwrap().as_list() else {
            panic!("the original Tilemap codec must emit a cell list");
        };
        assert_eq!(cells.len(), 4_096);
        nara_tilemap::validate_persisted_tilemap_membership(component.value(), |reference| {
            assert_eq!(reference, &AssetRef::path("terrain.tileset.ron").unwrap());
            Some(atlas)
        })
        .unwrap();
    }
}

fn assert_active_budget(
    actual: ProjectContentBudgetSnapshot,
    expected: ProjectContentBudgetSnapshot,
) {
    assert_eq!(actual.active_reservations(), expected.active_reservations());
    assert_eq!(actual.active_generations(), expected.active_generations());
    for &kind in ProjectContentBudgetKind::ALL {
        assert_eq!(actual.active(kind), expected.active(kind), "{kind:?}");
    }
}

#[test]
fn large_root_bound_tilemaps_release_original_documents_and_budget_at_their_last_owners() {
    let project = LazyContentProject::new();
    let (candidate, plan, loader) = project.inputs();

    let direct = tilemap_record(plan.schema_validation().snapshot(), 0);
    let inherited = tilemap_record(plan.schema_validation().snapshot(), SIDE * 2);
    assert_ne!(direct.value(), inherited.value());
    let prefab_key = SourceDocumentKey::project_location(
        "39f9d09d-63e3-5fc4-8e3f-71e84893cb8d".parse().unwrap(),
        AssetSourcePath::new("prefabs/terrain.prefab.json").unwrap(),
        SourceDocumentKind::Prefab,
    );
    let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("prefab-map").unwrap(),
    )
    .with_component(ComponentTypeId::new(TILEMAP), inherited.clone())]);
    let mut bound = SceneEntityRecord::new(SceneEntityId::new("bound-map").unwrap());
    bound.prefab = Some(PrefabInstance::root(
        AssetRef::path("terrain.prefab.json").unwrap(),
        prefab_key.clone(),
        PrefabEntityTarget::source(SceneEntityId::new("prefab-map").unwrap()),
    ));
    let scene = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(SceneEntityId::new("direct-map").unwrap())
            .with_component(ComponentTypeId::new(TILEMAP), direct.clone()),
        bound,
    ]);
    let tileset = TileSetSource::new(
        AssetRef::path("unused.png").unwrap(),
        TileAtlasLayout::grid(Vec2::splat(1.0), 1, 1),
    );
    for (path, bytes) in [
        ("scenes/startup.scene.json", scene.to_json_string().unwrap()),
        (
            "prefabs/terrain.prefab.json",
            prefab.to_json_string().unwrap(),
        ),
        (
            "assets/terrain.tileset.ron",
            tileset.encode_ron_bytes().unwrap(),
        ),
        (
            "assets/terrain.tileset.ron.meta",
            AssetMeta::new(
                AssetMetaIdentity::new(
                    TILESET_ID.parse().unwrap(),
                    AssetPath::new("terrain.tileset.ron").unwrap(),
                    AssetSourceKind::Other("tileset".into()),
                ),
                (),
            )
            .to_json_string()
            .unwrap(),
        ),
        (
            "assets/unused.png.meta",
            AssetMeta::new(
                AssetMetaIdentity::new(
                    IMAGE_ID.parse().unwrap(),
                    AssetPath::new("unused.png").unwrap(),
                    AssetSourceKind::Image,
                ),
                nara_image::ImageImportSettings::default(),
            )
            .to_json_string()
            .unwrap(),
        ),
    ] {
        fs::write(project.root.join(path), bytes).unwrap();
    }
    fs::write(
        project.root.join("assets/unused.png"),
        include_bytes!("../../../tests/fixtures/images/valid-rgba-1x1.png"),
    )
    .unwrap();

    let budget = loader.budget_host();
    assert_eq!(budget.limits(), ProjectContentLimits::default());
    let before = budget.snapshot();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    assert_eq!(snapshot.scene(), &scene);
    assert_eq!(snapshot.prefabs().len(), 1);
    assert_eq!(snapshot.prefabs()[0].document(), &prefab);
    assert_eq!(snapshot.prefabs()[0].source_key(), &prefab_key);
    assert!(snapshot.scene_dependencies().is_empty());
    assert_eq!(snapshot.images().len(), 1);
    assert_eq!(snapshot.images()[0].path().as_str(), "assets/unused.png");
    assert_eq!(
        snapshot.images()[0].source().stable_id(),
        StableAssetId::parse_str(IMAGE_ID).unwrap()
    );
    assert_eq!(snapshot.tilesets().len(), 1);
    assert_eq!(snapshot.tilesets()[0].source(), &tileset);
    assert_eq!(
        snapshot.tilesets()[0].stable_id(),
        StableAssetId::parse_str(TILESET_ID).unwrap()
    );
    assert_eq!(snapshot.tilesets()[0].image_path().as_str(), "unused.png");
    let atlas = snapshot.tilesets()[0].source().atlas;
    assert_tilemaps(snapshot.compiled_scene(), &direct, &inherited, atlas);
    let component_id = ComponentTypeId::new(TILEMAP);
    let direct_source = snapshot
        .scene()
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "direct-map")
        .unwrap()
        .components[&component_id]
        .value();
    let inherited_source =
        snapshot.prefabs()[0].document().entities[0].components[&component_id].value();
    for (name, source) in [
        ("direct-map", direct_source),
        ("bound-map", inherited_source),
    ] {
        let compiled = snapshot
            .compiled_scene()
            .entities()
            .iter()
            .find(|entity| entity.id().as_str() == name)
            .unwrap()
            .components()[&component_id]
            .value();
        assert!(
            std::ptr::eq(source, compiled.as_value().unwrap()),
            "{name} must share its original admitted payload"
        );
    }
    let independent_clone = snapshot.scene().clone();
    let mut public_clone = snapshot.scene().clone();
    let cloned = public_clone
        .entities
        .iter_mut()
        .find(|entity| entity.id.as_str() == "direct-map")
        .unwrap()
        .components
        .get_mut(&component_id)
        .unwrap();
    assert!(!std::ptr::eq(cloned.value(), direct_source));
    assert_eq!(cloned.value(), direct_source);
    cloned.set_value(ComponentValue::Null);
    assert_tilemaps(snapshot.compiled_scene(), &direct, &inherited, atlas);
    {
        let closure = snapshot.content_closure();
        assert_eq!(closure.members().len(), 7);
        for (kind, path) in [
            (ProjectContentMemberKind::ProjectManifest, "nara.toml"),
            (ProjectContentMemberKind::Scene, "scenes/startup.scene.json"),
            (
                ProjectContentMemberKind::Prefab,
                "prefabs/terrain.prefab.json",
            ),
            (
                ProjectContentMemberKind::AssetSource,
                "assets/terrain.tileset.ron",
            ),
            (
                ProjectContentMemberKind::AssetMetadata,
                "assets/terrain.tileset.ron.meta",
            ),
            (ProjectContentMemberKind::AssetSource, "assets/unused.png"),
            (
                ProjectContentMemberKind::AssetMetadata,
                "assets/unused.png.meta",
            ),
        ] {
            let member = closure
                .members()
                .iter()
                .find(|member| member.path().as_str() == path)
                .unwrap();
            assert_eq!(member.kind(), kind);
            assert_eq!(member.digest(), project.digest(path));
        }
    }

    let original_snapshot = Arc::downgrade(&snapshot.inner);
    let original_sources = Arc::downgrade(&snapshot.inner.sources);
    let raw_scene = Arc::downgrade(&snapshot.inner.sources.scenes.selected.document);
    let compiled = Arc::downgrade(&snapshot.inner.sources.scenes.selected.compiled);
    let raw_prefab = Arc::downgrade(&snapshot.inner.prefabs[0].document);
    let image = snapshot.share_image_for_consumer(0).unwrap();
    let original_pixels = image.pixels().as_ptr();
    let expected_pixels = image.pixels().to_vec();
    assert_eq!(
        original_pixels,
        snapshot.images()[0].image().pixels().as_ptr()
    );
    let first_source = snapshot.scene_source();
    let last_source = snapshot.scene_source();
    let last_snapshot = snapshot.clone();
    let loaded = budget.snapshot();
    assert_eq!(loaded.active_generations(), before.active_generations() + 1);
    assert!(loaded.active(ProjectContentBudgetKind::ArtifactBytes) > 0);
    drop((snapshot, loader));
    assert_active_budget(budget.snapshot(), loaded);
    assert_eq!(raw_prefab.upgrade().unwrap().as_ref(), &prefab);

    drop(last_snapshot);
    assert!(original_snapshot.upgrade().is_none());
    assert!(
        raw_prefab.upgrade().is_none(),
        "only snapshots retain raw Prefab documents"
    );
    assert!(original_sources.upgrade().is_some());
    assert_eq!(raw_scene.upgrade().unwrap().as_ref(), &scene);
    {
        let original = compiled.upgrade().unwrap();
        assert!(std::ptr::eq(first_source.document(), original.document()));
        assert!(std::ptr::eq(last_source.document(), original.document()));
        assert_tilemaps(original.document(), &direct, &inherited, atlas);
    }
    let source_only = budget.snapshot();
    assert_eq!(
        source_only.active_reservations(),
        before.active_reservations() + 1
    );
    assert_eq!(
        source_only.active_generations(),
        loaded.active_generations()
    );
    assert_eq!(
        source_only.active(ProjectContentBudgetKind::ArtifactBytes),
        loaded.active(ProjectContentBudgetKind::ArtifactBytes)
    );
    assert!(source_only.active(ProjectContentBudgetKind::RetainedBytes) > 0);
    assert_eq!(image.pixels().as_ptr(), original_pixels);
    assert_eq!(image.pixels(), expected_pixels);
    drop(image);
    assert_active_budget(budget.snapshot(), source_only);
    drop(first_source);
    assert_active_budget(budget.snapshot(), source_only);
    assert_eq!(raw_scene.upgrade().unwrap().as_ref(), &scene);
    assert_tilemaps(last_source.document(), &direct, &inherited, atlas);

    drop(last_source);
    assert!(original_sources.upgrade().is_none());
    assert!(raw_scene.upgrade().is_none());
    assert!(compiled.upgrade().is_none());
    assert!(raw_prefab.upgrade().is_none());
    assert_active_budget(budget.snapshot(), before);
    assert_eq!(
        public_clone
            .entities
            .iter()
            .find(|entity| entity.id.as_str() == "direct-map")
            .unwrap()
            .components[&component_id]
            .value(),
        &ComponentValue::Null
    );
    assert_eq!(
        independent_clone
            .entities
            .iter()
            .find(|entity| entity.id.as_str() == "direct-map")
            .unwrap()
            .components[&component_id]
            .value(),
        direct.value()
    );
}
