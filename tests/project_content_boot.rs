#![cfg(all(feature = "serde", feature = "runtime-2d"))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::sync::Arc;

use nara::advanced_prelude::Component;

use nara::asset::AssetMeta;
use nara::diagnostic::DiagnosticValueRef;
use nara::image::{ImageColorSpace, ImageImportSettings};
use nara::project_host::{
    ProjectContentBudgetKind, ProjectContentLoader, ProjectContentMemberKind,
};
use nara::reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue};
use nara::scene::{PrefabDocument, SceneComponentRecord, SceneDocument, SceneEntityRecord};
use project_content_fixture::{TestProject, player_image_meta, scene_id, tileset_meta};

struct EscapeOwner;

#[derive(Default)]
struct DeclaredContentPlugin;

#[derive(Component)]
struct DeclaredContentValue(ComponentValue);

impl nara::app::Plugin for DeclaredContentPlugin {
    fn declaration() -> &'static nara::app::PluginDeclaration {
        static DECLARATION: nara::app::PluginDeclaration = nara::app::PluginDeclaration::new(
            nara::app::PluginId::new("test.declared-content"),
            nara::app::PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[nara::app::PluginSchemaProviderId::new(
            "test.declared-content.schema",
        )]);
        &DECLARATION
    }

    fn build(&self, _app: &mut nara::app::App) -> Result<(), nara::app::PluginError> {
        Ok(())
    }
}

fn declared_content_schema() -> nara::reflect::ComponentSchema {
    use nara::reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentSchema, ComponentValueKind, ComponentValueShape,
    };
    ComponentSchema::new(
        ComponentTypeId::new("test.DeclaredContent"),
        "Declared Content",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("nested"),
            "Nested",
            ComponentFieldPath::from_fields(["nested"]),
            ComponentValueShape::list(ComponentValueShape::record([(
                "asset".to_owned(),
                ComponentValueKind::AssetRef.into(),
            )])),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::AssetRef),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("default_asset"),
            "Default",
            ComponentFieldPath::from_fields(["default_asset"]),
            ComponentValueKind::AssetRef,
            content_asset_reference("tilesets/terrain.tileset.ron"),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::AssetRef),
    ])
}

fn declared_content_provider() -> nara::reflect::ComponentSchemaProviderDefinition {
    nara::reflect::ComponentSchemaProviderDefinition::new(
        nara::reflect::ComponentSchemaOwnerId::new("test.declared-content"),
        nara::app::PluginSchemaProviderId::new("test.declared-content.schema"),
        nara::reflect::ComponentSchemaProviderBindingId::new("test.declared-content.native", 1),
        || {
            Ok(nara::reflect::ComponentSchemaCatalog {
                components: vec![declared_content_schema()],
                ..Default::default()
            })
        },
        |registry| {
            registry
                .register_persistent_component_with_codec::<DeclaredContentValue, _, _>(
                    declared_content_schema(),
                    |value| Ok(DeclaredContentValue(value.clone())),
                    |value| Ok(value.0.clone()),
                )
                .map(|_| ())
        },
    )
}

fn content_asset_reference(path: &str) -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String("path".to_owned())),
        ("value", ComponentValue::String(path.to_owned())),
    ])
}

fn persisted_tilemap(tileset: Option<&str>, tile: Option<u32>) -> SceneEntityRecord {
    SceneEntityRecord::new(scene_id("terrain"))
        .with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            project_content_fixture::identity_transform_record(),
        )
        .with_component(
            ComponentTypeId::new("nara.tilemap.Tilemap"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    (
                        "tile_size",
                        ComponentValue::map([
                            ("x", ComponentValue::f64(1.0).unwrap()),
                            ("y", ComponentValue::f64(1.0).unwrap()),
                        ]),
                    ),
                    (
                        "tileset",
                        tileset
                            .map(content_asset_reference)
                            .unwrap_or(ComponentValue::Null),
                    ),
                    (
                        "cells",
                        ComponentValue::List(
                            tile.map(|tile| {
                                vec![ComponentValue::map([
                                    (
                                        "coord",
                                        ComponentValue::map([
                                            ("x", ComponentValue::I64(-3)),
                                            ("y", ComponentValue::I64(7)),
                                        ]),
                                    ),
                                    (
                                        "cell",
                                        ComponentValue::map([
                                            ("tile", ComponentValue::U64(u64::from(tile))),
                                            (
                                                "color",
                                                ComponentValue::map([
                                                    ("r", ComponentValue::f64(1.0).unwrap()),
                                                    ("g", ComponentValue::f64(1.0).unwrap()),
                                                    ("b", ComponentValue::f64(1.0).unwrap()),
                                                    ("a", ComponentValue::f64(1.0).unwrap()),
                                                ]),
                                            ),
                                        ]),
                                    ),
                                ])]
                            })
                            .unwrap_or_default(),
                        ),
                    ),
                ]),
            ),
        )
}

#[test]
fn strict_content_rejects_cells_outside_the_admitted_tileset() {
    let project = TestProject::with_prefab_startup();
    let path = "tilesets/地形 atlas.tileset.ron";
    let source = nara::tilemap::TileSetSource::new(
        nara::asset::AssetRef::path("textures/player.png").unwrap(),
        nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::ONE, 1, 1),
    );
    project.write_tileset_source(
        path,
        &source,
        &tileset_meta("d3d3f9a6-f7a9-4db0-ae3c-e1d4264d6f12", path),
    );
    project.write_scene_source(&SceneDocument::from_ordered_roots([persisted_tilemap(
        Some(path),
        Some(1),
    )]));
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    let diagnostic = error
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "project.content.tile-index-out-of-range")
        .unwrap();
    for (key, expected) in [
        ("entity-id", DiagnosticValueRef::Identifier("terrain")),
        (
            "source-entity-id",
            DiagnosticValueRef::Identifier("terrain"),
        ),
        ("tileset", DiagnosticValueRef::ProjectRelative(path)),
        ("x", DiagnosticValueRef::Signed(-3)),
        ("y", DiagnosticValueRef::Signed(7)),
        ("tile", DiagnosticValueRef::Unsigned(1)),
    ] {
        assert_eq!(
            diagnostic
                .fields()
                .iter()
                .find(|field| field.key().as_str() == key)
                .map(|field| field.value()),
            Some(expected),
            "missing or incorrect membership diagnostic field {key}",
        );
    }
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);

    for (tileset, tile) in [
        (Some(path), Some(0)),
        (Some(path), None),
        (None, Some(u32::MAX)),
    ] {
        project.write_scene_source(&SceneDocument::from_ordered_roots([persisted_tilemap(
            tileset, tile,
        )]));
        let snapshot = loader.load(&candidate, &plan).unwrap();
        assert_eq!(snapshot.expanded_scene().entities.len(), 1);
    }
}

#[test]
fn declared_nested_and_default_tilesets_are_loaded_by_metadata_kind() {
    let project = TestProject::with_prefab_startup();
    let source = nara::tilemap::TileSetSource::new(
        nara::asset::AssetRef::path("textures/player.png").unwrap(),
        nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::ONE, 1, 1),
    );
    let path = "tilesets/terrain.tileset.ron";
    project.write_tileset_source(
        path,
        &source,
        &tileset_meta("d3d3f9a6-f7a9-4db0-ae3c-e1d4264d6f12", path),
    );
    std::fs::write(
        project.path().join("assets/unrelated.meta"),
        b"unrelated invalid sidecar",
    )
    .unwrap();
    let entity = SceneEntityRecord::new(scene_id("content")).with_component(
        ComponentTypeId::new("test.DeclaredContent"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([(
                "nested",
                ComponentValue::List(vec![ComponentValue::map([(
                    "asset",
                    content_asset_reference(path),
                )])]),
            )]),
        ),
    );
    project.write_scene_source(&SceneDocument::from_ordered_roots([entity]));
    let (candidate, _, root) = project.candidate_plan_and_root();
    let recipe = nara::ProductRecipe::new()
        .add_contribution(
            nara::SchemaContribution::<DeclaredContentPlugin>::for_default([
                declared_content_provider(),
            ])
            .unwrap(),
        )
        .unwrap();
    let plan = nara::project_host::resolve_product_recipe(&candidate, recipe).unwrap();
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    assert_eq!(snapshot.tilesets().len(), 1);
    assert_eq!(snapshot.images().len(), 1);
    assert_eq!(snapshot.tilesets()[0].path().as_str(), path);
}

#[test]
fn path_tileset_discovery_completes_the_catalogue_for_a_later_stable_image_dependency() {
    let project = TestProject::with_prefab_startup();
    let path = "tilesets/terrain.tileset.ron";
    let source = nara::tilemap::TileSetSource::new(
        nara::asset::AssetRef::StableId(player_image_meta().identity().stable_id()),
        nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::ONE, 1, 1),
    );
    project.write_tileset_source(
        path,
        &source,
        &tileset_meta("d3d3f9a6-f7a9-4db0-ae3c-e1d4264d6f12", path),
    );
    project.write_scene_source(&SceneDocument::from_ordered_roots([persisted_tilemap(
        Some(path),
        Some(0),
    )]));
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();

    assert_eq!(snapshot.tilesets().len(), 1);
    assert_eq!(
        snapshot.tilesets()[0].image_path().as_str(),
        "textures/player.png"
    );
    assert_eq!(snapshot.images().len(), 1);
    assert_eq!(
        snapshot.images()[0].image().source().stable_id(),
        player_image_meta().identity().stable_id()
    );
    assert!(
        loader
            .budget_snapshot()
            .high_water(ProjectContentBudgetKind::DirectoryEntries)
            > 0
    );
}

#[test]
fn strict_content_rejects_an_image_in_a_tilemap_tileset_field() {
    let project = TestProject::with_prefab_startup();
    project.write_scene_source(&SceneDocument::from_ordered_roots([persisted_tilemap(
        Some("textures/player.png"),
        None,
    )]));
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let error = ProjectContentLoader::new(root)
        .unwrap()
        .load(&candidate, &plan)
        .unwrap_err();

    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.invalid-component-payload" }),
        "{error:?}"
    );
}

#[test]
fn strict_content_rejects_a_tileset_in_a_sprite_image_field() {
    let project = TestProject::with_prefab_startup();
    let path = "tilesets/terrain.tileset.ron";
    project.write_tileset_source(
        path,
        &nara::tilemap::TileSetSource::new(
            nara::asset::AssetRef::path("textures/player.png").unwrap(),
            nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::ONE, 1, 1),
        ),
        &tileset_meta("d3d3f9a6-f7a9-4db0-ae3c-e1d4264d6f12", path),
    );
    let entity = SceneEntityRecord::new(scene_id("sprite"))
        .with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            project_content_fixture::identity_transform_record(),
        )
        .with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                project_content_fixture::sprite_value(&nara::asset::AssetRef::path(path).unwrap()),
            ),
        );
    project.write_scene_source(&SceneDocument::from_ordered_roots([entity]));
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let error = ProjectContentLoader::new(root)
        .unwrap()
        .load(&candidate, &plan)
        .unwrap_err();
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.invalid-component-payload" }),
        "{error:?}"
    );
}

#[test]
fn strict_project_content_load_uses_the_persisted_linear_recipe() {
    let project = TestProject::with_prefab_startup();
    let meta = player_image_meta();
    let (identity, _) = meta.into_parts();
    let meta = AssetMeta::new(identity, ImageImportSettings::new(ImageColorSpace::Linear));
    project.write_player_image_meta(&meta);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let snapshot = loader.load(&candidate, &plan).unwrap();
    let image = snapshot.images()[0].image();

    assert_eq!(image.color_space(), ImageColorSpace::Linear);
    assert_eq!(image.source().settings(), *meta.settings());
    assert_eq!(
        image.source().artifact().key().settings_hash(),
        meta.settings().settings_hash()
    );
}

#[test]
fn file_backed_tileset_enters_the_startup_closure_with_its_image_dependency() {
    let project = TestProject::with_prefab_startup();
    let tileset_id = "d3d3f9a6-f7a9-4db0-ae3c-e1d4264d6f12";
    let tilemap = SceneEntityRecord::new(scene_id("terrain"))
        .with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    (
                        "translation",
                        ComponentValue::map([
                            ("x", ComponentValue::f64(0.0).unwrap()),
                            ("y", ComponentValue::f64(0.0).unwrap()),
                        ]),
                    ),
                    ("rotation", ComponentValue::f64(0.0).unwrap()),
                    (
                        "scale",
                        ComponentValue::map([
                            ("x", ComponentValue::f64(1.0).unwrap()),
                            ("y", ComponentValue::f64(1.0).unwrap()),
                        ]),
                    ),
                ]),
            ),
        )
        .with_component(
            ComponentTypeId::new("nara.tilemap.Tilemap"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    (
                        "tile_size",
                        ComponentValue::map([
                            ("x", ComponentValue::f64(1.0).unwrap()),
                            ("y", ComponentValue::f64(1.0).unwrap()),
                        ]),
                    ),
                    (
                        "tileset",
                        ComponentValue::map([
                            ("kind", ComponentValue::String("stable_id".to_owned())),
                            ("value", ComponentValue::String(tileset_id.to_owned())),
                        ]),
                    ),
                    ("cells", ComponentValue::List(Vec::new())),
                ]),
            ),
        );
    project.write_scene_source(&SceneDocument::from_ordered_roots([tilemap]));
    let source = nara::tilemap::TileSetSource::new(
        nara::asset::AssetRef::StableId(player_image_meta().identity().stable_id()),
        nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::new(0.25, 0.25), 2, 2)
            .with_margin(nara::core::Vec2::new(0.125, 0.125))
            .with_spacing(nara::core::Vec2::new(0.25, 0.25)),
    );
    project.write_tileset_source(
        "tilesets/terrain.tileset.ron",
        &source,
        &tileset_meta(tileset_id, "tilesets/terrain.tileset.ron"),
    );

    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();

    assert_eq!(snapshot.tilesets().len(), 1);
    assert_eq!(
        snapshot.tilesets()[0].path().as_str(),
        "tilesets/terrain.tileset.ron"
    );
    assert_eq!(
        snapshot.tilesets()[0].image_path().as_str(),
        "textures/player.png"
    );
    assert_eq!(snapshot.images().len(), 1);
    assert!(
        snapshot
            .startup_content_closure()
            .expect("the loader selected the manifest's startup Scene")
            .members()
            .iter()
            .any(|member| { member.path().as_str() == "assets/tilesets/terrain.tileset.ron" })
    );
    assert!(
        snapshot
            .startup_content_closure()
            .expect("the loader selected the manifest's startup Scene")
            .members()
            .iter()
            .any(|member| { member.path().as_str() == "assets/tilesets/terrain.tileset.ron.meta" })
    );
    assert!(
        snapshot
            .startup_content_closure()
            .expect("the loader selected the manifest's startup Scene")
            .members()
            .iter()
            .any(|member| { member.path().as_str() == "assets/textures/player.png" })
    );
    assert!(
        snapshot
            .startup_content_closure()
            .expect("the loader selected the manifest's startup Scene")
            .members()
            .iter()
            .any(|member| { member.path().as_str() == "assets/textures/player.png.meta" })
    );

    let changed_source = source
        .clone()
        .with_tint(nara::core::Color::rgb(0.5, 1.0, 1.0));
    project.write_tileset_source(
        "tilesets/terrain.tileset.ron",
        &changed_source,
        &tileset_meta(tileset_id, "tilesets/terrain.tileset.ron"),
    );
    let successor = loader.load(&candidate, &plan).unwrap();
    assert_ne!(successor.revision(), snapshot.revision());
    assert_ne!(
        successor.tilesets()[0].source_hash(),
        snapshot.tilesets()[0].source_hash()
    );
    assert_eq!(snapshot.tilesets()[0].source(), &source);
    assert_eq!(successor.tilesets()[0].source(), &changed_source);

    for atlas in [
        nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::ONE, 1, 1)
            .with_margin(nara::core::Vec2::new(1.0e-20, 0.0)),
        nara::tilemap::TileAtlasLayout::grid(
            nara::core::Vec2::new(2.0_f32.powi(-24), 1.0),
            16_777_217,
            1,
        ),
        nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::ONE, 1, 1)
            .with_margin(nara::core::Vec2::new(1.0e-8, 0.0)),
        nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::new(0.5, 1.0), 2, 1)
            .with_spacing(nara::core::Vec2::new(1.0e-8, 0.0)),
        nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::new(0.5, 1.0), 1, 1),
        nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::new(2.0, 1.0), 1, 1),
        nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::new(1.5, 1.0), 1, 1),
        nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::ONE, 1, 1)
            .with_margin(nara::core::Vec2::new(0.5, 0.0)),
        nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::new(0.5, 1.0), 2, 1)
            .with_spacing(nara::core::Vec2::new(0.5, 0.0)),
    ] {
        let mismatched_source = nara::tilemap::TileSetSource::new(
            nara::asset::AssetRef::path("textures/player.png").unwrap(),
            atlas,
        );
        project.write_tileset_source(
            "tilesets/terrain.tileset.ron",
            &mismatched_source,
            &tileset_meta(tileset_id, "tilesets/terrain.tileset.ron"),
        );
        let before_rejection = loader.budget_snapshot();
        let error = loader.load(&candidate, &plan).unwrap_err();
        assert!(
            error.diagnostics().iter().any(|diagnostic| {
                diagnostic.code().as_str() == "project.content.tileset-image-extent-mismatch"
            }),
            "{error:?}"
        );
        let after_rejection = loader.budget_snapshot();
        for kind in ProjectContentBudgetKind::ALL {
            assert_eq!(
                after_rejection.active(*kind),
                before_rejection.active(*kind)
            );
        }
        assert_eq!(
            after_rejection.active_reservations(),
            before_rejection.active_reservations()
        );
        assert_eq!(snapshot.tilesets()[0].source(), &source);
        assert_eq!(successor.tilesets()[0].source(), &changed_source);
    }

    let before_rejection = loader.budget_snapshot();
    project.write_tileset_source(
        "tilesets/terrain.tileset.ron",
        &nara::tilemap::TileSetSource::new(
            nara::asset::AssetRef::path("textures/missing.png").unwrap(),
            source.atlas,
        ),
        &tileset_meta(tileset_id, "tilesets/terrain.tileset.ron"),
    );
    assert!(loader.load(&candidate, &plan).is_err());
    let after_rejection = loader.budget_snapshot();
    for kind in ProjectContentBudgetKind::ALL {
        assert_eq!(
            after_rejection.active(*kind),
            before_rejection.active(*kind)
        );
    }
    assert_eq!(
        after_rejection.active_reservations(),
        before_rejection.active_reservations()
    );
    assert_eq!(snapshot.tilesets()[0].source(), &source);
    assert_eq!(successor.tilesets()[0].source(), &changed_source);
}

#[test]
fn file_backed_tilesets_sharing_an_image_validate_each_atlas_before_snapshot_publication() {
    let project = TestProject::with_prefab_startup();
    let tilemaps = ["tilesets/ground.tileset.ron", "tilesets/water.tileset.ron"];
    project.write_scene_source(&SceneDocument::from_ordered_roots(tilemaps.map(|path| {
        SceneEntityRecord::new(scene_id(path))
            .with_component(
                ComponentTypeId::new("nara.transform.Transform2d"),
                project_content_fixture::identity_transform_record(),
            )
            .with_component(
                ComponentTypeId::new("nara.tilemap.Tilemap"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([
                        (
                            "tile_size",
                            ComponentValue::map([
                                ("x", ComponentValue::f64(1.0).unwrap()),
                                ("y", ComponentValue::f64(1.0).unwrap()),
                            ]),
                        ),
                        (
                            "tileset",
                            ComponentValue::map([
                                ("kind", ComponentValue::String("path".to_owned())),
                                ("value", ComponentValue::String(path.to_owned())),
                            ]),
                        ),
                        ("cells", ComponentValue::List(Vec::new())),
                    ]),
                ),
            )
    })));
    let ground = nara::tilemap::TileSetSource::new(
        nara::asset::AssetRef::path("textures/player.png").unwrap(),
        nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::ONE, 1, 1),
    );
    let water = nara::tilemap::TileSetSource::new(
        nara::asset::AssetRef::StableId(player_image_meta().identity().stable_id()),
        nara::tilemap::TileAtlasLayout::grid(nara::core::Vec2::new(0.5, 1.0), 2, 1),
    );
    let ground_meta = tileset_meta("d3d3f9a6-f7a9-4db0-ae3c-e1d4264d6f12", tilemaps[0]);
    let water_meta = tileset_meta("7bbab959-d42c-43f0-9406-472929854a3a", tilemaps[1]);
    project.write_tileset_source(tilemaps[0], &ground, &ground_meta);
    project.write_tileset_source(tilemaps[1], &water, &water_meta);
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    assert_eq!(snapshot.tilesets().len(), 2);
    assert_eq!(snapshot.images().len(), 1);

    let invalid_water = nara::tilemap::TileSetSource::new(
        water.image.clone(),
        water.atlas.with_spacing(nara::core::Vec2::new(0.5, 0.0)),
    );
    project.write_tileset_source(tilemaps[1], &invalid_water, &water_meta);
    let before_rejection = loader.budget_snapshot();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.content.tileset-image-extent-mismatch"
    }));
    let after_rejection = loader.budget_snapshot();
    for kind in ProjectContentBudgetKind::ALL {
        assert_eq!(
            after_rejection.active(*kind),
            before_rejection.active(*kind)
        );
    }
    assert_eq!(
        after_rejection.active_reservations(),
        before_rejection.active_reservations()
    );
    assert_eq!(snapshot.tilesets()[0].source(), &ground);
    assert_eq!(snapshot.tilesets()[1].source(), &water);
}

#[test]
fn authorized_startup_closure_publishes_an_immutable_snapshot() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let snapshot = loader.load(&candidate, &plan).unwrap();
    let cloned = snapshot.clone();
    let closure = snapshot
        .startup_content_closure()
        .expect("the loader selected the manifest's startup Scene");

    assert_eq!(snapshot.lineage(), candidate.lineage());
    assert_eq!(
        snapshot.schema_fingerprint(),
        plan.schema_validation().composition_fingerprint()
    );
    assert_eq!(closure.settings_lineage(), snapshot.lineage());
    assert_eq!(closure.manifest_digest(), candidate.manifest_digest());
    assert_eq!(closure.schema_fingerprint(), snapshot.schema_fingerprint());
    assert_eq!(closure.content_revision(), snapshot.revision());
    assert_eq!(closure.content_digest(), snapshot.content_digest());
    let members = closure
        .members()
        .iter()
        .map(|member| {
            (
                member.path().as_str(),
                member.kind(),
                member.byte_len(),
                member.digest(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        members
            .iter()
            .map(|(path, kind, _, _)| (*path, *kind))
            .collect::<Vec<_>>(),
        vec![
            (
                "assets/textures/player.png",
                ProjectContentMemberKind::AssetSource,
            ),
            (
                "assets/textures/player.png.meta",
                ProjectContentMemberKind::AssetMetadata,
            ),
            ("nara.toml", ProjectContentMemberKind::ProjectManifest),
            (
                "prefabs/enemy.prefab.json",
                ProjectContentMemberKind::Prefab,
            ),
            ("scenes/startup.scene.json", ProjectContentMemberKind::Scene,),
        ]
    );
    assert!(
        members
            .iter()
            .all(|(_, _, byte_len, digest)| { *byte_len > 0 && *byte_len == digest.length() })
    );
    assert_eq!(snapshot.prefabs().len(), 1);
    assert_eq!(snapshot.images().len(), 1);
    assert_eq!(snapshot.images()[0].path().as_str(), "textures/player.png");
    assert_eq!(snapshot.images()[0].image().extent().width, 1);
    assert_eq!(snapshot.images()[0].image().extent().height, 1);
    assert_eq!(snapshot.images()[0].image().pixels(), &[24, 120, 220, 255]);
    assert!(snapshot.expanded_scene_provenance().values().any(|source| {
        source.source_entity_id() == &scene_id("enemy")
            && source
                .projection_path()
                .is_some_and(|path| path.root_anchor() == &scene_id("enemy-anchor"))
    }));
    assert!(std::ptr::eq(
        snapshot.images()[0].image().pixels(),
        cloned.images()[0].image().pixels(),
    ));
    let mut escaped = snapshot.images()[0].image().share_retained().unwrap();
    assert!(!escaped.try_attach_retention_owner(Arc::new(EscapeOwner)));

    let leased = loader.budget_snapshot();
    assert!(leased.active(ProjectContentBudgetKind::ArtifactBytes) > 0);
    assert!(leased.active(ProjectContentBudgetKind::RetainedBytes) > 0);
    assert_eq!(leased.active_reservations(), 2);

    drop(snapshot);
    assert_eq!(loader.budget_snapshot(), leased);
    drop(cloned);
    let image_only = loader.budget_snapshot();
    assert_eq!(
        image_only.active_reservations(),
        leased.active_reservations()
    );
    for kind in ProjectContentBudgetKind::ALL {
        assert_eq!(image_only.active(*kind), leased.active(*kind));
    }
    assert_eq!(image_only.active_generations(), 1);
    drop(escaped);
    let closure_only = loader.budget_snapshot();
    assert_eq!(closure_only.active_generations(), 0);
    assert_eq!(
        closure_only.active(ProjectContentBudgetKind::ArtifactBytes),
        0
    );
    assert!(closure_only.active(ProjectContentBudgetKind::RetainedBytes) > 0);
    assert_eq!(closure_only.active_reservations(), 1);
    drop(closure);
    let released = loader.budget_snapshot();
    assert_eq!(released.active(ProjectContentBudgetKind::ArtifactBytes), 0);
    assert_eq!(released.active(ProjectContentBudgetKind::RetainedBytes), 0);
    assert_eq!(released.active_reservations(), 0);
}

#[test]
fn source_changes_publish_a_distinct_revision_without_mutating_old_consumers() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let first = loader.load(&candidate, &plan).unwrap();
    let first_revision = first.revision();
    let first_digest = first.content_digest();
    let first_prefab = first.prefabs()[0].document().clone();
    let first_pixels = first.images()[0].image().pixels().to_vec();
    let first_pixel_ptr = first.images()[0].image().pixels().as_ptr();
    let first_charge = loader.budget_snapshot();
    assert_eq!(first_charge.active_reservations(), 2);

    let mut changed_entities = first_prefab.entities.clone();
    changed_entities.push(SceneEntityRecord::new(scene_id("variant")));
    project.write_prefab_source(&PrefabDocument::from_ordered_roots(changed_entities));

    let second = loader.load(&candidate, &plan).unwrap();
    assert_ne!(second.revision(), first_revision);
    assert_ne!(second.content_digest(), first_digest);
    assert_eq!(second.schema_fingerprint(), first.schema_fingerprint());
    assert_ne!(
        second.images()[0].image().pixels().as_ptr(),
        first_pixel_ptr,
        "a new source revision must own a distinct residency charge",
    );
    assert_eq!(first.prefabs()[0].document(), &first_prefab);
    assert_eq!(first.images()[0].image().pixels(), first_pixels);
    assert_eq!(first.revision(), first_revision);
    assert_eq!(first.content_digest(), first_digest);

    let overlapped = loader.budget_snapshot();
    assert_eq!(overlapped.active_reservations(), 4);
    assert_eq!(
        overlapped.active(ProjectContentBudgetKind::ArtifactBytes),
        first_charge
            .active(ProjectContentBudgetKind::ArtifactBytes)
            .checked_mul(2)
            .unwrap(),
    );

    drop(first);
    assert_eq!(loader.budget_snapshot().active_reservations(), 2);
    drop(second);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[cfg(feature = "runtime-ui")]
#[test]
fn ui_visual_asset_reference_alone_enters_the_startup_content_closure() {
    use std::fs;

    use nara::{
        ecs::World,
        reflect::{ComponentRegistry, ComponentTypeId},
        ui::{UiColor, UiImageId, UiLayer, UiNode, UiRoot, UiStyle, UiVisual},
    };

    let project = TestProject::with_prefab_startup();
    fs::write(
        project.path().join("nara.toml"),
        r#"schema_version = 1

[project]
name = "UI Content Test"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[capabilities]
requested = ["runtime-2d", "runtime-ui"]
"#,
    )
    .unwrap();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let registry = plan.schema_validation().registry();
    let mut world = World::new();
    let root_source = world
        .spawn((UiRoot::new(UiLayer::Base), UiNode::new(UiStyle::fill())))
        .id();
    let image_source = world
        .spawn((
            UiNode::new(UiStyle::absolute(24.0, 16.0, 128.0, 128.0)),
            UiVisual::image(
                UiImageId::new("textures/player.png").unwrap(),
                UiColor::rgba(1.0, 1.0, 1.0, 1.0),
            ),
        ))
        .id();
    let root_id = scene_id("ui-root");
    let image_id = scene_id("ui-image");
    let root_record = SceneEntityRecord::new(root_id.clone())
        .with_children([image_id.clone()])
        .with_component(
            ComponentTypeId::new("nara.ui.UiRoot"),
            encoded_component(registry, &world, root_source, "nara.ui.UiRoot"),
        )
        .with_component(
            ComponentTypeId::new("nara.ui.UiNode"),
            encoded_component(registry, &world, root_source, "nara.ui.UiNode"),
        );
    let image_record = SceneEntityRecord::new(image_id)
        .with_component(
            ComponentTypeId::new("nara.ui.UiNode"),
            encoded_component(registry, &world, image_source, "nara.ui.UiNode"),
        )
        .with_component(
            ComponentTypeId::new("nara.ui.UiVisual"),
            encoded_component(registry, &world, image_source, "nara.ui.UiVisual"),
        );
    project.write_prefab_source(&PrefabDocument::new([root_id], [root_record, image_record]));

    let snapshot = ProjectContentLoader::new(root)
        .unwrap()
        .load(&candidate, &plan)
        .unwrap();
    assert_eq!(snapshot.images().len(), 1);
    assert_eq!(snapshot.images()[0].path().as_str(), "textures/player.png");
    let members = snapshot
        .startup_content_closure()
        .expect("the loader selected the manifest's startup Scene");
    assert!(members.members().iter().any(|member| {
        member.kind() == ProjectContentMemberKind::AssetSource
            && member.path().as_str() == "assets/textures/player.png"
    }));

    fn encoded_component(
        registry: &ComponentRegistry,
        world: &World,
        entity: nara::ecs::Entity,
        component: &str,
    ) -> SceneComponentRecord {
        let component = ComponentTypeId::new(component);
        let schema = registry.schema(&component).unwrap();
        let value = registry
            .encode_component(&component, world, entity)
            .expect("the selected runtime plan registers the UI component codec")
            .expect("the first-party UI component must encode")
            .expect("the fixture entity contains the selected UI component");
        SceneComponentRecord::new(schema.version, value)
    }
}
