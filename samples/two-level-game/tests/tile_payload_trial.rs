//! Decision probe for persistent Tilemap record granularity, not a production format or renderer.
//!
//! Uses the ordinary 16K source generator and public compilation/preparation/cook interfaces.
//! It isolates the two terrain records; gameplay, project delivery and GPU timing remain outside
//! this probe. Original source files, source keys and cell encoding are preserved as the control.
//! Both formats retain default cook-wire refusal; the complete positive round trip explicitly
//! requests two million wire nodes while keeping every other original receiving/cook limit.

#![cfg(all(feature = "desktop", feature = "cost-measurement"))]
#![forbid(unsafe_code)]

#[path = "tile_payload_trial/codec.rs"]
mod codec;
#[path = "../src/bin/runtime_workload/project.rs"]
#[allow(dead_code)]
mod project;

use std::{fs, ptr};

use nara::{
    asset::{
        AssetPathScope, AssetRecord, AssetRef, AssetServer, AssetSourceKind, AssetSourcePath,
        ProjectAssetDatabase, StableAssetId,
    },
    core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits},
    ecs::{Entity, World},
    identity::{EntityLookup, SpawnedSceneInstance},
    reflect::{
        ComponentRegistryBuilder, ComponentTypeId, ComponentValue,
        ComponentValueMaterializationLimits,
    },
    scene::{
        CompiledSceneCandidate, CompiledSceneDocument, InMemoryPrefabSourceResolver,
        PrefabDocument, PrefabDocumentCandidate, PrefabInstanceBinding, RuntimeSceneCandidate,
        RuntimeSceneDocumentCandidate, RuntimeSceneLimits, SceneComponentBindings,
        SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId,
        SceneOccurrenceKey, SourceDocumentKey, SourceDocumentKind,
        retire_and_despawn_scene_instance,
    },
    tilemap::{Tilemap, register_tilemap_components},
};

const TILEMAP: &str = "nara.tilemap.Tilemap";

struct Sources {
    project: project::Project,
    scene: SceneDocument,
    prefab: PrefabDocument,
    request: AssetRef,
    scene_key: SourceDocumentKey,
    prefab_key: SourceDocumentKey,
    source_bytes: Vec<(std::path::PathBuf, Vec<u8>)>,
}

impl Sources {
    fn capture() -> Self {
        let project = project::Project::new(16_384).unwrap();
        let scene_path = project.root().join("scenes/meadow.scene.json");
        let bytes = fs::read(&scene_path).unwrap();
        let complete = SceneDocumentCandidate::decode_json_bytes(&bytes)
            .unwrap()
            .into_composition_source()
            .unwrap();
        let mut terrain = Vec::new();
        for mut entity in complete.entities {
            if matches!(entity.id.as_str(), "checkpoint" | "portal") {
                entity.components.retain(|kind, _| kind.as_str() == TILEMAP);
                terrain.push(entity);
            }
        }
        assert_eq!(terrain.len(), 2);
        let portal = terrain
            .iter()
            .find(|entity| entity.id.as_str() == "portal")
            .unwrap();
        let instance = portal.prefab.as_ref().unwrap();
        let request = instance.source.clone();
        let PrefabInstanceBinding::Root { base, .. } = &instance.binding else {
            panic!("the 16K workload must retain its original root binding")
        };
        let prefab_key = base.clone();
        let nara::scene::SourceDocumentOrigin::ProjectLocation { universe, .. } =
            prefab_key.origin()
        else {
            panic!("the original workload uses its admitted project source universe")
        };
        let scene_key = SourceDocumentKey::project_location(
            *universe,
            AssetSourcePath::new("scenes/meadow.scene.json").unwrap(),
            SourceDocumentKind::Scene,
        );
        let AssetRef::Path(path) = &request else {
            panic!("the original workload uses its authored Prefab alias")
        };
        let prefab_path = project.root().join("prefabs").join(path.as_str());
        let prefab_bytes = fs::read(&prefab_path).unwrap();
        let prefab = PrefabDocumentCandidate::decode_json_bytes(&prefab_bytes)
            .unwrap()
            .into_composition_source()
            .unwrap();
        assert_eq!(prefab.entities.len(), 1);
        let placements = complete
            .placements
            .into_iter()
            .filter(|placement| {
                terrain.iter().any(|entity| {
                    placement.target
                        == nara::identity::PrefabEntityTarget::source(entity.id.clone())
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(placements.len(), 2);
        assert!(
            placements
                .iter()
                .all(|placement| placement.placement.parent
                    == nara::scene::ScenePlacementParent::Root)
        );
        let scene = SceneDocument::new(placements, terrain);
        Self {
            project,
            scene,
            prefab,
            request,
            scene_key,
            prefab_key,
            source_bytes: vec![(scene_path, bytes), (prefab_path, prefab_bytes)],
        }
    }

    fn resolver(&self, prefab: PrefabDocument) -> InMemoryPrefabSourceResolver {
        InMemoryPrefabSourceResolver::new().with_prefab(
            self.request.clone(),
            self.prefab_key.clone(),
            prefab,
        )
    }

    fn unchanged(&self) {
        for (path, bytes) in &self.source_bytes {
            assert_eq!(fs::read(path).unwrap(), *bytes);
        }
    }
}

fn kind(name: &str) -> ComponentTypeId {
    ComponentTypeId::new(name)
}
fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn bindings() -> SceneComponentBindings {
    let mut registry = ComponentRegistryBuilder::new();
    register_tilemap_components(&mut registry).unwrap();
    codec::register(&mut registry).unwrap();
    SceneComponentBindings::new(registry.freeze().unwrap(), []).unwrap()
}

fn split(components: &mut std::collections::BTreeMap<ComponentTypeId, SceneComponentRecord>) {
    let Some(record) = components.remove(&kind(TILEMAP)) else {
        return;
    };
    assert!(record.contextual_defaults.is_empty());
    assert!(record.owner.is_none());
    let version = record.version;
    let ComponentValue::Map(mut fields) = record.into_value() else {
        panic!("the original domain value is a map")
    };
    let cells = fields.remove("cells").unwrap();
    assert!(!fields.contains_key("sort_key") && !fields.contains_key("y_sort_anchor"));
    components.insert(
        kind(codec::CONFIG),
        SceneComponentRecord::new(version, ComponentValue::Map(fields)),
    );
    components.insert(
        kind(codec::CELLS),
        SceneComponentRecord::new(version, ComponentValue::map([("cells", cells)])),
    );
}

fn compile(
    bindings: &SceneComponentBindings,
    source: &SceneDocument,
    resolver: &InMemoryPrefabSourceResolver,
    key: &SourceDocumentKey,
) -> CompiledSceneCandidate {
    let report = bindings.compile_scene(source, resolver);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report.candidate.unwrap().bind_source(key.clone())
}

fn entity(world: &World, instance: &SpawnedSceneInstance, name: &str) -> Entity {
    match instance.resolve(world, &id(name)) {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("the original Scene member must remain live: {other:?}"),
    }
}

fn value<'a>(
    document: &'a CompiledSceneDocument,
    name: &str,
    component: &str,
) -> &'a ComponentValue {
    document
        .entities()
        .iter()
        .find(|entity| entity.id() == &id(name))
        .unwrap()
        .components()[&kind(component)]
        .value()
}

fn prepared(
    bindings: &SceneComponentBindings,
    candidate: &CompiledSceneCandidate,
    label: &str,
) -> (World, SpawnedSceneInstance) {
    let mut world = World::new();
    world.insert_resource(AssetServer::new());
    let pending = bindings
        .prepare_scene(
            &mut world,
            candidate,
            &SceneOccurrenceKey::new("tilemap-record-boundary-trial", label).unwrap(),
            None,
        )
        .unwrap();
    let report = pending.commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    (world, report.instance.unwrap())
}

fn joined(
    configuration: &codec::Configuration,
    cells: &codec::Cells,
    tileset: Option<nara::asset::Handle<nara::tilemap::TileSet>>,
) -> Tilemap {
    let mut joined = Tilemap::new(configuration.tile_size)
        .with_layer(configuration.layer)
        .with_sort_key(configuration.sort_key);
    joined.y_sort_anchor = configuration.y_sort_anchor;
    joined.tileset = tileset;
    for (coordinate, cell) in &cells.entries {
        assert!(joined.set_cell(*coordinate, *cell).is_none());
    }
    joined
}

#[test]
fn actual_16k_records_share_cells_across_defaults_and_preserve_native_values_and_cook() {
    let mut sources = Sources::capture();
    let bindings = bindings();
    let control_resolver = sources.resolver(sources.prefab.clone());
    let control = compile(
        &bindings,
        &sources.scene,
        &control_resolver,
        &sources.scene_key,
    );
    let original_direct = &sources
        .scene
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "checkpoint")
        .unwrap()
        .components[&kind(TILEMAP)];
    assert!(original_direct.value().get("sort_key").is_none());
    assert!(original_direct.value().get("y_sort_anchor").is_none());
    assert!(!ptr::eq(
        original_direct.value(),
        value(control.document(), "checkpoint", TILEMAP)
    ));
    let mut trial_scene = sources.scene.clone();
    let mut trial_prefab = sources.prefab.clone();
    for entity in &mut trial_scene.entities {
        split(&mut entity.components);
    }
    for entity in &mut trial_prefab.entities {
        split(&mut entity.components);
    }
    let trial_prefab_before = trial_prefab.clone();
    let trial_resolver = sources.resolver(trial_prefab);
    let direct = &trial_scene
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "checkpoint")
        .unwrap()
        .components[&kind(codec::CELLS)];
    let trial = compile(&bindings, &trial_scene, &trial_resolver, &sources.scene_key);
    assert_eq!(control.canonical_provenance(), trial.canonical_provenance());
    assert_eq!(control.document().entities().len(), 2);
    assert!(
        control
            .document()
            .entities()
            .iter()
            .all(|entity| entity.components().len() == 1)
    );
    assert!(
        trial
            .document()
            .entities()
            .iter()
            .all(|entity| entity.components().len() == 2)
    );
    assert!(ptr::eq(
        direct.value(),
        value(trial.document(), "checkpoint", codec::CELLS)
    ));
    let inherited =
        nara::scene::PrefabSourceResolver::resolve_prefab(&trial_resolver, &sources.request)
            .unwrap();
    assert_eq!(inherited.document(), &trial_prefab_before);
    assert!(ptr::eq(
        inherited.document().entities[0].components[&kind(codec::CELLS)].value(),
        value(trial.document(), "portal", codec::CELLS)
    ));
    for name in ["checkpoint", "portal"] {
        assert_eq!(
            value(control.document(), name, TILEMAP).get("cells"),
            value(trial.document(), name, codec::CELLS).get("cells")
        );
        assert_eq!(
            value(trial.document(), name, codec::CONFIG).get("sort_key"),
            Some(&ComponentValue::I64(0))
        );
        assert_eq!(
            value(trial.document(), name, codec::CONFIG).get("y_sort_anchor"),
            Some(&ComponentValue::Null)
        );
    }
    let (mut old_world, old) = prepared(&bindings, &control, "control");
    let (mut new_world, new) = prepared(&bindings, &trial, "trial");
    let mut count = 0;
    for name in ["checkpoint", "portal"] {
        let original = old_world
            .get::<Tilemap>(entity(&old_world, &old, name))
            .unwrap();
        let target = entity(&new_world, &new, name);
        let config = new_world.get::<codec::Configuration>(target).unwrap();
        let cells = new_world.get::<codec::Cells>(target).unwrap();
        assert_eq!(
            config.tileset,
            Some(AssetRef::path("tilesets/meadow.tileset.ron").unwrap())
        );
        let assembled = joined(config, cells, original.tileset);
        assert_eq!(&assembled, original);
        count += assembled.cell_count();
    }
    assert_eq!(count, 16_384);
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            StableAssetId::parse_str("f516527c-4e94-45b3-9941-13e659ccfb07").unwrap(),
            AssetSourcePath::new("tilesets/meadow.tileset.ron").unwrap(),
            AssetSourceKind::Other("tileset".into()),
        ))
        .unwrap();
    let default_limits = RuntimeSceneLimits::default();
    for candidate in [&control, &trial] {
        let refused = RuntimeSceneCandidate::cook(
            candidate,
            &bindings,
            database.resolver(&AssetPathScope::ROOT),
            default_limits,
        )
        .unwrap_err();
        assert!(
            refused.diagnostics().iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.runtime-budget"
                    && diagnostic.fields().iter().any(|field| {
                        field.key().as_str() == "budget-kind"
                            && field.value()
                                == nara::diagnostic::DiagnosticValueRef::Identifier("wire-nodes")
                    })
            }),
            "the complete stock and split controls must retain default wire refusal: {refused:?}"
        );
    }
    let shape = default_limits.base().shape();
    let cook_limits =
        default_limits.with_base(default_limits.base().with_shape(SerdeShapeLimits::new(
            shape.depth(),
            ItemLimit::new(2_000_000).unwrap(),
            shape.container_items(),
            shape.string_bytes(),
            shape.total_string_bytes(),
        )));
    let stock_cooked = RuntimeSceneCandidate::cook(
        &control,
        &bindings,
        database.resolver(&AssetPathScope::ROOT),
        cook_limits,
    )
    .unwrap();
    let cooked = RuntimeSceneCandidate::cook(
        &trial,
        &bindings,
        database.resolver(&AssetPathScope::ROOT),
        cook_limits,
    )
    .unwrap();
    for name in ["checkpoint", "portal"] {
        let cooked_cells = value(cooked.document(), name, codec::CELLS);
        let original_cells = value(trial.document(), name, codec::CELLS);
        assert_eq!(cooked_cells, original_cells);
        assert!(!ptr::eq(cooked_cells, original_cells));
        assert_eq!(
            value(stock_cooked.document(), name, TILEMAP).get("cells"),
            cooked_cells.get("cells")
        );
    }
    assert_eq!(
        stock_cooked.canonical_provenance(),
        cooked.canonical_provenance()
    );
    let key = cooked.source_key().clone();
    let wire = cooked.to_json_bytes().unwrap();
    let readmitted = RuntimeSceneDocumentCandidate::from_json_bytes(&wire, cook_limits)
        .unwrap()
        .admit(&key, &bindings)
        .unwrap();
    assert_eq!(readmitted.document(), cooked.document());
    let record = trial_scene
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("checkpoint"))
        .unwrap()
        .components
        .get_mut(&kind(codec::CELLS))
        .unwrap();
    let ComponentValue::Map(fields) = record.value_mut() else {
        unreachable!()
    };
    let ComponentValue::List(entries) = fields.get_mut("cells").unwrap() else {
        unreachable!()
    };
    let ComponentValue::Map(entry) = &mut entries[0] else {
        unreachable!()
    };
    let ComponentValue::Map(cell) = entry.get_mut("cell").unwrap() else {
        unreachable!()
    };
    let ComponentValue::Map(color) = cell.get_mut("color").unwrap() else {
        unreachable!()
    };
    color.insert("r".into(), ComponentValue::f64(0.25).unwrap());
    let edited = compile(&bindings, &trial_scene, &trial_resolver, &sources.scene_key);
    let original_cells =
        codec::decode_cells(value(trial.document(), "checkpoint", codec::CELLS)).unwrap();
    let mut expected_edit = original_cells.entries.clone();
    expected_edit[0].1.color.r = 0.25;
    assert_ne!(expected_edit, original_cells.entries);
    assert_eq!(
        codec::decode_cells(value(edited.document(), "checkpoint", codec::CELLS))
            .unwrap()
            .entries,
        expected_edit
    );
    assert!(!ptr::eq(
        value(edited.document(), "checkpoint", codec::CELLS),
        value(trial.document(), "checkpoint", codec::CELLS)
    ));
    assert!(ptr::eq(
        value(edited.document(), "portal", codec::CELLS),
        value(trial.document(), "portal", codec::CELLS)
    ));
    assert_eq!(
        new_world
            .get::<codec::Cells>(entity(&new_world, &new, "checkpoint"))
            .unwrap()
            .entries,
        original_cells.entries
    );
    assert_eq!(
        value(cooked.document(), "checkpoint", codec::CELLS),
        value(trial.document(), "checkpoint", codec::CELLS)
    );
    assert_eq!(
        value(readmitted.document(), "checkpoint", codec::CELLS),
        value(trial.document(), "checkpoint", codec::CELLS)
    );
    assert_eq!(edited.canonical_provenance(), trial.canonical_provenance());
    sources.unchanged();
    retire_and_despawn_scene_instance(&mut old_world, &old).unwrap();
    retire_and_despawn_scene_instance(&mut new_world, &new).unwrap();
    drop(inherited);
    drop(sources.scene);
    drop(sources.prefab);
    drop(trial_scene);
    drop(control_resolver);
    drop(trial_resolver);
    assert_eq!(
        value(trial.document(), "checkpoint", codec::CELLS).get("cells"),
        value(readmitted.document(), "checkpoint", codec::CELLS).get("cells")
    );
    for (path, bytes) in &sources.source_bytes {
        assert_eq!(fs::read(path).unwrap(), *bytes);
    }
    sources.project.cleanup().unwrap();
}

#[test]
fn split_cell_records_keep_complete_receiving_limits_and_do_not_gain_implicit_configuration() {
    let mut sources = Sources::capture();
    let bindings = bindings();
    let mut trial_scene = sources.scene.clone();
    let mut prefab = sources.prefab.clone();
    for entity in &mut trial_scene.entities {
        split(&mut entity.components);
    }
    for entity in &mut prefab.entities {
        split(&mut entity.components);
    }
    let resolver = sources.resolver(prefab);
    let compiled = compile(&bindings, &trial_scene, &resolver, &sources.scene_key);
    let cost = compiled
        .document()
        .entities()
        .iter()
        .flat_map(|entity| entity.components().values())
        .fold(nara::reflect::ComponentValueCost::ZERO, |cost, record| {
            cost.saturating_add(record.value().cost())
        });
    let limits = ComponentValueMaterializationLimits::new(
        ItemLimit::new(cost.nodes()).unwrap(),
        ByteLimit::new(cost.logical_bytes()).unwrap(),
        DepthLimit::new(6).unwrap(),
    );
    let mut world = World::new();
    world.spawn_empty();
    let occurrence = SceneOccurrenceKey::new("trial-limits", "current").unwrap();
    drop(
        bindings
            .prepare_scene_with_limits(&mut world, &compiled, &occurrence, None, limits)
            .unwrap(),
    );
    let baseline = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<std::collections::BTreeSet<_>>();
    for (limits, expected_error) in [
        (
            limits.with_nodes(ItemLimit::new(cost.nodes() - 1).unwrap()),
            "materialization-node-limit",
        ),
        (
            limits.with_bytes(ByteLimit::new(cost.logical_bytes() - 1).unwrap()),
            "materialization-byte-limit",
        ),
        (
            ComponentValueMaterializationLimits::new(
                ItemLimit::new(cost.nodes()).unwrap(),
                ByteLimit::new(cost.logical_bytes()).unwrap(),
                DepthLimit::new(5).unwrap(),
            ),
            "materialization-depth-limit",
        ),
    ] {
        let refused = bindings
            .prepare_scene_with_limits(
                &mut world,
                &compiled,
                &SceneOccurrenceKey::new("trial-limits", "current").unwrap(),
                None,
                limits,
            )
            .unwrap_err();
        assert!(
            refused
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "codec-error-kind"
                        && field.value()
                            == nara::diagnostic::DiagnosticValueRef::Identifier(expected_error)
                })),
            "expected {expected_error}: {refused:?}"
        );
        assert_eq!(
            world
                .iter_entities()
                .map(|entity| entity.id())
                .collect::<std::collections::BTreeSet<_>>(),
            baseline
        );
    }
    let exact = bindings
        .prepare_scene_with_limits(
            &mut world,
            &compiled,
            &SceneOccurrenceKey::new("trial-limits", "current").unwrap(),
            None,
            limits,
        )
        .unwrap();
    drop(exact);
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<std::collections::BTreeSet<_>>(),
        baseline
    );
    let mut orphan = SceneDocument::from_ordered_roots([trial_scene.entities[0].clone()]);
    orphan.entities[0].prefab = None;
    orphan.entities[0].components.remove(&kind(codec::CONFIG));
    let orphan = compile(
        &bindings,
        &orphan,
        &InMemoryPrefabSourceResolver::new(),
        &sources.scene_key,
    );
    let (mut orphan_world, live) = prepared(&bindings, &orphan, "orphan-control");
    let target = entity(
        &orphan_world,
        &live,
        orphan.document().entities()[0].id().as_str(),
    );
    assert!(orphan_world.get::<codec::Cells>(target).is_some());
    assert!(orphan_world.get::<codec::Configuration>(target).is_none());
    retire_and_despawn_scene_instance(&mut orphan_world, &live).unwrap();
    sources.unchanged();
    sources.project.cleanup().unwrap();
}
