#![cfg(all(
    feature = "project-workflow",
    feature = "runtime-2d",
    feature = "serde"
))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    fs,
    num::NonZeroU32,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use nara::{
    ProductConfiguration, ProductRecipe, Runtime2dPlugins, SchemaContribution,
    advanced_prelude::{
        CurrentScene, SceneProductTransactionLimits, StartupSceneActivationPlugin,
        materialize_startup_scene,
    },
    app::{
        AddPluginsError, App, CoreStage, Plugin, PluginCategory, PluginDeclaration,
        PluginDefinition, PluginDefinitionId, PluginError, PluginGroup, PluginGroupBuilder,
        PluginGroupId, PluginId, PluginPlan, PluginPlanError, RuntimeAdmissionReservation,
        RuntimeCandidateRetirementState, RuntimeClosePolicy, RuntimeObligationLedger,
    },
    core::ItemLimit,
    ecs::{Component, Entity, Resource, World},
    fs::RelativePath,
    identity::{EntityLookup, EntityReference, SpawnedSceneInstance},
    project::ProjectPath,
    project_host::{
        CompositionError, HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, HeadlessRunReport,
        ProjectContentLoader, ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes,
        ProjectSettingsCandidate, RuntimePlan, RuntimePlanError,
        advanced::{
            built_in_schema_providers, commit_prepared_project_scene, project_runtime_plugins,
            resolve_runtime_plan,
        },
        ingest_project_manifest, resolve_product_recipe,
    },
    project_workflow::{ProjectCheckRequest, check_project},
    reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentRegistrySnapshot, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerId,
        ComponentSchemaOwnerSnapshot, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
        preloaded_component_registry_plugin,
    },
    scene::{
        Name, PrefabDocument, PrefabInstance, SceneComponentBindingError, SceneComponentBindings,
        SceneComponentBindingsPlugin, SceneComponentRecord, SceneDataComponentError, SceneDocument,
        SceneEntityRecord, SceneEntitySource, scene_component_bindings,
    },
};
use project_content_fixture::{TestProject, scene_id};

const MOTION: &str = "test.product-data.Motion";
const SENSOR: &str = "test.product-data.Sensor";
const DATA_PLUGIN: PluginId = PluginId::new("test.product-data");
const NEXT_SCENE: &str = "second.scene.ron";

fn component(value: &str) -> ComponentTypeId {
    ComponentTypeId::new(value)
}

fn owner(value: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(value).unwrap()
}

fn record(kind: &str, value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner(kind))
}

fn named(id: &str, name: &str) -> SceneEntityRecord {
    SceneEntityRecord::new(scene_id(id)).with_component(
        component("nara.scene.Name"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String(name.into()),
        ),
    )
}

fn image_reference() -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String("path".into())),
        (
            "value",
            ComponentValue::String("textures/player.png".into()),
        ),
    ])
}

fn data_owners(gain: u64) -> Vec<ComponentSchemaOwnerSnapshot> {
    let motion = ComponentSchema::new(component(MOTION), "Motion", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("amount"),
                "Amount",
                ComponentFieldPath::from_fields(["amount"]),
                ComponentValueKind::U64,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("gain"),
                "Gain",
                ComponentFieldPath::from_fields(["gain"]),
                ComponentValueKind::U64,
                ComponentValue::U64(gain),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ]);
    let sensor = ComponentSchema::new(component(SENSOR), "Sensor", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("targets"),
                "Targets",
                ComponentFieldPath::from_fields(["targets"]),
                ComponentValueShape::list(ComponentValueShape::EntityRef),
                ComponentValue::List(vec![ComponentValue::EntityReference(
                    EntityReference::SceneLocal {
                        entity: scene_id("friend"),
                    },
                )]),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("image"),
                "Image",
                ComponentFieldPath::from_fields(["image"]),
                ComponentValueKind::AssetRef,
                image_reference(),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::AssetRef),
        ]);
    [(MOTION, motion), (SENSOR, sensor)]
        .into_iter()
        .map(|(name, schema)| {
            ComponentSchemaOwnerSnapshot::admit(
                owner(name),
                ComponentSchemaCatalog {
                    components: vec![schema],
                    ..Default::default()
                },
                None,
            )
            .unwrap()
        })
        .collect()
}

fn prefab(amount: u64) -> PrefabDocument {
    PrefabDocument::from_ordered_roots([
        named("actor", "Actor")
            .with_component(
                component(MOTION),
                record(
                    MOTION,
                    ComponentValue::map([("amount", ComponentValue::U64(amount))]),
                ),
            )
            .with_component(
                component(SENSOR),
                record(
                    SENSOR,
                    ComponentValue::map([] as [(&str, ComponentValue); 0]),
                ),
            ),
        named("friend", "Friend"),
    ])
}

fn linked(anchor: &str, path: &str) -> SceneEntityRecord {
    let mut entity = named(anchor, anchor);
    entity.prefab = Some(PrefabInstance {
        source: nara::asset::AssetRef::path(path).unwrap(),
        overrides: None,
    });
    entity
}

fn project_with_data() -> TestProject {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let manifest = fs::read_to_string(project.path().join("nara.toml")).unwrap();
    fs::write(
        project.path().join("nara.toml"),
        format!("{manifest}\n[content]\nscenes = [\"{NEXT_SCENE}\"]\n"),
    )
    .unwrap();
    project.write_scene_source(&SceneDocument::from_ordered_roots([
        linked("left", "actor.prefab.ron"),
        linked("right", "actor.prefab.ron"),
    ]));
    fs::write(
        project.path().join("prefabs/actor.prefab.ron"),
        prefab(17).to_ron_string().unwrap(),
    )
    .unwrap();
    project.write_prefab("next.prefab.json", &prefab(29));
    fs::write(
        project.path().join("scenes").join(NEXT_SCENE),
        SceneDocument::from_ordered_roots([linked("next", "next.prefab.json")])
            .to_ron_string()
            .unwrap(),
    )
    .unwrap();
    project
}

fn candidate(project: &TestProject) -> ProjectSettingsCandidate {
    let root = project.root_capability();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    ingest_project_manifest(&manifest, None).unwrap()
}

fn default_recipe(owners: &[ComponentSchemaOwnerSnapshot]) -> ProductRecipe {
    ProductRecipe::new()
        .add_contribution(
            SchemaContribution::<DataPlugin>::data_for_default(owners.iter().cloned()).unwrap(),
        )
        .unwrap()
}

fn install_bundle(project: &TestProject, recipe: ProductRecipe) -> RuntimePlan {
    install_bundle_at_root(project, recipe).1
}

fn install_bundle_at_root(
    project: &TestProject,
    recipe: ProductRecipe,
) -> (
    ProjectSettingsCandidate,
    RuntimePlan,
    nara::fs::DirectoryCapability,
) {
    let root = project.root_capability();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, recipe).unwrap();
    fs::write(
        project.path().join("schema/authoring.bundle.json"),
        plan.schema_validation()
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    (candidate, plan, root)
}

#[derive(Clone, Default, Resource)]
struct DataConfiguration {
    owners: Vec<ComponentSchemaOwnerSnapshot>,
    travel: bool,
}

impl ProductConfiguration for DataConfiguration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"product-data-workflow-v1");
        output.push(u8::from(self.travel));
    }
}

#[derive(Default)]
struct DataPlugin(DataConfiguration);

impl Plugin for DataPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration =
            PluginDeclaration::new(DATA_PLUGIN, PluginCategory::Runtime)
                .requires_plugins(&[nara::scene::SCENE_COMPONENT_BINDINGS_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(self.0.clone())?
            .init_resource::<DataOutcome>()?
            .init_resource::<Travel>()?;
        if self.0.travel {
            app.add_systems(CoreStage::Update, drive_product_workflow)?;
        }
        Ok(())
    }
}

fn runtime_recipe(owners: &[ComponentSchemaOwnerSnapshot]) -> ProductRecipe {
    let configuration = DataConfiguration {
        owners: owners.to_vec(),
        travel: true,
    };
    ProductRecipe::new()
        .add_contribution(
            SchemaContribution::<DataPlugin>::data_configured(
                configuration,
                |configuration| DataPlugin(configuration.clone()),
                owners.iter().cloned(),
            )
            .unwrap(),
        )
        .unwrap()
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Resource)]
struct DataOutcome {
    startup_actor_count: usize,
    replacement_actor_count: usize,
    retired_actor_count: usize,
    replacement_complete: bool,
}

#[derive(Default, Resource)]
struct Travel {
    request: Option<ProjectSceneRequestId>,
    prior_actors: Vec<Entity>,
    prior_instance: Option<SpawnedSceneInstance>,
}

fn inspect_actors(world: &mut World, expected_amount: u64) -> Vec<Entity> {
    let bindings = scene_component_bindings(world).unwrap().clone();
    for original in &world.resource::<DataConfiguration>().owners {
        assert!(
            bindings
                .data_owners()
                .iter()
                .any(|selected| selected.ptr_eq(original)),
            "the runtime must preserve each recipe owner admission"
        );
    }
    let instance = CurrentScene::get(world).unwrap().receipt().clone();
    let actors = world
        .query::<(Entity, &Name)>()
        .iter(world)
        .filter_map(|(entity, name)| (name.as_str() == "Actor").then_some(entity))
        .collect::<Vec<_>>();
    let foreign =
        SceneComponentBindings::new(bindings.native_snapshot().clone(), data_owners(3)).unwrap();
    let mut targets = Vec::new();
    for entity in &actors {
        assert!(matches!(
            foreign.data_component(world, *entity, &component(MOTION)),
            Err(SceneDataComponentError::BindingsMismatch)
        ));
        assert_eq!(
            bindings
                .data_components(world, *entity)
                .unwrap()
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>(),
            [component(MOTION), component(SENSOR)]
        );
        let motion = bindings
            .data_component(world, *entity, &component(MOTION))
            .unwrap()
            .unwrap();
        assert_eq!(motion.owner(), Some(&owner(MOTION)));
        assert_eq!(
            motion.value.field("amount"),
            Ok(&ComponentValue::U64(expected_amount))
        );
        assert_eq!(motion.value.field("gain"), Ok(&ComponentValue::U64(3)));
        let sensor = bindings
            .data_component(world, *entity, &component(SENSOR))
            .unwrap()
            .unwrap();
        assert_eq!(sensor.value.field("image"), Ok(&image_reference()));
        let ComponentValue::List(references) = sensor.value.field("targets").unwrap() else {
            panic!("the default target list must survive runtime materialization");
        };
        let [ComponentValue::EntityReference(EntityReference::SceneLocal { entity: target })] =
            references.as_slice()
        else {
            panic!("the default must be rebound to this Prefab occurrence");
        };
        let EntityLookup::Resolved(target) = instance.resolve(world, target) else {
            panic!("the default target must resolve in the current Scene receipt");
        };
        assert_eq!(world.get::<Name>(target).unwrap().as_str(), "Friend");
        assert!(
            !targets.contains(&target),
            "Prefab occurrences must not share targets"
        );
        targets.push(target);
    }
    actors
}

fn drive_product_workflow(world: &mut World) {
    if world.resource::<DataOutcome>().replacement_complete {
        return;
    }
    let Some(request) = world.resource::<Travel>().request else {
        let actors = inspect_actors(world, 17);
        assert_eq!(actors.len(), 2);
        let prior_instance = CurrentScene::get(world).unwrap().receipt().clone();
        let request = ProjectScenes::scope(world, |scenes| {
            scenes.request(ProjectPath::new(NEXT_SCENE).unwrap())
        })
        .unwrap();
        world.resource_mut::<DataOutcome>().startup_actor_count = actors.len();
        *world.resource_mut::<Travel>() = Travel {
            request: Some(request),
            prior_actors: actors,
            prior_instance: Some(prior_instance),
        };
        return;
    };
    match ProjectScenes::scope(world, |scenes| scenes.state(request)) {
        Some(ProjectSceneRequestState::Preparing) => std::thread::yield_now(),
        Some(ProjectSceneRequestState::Ready) => {
            let report = commit_prepared_project_scene(
                world,
                request,
                SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
                &[],
                |_| {},
            );
            assert!(!report.diagnostics.has_errors(), "{report:?}");
            let replacement = report.instance.unwrap();
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &replacement);
            assert_ne!(
                world.resource::<Travel>().prior_instance.as_ref(),
                Some(&replacement)
            );
            let retired = world.resource::<Travel>().prior_actors.clone();
            assert!(
                retired
                    .iter()
                    .all(|entity| world.get_entity(*entity).is_err())
            );
            let actors = inspect_actors(world, 29);
            assert_eq!(actors.len(), 1);
            assert!(ProjectScenes::scope(world, |scenes| scenes.acknowledge(request)));
            *world.resource_mut::<DataOutcome>() = DataOutcome {
                startup_actor_count: 2,
                replacement_actor_count: actors.len(),
                retired_actor_count: retired.len(),
                replacement_complete: true,
            };
        }
        state => panic!("the public Scene request failed: {state:?}"),
    }
}

fn finish_run(run: &mut HeadlessRun<DataOutcome>) -> HeadlessRunReport<DataOutcome> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let report = run.execute_bounded();
        if !matches!(report.outcome(), HeadlessRunOutcome::CleanupIncomplete) {
            return report;
        }
        assert!(
            Instant::now() < deadline,
            "product cleanup did not complete"
        );
        std::thread::yield_now();
    }
}

#[test]
fn public_check_capture_startup_and_replacement_preserve_two_data_owners() {
    let project = project_with_data();
    let owners = data_owners(3);
    let recipe = runtime_recipe(&owners);
    let (candidate, plan, root) = install_bundle_at_root(&project, recipe.clone());
    let source_paths = [
        "scenes/startup.scene.json",
        "scenes/second.scene.ron",
        "prefabs/actor.prefab.ron",
        "prefabs/next.prefab.json",
    ];
    let originals = source_paths.map(|path| fs::read(project.path().join(path)).unwrap());
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    assert_eq!(snapshot.expanded_scene().entities.len(), 6);
    assert_eq!(snapshot.prefabs().len(), 1);
    assert_eq!(snapshot.images().len(), 1);
    assert_eq!(
        snapshot.images()[0].path().as_str(),
        "assets/textures/player.png"
    );
    assert_eq!(
        snapshot.schema_fingerprint(),
        plan.schema_validation()
            .bindings()
            .composition_fingerprint()
    );
    let source_actor = snapshot.prefabs()[0]
        .document()
        .entities
        .iter()
        .find(|entity| entity.id == scene_id("actor"))
        .unwrap();
    assert!(
        source_actor.components[&component(MOTION)]
            .value
            .field("gain")
            .is_err()
    );
    assert!(
        source_actor.components[&component(SENSOR)]
            .value
            .field("targets")
            .is_err()
    );
    for actor in snapshot
        .expanded_scene()
        .entities
        .iter()
        .filter(|entity| entity.components.contains_key(&component(MOTION)))
    {
        assert_eq!(
            actor.components[&component(MOTION)].value.field("gain"),
            Ok(&ComponentValue::U64(3))
        );
        assert_eq!(
            actor.components[&component(SENSOR)].value.field("image"),
            Ok(&image_reference())
        );
    }
    let checked = check_project(ProjectCheckRequest::new(
        project.root_capability(),
        recipe.clone(),
    ))
    .unwrap();
    assert_eq!(checked.schema_fingerprint(), snapshot.schema_fingerprint());
    assert_eq!(
        checked.startup_content().content_digest(),
        snapshot.content_digest()
    );
    assert!(
        checked
            .members()
            .iter()
            .any(|member| { member.path().as_str() == "assets/textures/player.png" })
    );

    let intent =
        HeadlessRunIntent::frames(NonZeroU32::new(20_000).unwrap(), Duration::from_millis(1))
            .with_recipe(recipe)
            .stop_when(|outcome: &DataOutcome| outcome.replacement_complete);
    let mut run = HeadlessRun::new(project.root_capability(), intent, Vec::new());
    let report = finish_run(&mut run);
    assert_eq!(
        report.outcome(),
        &HeadlessRunOutcome::Completed(DataOutcome {
            startup_actor_count: 2,
            replacement_actor_count: 1,
            retired_actor_count: 2,
            replacement_complete: true,
        }),
        "{report:#?}"
    );
    assert!(!report.diagnostics().has_errors(), "{report:#?}");
    assert_eq!(
        report.startup_content_closure().unwrap().content_digest(),
        snapshot.content_digest()
    );
    for (path, original) in source_paths.into_iter().zip(originals) {
        assert_eq!(fs::read(project.path().join(path)).unwrap(), original);
    }
}

#[test]
fn direct_and_file_backed_recipes_keep_exact_owner_admissions() {
    let project = project_with_data();
    let owners = data_owners(3);
    let recipe = default_recipe(&owners);
    let plan = install_bundle(&project, recipe.clone());
    let mut app = App::new();
    app.add_plugins(recipe.with_defaults(Runtime2dPlugins))
        .unwrap();
    let sealed = app.seal().unwrap();
    let direct = scene_component_bindings(sealed.world()).unwrap();
    let planned = plan.schema_validation().bindings();
    for original in &owners {
        for selected in [direct, planned] {
            assert!(
                selected
                    .data_owners()
                    .iter()
                    .any(|owner| owner.ptr_eq(original))
            );
        }
    }
    assert_eq!(
        direct.composition_fingerprint(),
        planned.composition_fingerprint()
    );
    assert_eq!(
        plan.schema_validation().composition_fingerprint(),
        planned.composition_fingerprint()
    );
    let native_only = resolve_product_recipe(&candidate(&project), ProductRecipe::new()).unwrap();
    assert_eq!(
        plan.schema_validation().executable_fingerprint(),
        native_only.schema_validation().executable_fingerprint()
    );
    assert_ne!(
        plan.schema_validation().composition_fingerprint(),
        native_only.schema_validation().composition_fingerprint()
    );
    let changed =
        resolve_product_recipe(&candidate(&project), default_recipe(&data_owners(9))).unwrap();
    assert_eq!(
        plan.schema_validation().executable_fingerprint(),
        changed.schema_validation().executable_fingerprint()
    );
    assert_ne!(
        plan.schema_validation().composition_fingerprint(),
        changed.schema_validation().composition_fingerprint()
    );
    let rejected = check_project(ProjectCheckRequest::new(
        project.root_capability(),
        default_recipe(&data_owners(9)),
    ))
    .unwrap_err();
    assert!(
        rejected
            .content_error()
            .expect("the bundle must reject the divergent selected data owner")
            .diagnostics()
            .iter()
            .any(|diagnostic| {
                diagnostic.code().as_str() == "project.runtime-agreement.selected-owner-divergent"
            })
    );
}

#[test]
fn authoring_bundle_metadata_does_not_activate_unselected_data_owners() {
    let project = project_with_data();
    let owners = data_owners(3);
    install_bundle(&project, default_recipe(&owners));
    project.write_scene_source(&SceneDocument::from_ordered_roots([named(
        "plain", "Plain",
    )]));
    fs::write(
        project.path().join("scenes").join(NEXT_SCENE),
        SceneDocument::from_ordered_roots([named("other", "Other")])
            .to_ron_string()
            .unwrap(),
    )
    .unwrap();
    let root = project.root_capability();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
    assert!(plan.schema_validation().bindings().data_owners().is_empty());
    assert!(
        plan.schema_validation()
            .bindings()
            .schema(&component(MOTION))
            .is_none()
    );
    check_project(ProjectCheckRequest::new(
        project.root_capability(),
        ProductRecipe::new(),
    ))
    .unwrap();

    project.write_scene_source(&SceneDocument::from_ordered_roots([linked(
        "unselected",
        "actor.prefab.ron",
    )]));
    let loader = ProjectContentLoader::new(root).unwrap();
    let rejected = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(
        rejected.kind(),
        nara::project_host::ProjectContentErrorKind::PrefabExpansion
    );
    assert!(rejected.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.unknown-component"
            && diagnostic.fields().iter().any(|field| {
                field.key().as_str() == "component-id" && field.display_value().as_ref() == MOTION
            })
    }));
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    assert!(
        check_project(ProjectCheckRequest::new(
            project.root_capability(),
            ProductRecipe::new(),
        ))
        .is_err()
    );
}

struct SharedNativeDefaults(ComponentRegistrySnapshot);

impl PluginGroup for SharedNativeDefaults {
    const ID: PluginGroupId = PluginGroupId::new("test.product-data.shared-native");

    fn build(self) -> PluginGroupBuilder {
        let snapshot = Arc::new(OnceLock::new());
        assert!(snapshot.set(self.0).is_ok());
        PluginGroupBuilder::new().add_edited_group(
            Runtime2dPlugins
                .edit()
                .configure(preloaded_component_registry_plugin(snapshot)),
        )
    }
}

#[test]
fn captured_project_source_rejects_readmitted_data_with_the_same_native_snapshot() {
    let project = project_with_data();
    let owners = data_owners(3);
    let (candidate, plan, root) = install_bundle_at_root(&project, default_recipe(&owners));
    let snapshot = ProjectContentLoader::new(root)
        .unwrap()
        .load(&candidate, &plan)
        .unwrap();
    let readmitted = data_owners(3);
    assert!(owners.iter().zip(&readmitted).all(|(left, right)| {
        left.current().fingerprint() == right.current().fingerprint() && !left.ptr_eq(right)
    }));
    for (selected_owners, matches_source) in [(owners, true), (readmitted, false)] {
        let mut app = App::new();
        app.add_plugins(
            default_recipe(&selected_owners)
                .add_plugin::<StartupSceneActivationPlugin>()
                .unwrap()
                .with_defaults(SharedNativeDefaults(
                    plan.schema_validation().snapshot().clone(),
                )),
        )
        .unwrap();
        let sealed = app.seal().unwrap();
        let selected = scene_component_bindings(sealed.world()).unwrap();
        assert!(
            selected
                .native_snapshot()
                .ptr_eq(plan.schema_validation().snapshot())
        );
        assert_eq!(
            selected.composition_fingerprint(),
            snapshot.schema_fingerprint()
        );
        assert_eq!(
            selected.same_definitions(plan.schema_validation().bindings()),
            matches_source
        );
        assert_eq!(
            plan.schema_validation()
                .bindings()
                .witness()
                .matches(selected),
            matches_source
        );
        let mut candidate = RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(
                sealed,
                RuntimeObligationLedger::new(),
                RuntimeClosePolicy::default(),
            )
            .unwrap();
        let result = Arc::new(Mutex::new(None));
        let observation = Arc::clone(&result);
        let source = snapshot.scene_source();
        let scope = candidate
            .with_admission_scope(move |scope| {
                scope.apply_command(move |world: &mut World| {
                    let entities_before = world.entities().len();
                    let resources_before = world.iter_resources().count();
                    assert!(CurrentScene::get(world).is_none());
                    let result = materialize_startup_scene(world, source);
                    let live_data = world.query::<&SceneEntitySource>().iter(world).count();
                    if !matches_source {
                        assert_eq!(world.entities().len(), entities_before);
                        assert_eq!(world.iter_resources().count(), resources_before);
                        assert!(CurrentScene::get(world).is_none());
                    }
                    *observation.lock().unwrap() = Some((result, live_data));
                });
            })
            .unwrap();
        assert!(scope.post_operation_fault.is_none());
        let (materialization, live_data) = result.lock().unwrap().take().unwrap();
        // Completing startup makes any accidentally published pending activation observable.
        let runtime = candidate.complete_startup().unwrap().promote();
        let activated = CurrentScene::get(runtime.world()).is_some();
        let mut retirement = runtime.begin_retirement();
        let deadline = Instant::now() + Duration::from_secs(10);
        while retirement.drive_retirement() != RuntimeCandidateRetirementState::Retired {
            assert!(
                Instant::now() < deadline,
                "candidate cleanup did not complete"
            );
            std::thread::yield_now();
        }
        assert_eq!(activated, matches_source);
        if matches_source {
            assert!(materialization.is_ok(), "{materialization:?}");
            assert_eq!(live_data, 6);
        } else {
            let rejection = materialization.expect_err(
                "a content source must preserve exact data-owner admission through startup",
            );
            assert_eq!(
                rejection
                    .diagnostics()
                    .iter()
                    .map(|diagnostic| diagnostic.code().as_str())
                    .collect::<Vec<_>>(),
                ["startup.scene.component-bindings-mismatch"]
            );
            assert_eq!(
                live_data, 0,
                "rejection must precede Scene entity publication"
            );
        }
    }
}

const RESERVED_OWNER: ComponentSchemaOwnerId =
    ComponentSchemaOwnerId::new("test.product-data.native");
const RESERVED_TYPE: &str = "test.product-data.NativeReserved";
const RETIRED_TYPE: &str = "test.product-data.NativeRetired";

fn reserved_provider() -> ComponentSchemaProviderDefinition {
    ComponentSchemaProviderDefinition::new(
        RESERVED_OWNER,
        nara::app::PluginSchemaProviderId::new("test.product-data.native-schema"),
        ComponentSchemaProviderBindingId::new("test.product-data.native-bindings", 1),
        || {
            Ok(ComponentSchemaCatalog {
                components: vec![ComponentSchema::new(
                    component(RESERVED_TYPE),
                    "Native Reserved",
                    ComponentSchemaVersion::ONE,
                )],
                type_tombstones: vec![component(RETIRED_TYPE)],
                ..Default::default()
            })
        },
        |_| panic!("an inactive native provider must not register executable bindings"),
    )
}

#[test]
fn inactive_compiled_native_owner_type_and_tombstones_remain_reserved() {
    let project = project_with_data();
    let candidate = candidate(&project);
    let good = resolve_runtime_plan(
        &candidate,
        project_runtime_plugins(&candidate).with_recipe(default_recipe(&data_owners(3))),
        built_in_schema_providers()
            .into_iter()
            .chain([reserved_provider()]),
    )
    .unwrap();
    assert!(
        good.schema_validation()
            .bindings()
            .schema(&component(RESERVED_TYPE))
            .is_none()
    );
    for (owner_id, schema_id, tombstone, expected) in [
        (
            RESERVED_OWNER.as_str(),
            "test.product-data.Other",
            false,
            SceneComponentBindingError::DuplicateOwner {
                owner: owner(RESERVED_OWNER.as_str()),
            },
        ),
        (
            "test.product-data.foreign",
            RESERVED_TYPE,
            false,
            SceneComponentBindingError::DivergentComponentOwnership {
                component: component(RESERVED_TYPE),
            },
        ),
        (
            "test.product-data.foreign",
            RETIRED_TYPE,
            false,
            SceneComponentBindingError::DivergentComponentOwnership {
                component: component(RETIRED_TYPE),
            },
        ),
        (
            "test.product-data.foreign",
            RESERVED_TYPE,
            true,
            SceneComponentBindingError::DivergentComponentOwnership {
                component: component(RESERVED_TYPE),
            },
        ),
    ] {
        let catalog = if tombstone {
            ComponentSchemaCatalog {
                type_tombstones: vec![component(schema_id)],
                ..Default::default()
            }
        } else {
            ComponentSchemaCatalog {
                components: vec![ComponentSchema::new(
                    component(schema_id),
                    "Conflicting Data",
                    ComponentSchemaVersion::ONE,
                )],
                ..Default::default()
            }
        };
        let foreign = ComponentSchemaOwnerSnapshot::admit(owner(owner_id), catalog, None).unwrap();
        let error = resolve_runtime_plan(
            &candidate,
            project_runtime_plugins(&candidate).with_recipe(default_recipe(&[foreign])),
            built_in_schema_providers()
                .into_iter()
                .chain([reserved_provider()]),
        )
        .unwrap_err();
        assert_eq!(
            error,
            RuntimePlanError::Composition(CompositionError::SceneBindingsRejected {
                source: Box::new(expected),
            })
        );
    }
}

#[test]
fn direct_full_plan_rejects_removed_or_replaced_data_contributing_plugin() {
    let owners = data_owners(3);
    let recipe = default_recipe(&owners);
    let replacement = PluginDefinition::infallible::<DataPlugin, _>(
        PluginDefinitionId::new("test.product-data.unrelated-definition", 1),
        b"same-plugin-id-without-the-contribution",
        DataPlugin::default,
    );
    let mut app = App::new();
    let removed = app
        .add_plugins(
            recipe
                .clone()
                .with_defaults(Runtime2dPlugins)
                .edit()
                .disable::<DataPlugin>(),
        )
        .err()
        .expect("a required recipe contribution cannot be removed");
    // Ordinary recipe entries have no optional slot, so removal already fails in plan resolution.
    assert!(matches!(
        removed,
        AddPluginsError::Plan(PluginPlanError::RequiredSlotDisabled { .. })
    ));
    let mut app = App::new();
    let error = app
        .add_plugins(
            recipe
                .with_defaults(Runtime2dPlugins)
                .edit()
                .configure(replacement),
        )
        .err()
        .expect("an unrelated plugin definition cannot retain a data contribution");
    let failure = error
        .plugin_failure()
        .expect("the full plan must fail in preflight");
    assert!(matches!(
        failure.error(),
        PluginError::CommittedPreflightRejected {
            plugin,
            source,
        } if *plugin == PluginId::new("nara.product-recipe.schema-authority")
            && matches!(source.as_ref(), PluginError::SetupFailed { .. })
    ));
    assert!(scene_component_bindings(app.world()).is_none());
}

#[test]
fn direct_full_plan_rejects_outer_scene_selection_configuration() {
    let owners = data_owners(3);
    for recipe in [default_recipe(&owners), ProductRecipe::new()] {
        let original = PluginPlan::resolve(recipe.clone().with_defaults(Runtime2dPlugins)).unwrap();
        let contributor = original
            .entries()
            .iter()
            .find(|entry| entry.plugin_id() == DATA_PLUGIN)
            .map(|entry| entry.definition_key());
        for replacement_owners in [Vec::new(), data_owners(9)] {
            let replacement = PluginDefinition::infallible::<SceneComponentBindingsPlugin, _>(
                PluginDefinitionId::new("test.product-data.outer-scene-selection", 1),
                b"outer-selection",
                move || SceneComponentBindingsPlugin::new(replacement_owners.iter().cloned()),
            );
            let edited = PluginPlan::resolve(
                recipe
                    .clone()
                    .with_defaults(Runtime2dPlugins)
                    .edit()
                    .configure(replacement.clone()),
            )
            .unwrap();
            assert_eq!(
                edited
                    .entries()
                    .iter()
                    .find(|entry| entry.plugin_id() == DATA_PLUGIN)
                    .map(|entry| entry.definition_key()),
                contributor,
                "the contributing plugin must remain untouched by the outer Scene edit"
            );
            let mut app = App::new();
            let error = app
                .add_plugins(
                    recipe
                        .clone()
                        .with_defaults(Runtime2dPlugins)
                        .edit()
                        .configure(replacement),
                )
                .err()
                .expect("the recipe guard must reject an outer Scene selection in preflight");
            let failure = error.plugin_failure().unwrap();
            assert!(
                matches!(
                    failure.error(),
                    PluginError::CommittedPreflightRejected {
                        plugin,
                        source,
                    } if *plugin == PluginId::new("nara.product-recipe.schema-authority")
                        && matches!(source.as_ref(), PluginError::SetupFailed { .. })
                ),
                "unexpected outer Scene selection failure: {failure:?}"
            );
            assert!(scene_component_bindings(app.world()).is_none());
            assert!(!app.world().contains_resource::<DataConfiguration>());
        }
    }
}

#[test]
fn direct_recipe_guard_rejects_readmission_under_the_same_scene_definition_key() {
    let original = data_owners(3).into_iter().next().unwrap();
    let readmitted = ComponentSchemaOwnerSnapshot::admit(
        original.owner_id().clone(),
        original.current().clone(),
        None,
    )
    .unwrap();
    assert_eq!(original.current(), readmitted.current());
    assert!(!original.ptr_eq(&readmitted));
    let recipe = default_recipe(std::slice::from_ref(&original));
    let plan = PluginPlan::resolve(recipe.clone().with_defaults(Runtime2dPlugins)).unwrap();

    // Deliberately claim the generated definition's exact configuration while substituting a
    // separately admitted equivalent owner. Preflight identity alone cannot detect this factory.
    let mut configuration = Vec::from(b"nara.product-recipe.scene-definitions.v1\0".as_slice());
    let owner_id = original.owner_id().as_str();
    configuration.extend_from_slice(&(owner_id.len() as u64).to_le_bytes());
    configuration.extend_from_slice(owner_id.as_bytes());
    configuration.extend_from_slice(&original.current().generation().to_le_bytes());
    let fingerprint = original.current().fingerprint().to_hex();
    configuration.extend_from_slice(&(fingerprint.len() as u64).to_le_bytes());
    configuration.extend_from_slice(fingerprint.as_bytes());
    configuration.push(0);
    let replacement = PluginDefinition::infallible::<SceneComponentBindingsPlugin, _>(
        PluginDefinitionId::new("nara.product-recipe.scene-definitions", 1),
        configuration,
        move || SceneComponentBindingsPlugin::new([readmitted.clone()]),
    );
    assert_eq!(
        replacement.key(),
        plan.entries()
            .iter()
            .find(|entry| entry.plugin_id() == nara::scene::SCENE_COMPONENT_BINDINGS_PLUGIN_ID)
            .unwrap()
            .definition_key(),
        "the counterexample must pass the definition-key comparison"
    );
    let mut app = App::new();
    app.add_plugins(
        recipe
            .with_defaults(Runtime2dPlugins)
            .edit()
            .configure(replacement),
    )
    .expect("the identical definition key must reach finish-time admission");
    let rejection = app
        .seal()
        .err()
        .expect("equivalent metadata cannot replace the recipe's exact owner admission");
    assert!(matches!(
        rejection.error(),
        PluginError::SetupFailed { plugin, .. }
            if *plugin == PluginId::new("nara.product-recipe.schema-authority")
    ));
}

#[test]
fn generated_scene_plan_identity_includes_the_predecessors_own_lineage_link() {
    let schema = data_owners(3)[0].current().components()[0].clone();
    let branches = [
        ComponentSchemaCatalog::default(),
        ComponentSchemaCatalog {
            components: vec![schema.clone()],
            ..Default::default()
        },
    ]
    .map(|ancestor| {
        let mut previous = ComponentSchemaCatalog::successor_of(&ancestor).unwrap();
        previous.components = vec![schema.clone()];
        let previous =
            ComponentSchemaOwnerSnapshot::admit(owner(MOTION), previous, Some(ancestor)).unwrap();
        let mut current = ComponentSchemaCatalog::successor_of(previous.current()).unwrap();
        current.components = vec![schema.clone()];
        ComponentSchemaOwnerSnapshot::admit(
            owner(MOTION),
            current,
            Some(previous.current().clone()),
        )
        .unwrap()
    });
    let [left, right] = &branches;
    assert_eq!(left.current().generation(), 3);
    assert_eq!(left.current(), right.current());
    let left_previous = left.predecessor().unwrap();
    let right_previous = right.predecessor().unwrap();
    assert_eq!(left_previous.generation(), 2);
    assert_eq!(left_previous.generation(), right_previous.generation());
    assert_eq!(left_previous.fingerprint(), right_previous.fingerprint());
    assert_ne!(left_previous.predecessor(), right_previous.predecessor());

    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let left_recipe = default_recipe(std::slice::from_ref(left));
    let right_recipe = default_recipe(std::slice::from_ref(right));
    let left_plan = install_bundle(&project, left_recipe.clone());
    let right_plan = resolve_product_recipe(&candidate(&project), right_recipe.clone()).unwrap();
    assert_eq!(
        left_plan.schema_validation().composition_fingerprint(),
        right_plan.schema_validation().composition_fingerprint(),
        "semantic identity deliberately excludes historical lineage links"
    );
    assert_eq!(
        left_plan.schema_validation().executable_fingerprint(),
        right_plan.schema_validation().executable_fingerprint()
    );
    let scene_key = |plan: &RuntimePlan| {
        plan.plugin_plan()
            .entries()
            .iter()
            .find(|entry| entry.plugin_id() == nara::scene::SCENE_COMPONENT_BINDINGS_PLUGIN_ID)
            .unwrap()
            .definition_key()
    };
    assert_ne!(scene_key(&left_plan), scene_key(&right_plan));
    assert_ne!(
        left_plan.plugin_plan().fingerprint(),
        right_plan.plugin_plan().fingerprint(),
        "different bundle admission behavior must have different generated plan identity"
    );
    check_project(ProjectCheckRequest::new(
        project.root_capability(),
        left_recipe,
    ))
    .unwrap();
    let rejection = check_project(ProjectCheckRequest::new(
        project.root_capability(),
        right_recipe,
    ))
    .unwrap_err();
    assert!(
        rejection
            .content_error()
            .expect("the selected predecessor must agree with the stored authoring bundle")
            .diagnostics()
            .iter()
            .any(|diagnostic| {
                diagnostic.code().as_str() == "project.runtime-agreement.selected-owner-divergent"
            })
    );
}

const NATIVE_VALUE: &str = "test.product-data.NativeValue";
const NATIVE_PROVIDER_ID: nara::app::PluginSchemaProviderId =
    nara::app::PluginSchemaProviderId::new("test.product-data.native-value-schema");

#[derive(Component)]
struct NativeValue(ComponentValue);

#[derive(Default)]
struct NativeAndDataPlugin;

impl Plugin for NativeAndDataPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("test.product-data.native-and-data"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[NATIVE_PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn native_schema() -> ComponentSchema {
    ComponentSchema::new(
        component(NATIVE_VALUE),
        "Native Value",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("enabled"),
        "Enabled",
        ComponentFieldPath::from_fields(["enabled"]),
        ComponentValueKind::Bool,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
}

#[test]
fn one_native_contribution_can_own_data_admissions_without_data_codecs() {
    let owners = data_owners(3);
    let provider = ComponentSchemaProviderDefinition::new(
        ComponentSchemaOwnerId::new("test.product-data.native-value"),
        NATIVE_PROVIDER_ID,
        ComponentSchemaProviderBindingId::new("test.product-data.native-value-bindings", 1),
        || {
            Ok(ComponentSchemaCatalog {
                components: vec![native_schema()],
                ..Default::default()
            })
        },
        |registry| {
            registry
                .register_persistent_component_with_codec::<NativeValue, _, _>(
                    native_schema(),
                    |value| Ok(NativeValue(value.clone())),
                    |value| Ok(value.0.clone()),
                )
                .map(|_| ())
        },
    );
    let recipe = ProductRecipe::new()
        .add_contribution(
            SchemaContribution::<NativeAndDataPlugin>::for_default([provider])
                .unwrap()
                .with_data_owners(owners.iter().cloned())
                .unwrap(),
        )
        .unwrap();
    let project = project_with_data();
    let mut document = prefab(17);
    document.entities[0].components.insert(
        component(NATIVE_VALUE),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("enabled", ComponentValue::Bool(true))]),
        ),
    );
    fs::write(
        project.path().join("prefabs/actor.prefab.ron"),
        document.to_ron_string().unwrap(),
    )
    .unwrap();
    let (candidate, plan, root) = install_bundle_at_root(&project, recipe.clone());
    assert!(
        plan.schema_validation()
            .provider_ids()
            .contains(&NATIVE_PROVIDER_ID)
    );
    assert!(
        plan.schema_validation()
            .registry()
            .schema_for_type::<NativeValue>()
            .is_some()
    );
    for original in &owners {
        assert!(
            plan.schema_validation()
                .bindings()
                .data_owners()
                .iter()
                .any(|selected| selected.ptr_eq(original))
        );
    }
    let snapshot = ProjectContentLoader::new(root)
        .unwrap()
        .load(&candidate, &plan)
        .unwrap();
    assert_eq!(
        snapshot
            .expanded_scene()
            .entities
            .iter()
            .filter(|entity| {
                [NATIVE_VALUE, MOTION, SENSOR]
                    .iter()
                    .all(|name| entity.components.contains_key(&component(name)))
            })
            .count(),
        2
    );
    check_project(ProjectCheckRequest::new(project.root_capability(), recipe)).unwrap();
}
