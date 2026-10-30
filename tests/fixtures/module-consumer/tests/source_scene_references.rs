//! Independent source Scene dependencies and native Door travel through public product APIs.
//! The fixture writes authored documents and real sidecars; the production loader compiles them.

use std::{
    collections::BTreeSet,
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use nara_engine::{
    ProductConfiguration, ProductRecipe, SchemaContribution,
    advanced_prelude::{CurrentScene, SceneSourceView, add_startup_scene_activation_systems},
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetRef, AssetSourceKind,
        AssetSourcePath, ImportedProductId, StableAssetId,
    },
    core::ItemLimit,
    ecs::{Component, Entity, Resource, World},
    fs::{ContentDigest, DirectoryCapability, RelativePath},
    identity::{EntityLookup, SpawnedSceneInstance},
    project::ProjectPath,
    project_host::{
        HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, ProjectContentBudgetKind,
        ProjectContentBudgetSnapshot, ProjectContentErrorKind, ProjectContentLimits,
        ProjectContentLoader, ProjectContentMemberKind, ProjectContentSnapshot,
        ProjectSceneRequestError, ProjectSceneRequestId, ProjectSceneRequestState,
        ProjectSceneSelection, ProjectScenes, ProjectSettingsCandidate, RuntimePlan,
        advanced::commit_prepared_project_scene, ingest_project_manifest,
        open_explicit_product_project_root, resolve_product_recipe,
    },
    reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerId,
        ComponentSchemaOwnerSnapshot, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueShape, PersistentComponent, PersistentComponentProvider,
        PersistentComponentSchemaOwnerId, decode_asset_reference_value,
        encode_asset_reference_value,
    },
    scene::{
        PrefabComponentIntent, PrefabComponentSetMode, PrefabDocument, PrefabEntityTarget,
        PrefabInstance, PrefabOverlay, SceneComponentRecord, SceneDocument, SceneEntityId,
        SceneEntityRecord, SceneEntitySource, SourceDocumentKey, SourceDocumentKind,
        advanced::SceneProductTransactionLimits,
    },
};

const A: u64 = 1;
const B: u64 = 2;
const C: u64 = 3;
const D: u64 = 4;
const ASSETS: &str = "Content/Art";
const SCENES: &str = "Content/Levels";
const PREFABS: &str = "Content/Blueprints";
const DOOR: &str = "consumer.source-scene.DoorTarget";
const DEFAULT_TARGET: &str = "consumer.source-scene.DefaultTarget";
const DATA_OWNER: &str = "consumer.source-scene.defaults";

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.source-scene.DoorTarget",
    version = 1,
    alias = "Door target",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct DoorTarget {
    #[nara(
        id = "target",
        alias = "Target Scene",
        capabilities(scene, inspect, edit, asset_ref)
    )]
    target: AssetRef,
}

nara_engine::persistent_components! { mod components = [DoorTarget]; }

const PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("consumer.source-scene.schema");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.source-scene"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.source-scene.native", 1),
    components::initial_catalog,
    components::register,
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Fault {
    Malformed,
    MissingSource,
    MissingMetadata,
    WrongKind,
    Auxiliary,
}

impl Fault {
    const ALL: [Self; 5] = [
        Self::Malformed,
        Self::MissingSource,
        Self::MissingMetadata,
        Self::WrongKind,
        Self::Auxiliary,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Resource)]
enum Case {
    Observe,
    RoundTrip,
    DependencyOnly,
    Reject(Fault),
}

impl ProductConfiguration for Case {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        match self {
            Self::Observe => output.push(0),
            Self::RoundTrip => output.push(1),
            Self::DependencyOnly => output.push(2),
            Self::Reject(fault) => output.extend_from_slice(&[3, *fault as u8]),
        }
    }
}

struct DoorProbePlugin(Case);

impl Plugin for DoorProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.source-scene"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara_engine::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(self.0)?.init_resource::<Outcome>()?;
        add_startup_scene_activation_systems(app, inspect_startup)?;
        app.add_systems(CoreStage::Update, travel)?;
        Ok(())
    }
}

fn recipe(case: Case) -> ProductRecipe {
    let defaults = ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new(DATA_OWNER).unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new(DEFAULT_TARGET),
                    "Default Scene target",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::optional_with_default(
                    ComponentFieldId::new("target"),
                    "Target Scene",
                    ComponentFieldPath::from_fields(["target"]),
                    ComponentValueShape::AssetRef,
                    encode_asset_reference_value(&AssetRef::Product(product(B))),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_capability(ComponentCapability::AssetRef)]),
            ],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    ProductRecipe::new()
        .add_contribution(
            SchemaContribution::<DoorProbePlugin>::configured(
                case,
                |case| DoorProbePlugin(*case),
                [PROVIDER],
            )
            .unwrap()
            .with_data_owners([defaults])
            .unwrap(),
        )
        .unwrap()
}

struct Fixture {
    recipe: ProductRecipe,
    candidate: ProjectSettingsCandidate,
    plan: RuntimePlan,
    loader: ProjectContentLoader,
    root_capability: DirectoryCapability,
    directory: tempfile::TempDir,
}

impl Fixture {
    fn new(case: Case, graph: &[(u64, &[u64])], declared: &[u64]) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        for path in [ASSETS, SCENES, PREFABS, "schema"] {
            fs::create_dir_all(root.join(path)).unwrap();
        }
        let declared: Vec<_> = declared.iter().map(|scene| scene_name(*scene)).collect();
        let manifest = format!(
            concat!(
                "schema_version = 1\n",
                "[project]\nname = 'Source Scene consumer'\n",
                "source_universe = '742113f3-6d10-429a-bb0e-c80115e4c60f'\n",
                "native_document_identity = 'stable_metadata'\n",
                "[paths]\nassets = '{assets}'\nscenes = '{scenes}'\nprefabs = '{prefabs}'\n",
                "[startup]\ndefault_scene = '{startup}'\n",
                "[content]\nscenes = {declared}\n",
                "[authoring]\nschema_bundle = 'schema/authoring.bundle.json'\n",
                "[runtime]\npreset = 'local-headless'\n",
                "[capabilities]\nrequested = ['runtime-2d']\n",
                "[tasks.io]\nworkers = 1\npending_capacity = 16\n",
                "[tasks.compute]\nworkers = 1\npending_capacity = 16\n",
                "[tasks.async_compute]\nworkers = 1\npending_capacity = 16\n",
            ),
            assets = ASSETS,
            scenes = SCENES,
            prefabs = PREFABS,
            startup = scene_name(A),
            declared = serde_json::to_string(&declared).unwrap(),
        );
        fs::write(root.join("nara.toml"), manifest).unwrap();
        let capability = open_explicit_product_project_root(root).unwrap();
        let manifest = capability
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        drop(manifest);
        let recipe = recipe(case);
        let plan = resolve_product_recipe(&candidate, recipe.clone()).unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let fixture = Self {
            recipe,
            candidate,
            plan,
            loader: ProjectContentLoader::new(capability.share()).unwrap(),
            root_capability: capability,
            directory,
        };
        for (scene, targets) in graph {
            fixture.write_scene(*scene, &document(*scene, targets));
            fixture.write_scene_metadata(*scene, AssetSourceKind::Scene);
        }
        fixture
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn capability(&self) -> DirectoryCapability {
        self.root_capability.share()
    }

    fn scene_path(&self, scene: u64) -> PathBuf {
        self.root().join(SCENES).join(scene_name(scene))
    }

    fn metadata_path(&self, scene: u64) -> PathBuf {
        self.root()
            .join(SCENES)
            .join(format!("{}.meta", scene_name(scene)))
    }

    fn write_scene(&self, scene: u64, document: &SceneDocument) {
        fs::write(self.scene_path(scene), document.to_json_string().unwrap()).unwrap();
    }

    fn write_scene_metadata(&self, scene: u64, kind: AssetSourceKind) {
        fs::write(
            self.metadata_path(scene),
            AssetMeta::new(
                AssetMetaIdentity::new(
                    product(scene).source(),
                    AssetPath::new(scene_name(scene)).unwrap(),
                    kind,
                ),
                (),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
    }

    fn corrupt(&self, scene: u64, fault: Fault) {
        match fault {
            Fault::Malformed => fs::write(self.scene_path(scene), b"{invalid Scene").unwrap(),
            Fault::MissingSource => fs::remove_file(self.scene_path(scene)).unwrap(),
            Fault::MissingMetadata => fs::remove_file(self.metadata_path(scene)).unwrap(),
            Fault::WrongKind => self.write_scene_metadata(scene, AssetSourceKind::Prefab),
            Fault::Auxiliary => {}
        }
    }

    fn load(&self) -> ProjectContentSnapshot {
        self.loader.load(&self.candidate, &self.plan).unwrap()
    }
}

fn scene_name(scene: u64) -> String {
    format!("{scene}.scene.json")
}

fn canonical_scene_path(scene: u64) -> AssetSourcePath {
    AssetSourcePath::new(format!("{SCENES}/{}", scene_name(scene))).unwrap()
}

fn product(scene: u64) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(format!("74210000-0000-4000-8000-{scene:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn auxiliary(scene: u64) -> AssetProductRef {
    AssetProductRef::new(
        product(scene).source(),
        ImportedProductId::parse_subproduct("74210000-0000-4000-9000-000000000001").unwrap(),
    )
    .unwrap()
}

fn source_key(scene: u64) -> SourceDocumentKey {
    SourceDocumentKey::stable_asset(product(scene).source(), SourceDocumentKind::Scene)
}

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn marker(scene: u64) -> SceneEntityId {
    id(&format!("level-{scene}"))
}

fn door_id(scene: u64, index: usize) -> SceneEntityId {
    id(&format!("door-{scene}-{index}"))
}

fn door_record(target: AssetProductRef) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        DoorTarget {
            target: AssetRef::Product(target),
        }
        .__encode_persistent_component()
        .unwrap(),
    )
}

fn document(scene: u64, targets: &[u64]) -> SceneDocument {
    SceneDocument::from_ordered_roots(
        std::iter::once(SceneEntityRecord::new(marker(scene))).chain(
            targets.iter().enumerate().map(|(index, target)| {
                SceneEntityRecord::new(door_id(scene, index))
                    .with_component(ComponentTypeId::new(DOOR), door_record(product(*target)))
            }),
        ),
    )
}

fn assert_flat_dependencies(
    fixture: &Fixture,
    snapshot: &ProjectContentSnapshot,
    graph: &[(u64, &[u64])],
    expected: &[u64],
) {
    assert_eq!(snapshot.selected_scene_path().as_str(), scene_name(A));
    assert_eq!(snapshot.selected_scene_product(), Some(product(A)));
    assert_eq!(snapshot.selected_scene_source_key(), &source_key(A));
    let dependencies = snapshot.scene_dependencies();
    assert_eq!(dependencies.len(), expected.len());
    assert_eq!(
        dependencies
            .iter()
            .map(|scene| scene.path().clone())
            .collect::<BTreeSet<_>>(),
        expected
            .iter()
            .map(|scene| canonical_scene_path(*scene))
            .collect::<BTreeSet<_>>(),
    );
    let closure = snapshot.content_closure();
    let members = closure.members();
    assert_eq!(
        members
            .iter()
            .filter(|member| member.kind() == ProjectContentMemberKind::Scene)
            .count(),
        expected.len() + 1,
    );
    for scene in std::iter::once(A).chain(expected.iter().copied()) {
        let source_path = canonical_scene_path(scene);
        for (kind, path) in [
            (
                ProjectContentMemberKind::Scene,
                source_path.as_str().to_owned(),
            ),
            (
                ProjectContentMemberKind::AssetMetadata,
                format!("{}.meta", source_path.as_str()),
            ),
        ] {
            let matching: Vec<_> = members
                .iter()
                .filter(|member| member.kind() == kind && member.path().as_str() == path)
                .collect();
            assert_eq!(matching.len(), 1, "{kind:?}: {path}");
            assert_eq!(
                matching[0].digest(),
                ContentDigest::of_bytes(&fs::read(fixture.root().join(path)).unwrap()),
            );
        }
        if scene != A {
            let dependent = dependencies
                .iter()
                .find(|data| data.path() == &source_path)
                .unwrap();
            assert_eq!(dependent.source_key(), &source_key(scene));
            let targets = graph.iter().find(|(number, _)| *number == scene).unwrap().1;
            assert_eq!(dependent.document(), &document(scene, targets));
            assert_eq!(dependent.expanded_scene(), dependent.document());
        }
    }
}

fn assert_graph(graph: &[(u64, &[u64])], expected: &[u64]) {
    let fixture = Fixture::new(Case::Observe, graph, &[]);
    let snapshot = fixture.load();
    assert_flat_dependencies(&fixture, &snapshot, graph, expected);
    let outcome = execute(&fixture);
    assert_eq!(
        outcome.startup_document.as_ref(),
        Some(snapshot.expanded_scene())
    );
    assert_eq!(
        outcome.startup_entities,
        snapshot.expanded_scene().entities.len()
    );
    assert_eq!(outcome.commits, 0);
    assert_eq!(outcome.receipts.len(), 1);
    drop(snapshot);
    assert_idle(&fixture.loader);
}

#[test]
fn source_self_reference_is_finite_and_instantiates_only_the_root() {
    assert_graph(&[(A, &[A])], &[]);
}

#[test]
fn source_scene_cycle_is_flat_and_instantiates_only_the_root() {
    assert_graph(&[(A, &[B]), (B, &[A])], &[B]);
    let fixture = Fixture::new(Case::Observe, &[(A, &[B]), (B, &[A])], &[]);
    let selected = fixture
        .loader
        .load_scene(
            &fixture.candidate,
            &fixture.plan,
            &ProjectPath::new(scene_name(B)).unwrap(),
        )
        .unwrap();
    assert_eq!(selected.selected_scene_product(), Some(product(B)));
    assert_eq!(selected.selected_scene_path().as_str(), scene_name(B));
    assert_eq!(selected.selected_scene_source_key(), &source_key(B));
    assert!(selected.startup_content_closure().is_none());
    assert_eq!(selected.scene(), &document(B, &[A]));
    assert_eq!(selected.scene_dependencies().len(), 1);
    assert_eq!(
        selected.scene_dependencies()[0].source_key(),
        &source_key(A)
    );
    assert_eq!(
        selected.scene_dependencies()[0].document(),
        &document(A, &[B])
    );
    assert_eq!(
        fixture
            .candidate
            .settings()
            .startup
            .default_scene
            .as_ref()
            .unwrap()
            .as_str(),
        scene_name(A),
    );
    drop(selected);
    assert_idle(&fixture.loader);
}

#[test]
fn source_scene_diamond_retains_each_dependency_once_and_instantiates_only_the_root() {
    assert_graph(&[(A, &[B, C]), (B, &[D]), (C, &[D]), (D, &[])], &[B, C, D]);
}

#[test]
fn omitted_schema_default_introduces_a_required_scene_dependency() {
    let fixture = Fixture::new(Case::Observe, &[(A, &[]), (B, &[])], &[]);
    let component = ComponentTypeId::new(DEFAULT_TARGET);
    let mut authored = document(A, &[]);
    authored.entities[0].components.insert(
        component.clone(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map(std::iter::empty::<(&str, ComponentValue)>()),
        )
        .with_owner(PersistentComponentSchemaOwnerId::try_new(DATA_OWNER).unwrap()),
    );
    fixture.write_scene(A, &authored);
    let snapshot = fixture.load();
    assert_eq!(snapshot.scene(), &authored);
    assert!(
        snapshot.scene().entities[0].components[&component]
            .value
            .get("target")
            .is_none()
    );
    let effective = &snapshot.expanded_scene().entities[0].components[&component].value;
    assert_eq!(
        decode_asset_reference_value(effective.get("target").unwrap()).unwrap(),
        AssetRef::Product(product(B)),
    );
    assert_eq!(snapshot.scene_dependencies().len(), 1);
    assert_eq!(
        snapshot.scene_dependencies()[0].source_key(),
        &source_key(B)
    );
    assert_eq!(
        snapshot.scene_dependencies()[0].expanded_scene(),
        &document(B, &[])
    );
    assert!(snapshot.content_closure().members().iter().any(|member| {
        member.kind() == ProjectContentMemberKind::Scene
            && member.path().as_str() == canonical_scene_path(B).as_str()
    }));
    let outcome = execute(&fixture);
    assert_eq!(
        outcome.startup_document.as_ref(),
        Some(snapshot.expanded_scene())
    );
    drop(snapshot);
    fixture.corrupt(B, Fault::MissingSource);
    let failure = fixture
        .loader
        .load(&fixture.candidate, &fixture.plan)
        .unwrap_err();
    assert!(failure.diagnostics().has_errors());
    assert_idle(&fixture.loader);
}

#[test]
fn prefab_overlay_collects_only_the_effective_door_scene_target() {
    let fixture = Fixture::new(Case::Observe, &[(A, &[]), (B, &[])], &[]);
    let prefab_name = "door.prefab.json";
    let prefab_id = StableAssetId::parse_str("74210000-0000-4000-a000-000000000001").unwrap();
    let prefab_key = SourceDocumentKey::stable_asset(prefab_id, SourceDocumentKind::Prefab);
    let inherited = id("inherited-door");
    fs::write(
        fixture.root().join(PREFABS).join(prefab_name),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(inherited.clone())
            .with_component(ComponentTypeId::new(DOOR), door_record(product(C)))])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    fs::write(
        fixture
            .root()
            .join(PREFABS)
            .join(format!("{prefab_name}.meta")),
        AssetMeta::new(
            AssetMetaIdentity::new(
                prefab_id,
                AssetPath::new(prefab_name).unwrap(),
                AssetSourceKind::Prefab,
            ),
            (),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let mut overlay = PrefabOverlay::new(prefab_key.clone());
    overlay.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Replace,
        target: PrefabEntityTarget::source(inherited),
        component: ComponentTypeId::new(DOOR),
        record: door_record(product(B)),
        bindings: vec![],
    });
    let mut authored = document(A, &[]);
    authored.entities[0].prefab = Some(PrefabInstance {
        source: AssetRef::Product(
            AssetProductRef::new(prefab_id, ImportedProductId::PRIMARY).unwrap(),
        ),
        overrides: Some(overlay),
    });
    fixture.write_scene(A, &authored);
    // C has neither a document nor metadata. Its discarded inherited reference cannot be loaded.
    let snapshot = fixture.load();
    assert_eq!(snapshot.scene(), &authored);
    assert_eq!(snapshot.prefabs().len(), 1);
    assert_eq!(snapshot.prefabs()[0].source_key(), &prefab_key);
    assert_eq!(snapshot.scene_dependencies().len(), 1);
    assert_eq!(
        snapshot.scene_dependencies()[0].source_key(),
        &source_key(B)
    );
    let targets: Vec<_> = snapshot
        .expanded_scene()
        .entities
        .iter()
        .filter_map(|entity| {
            entity
                .components
                .get(&ComponentTypeId::new(DOOR))
                .map(|record| {
                    DoorTarget::__decode_persistent_component(&record.value)
                        .unwrap()
                        .target
                })
        })
        .collect();
    assert_eq!(targets, [AssetRef::Product(product(B))]);
    let outcome = execute(&fixture);
    assert_eq!(
        outcome.startup_document.as_ref(),
        Some(snapshot.expanded_scene())
    );
    drop(snapshot);
    assert_idle(&fixture.loader);
}

#[test]
fn malformed_missing_wrong_kind_and_auxiliary_dependencies_preserve_the_before_snapshot() {
    for fault in Fault::ALL {
        let fixture = Fixture::new(Case::Observe, &[(A, &[A]), (B, &[A])], &[B]);
        let before = fixture.load();
        let authored = before.scene().clone();
        let effective = before.expanded_scene().clone();
        let revision = before.revision();
        let digest = before.content_digest();
        let budget = fixture.loader.budget_snapshot();
        let target = if fault == Fault::Auxiliary {
            auxiliary(B)
        } else {
            product(B)
        };
        let mut candidate = document(A, &[B]);
        candidate
            .entities
            .iter_mut()
            .find(|entity| entity.id == door_id(A, 0))
            .unwrap()
            .components
            .insert(ComponentTypeId::new(DOOR), door_record(target));
        fixture.write_scene(A, &candidate);
        fixture.corrupt(B, fault);
        let error = fixture
            .loader
            .load(&fixture.candidate, &fixture.plan)
            .unwrap_err();
        assert!(error.diagnostics().has_errors(), "{fault:?}: {error:?}");
        assert_eq!(before.scene(), &authored, "{fault:?}");
        assert_eq!(before.expanded_scene(), &effective, "{fault:?}");
        assert_eq!(before.revision(), revision, "{fault:?}");
        assert_eq!(before.content_digest(), digest, "{fault:?}");
        assert_eq!(before.selected_scene_product(), Some(product(A)));
        assert!(before.scene_dependencies().is_empty());
        assert_active_budget(fixture.loader.budget_snapshot(), budget);
        drop(before);
        assert_idle(&fixture.loader);
    }
}

#[test]
fn required_scene_changes_startup_digest_but_unrelated_declared_scene_does_not() {
    let graph: &[(u64, &[u64])] = &[(A, &[B]), (B, &[A]), (C, &[])];
    let fixture = Fixture::new(Case::Observe, graph, &[B, C]);
    let initial = fixture.load();
    assert_flat_dependencies(&fixture, &initial, graph, &[B]);
    let digest = initial.startup_content_closure().unwrap().content_digest();
    let revision = initial.revision();
    drop(initial);
    let mut unrelated = document(C, &[]);
    unrelated.roots.push(id("unrelated-edit"));
    unrelated
        .entities
        .push(SceneEntityRecord::new(id("unrelated-edit")));
    fixture.write_scene(C, &unrelated);
    let after_unrelated = fixture.load();
    assert_eq!(after_unrelated.content_digest(), digest);
    assert_eq!(after_unrelated.revision(), revision);
    assert!(
        !after_unrelated
            .content_closure()
            .members()
            .iter()
            .any(|member| { member.path().as_str() == canonical_scene_path(C).as_str() })
    );
    drop(after_unrelated);
    let mut dependent = document(B, &[A]);
    dependent.roots.push(id("dependent-edit"));
    dependent
        .entities
        .push(SceneEntityRecord::new(id("dependent-edit")));
    dependent
        .entities
        .sort_by(|left, right| left.id.cmp(&right.id));
    fixture.write_scene(B, &dependent);
    let after_dependency = fixture.load();
    assert_ne!(after_dependency.content_digest(), digest);
    assert_ne!(after_dependency.revision(), revision);
    assert_eq!(after_dependency.scene(), &document(A, &[B]));
    assert_eq!(
        after_dependency.scene_dependencies()[0].document(),
        &dependent
    );
    drop(after_dependency);
    assert_idle(&fixture.loader);
}

#[test]
fn escaped_scene_source_keeps_its_generation_until_the_last_source_owner_drops() {
    let fixture = Fixture::new(Case::Observe, &[(A, &[B]), (B, &[A])], &[]);
    let snapshot = fixture.load();
    assert_eq!(snapshot.scene_dependencies().len(), 1);
    let first_source = snapshot.scene_source();
    let last_source = snapshot.scene_source();
    drop(snapshot);
    let escaped = fixture.loader.budget_snapshot();
    assert_eq!(escaped.active_generations(), 1);
    assert!(escaped.active(ProjectContentBudgetKind::RetainedBytes) > 0);
    drop(first_source);
    assert_active_budget(fixture.loader.budget_snapshot(), escaped);
    drop(last_source);
    assert_idle(&fixture.loader);
}

#[test]
fn source_scene_closure_accepts_exact_retained_budget_and_returns_one_short_reservations() {
    let fixture = Fixture::new(
        Case::Observe,
        &[(A, &[B, C]), (B, &[D]), (C, &[D]), (D, &[])],
        &[],
    );
    let measured = fixture.load();
    let peak = fixture
        .loader
        .budget_snapshot()
        .high_water(ProjectContentBudgetKind::RetainedBytes);
    assert!(peak > 1);
    drop(measured);
    assert_idle(&fixture.loader);
    let exact_limits = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::RetainedBytes, peak)
        .unwrap();
    let exact = ProjectContentLoader::with_limits(fixture.capability(), exact_limits).unwrap();
    let snapshot = exact.load(&fixture.candidate, &fixture.plan).unwrap();
    assert_eq!(snapshot.scene_dependencies().len(), 3);
    assert_eq!(
        exact
            .budget_snapshot()
            .high_water(ProjectContentBudgetKind::RetainedBytes),
        peak,
    );
    drop(snapshot);
    assert_idle(&exact);
    let short_limits = exact_limits
        .with_limit(ProjectContentBudgetKind::RetainedBytes, peak - 1)
        .unwrap();
    let short = ProjectContentLoader::with_limits(fixture.capability(), short_limits).unwrap();
    let failure = short.load(&fixture.candidate, &fixture.plan).unwrap_err();
    assert_eq!(failure.kind(), ProjectContentErrorKind::BudgetExceeded);
    assert_idle(&short);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Request,
    Preparing,
    Ready,
    Done,
}

#[derive(Clone, Debug, Default, Resource)]
struct Outcome {
    phase: Phase,
    request: Option<ProjectSceneRequestId>,
    started: Option<Instant>,
    commits: u8,
    ready_boundaries: u8,
    rejected: bool,
    startup_document: Option<SceneDocument>,
    startup_entities: usize,
    source_view: Option<SceneSourceView>,
    receipts: Vec<SpawnedSceneInstance>,
}

fn inspect_startup(world: &mut World) {
    let (document, receipt) = live_scene(world, A);
    let view = CurrentScene::get(world).unwrap().source_view();
    let count = world.query::<&SceneEntitySource>().iter(world).count();
    let observe = *world.resource::<Case>() == Case::Observe;
    let mut outcome = world.resource_mut::<Outcome>();
    outcome.startup_document = Some(document);
    outcome.startup_entities = count;
    outcome.source_view = Some(view);
    outcome.receipts.push(receipt);
    outcome.started = Some(Instant::now());
    if observe {
        outcome.phase = Phase::Done;
    }
}

fn live_scene(world: &mut World, scene: u64) -> (SceneDocument, SpawnedSceneInstance) {
    let (document, receipt, view) = {
        let current = CurrentScene::get(world).unwrap();
        (
            current.source().clone(),
            current.receipt().clone(),
            current.source_view(),
        )
    };
    assert!(document.roots.contains(&marker(scene)));
    view.with_expanded_scene(|candidate, resolver| {
        assert_eq!(candidate.source_key(), &source_key(scene));
        assert!(resolver.is_some());
        assert_eq!(candidate.document(), &document);
    })
    .expect("the current source Scene retains its admitted expansion");
    let mut doors = 0;
    for record in &document.entities {
        let EntityLookup::Resolved(entity) = receipt.resolve(world, &record.id) else {
            panic!("the current root must resolve each member through its real receipt")
        };
        if let Some(record) = record.components.get(&ComponentTypeId::new(DOOR)) {
            let expected = DoorTarget::__decode_persistent_component(&record.value).unwrap();
            assert_eq!(
                world.get::<DoorTarget>(entity).unwrap().target,
                expected.target
            );
            doors += 1;
        }
    }
    assert_eq!(world.query::<&DoorTarget>().iter(world).count(), doors);
    assert_eq!(
        world.query::<&SceneEntitySource>().iter(world).count(),
        document.entities.len(),
    );
    (document, receipt)
}

fn live_door(world: &mut World, scene: u64) -> (Entity, AssetProductRef, SpawnedSceneInstance) {
    let (_, receipt) = live_scene(world, scene);
    let EntityLookup::Resolved(entity) = receipt.resolve(world, &door_id(scene, 0)) else {
        panic!("the current Scene must contain its native Door")
    };
    let target = world
        .get::<DoorTarget>(entity)
        .unwrap()
        .target
        .as_product()
        .unwrap();
    assert_eq!(target, product(if scene == A { B } else { A }));
    (entity, target, receipt)
}

fn travel(world: &mut World) {
    let (phase, commits, case) = {
        let outcome = world.resource::<Outcome>();
        (outcome.phase, outcome.commits, *world.resource::<Case>())
    };
    if phase == Phase::Done {
        return;
    }
    assert!(world.resource::<Outcome>().started.unwrap().elapsed() < Duration::from_secs(15));
    let active = if commits == 0 { A } else { B };
    let (old_door, door_target, receipt) = live_door(world, active);
    assert_eq!(world.resource::<Outcome>().receipts.last(), Some(&receipt));
    match phase {
        Phase::Request => {
            let selection = match case {
                Case::Reject(Fault::Auxiliary) => auxiliary(C),
                Case::Reject(_) => product(C),
                _ => door_target,
            };
            let request = ProjectScenes::scope(world, |scenes| {
                let request = scenes.request(selection).unwrap();
                assert_eq!(
                    scenes.state(request),
                    Some(ProjectSceneRequestState::Preparing),
                );
                assert!(scenes.prepared(request).is_none());
                assert!(!scenes.acknowledge(request));
                request
            });
            let mut outcome = world.resource_mut::<Outcome>();
            outcome.request = Some(request);
            outcome.phase = Phase::Preparing;
        }
        Phase::Preparing => {
            let request = world.resource::<Outcome>().request.unwrap();
            let state = ProjectScenes::scope(world, |scenes| scenes.state(request)).unwrap();
            if state == ProjectSceneRequestState::Preparing {
                std::thread::sleep(Duration::from_millis(1));
                return;
            }
            if matches!(case, Case::DependencyOnly | Case::Reject(_)) {
                if case == Case::DependencyOnly {
                    assert_eq!(
                        state,
                        ProjectSceneRequestState::Rejected(
                            ProjectSceneRequestError::UndeclaredScene
                        ),
                    );
                } else {
                    assert!(
                        matches!(
                            state,
                            ProjectSceneRequestState::Rejected(ProjectSceneRequestError::Content(
                                _
                            )),
                        ),
                        "{case:?}: {state:?}",
                    );
                }
                ProjectScenes::scope(world, |scenes| {
                    assert!(scenes.prepared(request).is_none());
                    assert!(scenes.diagnostics(request).unwrap().has_errors());
                    assert!(scenes.acknowledge(request));
                });
                assert_eq!(live_door(world, A).2, receipt);
                assert_eq!(
                    CurrentScene::get(world).unwrap().source(),
                    world
                        .resource::<Outcome>()
                        .startup_document
                        .as_ref()
                        .unwrap(),
                );
                let mut outcome = world.resource_mut::<Outcome>();
                outcome.rejected = true;
                outcome.request = None;
                outcome.phase = Phase::Done;
                return;
            }
            assert_eq!(state, ProjectSceneRequestState::Ready);
            let next = if commits == 0 { B } else { A };
            ProjectScenes::scope(world, |scenes| {
                let prepared = scenes.prepared(request).unwrap();
                assert_eq!(
                    prepared.selected_scene(),
                    &ProjectSceneSelection::Product(door_target),
                );
                assert_eq!(prepared.selected_scene_product(), Some(door_target));
                assert_eq!(
                    prepared.selected_scene_path().unwrap().as_str(),
                    scene_name(next)
                );
                assert_eq!(prepared.expected_instance(), &receipt);
                assert_eq!(prepared.scene(), &document(next, &[active]));
            });
            // An entire ordinary frame ends with only the old root instantiated and B/A Ready.
            assert_eq!(live_door(world, active).2, receipt);
            let mut outcome = world.resource_mut::<Outcome>();
            outcome.ready_boundaries += 1;
            outcome.phase = Phase::Ready;
        }
        Phase::Ready => {
            let request = world.resource::<Outcome>().request.unwrap();
            let report = commit_prepared_project_scene(
                world,
                request,
                SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
                &[],
                |_| {},
            );
            assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
            let published = report.instance.unwrap();
            let next = if commits == 0 { B } else { A };
            assert_eq!(live_door(world, next).2, published);
            assert_ne!(published.instance_id(), receipt.instance_id());
            assert!(world.get::<DoorTarget>(old_door).is_none());
            ProjectScenes::scope(world, |scenes| {
                assert_eq!(
                    scenes.state(request),
                    Some(ProjectSceneRequestState::Committed)
                );
                assert!(scenes.acknowledge(request));
            });
            let mut outcome = world.resource_mut::<Outcome>();
            outcome.receipts.push(published);
            outcome.commits += 1;
            outcome.request = None;
            outcome.phase = if outcome.commits == 2 {
                Phase::Done
            } else {
                Phase::Request
            };
        }
        Phase::Done => unreachable!(),
    }
}

fn execute(fixture: &Fixture) -> Outcome {
    let intent = HeadlessRunIntent::frames(NonZeroU32::new(20_000).unwrap(), Duration::ZERO)
        .with_recipe(fixture.recipe.clone())
        .stop_when(|outcome: &Outcome| outcome.phase == Phase::Done);
    let mut run = HeadlessRun::<Outcome>::new(fixture.capability(), intent, Vec::new());
    let mut cleanup_deadline = None;
    loop {
        let report = run.execute_bounded();
        if matches!(report.outcome(), HeadlessRunOutcome::CleanupIncomplete) {
            let deadline =
                cleanup_deadline.get_or_insert_with(|| Instant::now() + Duration::from_secs(10));
            assert!(
                Instant::now() < *deadline,
                "source Host cleanup did not return"
            );
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        assert!(
            !report.diagnostics().has_errors(),
            "{:?}",
            report.diagnostics()
        );
        assert!(report.startup_content_closure().is_some());
        let HeadlessRunOutcome::Completed(outcome) = report.into_outcome() else {
            panic!("the bounded source Scene workflow must complete")
        };
        assert_eq!(outcome.phase, Phase::Done);
        assert!(
            outcome
                .source_view
                .as_ref()
                .unwrap()
                .with_document(|_| ())
                .is_none()
        );
        return outcome;
    }
}

#[test]
fn source_door_products_travel_a_to_b_to_a_through_ready_and_explicit_commit() {
    let fixture = Fixture::new(Case::RoundTrip, &[(A, &[B]), (B, &[A])], &[B]);
    let outcome = execute(&fixture);
    assert_eq!(outcome.commits, 2);
    assert_eq!(outcome.ready_boundaries, 2);
    assert_eq!(outcome.receipts.len(), 3);
    for (left, right) in [(0, 1), (1, 2), (0, 2)] {
        assert_ne!(
            outcome.receipts[left].instance_id(),
            outcome.receipts[right].instance_id()
        );
    }
}

#[test]
fn source_dependency_membership_does_not_grant_door_travel_authority() {
    let fixture = Fixture::new(Case::DependencyOnly, &[(A, &[B]), (B, &[A])], &[]);
    let snapshot = fixture.load();
    assert_eq!(
        snapshot.scene_dependencies()[0].source_key(),
        &source_key(B)
    );
    let outcome = execute(&fixture);
    assert!(outcome.rejected);
    assert_eq!(outcome.commits, 0);
    assert_eq!(outcome.receipts.len(), 1);
    drop(snapshot);
    assert_idle(&fixture.loader);
}

#[test]
fn rejected_source_product_candidates_preserve_the_live_door_root() {
    for fault in Fault::ALL {
        let fixture = Fixture::new(
            Case::Reject(fault),
            &[(A, &[B]), (B, &[A]), (C, &[A])],
            &[B, C],
        );
        fixture.corrupt(C, fault);
        let outcome = execute(&fixture);
        assert!(outcome.rejected, "{fault:?}");
        assert_eq!(outcome.commits, 0, "{fault:?}");
        assert_eq!(outcome.ready_boundaries, 0, "{fault:?}");
        assert_eq!(outcome.receipts.len(), 1, "{fault:?}");
    }
}

fn assert_active_budget(
    actual: ProjectContentBudgetSnapshot,
    expected: ProjectContentBudgetSnapshot,
) {
    assert_eq!(actual.active_generations(), expected.active_generations());
    assert_eq!(actual.active_reservations(), expected.active_reservations());
    for &kind in ProjectContentBudgetKind::ALL {
        assert_eq!(actual.active(kind), expected.active(kind), "{kind:?}");
    }
}

fn assert_idle(loader: &ProjectContentLoader) {
    let budget = loader.budget_snapshot();
    assert_eq!(budget.active_generations(), 0);
    assert_eq!(budget.active_reservations(), 0);
    for &kind in ProjectContentBudgetKind::ALL {
        assert_eq!(budget.active(kind), 0, "{kind:?}");
    }
}
