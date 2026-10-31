//! Real source admission and processed delivery for the stock capture consumer.

use std::{fs, path::Path, sync::Arc};

use nara::{
    ProductRecipe,
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetRef, AssetSourceKind,
        ImportedProductId, RuntimeAssetCodec, StableAssetId,
    },
    core::{FormatKind, FormatVersion},
    fs::{ContentDigest, RelativePath},
    image::{ImageColorSpace, ImageImportSettings, ImageRuntimeCodec},
    project::{ProductCapability, RuntimeConfiguration, RuntimePreset},
    project_host::{
        ProjectContentLoader, ProjectContentSnapshot, RuntimeConfigurationPlan,
        ingest_project_manifest, open_explicit_product_project_root, resolve_product_recipe,
        resolve_runtime_configuration,
    },
    reflect::{ComponentTypeId, ComponentValue, encode_asset_reference_value},
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeCodecDefinitionId, RuntimeContentBase,
        RuntimeContentCatalog, RuntimeContentCatalogDefinition, RuntimeContentCatalogLimits,
        RuntimeContentDependency, RuntimeContentDependencyRole, RuntimeContentExpectation,
        RuntimeContentLimits, RuntimeContentMember, RuntimeContentMemberKind, RuntimeContentTarget,
    },
    scene::{
        ExpandedSceneCandidate, RuntimeSceneCandidate, RuntimeSceneLimits, SceneComponentBindings,
        SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    },
};

pub const SIZE: u32 = 16;
pub const RED: [u8; 4] = [255, 0, 0, 255];
pub const GREEN: [u8; 4] = [0, 255, 0, 255];

const SCENES: [&str; 2] = [
    "f616982d-cfbf-43fd-bd47-671dd7e23101",
    "f616982d-cfbf-43fd-bd47-671dd7e23102",
];
const IMAGES: [&str; 2] = [
    "f616982d-cfbf-43fd-bd47-671dd7e23103",
    "f616982d-cfbf-43fd-bd47-671dd7e23104",
];
const PNG_RED: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 31, 0,
    5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];
const PNG_GREEN: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 96, 248, 207, 240, 31, 0, 4,
    1, 1, 255, 113, 235, 71, 229, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

pub fn image_product(later: bool) -> AssetProductRef {
    primary(IMAGES[usize::from(later)])
}

pub fn scene_product(later: bool) -> AssetProductRef {
    primary(SCENES[usize::from(later)])
}

fn stable(value: &str) -> StableAssetId {
    StableAssetId::parse_str(value).unwrap()
}

fn primary(source: &str) -> AssetProductRef {
    AssetProductRef::new(stable(source), ImportedProductId::PRIMARY).unwrap()
}

pub fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_runtime_image_codec::<ImageRuntimeCodec>(RuntimeCodecDefinitionId::new(
            "consumer.render-composition.image",
            1,
        ))
        .unwrap()
}

pub struct ContentFixture {
    source: tempfile::TempDir,
    processed: tempfile::TempDir,
    expected: RuntimeContentExpectation,
    recipe: ProductRecipe,
}

impl ContentFixture {
    pub fn new() -> Self {
        Self::with_recipe(recipe())
    }

    pub fn with_recipe(recipe: ProductRecipe) -> Self {
        let source = tempfile::tempdir().unwrap();
        let processed = tempfile::tempdir().unwrap();
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir(source.path().join(directory)).unwrap();
        }
        fs::write(source.path().join("nara.toml"), MANIFEST).unwrap();

        let configuration = runtime_configuration();
        let runtime_plan =
            resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone())
                .unwrap();
        fs::write(
            source.path().join("schema/authoring.bundle.json"),
            runtime_plan
                .schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let bindings = runtime_plan.schema_validation().bindings();
        for later in [false, true] {
            let name = if later { "later" } else { "startup" };
            let image_name = if later { "green.png" } else { "red.png" };
            fs::write(
                source.path().join("assets").join(image_name),
                if later { PNG_GREEN } else { PNG_RED },
            )
            .unwrap();
            let image_meta = AssetMeta::new(
                AssetMetaIdentity::new(
                    stable(IMAGES[usize::from(later)]),
                    AssetPath::new(image_name).unwrap(),
                    AssetSourceKind::Image,
                ),
                ImageImportSettings::new(ImageColorSpace::Srgb),
            );
            fs::write(
                source
                    .path()
                    .join("assets")
                    .join(format!("{image_name}.meta")),
                image_meta.to_json_string().unwrap(),
            )
            .unwrap();
            let scene = scene_document(bindings, &AssetRef::path(image_name).unwrap());
            fs::write(
                source
                    .path()
                    .join("scenes")
                    .join(format!("{name}.scene.json")),
                scene.to_json_string().unwrap(),
            )
            .unwrap();
            let metadata = AssetMeta::new(
                AssetMetaIdentity::new(
                    stable(SCENES[usize::from(later)]),
                    AssetPath::new(format!("{name}.scene.json")).unwrap(),
                    AssetSourceKind::Scene,
                ),
                (),
            );
            fs::write(
                source
                    .path()
                    .join("scenes")
                    .join(format!("{name}.scene.json.meta")),
                metadata.to_json_string().unwrap(),
            )
            .unwrap();
        }

        // Import both actual PNGs through the admitted source content owner. Processed members
        // are encoded while these snapshots still account for source and pixel storage.
        let root = open_explicit_product_project_root(source.path()).unwrap();
        let manifest = root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let source_plan = resolve_product_recipe(&candidate, recipe.clone()).unwrap();
        let loader = ProjectContentLoader::new(root).unwrap();
        let first = loader.load(&candidate, &source_plan).unwrap();
        let second = loader
            .load_scene(
                &candidate,
                &source_plan,
                &nara::project::ProjectPath::new("later.scene.json").unwrap(),
            )
            .unwrap();
        let members = [(&first, false), (&second, true)]
            .into_iter()
            .flat_map(|(snapshot, later)| {
                write_processed_members(processed.path(), &runtime_plan, snapshot, later)
            })
            .collect();
        let target =
            RuntimeContentTarget::new("native-stock-image", "content-interleaving").unwrap();
        let catalog = RuntimeContentCatalog::new(
            RuntimeContentCatalogDefinition {
                target: target.clone(),
                configuration,
                plugin_plan_fingerprint: runtime_plan.plugin_plan().fingerprint().as_bytes(),
                schema_fingerprint: runtime_plan
                    .schema_validation()
                    .composition_fingerprint()
                    .as_bytes(),
                runtime_codecs_fingerprint: runtime_plan.runtime_codecs_fingerprint().as_bytes(),
                startup_scene: scene_product(false),
                scenes: vec![scene_product(false), scene_product(true)],
                addressable_roots: vec![image_product(false), image_product(true)],
                members,
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap();
        let bytes = catalog.to_json_bytes().unwrap();
        // This is the producer's independent expected result, before consumer admission.
        let expected = RuntimeContentExpectation::new(ContentDigest::of_bytes(&bytes), target);
        fs::write(processed.path().join(RUNTIME_CONTENT_CATALOG_FILE), bytes).unwrap();
        drop((first, second));
        assert_eq!(loader.budget_snapshot().active_generations(), 0);
        Self {
            source,
            processed,
            expected,
            recipe,
        }
    }

    pub fn source_root(&self) -> &Path {
        self.source.path()
    }

    pub fn open_processed(&self) -> RuntimeContentBase {
        RuntimeContentBase::open(
            &open_explicit_product_project_root(self.processed.path()).unwrap(),
            &self.expected,
            self.recipe.clone(),
            RuntimeContentLimits::default(),
        )
        .unwrap()
    }

    pub fn destroy_backing(&self) {
        // Mutate exact inputs after both modes have admitted them. All paths belong to these
        // TempDir owners; no recursive deletion or external repository fixture is involved.
        for path in [
            "assets/red.png",
            "assets/green.png",
            "scenes/startup.scene.json",
            "scenes/later.scene.json",
        ] {
            fs::write(self.source.path().join(path), b"retired source input").unwrap();
        }
        for path in [
            "startup.runtime.json",
            "later.runtime.json",
            "red.runtime.image",
            "green.runtime.image",
            RUNTIME_CONTENT_CATALOG_FILE,
        ] {
            fs::remove_file(self.processed.path().join(path)).unwrap();
        }
    }
}

fn runtime_configuration() -> RuntimeConfiguration {
    let mut configuration = RuntimeConfiguration::default();
    configuration.runtime.preset = RuntimePreset::LocalHeadless;
    configuration.capabilities.requested = vec![
        ProductCapability::Runtime2d,
        ProductCapability::Presentation2d,
        ProductCapability::DesktopWinit,
        ProductCapability::RenderWgpu,
    ];
    configuration.tasks.io.workers = 1;
    configuration.tasks.io.pending_capacity = 16;
    configuration.tasks.compute.workers = 1;
    configuration.tasks.compute.pending_capacity = 16;
    configuration.tasks.async_compute.workers = 1;
    configuration.tasks.async_compute.pending_capacity = 16;
    configuration.window.enabled = true;
    configuration.window.width = SIZE;
    configuration.window.height = SIZE;
    configuration
}

fn initial(bindings: &SceneComponentBindings, name: &str) -> SceneComponentRecord {
    let schema = bindings.schema(&ComponentTypeId::new(name)).unwrap();
    SceneComponentRecord::new(
        schema.version(),
        schema.authoring_initial_value().unwrap().clone(),
    )
}

fn scene_document(bindings: &SceneComponentBindings, image: &AssetRef) -> SceneDocument {
    let transform = initial(bindings, "nara.transform.Transform2d");
    let mut sprite = initial(bindings, "nara.sprite.Sprite");
    let ComponentValue::Map(fields) = &mut sprite.value else {
        panic!("Sprite map")
    };
    fields.insert(
        "size".into(),
        ComponentValue::map([
            ("x", ComponentValue::f64(f64::from(SIZE)).unwrap()),
            ("y", ComponentValue::f64(f64::from(SIZE)).unwrap()),
        ]),
    );
    let Some(ComponentValue::Map(material)) = fields.get_mut("material") else {
        panic!("Sprite material")
    };
    material.insert("image".into(), encode_asset_reference_value(image));
    let camera = SceneComponentRecord::new(
        bindings
            .schema(&ComponentTypeId::new("nara.render.Camera2d"))
            .unwrap()
            .version(),
        ComponentValue::map([
            (
                "viewport_height",
                ComponentValue::f64(f64::from(SIZE)).unwrap(),
            ),
            (
                "clear_color",
                ComponentValue::map([
                    ("r", ComponentValue::f64(0.0).unwrap()),
                    ("g", ComponentValue::f64(0.0).unwrap()),
                    ("b", ComponentValue::f64(1.0).unwrap()),
                    ("a", ComponentValue::f64(1.0).unwrap()),
                ]),
            ),
        ]),
    );
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(SceneEntityId::new("sprite").unwrap())
            .with_component(
                ComponentTypeId::new("nara.transform.Transform2d"),
                transform.clone(),
            )
            .with_component(ComponentTypeId::new("nara.sprite.Sprite"), sprite),
        SceneEntityRecord::new(SceneEntityId::new("camera").unwrap())
            .with_component(
                ComponentTypeId::new("nara.transform.Transform2d"),
                transform,
            )
            .with_component(ComponentTypeId::new("nara.render.Camera2d"), camera),
    ])
}

fn write_processed_members(
    root: &Path,
    plan: &RuntimeConfigurationPlan,
    snapshot: &ProjectContentSnapshot,
    later: bool,
) -> [RuntimeContentMember; 2] {
    assert_eq!(snapshot.images().len(), 1);
    let image = snapshot.images()[0].image();
    assert_eq!(image.product(), Some(image_product(later)));
    assert!(image.source().is_some());
    assert_eq!(image.pixels(), if later { GREEN } else { RED });
    let image_bytes = ImageRuntimeCodec::encode_runtime(image).unwrap();
    let image_file = if later {
        "green.runtime.image"
    } else {
        "red.runtime.image"
    };
    fs::write(root.join(image_file), &image_bytes).unwrap();

    // These two fixtures have no Prefabs. Cook their actual flat source expansions through the
    // production owner, including defaults and canonical references; never manufacture the wire.
    assert!(
        snapshot
            .expanded_scene()
            .entities
            .iter()
            .all(|entity| entity.prefab.is_none())
    );
    let expanded = ExpandedSceneCandidate::plain(
        snapshot.selected_scene_source_key().clone(),
        Arc::new(snapshot.expanded_scene().clone()),
    );
    assert_eq!(expanded.provenance(), snapshot.expanded_scene_provenance());
    let admitted = RuntimeSceneCandidate::cook(
        &expanded,
        plan.schema_validation().bindings(),
        snapshot.asset_resolution(),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let scene_bytes = admitted.to_json_bytes().unwrap();
    let scene_file = if later {
        "later.runtime.json"
    } else {
        "startup.runtime.json"
    };
    fs::write(root.join(scene_file), &scene_bytes).unwrap();
    [
        RuntimeContentMember::new(
            scene_file,
            ContentDigest::of_bytes(&scene_bytes),
            RuntimeContentMemberKind::Scene {
                source: admitted.source_key().clone(),
            },
            vec![scene_product(later)],
            vec![RuntimeContentDependency::new(
                image_product(later),
                RuntimeContentDependencyRole::Required,
            )],
        )
        .unwrap(),
        RuntimeContentMember::new(
            image_file,
            ContentDigest::of_bytes(&image_bytes),
            RuntimeContentMemberKind::Asset {
                format: FormatKind::new(ImageRuntimeCodec::RUNTIME_KIND).unwrap(),
                version: FormatVersion::ONE,
            },
            vec![image_product(later)],
            vec![],
        )
        .unwrap(),
    ]
}

const MANIFEST: &str = r#"schema_version = 1

[project]
source_universe = "f616982d-cfbf-43fd-bd47-671dd7e23105"
native_document_identity = "project_location"
name = "Stock render content consumer"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[content]
scenes = ["later.scene.json"]

[authoring]
schema_bundle = "schema/authoring.bundle.json"

[runtime]
preset = "local-headless"

[tasks.io]
workers = 1
pending_capacity = 16
[tasks.compute]
workers = 1
pending_capacity = 16
[tasks.async_compute]
workers = 1
pending_capacity = 16

[window]
enabled = true
width = 16
height = 16

[capabilities]
requested = ["runtime-2d", "presentation-2d", "desktop-winit", "render-wgpu"]
"#;
