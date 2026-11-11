//! One admitted source Scene and its two-member cooked Runtime delivery.

use std::{fs, num::NonZeroU32, path::Path};

use anyhow::{Context, Result, ensure};
use nara_engine::{
    ProductRecipe,
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetRef, AssetSourceKind,
        ImportedProductId, RuntimeAssetCodec, StableAssetId,
    },
    core::{ByteLimit, FormatKind, FormatVersion},
    fs::{ContentDigest, RelativePath},
    image::{
        ImageColorSpace, ImageCookLimits, ImageCookSettings, ImageCookTarget, ImageExtent,
        ImageImportSettings, ImageRuntimeCodec, cook_runtime_image,
    },
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
        RuntimeSceneCandidate, RuntimeSceneLimits, SceneCompilationOptions, SceneComponentBindings,
        SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    },
};
use tempfile::TempDir;

use super::{HEIGHT, TARGET_HEIGHT, TARGET_WIDTH, WIDTH, capture::CapturePlugin, validate_pixels};

const SCENE_ID: &str = "c9de1082-f645-4cd7-8b72-932fc2b91201";
const IMAGE_ID: &str = "c9de1082-f645-4cd7-8b72-932fc2b91202";
const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 12, 0, 0, 0, 4, 8, 6, 0,
    0, 0, 186, 38, 222, 138, 0, 0, 0, 26, 73, 68, 65, 84, 120, 156, 99, 104, 112, 56, 192, 128,
    132, 29, 144, 112, 3, 18, 254, 15, 195, 12, 131, 79, 3, 0, 167, 148, 92, 245, 236, 185, 187,
    42, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

pub(super) struct ContentFixture {
    source: TempDir,
    processed: TempDir,
    expected: RuntimeContentExpectation,
    recipe: ProductRecipe,
}

impl ContentFixture {
    pub(super) fn new() -> Result<Self> {
        let source = tempfile::tempdir().context("create the source content directory")?;
        let processed = tempfile::tempdir().context("create the processed content directory")?;
        let recipe = ProductRecipe::new()
            .add_runtime_image_codec::<ImageRuntimeCodec>(RuntimeCodecDefinitionId::new(
                "consumer.gpui-view.image",
                1,
            ))?
            .add_plugin::<CapturePlugin>()?;
        let configuration = runtime_configuration();
        let runtime_plan =
            resolve_runtime_configuration(&configuration.resolve()?, recipe.clone())?;

        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir(source.path().join(directory))?;
        }
        fs::write(source.path().join("nara.toml"), MANIFEST)?;
        fs::write(
            source.path().join("schema/authoring.bundle.json"),
            runtime_plan
                .schema_validation()
                .authoring_schema_bundle()
                .to_json_string()?,
        )?;
        fs::write(source.path().join("assets/alpha.png"), PNG)?;
        let image_meta = AssetMeta::new(
            AssetMetaIdentity::new(
                image_product()?.source(),
                AssetPath::new("alpha.png")?,
                AssetSourceKind::Image,
            ),
            ImageImportSettings::new(ImageColorSpace::Srgb),
        );
        fs::write(
            source.path().join("assets/alpha.png.meta"),
            image_meta.to_json_string()?,
        )?;
        let scene = scene_document(runtime_plan.schema_validation().bindings())?;
        fs::write(
            source.path().join("scenes/startup.scene.json"),
            scene.to_json_string()?,
        )?;
        let scene_meta = AssetMeta::new(
            AssetMetaIdentity::new(
                scene_product()?.source(),
                AssetPath::new("startup.scene.json")?,
                AssetSourceKind::Scene,
            ),
            (),
        );
        fs::write(
            source.path().join("scenes/startup.scene.json.meta"),
            scene_meta.to_json_string()?,
        )?;

        let root = open_explicit_product_project_root(source.path())?;
        let manifest = root.open_file(&RelativePath::new("nara.toml")?)?;
        let candidate = ingest_project_manifest(&manifest, None)?;
        drop(manifest);
        let source_plan = resolve_product_recipe(&candidate, recipe.clone())?;
        let loader = ProjectContentLoader::new(root)?;
        let snapshot = loader.load(&candidate, &source_plan)?;
        let members = write_processed_members(processed.path(), &runtime_plan, &snapshot)?;
        let target = RuntimeContentTarget::new("gpui-view-stock-image", "alpha-bands")?;
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
                startup_scene: scene_product()?,
                scenes: vec![scene_product()?],
                addressable_roots: vec![image_product()?],
                members: members.into(),
            },
            RuntimeContentCatalogLimits::default(),
        )?;
        let bytes = catalog.to_json_bytes()?;
        let expected = RuntimeContentExpectation::new(ContentDigest::of_bytes(&bytes), target);
        fs::write(processed.path().join(RUNTIME_CONTENT_CATALOG_FILE), bytes)?;
        drop(snapshot);
        ensure!(
            loader.budget_snapshot().active_generations() == 0,
            "the producer must release its admitted source snapshot after encoding"
        );
        drop(loader);
        Ok(Self {
            source,
            processed,
            expected,
            recipe,
        })
    }

    pub(super) fn source_root(&self) -> &Path {
        self.source.path()
    }

    pub(super) fn recipe(&self) -> &ProductRecipe {
        &self.recipe
    }

    pub(super) fn open_processed(&self) -> Result<RuntimeContentBase> {
        RuntimeContentBase::open(
            &open_explicit_product_project_root(self.processed.path())?,
            &self.expected,
            self.recipe.clone(),
            RuntimeContentLimits::default(),
        )
        .context("admit the actual two-member cooked content and its producer expectation")
    }

    pub(super) fn destroy_backing(self) -> Result<()> {
        for path in [
            "assets/alpha.png",
            "assets/alpha.png.meta",
            "scenes/startup.scene.json",
            "scenes/startup.scene.json.meta",
            "schema/authoring.bundle.json",
            "nara.toml",
        ] {
            fs::remove_file(self.source.path().join(path))
                .with_context(|| format!("remove the owned source input {path}"))?;
        }
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::remove_dir(self.source.path().join(directory))?;
        }
        for path in [
            "startup.runtime.json",
            "alpha.runtime.image",
            RUNTIME_CONTENT_CATALOG_FILE,
        ] {
            fs::remove_file(self.processed.path().join(path))
                .with_context(|| format!("remove the owned processed input {path}"))?;
        }
        self.source
            .close()
            .context("close the empty source content directory")?;
        self.processed
            .close()
            .context("close the empty processed content directory")?;
        Ok(())
    }
}

pub(super) fn image_product() -> Result<AssetProductRef> {
    Ok(AssetProductRef::new(
        StableAssetId::parse_str(IMAGE_ID)?,
        ImportedProductId::PRIMARY,
    )?)
}

fn scene_product() -> Result<AssetProductRef> {
    Ok(AssetProductRef::new(
        StableAssetId::parse_str(SCENE_ID)?,
        ImportedProductId::PRIMARY,
    )?)
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
    configuration.window.width = TARGET_WIDTH;
    configuration.window.height = TARGET_HEIGHT;
    configuration
}

fn initial(bindings: &SceneComponentBindings, name: &str) -> Result<SceneComponentRecord> {
    let schema = bindings
        .schema(&ComponentTypeId::new(name))
        .with_context(|| format!("missing stock schema {name}"))?;
    Ok(SceneComponentRecord::new(
        schema.version(),
        schema
            .authoring_initial_value()
            .with_context(|| format!("missing stock initial value {name}"))?
            .clone(),
    ))
}

fn scene_document(bindings: &SceneComponentBindings) -> Result<SceneDocument> {
    let transform = initial(bindings, "nara.transform.Transform2d")?;
    let mut sprite = initial(bindings, "nara.sprite.Sprite")?;
    let ComponentValue::Map(fields) = &mut sprite.value else {
        anyhow::bail!("the stock Sprite initial value must be a map");
    };
    fields.insert(
        "size".into(),
        ComponentValue::map([
            ("x", ComponentValue::f64(f64::from(TARGET_WIDTH))?),
            ("y", ComponentValue::f64(f64::from(TARGET_HEIGHT))?),
        ]),
    );
    fields.insert(
        "material".into(),
        ComponentValue::map([
            (
                "image",
                encode_asset_reference_value(&AssetRef::path("alpha.png")?),
            ),
            (
                "sampler",
                ComponentValue::map([
                    ("min_filter", ComponentValue::String("nearest".into())),
                    ("mag_filter", ComponentValue::String("nearest".into())),
                    ("mipmap_filter", ComponentValue::String("nearest".into())),
                    (
                        "address_mode_u",
                        ComponentValue::String("clamp_to_edge".into()),
                    ),
                    (
                        "address_mode_v",
                        ComponentValue::String("clamp_to_edge".into()),
                    ),
                ]),
            ),
            ("alpha_mode", ComponentValue::String("blend".into())),
            (
                "tint",
                ComponentValue::map([
                    ("r", ComponentValue::f64(1.0)?),
                    ("g", ComponentValue::f64(1.0)?),
                    ("b", ComponentValue::f64(1.0)?),
                    ("a", ComponentValue::f64(1.0)?),
                ]),
            ),
        ]),
    );
    let camera = SceneComponentRecord::new(
        bindings
            .schema(&ComponentTypeId::new("nara.render.Camera2d"))
            .context("missing stock Camera2d schema")?
            .version(),
        ComponentValue::map([
            (
                "viewport_height",
                ComponentValue::f64(f64::from(TARGET_HEIGHT))?,
            ),
            (
                "clear_color",
                ComponentValue::map([
                    ("r", ComponentValue::f64(0.0)?),
                    ("g", ComponentValue::f64(0.0)?),
                    ("b", ComponentValue::f64(0.0)?),
                    ("a", ComponentValue::f64(0.0)?),
                ]),
            ),
        ]),
    );
    Ok(SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(SceneEntityId::new("sprite")?)
            .with_component(
                ComponentTypeId::new("nara.transform.Transform2d"),
                transform.clone(),
            )
            .with_component(ComponentTypeId::new("nara.sprite.Sprite"), sprite),
        SceneEntityRecord::new(SceneEntityId::new("camera")?)
            .with_component(
                ComponentTypeId::new("nara.transform.Transform2d"),
                transform,
            )
            .with_component(ComponentTypeId::new("nara.render.Camera2d"), camera),
    ]))
}

fn write_processed_members(
    root: &Path,
    plan: &RuntimeConfigurationPlan,
    snapshot: &ProjectContentSnapshot,
) -> Result<[RuntimeContentMember; 2]> {
    ensure!(
        snapshot.images().len() == 1,
        "the Scene must admit exactly one actual PNG"
    );
    let image = snapshot.images()[0].image();
    ensure!(
        image.product() == Some(image_product()?),
        "incorrect imported Image Product"
    );
    let source = image
        .source()
        .context("the producer must import real source provenance")?;
    ensure!(
        source.path().as_str() == "assets/alpha.png",
        "incorrect imported PNG source path"
    );
    ensure!(
        source.source_hash().as_bytes() == *ContentDigest::of_bytes(PNG).as_bytes(),
        "the Image import must describe the actual PNG bytes"
    );
    ensure!(
        image.extent() == ImageExtent::new(WIDTH, HEIGHT),
        "incorrect imported PNG extent"
    );
    ensure!(
        image.color_space() == ImageColorSpace::Srgb,
        "the PNG must use sRGB import settings"
    );
    validate_pixels(image.pixels())?;
    ensure!(
        image.share_retained().is_some(),
        "the admitted PNG must retain its source pixel reservation"
    );
    let cooked = cook_runtime_image(
        image,
        ImageCookSettings::default(),
        ImageCookTarget::new(
            NonZeroU32::new(WIDTH).context("the image cook target must be nonzero")?,
        ),
        ImageCookLimits::new(
            ByteLimit::new(4096).context("the retained cook bound must be nonzero")?,
            ByteLimit::new(8192).context("the allocation cook bound must be nonzero")?,
        ),
    )?;
    ensure!(
        cooked.source().is_none(),
        "the cooked Image must be source-independent"
    );
    ensure!(
        cooked.product() == image.product(),
        "cooking must preserve the Image Product"
    );
    ensure!(
        cooked.pixels() == image.pixels(),
        "the consumer target must preserve every source texel"
    );
    let image_bytes = ImageRuntimeCodec::encode_runtime(&cooked)?;
    fs::write(root.join("alpha.runtime.image"), &image_bytes)?;

    ensure!(
        snapshot.prefabs().is_empty(),
        "the fixture has no Prefab sources"
    );
    let compilation = plan
        .schema_validation()
        .bindings()
        .try_compile_scene_with_loader(
            snapshot.scene(),
            SceneCompilationOptions::default(),
            Some(snapshot.asset_resolution()),
            |_, _| Ok::<_, std::convert::Infallible>(None),
        )?;
    ensure!(
        !compilation.diagnostics.has_errors(),
        "stock Scene compilation failed: {:?}",
        compilation.diagnostics
    );
    let compiled = compilation
        .candidate
        .context("the actual stock Scene must compile")?
        .bind_source(snapshot.selected_scene_source_key().clone());
    ensure!(
        compiled.document() == snapshot.compiled_scene(),
        "cooking must use the admitted Scene document"
    );
    ensure!(
        compiled.canonical_provenance() == snapshot.compiled_scene_provenance(),
        "cooking must use the admitted Scene provenance"
    );
    let scene = RuntimeSceneCandidate::cook(
        &compiled,
        plan.schema_validation().bindings(),
        snapshot.asset_resolution(),
        RuntimeSceneLimits::default(),
    )?;
    let scene_bytes = scene.to_json_bytes()?;
    fs::write(root.join("startup.runtime.json"), &scene_bytes)?;
    Ok([
        RuntimeContentMember::new(
            "startup.runtime.json",
            ContentDigest::of_bytes(&scene_bytes),
            RuntimeContentMemberKind::Scene {
                source: scene.source_key().clone(),
            },
            vec![scene_product()?],
            vec![RuntimeContentDependency::new(
                image_product()?,
                RuntimeContentDependencyRole::Required,
            )],
        )?,
        RuntimeContentMember::new(
            "alpha.runtime.image",
            ContentDigest::of_bytes(&image_bytes),
            RuntimeContentMemberKind::Asset {
                format: FormatKind::new(ImageRuntimeCodec::RUNTIME_KIND)?,
                version: FormatVersion::ONE,
            },
            vec![image_product()?],
            vec![],
        )?,
    ])
}

const MANIFEST: &str = r#"schema_version = 1

[project]
source_universe = "c9de1082-f645-4cd7-8b72-932fc2b91203"
native_document_identity = "project_location"
name = "GPUI stock content capture"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[authoring]
state = "ready"
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
width = 96
height = 32

[capabilities]
requested = ["runtime-2d", "presentation-2d", "desktop-winit", "render-wgpu"]
"#;
