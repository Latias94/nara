use std::{mem::size_of, sync::Arc};

use nara_asset::{AssetPath, AssetRef, StableAssetId};
use nara_core::{ByteLimit, SourceUniverseId};
use nara_diagnostic::{DiagnosticReport, DiagnosticValueRef};
use nara_ecs::World;
use nara_hierarchy::HierarchyRoots;
use nara_identity::WorldIdentityDomain;
use nara_reflect::ComponentRegistry;
use nara_scene::{
    PrefabAddition, PrefabDocument, PrefabEntityTarget, PrefabExpansionBudgetKind,
    PrefabExpansionLimits, PrefabExpansionOptions, PrefabInstance, PrefabOverlay, PrefabPlacement,
    PrefabPlacementParent, PrefabPlacementSlot, PrefabProjectionStep, PrefabSourceResolver,
    ResolvedPrefabSource, SceneDocument, SceneEntityId, SceneEntityProvenance,
    SceneEntityProvenanceMap, SceneEntityRecord, SceneEntitySource, SourceDocumentKey,
    SourceDocumentKind, SourceDocumentOrigin,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    registry
}

fn inherited_key(step: &PrefabProjectionStep) -> &SourceDocumentKey {
    match step {
        PrefabProjectionStep::SourceEntity {
            canonical_source, ..
        } => canonical_source,
        PrefabProjectionStep::InstanceAddition { .. } => panic!("expected inherited entity"),
    }
}

#[test]
fn instance_addition_origin_survives_nested_anchors_suppression_and_runtime_publication() {
    use nara_scene::InMemoryPrefabSourceResolver;
    let registry = registry();
    let outer = AssetRef::path("prefabs/outer.ron").unwrap();
    let inner = AssetRef::path("prefabs/inner.ron").unwrap();
    let outer_key =
        SourceDocumentKey::named(universe(), "outer", SourceDocumentKind::Prefab).unwrap();
    let inner_key =
        SourceDocumentKey::named(universe(), "inner", SourceDocumentKind::Prefab).unwrap();
    let mut inner_overlay = PrefabOverlay::new(inner_key.clone());
    inner_overlay
        .additions
        .push(PrefabAddition::new(id("added_leaf")));
    inner_overlay.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(id("added_leaf")),
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    });
    let mut nested = anchor("nested", inner.clone());
    nested.prefab.as_mut().unwrap().overrides = Some(inner_overlay.clone());
    let mut reused = PrefabAddition::new(id("reused"));
    reused.prefab = Some(PrefabInstance {
        source: inner.clone(),
        overrides: Some(inner_overlay),
    });
    let mut overrides = PrefabOverlay::new(outer_key.clone());
    overrides
        .suppressions
        .push(PrefabEntityTarget::source(id("reused")));
    overrides.additions = vec![
        reused,
        PrefabAddition::new(id("restored")),
        PrefabAddition::new(id("restored_child")),
    ];
    overrides.placements = vec![
        PrefabPlacement {
            target: PrefabEntityTarget::addition(id("reused")),
            parent: PrefabPlacementParent::Root,
            slot: PrefabPlacementSlot::Last,
        },
        PrefabPlacement {
            target: PrefabEntityTarget::addition(id("restored")),
            parent: PrefabPlacementParent::Root,
            slot: PrefabPlacementSlot::Last,
        },
        PrefabPlacement {
            target: PrefabEntityTarget::addition(id("restored_child")),
            parent: PrefabPlacementParent::Entity(PrefabEntityTarget::addition(id("restored"))),
            slot: PrefabPlacementSlot::Last,
        },
    ];
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            outer.clone(),
            outer_key.clone(),
            PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("reused")),
                nested,
                SceneEntityRecord::new(id("added_leaf")),
            ]),
        )
        .with_prefab(
            inner.clone(),
            inner_key.clone(),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf"))]),
        );
    let scene = SceneDocument::from_ordered_roots(["host_a", "host_b"].map(|name| {
        let mut host = anchor(name, outer.clone());
        host.prefab.as_mut().unwrap().overrides = Some(overrides.clone());
        host
    }));
    let report = scene.expand_prefabs(&registry, &resolver);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let candidate = report.candidate.unwrap().bind_source(
        SourceDocumentKey::named(universe(), "containing", SourceDocumentKind::Scene).unwrap(),
    );
    let inherited = |source: &AssetRef, key: &SourceDocumentKey, entity: &str| {
        PrefabProjectionStep::SourceEntity {
            source_document: source.clone(),
            canonical_source: key.clone(),
            entity: id(entity),
        }
    };
    let added = |entity: &str| PrefabProjectionStep::InstanceAddition { entity: id(entity) };
    let expected = [
        // Nested creation payloads cannot claim a same-spelled record in this source.
        (
            "added_leaf",
            vec![inherited(&outer, &outer_key, "added_leaf")],
        ),
        ("reused", vec![added("reused")]),
        (
            "leaf",
            vec![added("reused"), inherited(&inner, &inner_key, "leaf")],
        ),
        ("added_leaf", vec![added("reused"), added("added_leaf")]),
        ("nested", vec![inherited(&outer, &outer_key, "nested")]),
        (
            "leaf",
            vec![
                inherited(&outer, &outer_key, "nested"),
                inherited(&inner, &inner_key, "leaf"),
            ],
        ),
        (
            "added_leaf",
            vec![inherited(&outer, &outer_key, "nested"), added("added_leaf")],
        ),
        ("restored", vec![added("restored")]),
        ("restored_child", vec![added("restored_child")]),
    ];
    for host in ["host_a", "host_b"] {
        for (local, steps) in &expected {
            let matches = candidate
                .provenance()
                .values()
                .filter(|source| {
                    source.local_entity_id() == &id(local)
                        && source.projection_path().is_some_and(|path| {
                            path.root_anchor() == &id(host) && path.steps() == steps
                        })
                })
                .count();
            assert_eq!(matches, 1, "{host}/{local}: {steps:?}");
        }
    }
    let mut world = World::new();
    let report = nara_scene::spawn_scene(
        &mut world,
        &registry,
        &candidate,
        &nara_scene::SceneOccurrenceKey::new("tests", "addition-origin").unwrap(),
        None,
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let instance = report.instance.unwrap();
    for (materialized, provenance) in candidate.provenance() {
        let nara_identity::EntityLookup::Resolved(entity) = instance.resolve(&world, materialized)
        else {
            panic!("missing materialized entity");
        };
        assert_eq!(
            world.get::<SceneEntitySource>(entity).unwrap().provenance(),
            provenance
        );
    }
}

#[test]
fn addition_origin_typed_path_and_source_stack_are_admitted_before_copying() {
    use nara_scene::InMemoryPrefabSourceResolver;
    let registry = registry();
    let source = AssetRef::path("prefabs/empty.ron").unwrap();
    let key = SourceDocumentKey::named(universe(), "empty", SourceDocumentKind::Prefab).unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        key.clone(),
        PrefabDocument::default(),
    );
    let mut host = anchor("host", source.clone());
    let mut overlay = PrefabOverlay::new(key.clone());
    overlay.additions.push(PrefabAddition::new(id("local")));
    overlay.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(id("local")),
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    });
    host.prefab.as_mut().unwrap().overrides = Some(overlay);
    let scene = SceneDocument::from_ordered_roots([host]);
    let before_path = size_of::<SceneEntityProvenanceMap>()
        + entry_bytes("host", "host")
        + source_binding_bytes(&source, &key);
    let rejected = scene.expand_prefabs_with_options(
        &registry,
        &resolver,
        provenance_options(before_path - 1),
    );
    assert!(rejected.candidate.is_none());
    assert_eq!(
        assert_provenance_budget(&rejected.diagnostics, before_path - 1),
        before_path
    );

    let full = before_path
        + entry_bytes("prefab_0", "local")
        + string_bytes("host")
        + size_of::<PrefabProjectionStep>()
        + string_bytes("local");
    let accepted =
        scene.expand_prefabs_with_options(&registry, &resolver, provenance_options(full));
    assert!(
        !accepted.diagnostics.has_errors(),
        "{:?}",
        accepted.diagnostics
    );
    assert!(accepted.candidate.is_some());
    let rejected =
        scene.expand_prefabs_with_options(&registry, &resolver, provenance_options(full - 1));
    assert!(rejected.candidate.is_none());
    assert_eq!(
        assert_provenance_budget(&rejected.diagnostics, full - 1),
        full
    );
}

fn anchor(name: &str, source: AssetRef) -> SceneEntityRecord {
    let mut entity = SceneEntityRecord::new(id(name));
    entity.prefab = Some(PrefabInstance {
        source,
        overrides: None,
    });
    entity
}

fn universe() -> SourceUniverseId {
    "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap()
}

#[cfg(feature = "serde")]
#[test]
fn rejected_file_publication_preserves_all_aliases_and_canonical_documents() {
    use nara_asset::ProjectAssetDatabase;
    use nara_scene::{InMemoryPrefabSourceResolver, PrefabDocumentCandidate};

    let path = AssetRef::path("prefabs/source.ron").unwrap();
    let alias = AssetRef::path("aliases/source.ron").unwrap();
    let original =
        SourceDocumentKey::named(universe(), "original", SourceDocumentKind::Prefab).unwrap();
    let replacement =
        SourceDocumentKey::named(universe(), "replacement", SourceDocumentKind::Prefab).unwrap();
    let valid = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("local"))]);
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(path.clone(), original.clone(), valid.clone())
        .with_prefab(alias.clone(), original.clone(), valid);
    let invalid = PrefabDocument::new([id("missing")], [])
        .to_json_string()
        .unwrap();
    let registry = registry();
    let database = ProjectAssetDatabase::default();

    for key in [original.clone(), replacement] {
        for with_database in [false, true] {
            let mut candidate_resolver = resolver.clone();
            let candidate = PrefabDocumentCandidate::decode_json_bytes(invalid.as_bytes()).unwrap();
            let result = if with_database {
                candidate_resolver.insert_file_candidate_with_asset_database(
                    path.clone(),
                    key.clone(),
                    candidate,
                    &registry,
                    &database,
                )
            } else {
                candidate_resolver.insert_file_candidate(
                    path.clone(),
                    key.clone(),
                    candidate,
                    &registry,
                )
            };
            let error = result.unwrap_err();
            assert!(
                error
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| { diagnostic.code().as_str() == "scene.dangling-root" })
            );
            assert_eq!(candidate_resolver, resolver);
            let first = candidate_resolver.resolve_prefab(&path).unwrap();
            let second = candidate_resolver.resolve_prefab(&alias).unwrap();
            assert_eq!(first.key(), &original);
            assert!(std::ptr::eq(first.document(), second.document()));
        }
    }
}

fn provenance_options(bytes: usize) -> PrefabExpansionOptions {
    PrefabExpansionOptions::default().with_limits(
        PrefabExpansionLimits::default().with_provenance_bytes(ByteLimit::new(bytes).unwrap()),
    )
}

// Independent expression of the documented logical ownership model, including the same
// conservative string/tree overhead as the root Scene retention plan.
// Provenance owns the map, root entries, selected request/key bindings and final projected
// entries with complete typed paths. Shell/scope ancestry is admitted as working scratch.
fn string_bytes(value: &str) -> usize {
    value.len() + 2 * size_of::<usize>()
}

fn request_bytes(source: &AssetRef) -> usize {
    match source {
        AssetRef::Path(path) => string_bytes(path.as_str()),
        AssetRef::StableId(_) => 16,
    }
}

fn key_bytes(key: &SourceDocumentKey) -> usize {
    size_of::<SourceDocumentKey>()
        + match key.origin() {
            SourceDocumentOrigin::Named { name, .. } => string_bytes(name),
            SourceDocumentOrigin::ProjectLocation { path, .. } => string_bytes(path.as_str()),
            SourceDocumentOrigin::StableAsset { .. } => 0,
        }
}

fn source_binding_bytes(source: &AssetRef, key: &SourceDocumentKey) -> usize {
    size_of::<AssetRef>() + size_of::<SourceDocumentKey>() + request_bytes(source) + key_bytes(key)
}

fn entry_bytes(expanded: &str, local: &str) -> usize {
    size_of::<SceneEntityId>()
        + size_of::<SceneEntityProvenance>()
        + 6 * size_of::<usize>()
        + string_bytes(expanded)
        + string_bytes(local)
}

fn step_bytes(source: &AssetRef, key: &SourceDocumentKey, entity: &str) -> usize {
    size_of::<PrefabProjectionStep>()
        + request_bytes(source)
        + key_bytes(key)
        + string_bytes(entity)
}

fn assert_provenance_budget(diagnostics: &DiagnosticReport, maximum: usize) -> usize {
    assert!(diagnostics.has_errors(), "{diagnostics:?}");
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "scene.prefab-expansion-budget-exceeded")
        .unwrap_or_else(|| panic!("missing budget diagnostic: {diagnostics:?}"));
    let field = |key: &str| {
        diagnostic
            .fields()
            .iter()
            .find(|field| field.key().as_str() == key)
            .unwrap()
            .value()
    };
    assert_eq!(
        field("budget-kind"),
        DiagnosticValueRef::Identifier("provenance-bytes"),
    );
    assert_eq!(
        field("maximum"),
        DiagnosticValueRef::Unsigned(maximum as u64)
    );
    let DiagnosticValueRef::Unsigned(observed) = field("observed") else {
        panic!("missing observed byte count: {diagnostics:?}");
    };
    assert!(observed > maximum as u64);
    assert!(diagnostics.retained_len() <= diagnostics.settings().entry_limit().get());
    assert!(diagnostics.retained_bytes() <= diagnostics.settings().byte_limit().get());
    observed as usize
}

struct AliasedSource {
    path: AssetRef,
    stable: AssetRef,
    key: SourceDocumentKey,
    document: Arc<PrefabDocument>,
}

impl AliasedSource {
    fn new(document: PrefabDocument) -> Self {
        let stable_id = StableAssetId::parse_str("62528c5d-6169-4a1a-9579-e2bf1c6b770c").unwrap();
        Self {
            path: AssetRef::path("source.prefab.json").unwrap(),
            stable: AssetRef::StableId(stable_id),
            key: SourceDocumentKey::stable_asset(stable_id, SourceDocumentKind::Prefab),
            document: Arc::new(document),
        }
    }
}

impl PrefabSourceResolver for AliasedSource {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<ResolvedPrefabSource> {
        (source == &self.path || source == &self.stable)
            .then(|| ResolvedPrefabSource::bound(Arc::clone(&self.document), self.key.clone()))
    }
}

#[test]
fn aliases_compare_canonical_ancestry_before_the_next_source_is_expanded() {
    let mut source = AliasedSource::new(PrefabDocument::default());
    source.document = Arc::new(PrefabDocument::from_ordered_roots([anchor(
        "recursive",
        source.stable.clone(),
    )]));
    let scene = SceneDocument::from_ordered_roots([anchor("outer", source.path.clone())]);
    let report = scene.expand_prefabs_with_options(
        &registry(),
        &source,
        PrefabExpansionOptions::default().with_max_depth(2),
    );
    assert!(report.candidate.is_none());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.prefab-cycle"),
        "{:?}",
        report.diagnostics
    );
    assert!(
        report
            .diagnostics
            .iter()
            .all(|diagnostic| { diagnostic.code().as_str() != "scene.prefab-depth-exceeded" })
    );
}

#[test]
fn sibling_aliases_share_a_source_but_keep_distinct_authored_placements() {
    let source = AliasedSource::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("local")),
    ]));
    let scene = SceneDocument::from_ordered_roots([
        anchor("left", source.path.clone()),
        anchor("right", source.stable.clone()),
    ]);
    let exact = size_of::<SceneEntityProvenanceMap>()
        + entry_bytes("left", "left")
        + entry_bytes("right", "right")
        + source_binding_bytes(&source.path, &source.key)
        + source_binding_bytes(&source.stable, &source.key)
        + string_bytes("left")
        + string_bytes("right")
        + step_bytes(&source.path, &source.key, "local")
        + step_bytes(&source.stable, &source.key, "local")
        + entry_bytes("prefab_0", "local")
        + entry_bytes("prefab_1", "local");
    let report = scene.expand_prefabs_with_options(&registry(), &source, provenance_options(exact));
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let provenance = report.provenance().unwrap();
    let projections = provenance
        .values()
        .filter_map(|entity| entity.projection_path())
        .collect::<Vec<_>>();
    assert_eq!(projections.len(), 2);
    assert_ne!(projections[0].root_anchor(), projections[1].root_anchor());
    for path in projections {
        assert_eq!(path.steps().len(), 1);
        assert_eq!(inherited_key(&path.steps()[0]), &source.key);
        assert_eq!(path.steps()[0].entity_id(), &id("local"));
    }
    let rejected =
        scene.expand_prefabs_with_options(&registry(), &source, provenance_options(exact - 1));
    assert!(rejected.candidate.is_none());
    assert_eq!(
        assert_provenance_budget(&rejected.diagnostics, exact - 1),
        exact
    );
}

#[test]
fn prefab_resolution_rejects_a_scene_key_instead_of_publishing_wrong_kind_provenance() {
    let mut source = AliasedSource::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("local")),
    ]));
    let AssetRef::StableId(stable_id) = source.stable else {
        unreachable!()
    };
    source.key = SourceDocumentKey::stable_asset(stable_id, SourceDocumentKind::Scene);
    let scene = SceneDocument::from_ordered_roots([anchor("outer", source.path.clone())]);
    let report = scene.expand_prefabs(&registry(), &source);
    assert!(report.candidate.is_none());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.prefab-source-kind-invalid" })
    );
}

#[test]
fn provenance_limit_defaults_to_64_mib_and_has_a_public_budget_kind() {
    assert_eq!(
        PrefabExpansionLimits::default().provenance_bytes().get(),
        64 * 1024 * 1024,
    );
    assert_eq!(
        PrefabExpansionBudgetKind::ProvenanceBytes.to_string(),
        "provenance-bytes",
    );
    assert_eq!(
        PrefabExpansionBudgetKind::OverlayDeclarations.to_string(),
        "overlay-declarations",
    );
    assert_eq!(
        PrefabExpansionLimits::default()
            .with_overlay_declarations(nara_core::ItemLimit::ONE)
            .overlay_declarations(),
        nara_core::ItemLimit::ONE,
    );
}

#[test]
fn long_source_keys_are_admitted_before_stack_clones_and_source_materialization() {
    let long_path = format!("prefabs/{}.prefab.json", "long-key".repeat(256));
    for key in [
        SourceDocumentKey::project_location(
            universe(),
            AssetPath::new(&long_path).unwrap(),
            SourceDocumentKind::Prefab,
        ),
        SourceDocumentKey::named(universe(), &long_path, SourceDocumentKind::Prefab).unwrap(),
    ] {
        // Invalid source structure must remain unvisited when the active source cannot fit.
        let mut source = AliasedSource::new(PrefabDocument {
            roots: vec![id("missing")],
            entities: vec![],
        });
        source.key = key;
        source.path = AssetRef::path(format!("requests/{long_path}")).unwrap();
        let scene = SceneDocument::from_ordered_roots([anchor("outer", source.path.clone())]);
        let stack_admission = size_of::<SceneEntityProvenanceMap>()
            + entry_bytes("outer", "outer")
            + source_binding_bytes(&source.path, &source.key);
        let rejected = scene.expand_prefabs_with_options(
            &registry(),
            &source,
            provenance_options(stack_admission - 1),
        );
        assert!(rejected.candidate.is_none());
        assert_eq!(
            assert_provenance_budget(&rejected.diagnostics, stack_admission - 1),
            stack_admission,
        );
        assert_eq!(rejected.diagnostics.iter().count(), 1);
        let structure_report = scene.expand_prefabs_with_options(
            &registry(),
            &source,
            provenance_options(stack_admission),
        );
        assert!(structure_report.candidate.is_none());
        assert!(structure_report.diagnostics.has_errors());
        assert!(structure_report.diagnostics.iter().all(|diagnostic| {
            diagnostic.code().as_str() != "scene.prefab-expansion-budget-exceeded"
        }));

        let leaf = "leaf".repeat(512);
        source.document = Arc::new(PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id(&leaf)),
        ]));
        let exact = stack_admission
            + string_bytes("outer")
            + step_bytes(&source.path, &source.key, &leaf)
            + entry_bytes("prefab_0", &leaf);
        let admitted =
            scene.expand_prefabs_with_options(&registry(), &source, provenance_options(exact));
        assert!(admitted.candidate.is_some(), "{:?}", admitted.diagnostics);
        let rejected =
            scene.expand_prefabs_with_options(&registry(), &source, provenance_options(exact - 1));
        assert!(rejected.candidate.is_none());
        assert_eq!(
            assert_provenance_budget(&rejected.diagnostics, exact - 1),
            exact
        );
    }
}

struct SourceChain(Vec<(AssetRef, SourceDocumentKey, Arc<PrefabDocument>)>);

impl SourceChain {
    fn new(depth: usize, leaves: usize) -> Self {
        let requests = (0..depth)
            .map(|index| AssetRef::path(format!("prefabs/{index}.prefab.json")).unwrap())
            .collect::<Vec<_>>();
        Self(
            requests
                .iter()
                .enumerate()
                .map(|(index, request)| {
                    let key = SourceDocumentKey::project_location(
                        universe(),
                        AssetPath::new(
                            format!("prefabs/{index}/{}.prefab.json", "x".repeat(2048),),
                        )
                        .unwrap(),
                        SourceDocumentKind::Prefab,
                    );
                    let document = if let Some(next) = requests.get(index + 1) {
                        PrefabDocument::from_ordered_roots([anchor("nested", next.clone())])
                    } else {
                        PrefabDocument::from_ordered_roots(
                            (0..leaves)
                                .map(|index| SceneEntityRecord::new(id(&format!("leaf_{index}")))),
                        )
                    };
                    (request.clone(), key, Arc::new(document))
                })
                .collect(),
        )
    }
}

impl PrefabSourceResolver for SourceChain {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<ResolvedPrefabSource> {
        self.0
            .iter()
            .find(|(request, _, _)| request == source)
            .map(|(_, key, document)| {
                ResolvedPrefabSource::bound(Arc::clone(document), key.clone())
            })
    }
}

#[test]
fn nested_projection_budget_counts_the_full_parent_key_for_every_leaf() {
    let sources = SourceChain::new(2, 2);
    let (outer_request, outer_key, _) = &sources.0[0];
    let (inner_request, inner_key, _) = &sources.0[1];
    let scene = SceneDocument::from_ordered_roots([anchor("outer", outer_request.clone())]);
    let parent_step = step_bytes(outer_request, outer_key, "nested");
    let exact = size_of::<SceneEntityProvenanceMap>()
        + entry_bytes("outer", "outer")
        + source_binding_bytes(outer_request, outer_key)
        + source_binding_bytes(inner_request, inner_key)
        + string_bytes("outer")
        + parent_step
        + entry_bytes("prefab_0", "nested")
        + string_bytes("outer")
        + parent_step
        + step_bytes(inner_request, inner_key, "leaf_0")
        + entry_bytes("prefab_1", "leaf_0")
        + string_bytes("outer")
        + parent_step
        + step_bytes(inner_request, inner_key, "leaf_1")
        + entry_bytes("prefab_2", "leaf_1");
    let admitted =
        scene.expand_prefabs_with_options(&registry(), &sources, provenance_options(exact));
    assert!(admitted.candidate.is_some(), "{:?}", admitted.diagnostics);
    let provenance = admitted.provenance().unwrap();
    for leaf in ["leaf_0", "leaf_1"] {
        let path = provenance
            .values()
            .find(|source| source.local_entity_id() == &id(leaf))
            .unwrap()
            .projection_path()
            .unwrap();
        assert_eq!(path.root_anchor(), &id("outer"));
        assert_eq!(path.steps().len(), 2);
        assert_eq!(inherited_key(&path.steps()[0]), outer_key);
        assert_eq!(inherited_key(&path.steps()[1]), inner_key);
    }
    let rejected =
        scene.expand_prefabs_with_options(&registry(), &sources, provenance_options(exact - 1));
    assert!(rejected.candidate.is_none());
    assert_eq!(
        assert_provenance_budget(&rejected.diagnostics, exact - 1),
        exact
    );
}

#[test]
fn default_provenance_rejection_before_source_binding_preserves_world_and_identity() {
    let registry = registry();
    let mut world = World::new();
    let existing = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let existing_spawn = nara_scene::spawn_scene(
        &mut world,
        &registry,
        &nara_scene::ExpandedSceneCandidate::plain(
            scene_source_key(
                "default_provenance_rejection_through_public_spawn_preserves_world_and_identity/existing",
            ),
            std::sync::Arc::new((existing).clone()),
        ),
        &scene_occurrence(
            "default_provenance_rejection_through_public_spawn_preserves_world_and_identity/existing_spawn",
        ),
        None,
    );
    assert!(
        existing_spawn.instance.is_some(),
        "{:?}",
        existing_spawn.diagnostics,
    );
    world.increment_change_tick();
    let before_entities = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    let before_lookup = existing_spawn
        .instance
        .as_ref()
        .unwrap()
        .resolve(&world, &id("existing"));
    let nara_identity::EntityLookup::Resolved(existing_entity) = before_lookup else {
        panic!("existing Scene entity did not resolve: {before_lookup:?}");
    };
    let before_source = world
        .get::<SceneEntitySource>(existing_entity)
        .unwrap()
        .clone();
    let before_domain = world.resource::<WorldIdentityDomain>().id();
    let before_identity = world.resource::<WorldIdentityDomain>().stats();
    let before_ticks = world
        .get_resource_change_ticks::<WorldIdentityDomain>()
        .map(|ticks| (ticks.added, ticks.changed));
    let before_roots = world.resource::<HierarchyRoots>().as_slice().to_vec();

    // Short requests and IDs isolate the repeated 2 KiB canonical ProjectLocation keys.
    // The default depth/entity/value/identifier budgets all permit this finite source tree.
    let sources = SourceChain::new(30, 1_200);
    let scene = SceneDocument::from_ordered_roots([anchor("outer", sources.0[0].0.clone())]);
    let rejected = (scene).expand_prefabs(&registry, &sources);
    assert!(rejected.candidate.is_none());
    assert_provenance_budget(&rejected.diagnostics, 64 * 1024 * 1024);
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        before_entities,
    );
    assert_eq!(
        world.get::<SceneEntitySource>(existing_entity),
        Some(&before_source),
    );
    assert_eq!(world.resource::<WorldIdentityDomain>().id(), before_domain);
    assert_eq!(
        world.resource::<WorldIdentityDomain>().stats(),
        before_identity
    );
    assert_eq!(
        existing_spawn
            .instance
            .as_ref()
            .unwrap()
            .resolve(&world, &id("existing")),
        before_lookup,
    );
    assert_eq!(
        world
            .get_resource_change_ticks::<WorldIdentityDomain>()
            .map(|ticks| (ticks.added, ticks.changed)),
        before_ticks,
    );
    assert_eq!(world.resource::<HierarchyRoots>().as_slice(), before_roots);
}

fn scene_source_key(name: &str) -> nara_scene::SourceDocumentKey {
    nara_scene::SourceDocumentKey::named(
        "ae9c4204-2585-4d22-9da7-6400ebd89664".parse().unwrap(),
        name,
        nara_scene::SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn scene_occurrence(slot: &str) -> nara_scene::SceneOccurrenceKey {
    nara_scene::SceneOccurrenceKey::new("nara.scene.tests", slot).unwrap()
}
