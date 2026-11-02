//! One real delivery operation: two cooked Scenes and the actual built compound bundle.

use std::{fs, sync::Arc, time::Duration};

use nara::{
    ProductRecipe, SchemaContribution,
    advanced_prelude::{
        CurrentScene, materialize_startup_scene, replace_current_scene_with_product,
    },
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId, RuntimeAdmissionReservation, RuntimeClosePolicy,
        RuntimeObligationLedger,
    },
    asset::{AssetRecord, AssetRef, ProjectAssetDatabase},
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildObservation, ArtifactBuildOutput, ArtifactBuildSeed,
        ArtifactGroupCandidate, VerifiedArtifactGroup,
    },
    core::{FormatKind, ItemLimit},
    ecs::{Component, Resource, World},
    identity::{EntityLookup, SpawnedSceneInstance},
    project::{ProductCapability, RuntimeConfiguration, RuntimePreset},
    project_host::{RuntimeConfigurationPlan, resolve_runtime_configuration},
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponent, PersistentComponentProvider,
    },
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeCodecDefinitionId, RuntimeContentAsset,
        RuntimeContentBase, RuntimeContentCatalog, RuntimeContentCatalogDefinition,
        RuntimeContentCatalogLimits, RuntimeContentDependency, RuntimeContentDependencyRole,
        RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentMember,
        RuntimeContentMemberKind, RuntimeContentScene, RuntimeContentTarget,
    },
    scene::{
        RuntimeSceneCandidate, RuntimeSceneLimits, SceneComponentRecord, SceneDocument,
        SceneEntityId, SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
        advanced::SceneProductTransactionLimits, scene_component_bindings,
    },
};
use nara_asset::AssetReferenceDemand;
use serde_json::json;

use super::{
    ArtifactContentDigest, AssetPathScope, AssetProductRef, AssetSourceKind, AssetSourcePath,
    CapabilityRights, FileCapability, Fixture, FormatVersion, ImportedProductId, RuntimeAssetCodec,
    SOURCE, SourceAssetImporter, StableAssetId, TextBundle, TextBundleDomain, TrustMode,
    definition, directory, reference,
};

const STARTUP: &str = "b9cf8d3e-8776-4f29-a67d-fd902e7398b9";
const LATER: &str = "b0e10861-31ef-4694-9d82-9a5131d6dc46";
const CAPTION: &str = "consumer.durable-build.Caption";
const SCENE_A: &str = "scene-a.runtime.json";
const SCENE_B: &str = "scene-b.runtime.json";
const BUNDLE: &str = "bundle.runtime.json";
const CODEC: &str = "consumer.durable-build.bundle-codec";

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.durable-build.Caption",
    version = 1,
    alias = "Built Caption",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct Caption {
    #[nara(
        id = "product",
        alias = "Product",
        capabilities(scene, inspect, edit, asset_ref)
    )]
    product: AssetRef,
}

nara::persistent_components! { mod components = [Caption]; }

const PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("consumer.durable-build.schema");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.durable-build"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.durable-build.native", 1),
    components::initial_catalog,
    components::register,
);

#[derive(Default)]
struct DeliveryPlugin;

impl Plugin for DeliveryPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.durable-build"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(CoreStage::Update, travel)?;
        Ok(())
    }
}

fn scene_product(source: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(source).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn caption_id() -> SceneEntityId {
    SceneEntityId::new("caption").unwrap()
}

/// Retains the original compiled plan used to cook both Scenes and describe the delivery catalog.
pub(super) struct Delivery {
    recipe: ProductRecipe,
    plan: RuntimeConfigurationPlan,
    configuration: RuntimeConfiguration,
    selections: [AssetProductRef; 2],
}

impl Delivery {
    pub(super) fn new(first: ImportedProductId, second: ImportedProductId) -> Self {
        let recipe = ProductRecipe::new()
            .add_contribution(
                SchemaContribution::<DeliveryPlugin>::for_default([PROVIDER]).unwrap(),
            )
            .unwrap()
            .add_runtime_asset_codec::<TextBundleDomain, TextBundleDomain>(
                RuntimeCodecDefinitionId::new(CODEC, 1),
            )
            .unwrap();
        let mut configuration = RuntimeConfiguration::default();
        configuration.runtime.preset = RuntimePreset::LocalHeadless;
        configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
        configuration.tasks.io.workers = 1;
        configuration.tasks.compute.workers = 1;
        configuration.tasks.async_compute.workers = 1;
        let plan = resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone())
            .unwrap();
        Self {
            recipe,
            plan,
            configuration,
            selections: [reference(first), reference(second)],
        }
    }

    fn scene_bytes(&self, source: &str, selection: AssetProductRef) -> Vec<u8> {
        let caption = Caption {
            product: AssetRef::Product(selection),
        };
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(caption_id())
            .with_component(
                ComponentTypeId::new(CAPTION),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    caption.__encode_persistent_component().unwrap(),
                ),
            )]);
        let mut database = ProjectAssetDatabase::default();
        database
            .insert(AssetRecord::new(
                StableAssetId::parse_str(SOURCE).unwrap(),
                AssetSourcePath::new("assets/bundle.json").unwrap(),
                AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
            ))
            .unwrap();
        let compilation = self
            .plan
            .schema_validation()
            .bindings()
            .try_compile_scene_with_loader(
                &document,
                Default::default(),
                Some(database.resolver(&AssetPathScope::ROOT)),
                |_, _| Ok::<_, std::convert::Infallible>(None),
            )
            .unwrap();
        assert!(
            !compilation.diagnostics.has_errors(),
            "{:?}",
            compilation.diagnostics
        );
        let expanded = compilation
            .candidate
            .unwrap()
            .bind_source(SourceDocumentKey::stable_asset(
                StableAssetId::parse_str(source).unwrap(),
                SourceDocumentKind::Scene,
            ));
        RuntimeSceneCandidate::cook(
            &expanded,
            self.plan.schema_validation().bindings(),
            database.resolver(&AssetPathScope::ROOT),
            RuntimeSceneLimits::default(),
        )
        .unwrap()
        .to_json_bytes()
        .unwrap()
    }

    pub(super) fn candidate(&self, bundle: &VerifiedArtifactGroup) -> ArtifactGroupCandidate {
        let input = &bundle.members()[0];
        assert_eq!(input.name, "runtime");
        let mut outputs = Vec::new();
        for (name, source, selection) in [
            (SCENE_A, STARTUP, self.selections[0]),
            (SCENE_B, LATER, self.selections[1]),
        ] {
            outputs.push(ArtifactBuildOutput {
                name: name.to_owned(),
                format: FormatKind::new("runtime_scene").unwrap(),
                format_version: FormatVersion::ONE,
                products: vec![scene_product(source)],
                dependencies: vec![(selection, AssetReferenceDemand::Immediate)],
                bytes: Arc::from(self.scene_bytes(source, selection)),
            });
        }
        outputs.push(ArtifactBuildOutput {
            name: BUNDLE.to_owned(),
            format: input.format.clone(),
            format_version: input.format_version,
            products: input.products.clone(),
            dependencies: input.dependencies.clone(),
            bytes: Arc::from(bundle.member_bytes("runtime").unwrap()),
        });
        assert!(input.dependencies.is_empty());
        let tool = definition().implementation;
        ArtifactGroupCandidate {
            seed: ArtifactBuildSeed {
                source: scene_product(STARTUP),
                location: None,
                primary: None,
                definitions: vec![
                    ArtifactBuildDefinition {
                        id: "consumer.durable-build.scene-delivery".to_owned(),
                        revision: FormatVersion::ONE,
                        implementation: tool,
                    },
                    ArtifactBuildDefinition {
                        id: CODEC.to_owned(),
                        revision: FormatVersion::new(TextBundleDomain::RUNTIME_VERSION).unwrap(),
                        implementation: tool,
                    },
                ],
                // These actual selections configure the cooked Scene bytes; they are not unused labels.
                settings: serde_json::to_vec(&json!({
                    "configuration": self.configuration,
                    "selections": self.selections,
                    "plugin_plan": self.plan.plugin_plan().fingerprint().as_bytes(),
                    "schema": self.plan.schema_validation().composition_fingerprint().as_bytes(),
                    "runtime_codecs": self.plan.runtime_codecs_fingerprint().as_bytes(),
                }))
                .unwrap(),
                target: serde_json::to_vec(&self.target()).unwrap(),
                format: FormatKind::new("runtime_content_catalog").unwrap(),
                format_version: FormatVersion::ONE,
            },
            observations: vec![ArtifactBuildObservation::Processed {
                product: reference(ImportedProductId::PRIMARY),
                group: bundle.digest(),
                member: input.digest,
                format: input.format.clone(),
                format_version: input.format_version,
            }],
            outputs,
        }
    }

    fn target(&self) -> RuntimeContentTarget {
        RuntimeContentTarget::new(
            format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
            "consumer-durable-build",
        )
        .unwrap()
    }

    pub(super) fn pack(
        &self,
        fixture: &Fixture,
        group: &VerifiedArtifactGroup,
    ) -> RuntimeContentBase {
        assert_eq!(group.members().len(), 3);
        let root = fixture.root.join("delivery");
        let mut members = Vec::new();
        for member in group.members() {
            let bytes = group.member_bytes(&member.name).unwrap();
            assert_eq!(ArtifactContentDigest::of_bytes(bytes), member.digest);
            fs::write(root.join(&member.name), bytes).unwrap();
            let kind = if member.format.as_str() == "runtime_scene" {
                assert_eq!(member.products.len(), 1);
                RuntimeContentMemberKind::Scene {
                    source: SourceDocumentKey::stable_asset(
                        member.products[0].source(),
                        SourceDocumentKind::Scene,
                    ),
                }
            } else {
                RuntimeContentMemberKind::Asset {
                    format: member.format.clone(),
                    version: member.format_version,
                }
            };
            let dependencies = member
                .dependencies
                .iter()
                .map(|(reference, demand)| {
                    RuntimeContentDependency::new(
                        *reference,
                        match demand {
                            AssetReferenceDemand::Immediate => {
                                RuntimeContentDependencyRole::Required
                            }
                            AssetReferenceDemand::Deferred => {
                                RuntimeContentDependencyRole::Reference
                            }
                        },
                    )
                })
                .collect();
            members.push(
                RuntimeContentMember::new(
                    &member.name,
                    member.digest.content(),
                    kind,
                    member.products.clone(),
                    dependencies,
                )
                .unwrap(),
            );
        }
        let catalog = RuntimeContentCatalog::new(
            RuntimeContentCatalogDefinition {
                target: self.target(),
                configuration: self.configuration.clone(),
                plugin_plan_fingerprint: self.plan.plugin_plan().fingerprint().as_bytes(),
                schema_fingerprint: self
                    .plan
                    .schema_validation()
                    .composition_fingerprint()
                    .as_bytes(),
                runtime_codecs_fingerprint: self.plan.runtime_codecs_fingerprint().as_bytes(),
                startup_scene: scene_product(STARTUP),
                scenes: vec![scene_product(STARTUP), scene_product(LATER)],
                addressable_roots: self.selections.to_vec(),
                members,
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap();
        let encoded = catalog.to_json_bytes().unwrap();
        let expected = RuntimeContentExpectation::new(
            nara::fs::ContentDigest::of_bytes(&encoded),
            self.target(),
        );
        fs::write(root.join(RUNTIME_CONTENT_CATALOG_FILE), &encoded).unwrap();
        let loose = RuntimeContentBase::open(
            &directory(&root, CapabilityRights::ReadOnly),
            &expected,
            self.recipe.clone(),
            RuntimeContentLimits::default(),
        )
        .unwrap();
        // A distinct retained pack object lets an old base survive the successor's publication.
        let pack_name = format!(
            "delivery-{}.pack",
            group
                .recipe_digest()
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let pack_path = root.join(pack_name);
        let mut output = fs::File::create(&pack_path).unwrap();
        loose.write_pack(&mut output).unwrap();
        drop(output);
        drop(loose);
        for member in catalog.members() {
            fs::remove_file(root.join(member.path())).unwrap();
        }
        fs::remove_file(root.join(RUNTIME_CONTENT_CATALOG_FILE)).unwrap();
        let file = FileCapability::from_host_handle(
            fs::File::open(pack_path).unwrap(),
            TrustMode::TrustedLocal,
            0,
        )
        .unwrap();
        RuntimeContentBase::open_pack(
            file,
            &expected,
            self.recipe.clone(),
            RuntimeContentLimits::default(),
        )
        .unwrap()
    }

    pub(super) fn run_managed(&self, base: RuntimeContentBase, first: &str, second: &str) {
        let plan = base.plan().clone();
        let startup = base.acquire_startup_scene().unwrap();
        let later = base.acquire_scene(scene_product(LATER)).unwrap();
        let bundle = base
            .acquire_asset::<TextBundleDomain, TextBundleDomain>(self.selections[0])
            .unwrap();
        let other = base
            .acquire_asset::<TextBundleDomain, TextBundleDomain>(self.selections[1])
            .unwrap();
        assert_eq!(
            other.product().get(&self.selections[1]).unwrap().text,
            second
        );
        let expected = [first.to_owned(), second.to_owned()];
        drop(other);
        drop(base);
        let mut candidate = plan
            .plugin_plan()
            .instantiate_runtime_candidate(
                RuntimeAdmissionReservation::try_acquire().unwrap(),
                RuntimeObligationLedger::new(),
                RuntimeClosePolicy::default(),
            )
            .unwrap();
        let bindings = plan.schema_validation().bindings().clone();
        let admission = candidate
            .with_admission_scope(move |scope| {
                scope.apply_command(move |world: &mut World| {
                    assert!(bindings.same_definitions(scene_component_bindings(world).unwrap()));
                    world.insert_resource(Travel {
                        later: Some(later),
                        bundle,
                        expected,
                        completed: false,
                    });
                    assert!(
                        !materialize_startup_scene(world, startup.scene_source())
                            .unwrap()
                            .has_errors()
                    );
                    assert!(CurrentScene::get(world).is_none());
                    assert!(!world.contains_resource::<ProjectAssetDatabase>());
                });
            })
            .unwrap();
        assert!(admission.post_operation_fault.is_none());
        let mut runtime = candidate.complete_startup().unwrap().promote();
        assert_eq!(
            caption_text(
                runtime.world(),
                CurrentScene::get(runtime.world()).unwrap().receipt()
            ),
            first
        );
        runtime.drive(Duration::ZERO).unwrap();
        assert!(runtime.world().resource::<Travel>().completed);
        assert_eq!(
            caption_text(
                runtime.world(),
                CurrentScene::get(runtime.world()).unwrap().receipt()
            ),
            second
        );
        runtime.finish().unwrap();
    }
}

#[derive(Resource)]
struct Travel {
    later: Option<RuntimeContentScene>,
    bundle: RuntimeContentAsset<TextBundle>,
    expected: [String; 2],
    completed: bool,
}

fn caption_text(world: &World, receipt: &SpawnedSceneInstance) -> String {
    let EntityLookup::Resolved(entity) = receipt.resolve(world, &caption_id()) else {
        panic!("the managed Scene must contain the authored caption entity");
    };
    let reference = world
        .get::<Caption>(entity)
        .unwrap()
        .product
        .as_product()
        .unwrap();
    world
        .resource::<Travel>()
        .bundle
        .product()
        .get(&reference)
        .unwrap()
        .text
        .clone()
}

fn travel(world: &mut World) {
    let next = {
        let Some(mut state) = world.get_resource_mut::<Travel>() else {
            return;
        };
        let Some(next) = state.later.take() else {
            return;
        };
        next
    };
    let initial = CurrentScene::get(world).unwrap().receipt().clone();
    assert_eq!(
        caption_text(world, &initial),
        world.resource::<Travel>().expected[0]
    );
    let committed = replace_current_scene_with_product(
        world,
        &initial,
        next.scene_source(),
        SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
        &[],
        |_| {},
    );
    assert!(!committed.diagnostics.has_errors(), "{committed:?}");
    let replacement = committed.instance.unwrap();
    assert_eq!(CurrentScene::get(world).unwrap().receipt(), &replacement);
    assert_eq!(
        caption_text(world, &replacement),
        world.resource::<Travel>().expected[1]
    );
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    world.resource_mut::<Travel>().completed = true;
}
