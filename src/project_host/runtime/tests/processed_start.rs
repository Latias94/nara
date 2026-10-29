use std::{fs, path::PathBuf, time::Instant};

use nara_app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
use nara_asset::{AssetProductRef, AssetRef, ImportedProductId, RuntimeAssetCodec, StableAssetId};
use nara_core::{ByteLimit, FormatKind, FormatVersion};
use nara_fs::ContentDigest;
use nara_identity::{EntityLookup, SceneEntityId, WorldIdentityDomain};
use nara_image::{ImageColorSpace, ImageDomain, ImageExtent, ImageFormat, ImageRuntimeCodec};
use nara_project::{ProductCapability, ProjectPath, RuntimeConfiguration, RuntimePreset};
use nara_reflect::{ComponentTypeId, ComponentValue, encode_asset_reference_value};
use nara_scene::{
    RuntimeSceneDocumentCandidate, RuntimeSceneLimits, SceneComponentRecord, SourceDocumentKey,
    SourceDocumentKind,
};

use super::*;
use crate::{
    ProductRecipe,
    project_host::{open_explicit_product_project_root, resolve_runtime_configuration},
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeCodecDefinitionId, RuntimeContentCatalog,
        RuntimeContentCatalogDefinition, RuntimeContentCatalogLimits, RuntimeContentDependency,
        RuntimeContentDependencyRole, RuntimeContentExpectation, RuntimeContentLimits,
        RuntimeContentMember, RuntimeContentMemberKind, RuntimeContentTarget,
    },
    startup_scene::CurrentSceneOwner,
};

const STARTUP_FILE: &str = "startup.runtime.json";
const LATER_FILE: &str = "later.runtime.json";
const IMAGE_FILE: &str = "sprite.runtime.image";
const PIXELS: [u8; 4] = [17, 31, 47, 255];
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
static SOURCE_PLAN_BUILDS: AtomicU64 = AtomicU64::new(0);

fn product(source: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(source).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn startup() -> AssetProductRef {
    product("72100000-0000-4000-8000-000000000001")
}

fn later() -> AssetProductRef {
    product("72100000-0000-4000-8000-000000000002")
}

fn image_product() -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str("72100000-0000-4000-8000-000000000003").unwrap(),
        ImportedProductId::parse_subproduct("72100000-0000-4000-8000-000000000004").unwrap(),
    )
    .unwrap()
}

fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_runtime_image_codec::<ImageRuntimeCodec>(RuntimeCodecDefinitionId::new(
            "nara.test.processed-start.image",
            1,
        ))
        .unwrap()
}

struct Fixture {
    root: PathBuf,
    recipe: ProductRecipe,
    expected: RuntimeContentExpectation,
}

impl Fixture {
    fn new(recipe: ProductRecipe) -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_processed_start_{}_{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        let mut configuration = RuntimeConfiguration::default();
        configuration.runtime.preset = RuntimePreset::LocalHeadless;
        configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
        configuration.tasks.io.workers = 1;
        configuration.tasks.compute.workers = 1;
        configuration.tasks.async_compute.workers = 1;
        let plan = resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone())
            .unwrap();
        let mut members = Vec::new();
        for (path, reference, name) in [
            (STARTUP_FILE, startup(), "startup"),
            (LATER_FILE, later(), "later"),
        ] {
            let encoded = scene_bytes(&plan, reference, name);
            fs::write(root.join(path), &encoded).unwrap();
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Scene {
                        source: SourceDocumentKey::stable_asset(
                            reference.source(),
                            SourceDocumentKind::Scene,
                        ),
                    },
                    vec![reference],
                    vec![RuntimeContentDependency::new(
                        image_product(),
                        RuntimeContentDependencyRole::Required,
                    )],
                )
                .unwrap(),
            );
        }
        let image = ImageAsset::from_product(
            image_product(),
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            PIXELS.to_vec(),
        )
        .unwrap();
        let encoded = ImageRuntimeCodec::encode_runtime(&image).unwrap();
        fs::write(root.join(IMAGE_FILE), &encoded).unwrap();
        members.push(
            RuntimeContentMember::new(
                IMAGE_FILE,
                ContentDigest::of_bytes(&encoded),
                RuntimeContentMemberKind::Asset {
                    format: FormatKind::new(ImageRuntimeCodec::RUNTIME_KIND).unwrap(),
                    version: FormatVersion::ONE,
                },
                vec![image_product()],
                vec![],
            )
            .unwrap(),
        );
        let target = RuntimeContentTarget::new("test-host", "processed-start").unwrap();
        let catalog = RuntimeContentCatalog::new(
            RuntimeContentCatalogDefinition {
                target: target.clone(),
                configuration,
                plugin_plan_fingerprint: plan.plugin_plan().fingerprint().as_bytes(),
                schema_fingerprint: plan
                    .schema_validation()
                    .composition_fingerprint()
                    .as_bytes(),
                runtime_codecs_fingerprint: plan.runtime_codecs_fingerprint().as_bytes(),
                startup_scene: startup(),
                scenes: vec![startup(), later()],
                addressable_roots: vec![],
                members,
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap();
        let encoded = catalog.to_json_bytes().unwrap();
        fs::write(root.join(RUNTIME_CONTENT_CATALOG_FILE), &encoded).unwrap();
        Self {
            root,
            recipe,
            expected: RuntimeContentExpectation::new(ContentDigest::of_bytes(&encoded), target),
        }
    }

    fn open(&self, limits: RuntimeContentLimits) -> RuntimeContentBase {
        RuntimeContentBase::open(
            &open_explicit_product_project_root(&self.root).unwrap(),
            &self.expected,
            self.recipe.clone(),
            limits,
        )
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara_processed_start_")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn scene_bytes(plan: &RuntimeConfigurationPlan, reference: AssetProductRef, name: &str) -> Vec<u8> {
    let schema = plan
        .schema_validation()
        .registry()
        .schema(&ComponentTypeId::new("nara.sprite.Sprite"))
        .unwrap();
    let mut value = schema.authoring_initial_value().unwrap().clone();
    let ComponentValue::Map(fields) = &mut value else {
        unreachable!()
    };
    let Some(ComponentValue::Map(material)) = fields.get_mut("material") else {
        unreachable!()
    };
    material.insert(
        "image".into(),
        encode_asset_reference_value(&AssetRef::Product(image_product())),
    );
    let encoded = serde_json::json!({
        "kind": "runtime_scene", "format_version": 1, "engine_min_version": "0.1.0",
        "generator": {"name": "processed-start-test", "version": "0.0.0"},
        "payload": {
            "source_key": {"kind": "scene", "origin": {"stable_asset": {"stable_id": reference.source()}}},
            "roots": [name],
            "entities": [{"id": name, "children": [], "components": {
                "nara.sprite.Sprite": SceneComponentRecord::new(schema.version, value)
            }}],
            "provenance": [{"entity": name, "local_entity": name, "origin": {"kind": "local"}}]
        }
    });
    RuntimeSceneDocumentCandidate::from_json_bytes(
        &serde_json::to_vec(&encoded).unwrap(),
        RuntimeSceneLimits::default(),
    )
    .unwrap()
    .admit(
        &SourceDocumentKey::stable_asset(reference.source(), SourceDocumentKind::Scene),
        plan.schema_validation().bindings(),
    )
    .unwrap()
    .to_json_bytes()
    .unwrap()
}

#[derive(Default, Resource)]
struct RequestProbe {
    request: Option<ProjectSceneRequestId>,
    ready: bool,
}

#[derive(Default)]
struct RequestProbePlugin;

impl Plugin for RequestProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.processed-start.requests"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<RequestProbe>()?;
        app.add_systems(CoreStage::Update, |world: &mut World| {
            let request = world.resource::<RequestProbe>().request;
            let request = request.unwrap_or_else(|| {
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(
                        scenes.request(ProjectPath::new("later.scene.json").unwrap()),
                        Err(ProjectSceneRequestError::UndeclaredScene),
                    );
                    scenes.request(later()).unwrap()
                })
            });
            let ready = ProjectScenes::scope(world, |scenes| {
                if scenes.state(request) != Some(ProjectSceneRequestState::Ready) {
                    return false;
                }
                let view = scenes.prepared(request).unwrap();
                assert_eq!(view.selected_scene_path(), None);
                assert_eq!(view.selected_scene_product(), Some(later()));
                assert_eq!(view.scene().roots, [SceneEntityId::new("later").unwrap()]);
                true
            });
            world.insert_resource(RequestProbe {
                request: Some(request),
                ready,
            });
        })?;
        Ok(())
    }
}

#[derive(Default)]
struct SourcePlanProbePlugin;

impl Plugin for SourcePlanProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.processed-start.source-plan"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        SOURCE_PLAN_BUILDS.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

fn retire(host: &mut ProjectHost) {
    host.retire_running();
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.has_cleanup_owner() {
        if let CleanupDriveOutcome::Complete {
            failed,
            diagnostics,
            ..
        } = host.drive_cleanup_once()
        {
            assert!(!failed, "{diagnostics:?}");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Runtime cleanup did not physically complete"
        );
        std::thread::yield_now();
    }
    assert!(!host.has_runtime_custody());
}

#[test]
fn processed_start_installs_native_values_and_original_request_context_without_files() {
    let fixture = Fixture::new(recipe().add_plugin::<RequestProbePlugin>().unwrap());
    let base = fixture.open(RuntimeContentLimits::default());
    for path in [
        STARTUP_FILE,
        LATER_FILE,
        IMAGE_FILE,
        RUNTIME_CONTENT_CATALOG_FILE,
    ] {
        fs::remove_file(fixture.root.join(path)).unwrap();
    }
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host
        .begin_processed_start(base.clone(), Vec::new())
        .unwrap();
    let prepared = host.prepare_start(&mut attempt).unwrap();
    assert!(prepared.source().is_none());
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
    assert!(host.has_runtime_custody());
    assert!(!host.commit_prepared_start(prepared).has_errors());
    assert_eq!(host.running_content_revision(), None);
    host.with_running_world(|world| {
        let current = world.resource::<CurrentSceneOwner>();
        assert!(current.is_content_managed());
        assert!(!current.is_project_backed());
        assert!(current.hosted_runtime_base().unwrap().same_base(&base));
        assert!(!world.contains_resource::<nara_asset::ProjectAssetDatabase>());
        let mut server = world.resource::<AssetServer>().clone();
        let handle = server
            .resolve_product::<ImageAsset>(image_product())
            .unwrap();
        let image = world.resource::<Assets<ImageAsset>>().get(handle).unwrap();
        assert_eq!(image.pixels(), PIXELS);
        assert!(image.source().is_none());
        let reference = current
            .receipt()
            .runtime_reference(&SceneEntityId::new("startup").unwrap())
            .unwrap();
        let EntityLookup::Resolved(entity) = world
            .resource::<WorldIdentityDomain>()
            .lookup(world, &reference)
        else {
            panic!("the processed startup entity is live")
        };
        assert_eq!(
            world
                .get::<nara_sprite::Sprite>(entity)
                .unwrap()
                .material
                .image,
            Some(handle)
        );
        let domains = world.resource::<crate::project_content::ProjectDomainAssets>();
        let collection = domains.collection::<ImageDomain>().unwrap();
        assert_eq!(collection.source_records().len(), 0);
        assert_eq!(
            collection
                .get(&AssetRef::Product(image_product()))
                .unwrap()
                .pixels(),
            PIXELS
        );
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !host
        .with_running_world(|world| world.resource::<RequestProbe>().ready)
        .unwrap()
    {
        assert!(
            Instant::now() < deadline,
            "the captured later Scene did not become ready"
        );
        host.drive_one_frame(Duration::ZERO).unwrap();
        std::thread::yield_now();
    }
    retire(&mut host);
}

#[test]
fn processed_start_stamp_rejects_an_equal_catalog_base_and_reprepared_scene() {
    let fixture = Fixture::new(recipe());
    let base = fixture.open(RuntimeContentLimits::default());
    let foreign = fixture.open(RuntimeContentLimits::default());
    assert_eq!(base.catalog_digest(), foreign.catalog_digest());
    assert!(!base.same_base(&foreign));
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host
        .begin_processed_start(base.clone(), Vec::new())
        .unwrap();
    let original = attempt.inputs.as_ref().unwrap().content.clone();
    for replacement in [
        RuntimeStartContent::Processed {
            base: foreign.clone(),
            content: foreign.prepare_scene_content(startup()).unwrap(),
        },
        RuntimeStartContent::Processed {
            base: base.clone(),
            content: base.prepare_scene_content(startup()).unwrap(),
        },
        RuntimeStartContent::Processed {
            base: base.clone(),
            content: base.prepare_scene_content(later()).unwrap(),
        },
    ] {
        attempt.inputs.as_mut().unwrap().content = replacement;
        let error = attempt.take_for(&host).err().unwrap();
        assert_eq!(
            error.diagnostics.iter().next().unwrap().code().as_str(),
            "project.run.stale-start"
        );
        assert!(attempt.inputs.is_some());
        assert!(host.start_claim.is_active(attempt.epoch));
    }
    attempt.inputs.as_mut().unwrap().content = original;
    let RuntimeStartStamp::Processed { bindings, .. } = &mut attempt.stamp else {
        unreachable!()
    };
    assert!(!bindings.same_definitions(foreign.plan().schema_validation().bindings()));
    *bindings = foreign.plan().schema_validation().bindings().clone();
    assert!(attempt.take_for(&host).is_err());
    drop(attempt);
    assert!(!host.has_runtime_custody());
}

#[test]
fn processed_source_execution_is_rejected_before_any_runtime_plugin_builds() {
    let fixture = Fixture::new(
        recipe()
            .add_plugin::<nara_asset::AssetSourcePlugin>()
            .unwrap()
            .add_plugin::<SourcePlanProbePlugin>()
            .unwrap(),
    );
    let base = fixture.open(RuntimeContentLimits::default());
    let before = SOURCE_PLAN_BUILDS.load(Ordering::Relaxed);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let error = host.begin_processed_start(base, Vec::new()).err().unwrap();
    assert_eq!(
        error.diagnostics.iter().next().unwrap().code().as_str(),
        "project.run.source-execution-not-allowed"
    );
    assert_eq!(SOURCE_PLAN_BUILDS.load(Ordering::Relaxed), before);
    assert_eq!(host.next_epoch, 1);
    assert!(!host.has_runtime_custody());
}

#[test]
fn processed_preparation_failure_leaves_no_start_claim_or_runtime() {
    let fixture = Fixture::new(recipe());
    let base = fixture.open(
        RuntimeContentLimits::default()
            .with_scene(RuntimeSceneLimits::default().with_working_bytes(ByteLimit::ONE)),
    );
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let error = host.begin_processed_start(base, Vec::new()).err().unwrap();
    assert_eq!(
        error.diagnostics.iter().next().unwrap().code().as_str(),
        "project.run.runtime-content-rejected"
    );
    assert_eq!(host.next_epoch, 1);
    assert!(!host.has_runtime_custody());
}

#[test]
fn processed_publication_failure_and_aborted_preparation_retain_existing_cleanup_custody() {
    let fixture = Fixture::new(recipe());
    let base = fixture.open(RuntimeContentLimits::default());
    for publication_fault in [true, false] {
        let mut host = ProjectHost::new(RuntimeClosePolicy::default());
        let mut attempt = host
            .begin_processed_start(base.clone(), Vec::new())
            .unwrap();
        if publication_fault {
            arm_publication_fault_for_test();
            let error = host.prepare_start(&mut attempt).err().unwrap();
            assert_eq!(
                error.diagnostics.iter().next().unwrap().code().as_str(),
                "project.run.publication-faulted"
            );
        } else {
            let prepared = host.prepare_start(&mut attempt).unwrap();
            host.abort_prepared_start(prepared);
        }
        let ProjectHostSlot::Cleaning { owner, .. } = &host.slot else {
            panic!("unpublished Runtime cleanup remains in the existing Host owner")
        };
        assert!(
            owner
                .world()
                .unwrap()
                .resource::<CurrentSceneOwner>()
                .hosted_runtime_base()
                .unwrap()
                .same_base(&base)
        );
        assert!(host.has_runtime_custody());
        drop(attempt);
        retire(&mut host);
    }
}
