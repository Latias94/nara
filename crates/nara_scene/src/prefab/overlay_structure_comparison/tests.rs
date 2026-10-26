use super::*;
use crate::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, InMemoryPrefabSourceResolver,
    PrefabDocument, PrefabInstance, SceneDocumentCandidate, SourceDocumentKind,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentRegistry, ComponentSchema,
    ComponentSchemaCatalog, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "a8cc4554-90fd-428e-8aa8-f6c4dfbf6060".parse().unwrap(),
        name,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn path(name: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/{name}.prefab.json")).unwrap()
}

fn inherited(steps: &[(&str, &str)]) -> Target {
    let (_, terminal) = steps.last().unwrap();
    Target {
        through: steps
            .windows(2)
            .map(|pair| InstanceHop {
                anchor: EntityOrigin::Source(id(pair[0].1)),
                expected_base: key(pair[1].0),
            })
            .collect(),
        entity: EntityOrigin::Source(id(terminal)),
    }
}

fn direct(entity: &str) -> Target {
    inherited(&[("outer", entity)])
}
fn nested(anchor: &str, entity: &str) -> Target {
    inherited(&[("outer", anchor), ("leaf", entity)])
}
fn added(entity: &str) -> Target {
    Target::added(id(entity))
}

fn instance(name: &str, source: AssetRef) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(id(name));
    record.prefab = Some(PrefabInstance {
        source,
        overrides: ScenePatchDocument::default(),
    });
    record
}

fn outer(order: &[&str]) -> PrefabDocument {
    PrefabDocument::from_ordered_roots(order.iter().map(|name| {
        if name.starts_with("nest_") {
            instance(name, path("leaf"))
        } else {
            SceneEntityRecord::new(id(name))
        }
    }))
}

fn resolver(outer: PrefabDocument) -> InMemoryPrefabSourceResolver {
    InMemoryPrefabSourceResolver::new()
        .with_prefab(path("outer"), key("outer"), outer.clone())
        .with_prefab(path("alias"), key("outer"), outer)
        .with_prefab(
            path("leaf"),
            key("leaf"),
            PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("socket")),
                SceneEntityRecord::new(id("tip")),
            ]),
        )
}

fn registry() -> ComponentRegistry {
    #[derive(nara_ecs::Component)]
    struct Link(ComponentValue);
    let schema = schemas().owners()[0].current().components[0].clone();
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<Link, _, _>(
            schema,
            |value| Ok(Link(value.clone())),
            |value| Ok(value.0.clone()),
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn projection(outer: PrefabDocument, alias: bool) -> Projection {
    let source = if alias { path("alias") } else { path("outer") };
    let scene = SceneDocument::from_ordered_roots([
        instance("host_a", source),
        instance("host_b", path("outer")),
        SceneEntityRecord::new(id("unrelated")),
        SceneEntityRecord::new(id("socket")),
    ]);
    Projection::expand(id("host_a"), &scene, &registry(), &resolver(outer))
}

fn mixed_origin_projection() -> Projection {
    let mut inherited_anchor = instance("nested", path("leaf"));
    inherited_anchor.prefab.as_mut().unwrap().overrides =
        ScenePatchDocument::new([ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(id("added_leaf")),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        }]);
    let source = PrefabDocument::from_ordered_roots([
        inherited_anchor,
        SceneEntityRecord::new(id("added_leaf")),
    ]);
    let resolver = resolver(source);
    let scene = SceneDocument::from_ordered_roots(["host_a", "host_b"].map(|name| {
        let mut added_anchor = instance("added_anchor", path("leaf"));
        added_anchor.prefab.as_mut().unwrap().overrides =
            ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(id("added_leaf")),
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            }]);
        let mut anchor = instance(name, path("outer"));
        anchor.prefab.as_mut().unwrap().overrides =
            ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                entity: added_anchor,
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            }]);
        anchor
    }));
    Projection::expand(id("host_a"), &scene, &registry(), &resolver)
}

#[test]
fn target_index_keeps_all_mixed_origin_paths_in_one_repeated_placement() {
    let projection = mixed_origin_projection();
    let expected = projection
        .expanded
        .provenance()
        .values()
        .filter(|origin| {
            origin
                .projection_path()
                .is_some_and(|path| path.root_anchor() == &id("host_a"))
        })
        .count();
    assert_eq!(projection.targets().unwrap().len(), expected);
}

fn inside(anchor: EntityOrigin, base: &str, entity: EntityOrigin) -> Target {
    Target {
        through: vec![InstanceHop {
            anchor,
            expected_base: key(base),
        }],
        entity,
    }
}

#[test]
fn mixed_origin_targets_bind_references_without_editing_the_repeated_occurrence() {
    let projection = mixed_origin_projection();
    let inherited_addition = inside(
        EntityOrigin::Source(id("nested")),
        "leaf",
        EntityOrigin::Addition(id("added_leaf")),
    );
    let added_inherited = inside(
        EntityOrigin::Addition(id("added_anchor")),
        "leaf",
        EntityOrigin::Source(id("socket")),
    );
    let added_addition = inside(
        EntityOrigin::Addition(id("added_anchor")),
        "leaf",
        EntityOrigin::Addition(id("added_leaf")),
    );
    let mut intent = overlay();
    intent.components = vec![
        link(
            inherited_addition.clone(),
            Some(added_inherited.clone()),
            "inherited addition",
        ),
        link(
            added_inherited.clone(),
            Some(added_addition.clone()),
            "added anchor source",
        ),
        link(
            added_addition.clone(),
            Some(direct("added_leaf")),
            "added anchor addition",
        ),
    ];
    let roundtrip: Overlay = serde_json::from_slice(&serde_json::to_vec(&intent).unwrap()).unwrap();
    let base = open(projection.expanded.document());
    let before = base.revision();
    let (candidate, targets) = prepare(&projection, &base, &roundtrip).unwrap();
    let candidate = candidate.unwrap();
    for (from, to, note) in [
        (&inherited_addition, &added_inherited, "inherited addition"),
        (&added_inherited, &added_addition, "added anchor source"),
        (
            &added_addition,
            &direct("added_leaf"),
            "added anchor addition",
        ),
    ] {
        assert_link(candidate.document(), &targets[from], &targets[to], note);
    }
    assert_eq!(candidate.provenance(), projection.expanded.provenance());
    for (entity, origin) in projection.expanded.provenance() {
        if origin
            .projection_path()
            .is_some_and(|path| path.root_anchor() == &id("host_b"))
        {
            assert_eq!(
                candidate
                    .document()
                    .entities
                    .iter()
                    .find(|record| &record.id == entity),
                projection
                    .expanded
                    .document()
                    .entities
                    .iter()
                    .find(|record| &record.id == entity)
            );
        }
    }
    assert_eq!(base.document(), projection.expanded.document());
    assert_eq!(base.revision(), before);
    assert_eq!(base.history_status().undo_depth, 0);
}

fn addition_only_projection(
    root_base: &str,
    nested_base: &str,
    alias: bool,
    nested: bool,
) -> Projection {
    let addition = ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(id("only_addition")),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    };
    let mut nested_anchor = instance(
        "nested",
        if alias {
            path("nested_alias")
        } else {
            path("inner")
        },
    );
    nested_anchor.prefab.as_mut().unwrap().overrides = ScenePatchDocument::new([addition.clone()]);
    let root = if nested {
        PrefabDocument::from_ordered_roots([nested_anchor])
    } else {
        PrefabDocument::default()
    };
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(path("outer"), key(root_base), root.clone())
        .with_prefab(path("alias"), key(root_base), root)
        .with_prefab(path("inner"), key(nested_base), PrefabDocument::default())
        .with_prefab(
            path("nested_alias"),
            key(nested_base),
            PrefabDocument::default(),
        );
    let mut anchor = instance("host_a", if alias { path("alias") } else { path("outer") });
    if !nested {
        anchor.prefab.as_mut().unwrap().overrides = ScenePatchDocument::new([addition]);
    }
    Projection::expand(
        id("host_a"),
        &SceneDocument::from_ordered_roots([anchor]),
        &registry(),
        &resolver,
    )
}

#[test]
fn addition_only_targets_still_bind_root_and_nested_bases_while_aliases_keep_meaning() {
    for nested in [false, true] {
        let original = addition_only_projection("outer", "inner", false, nested);
        let target = if nested {
            inside(
                EntityOrigin::Source(id("nested")),
                "inner",
                EntityOrigin::Addition(id("only_addition")),
            )
        } else {
            added("only_addition")
        };
        let mut intent = overlay();
        intent
            .components
            .push(link(target.clone(), None, "bound addition"));
        let aliased = addition_only_projection("outer", "inner", true, nested);
        let (effective, targets) = prepare_document(&aliased, &intent);
        assert!(
            effective
                .entities
                .iter()
                .any(|record| record.id == targets[&target])
        );
        let changed_root = addition_only_projection("replacement_root", "inner", false, nested);
        assert_eq!(
            assert_conflict(&changed_root, &intent),
            Conflict::SourceChanged
        );
        if nested {
            let changed_inner = addition_only_projection("outer", "replacement_inner", false, true);
            assert_eq!(
                assert_conflict(&changed_inner, &intent),
                Conflict::MissingTarget(target.clone())
            );
            let ComponentIntent::Set { target, .. } = &mut intent.components[0] else {
                unreachable!()
            };
            target.through[0].expected_base = key("replacement_inner");
            assert!(
                prepare_document(&changed_inner, &intent)
                    .0
                    .entities
                    .iter()
                    .any(|record| !record.components.is_empty())
            );
        }
        assert_eq!(
            original.targets().unwrap().len(),
            if nested { 2 } else { 1 }
        );
    }
}

#[test]
fn canonical_source_codec_retains_kind_through_overlay_and_nested_binding_round_trips() {
    let projection = mixed_origin_projection();
    let mut intent = overlay();
    let target = inside(
        EntityOrigin::Addition(id("added_anchor")),
        "leaf",
        EntityOrigin::Source(id("socket")),
    );
    let referent = inside(
        EntityOrigin::Source(id("nested")),
        "leaf",
        EntityOrigin::Addition(id("added_leaf")),
    );
    intent
        .components
        .push(link(target.clone(), Some(referent), "bound"));
    intent.placements.push(place(
        target,
        Parent::Entity(added("added_anchor")),
        Slot::Before(inside(
            EntityOrigin::Addition(id("added_anchor")),
            "leaf",
            EntityOrigin::Source(id("tip")),
        )),
    ));
    let encoded = ron::ser::to_string(&intent).unwrap();
    let loaded: Overlay = ron::from_str(&encoded).unwrap();
    assert_eq!(loaded, intent);
    let expected = prepare_document(&projection, &intent).0;
    assert_eq!(prepare_document(&projection, &loaded).0, expected);
    let json_loaded: Overlay =
        serde_json::from_slice(&serde_json::to_vec(&intent).unwrap()).unwrap();
    assert_eq!(json_loaded, intent);
    assert_eq!(prepare_document(&projection, &json_loaded).0, expected);

    let mut wrong_kind = loaded.clone();
    wrong_kind.source = SourceDocumentKey::named(
        match key("outer").origin() {
            crate::SourceDocumentOrigin::Named { universe, .. } => *universe,
            _ => unreachable!(),
        },
        "outer",
        SourceDocumentKind::Scene,
    )
    .unwrap();
    assert_eq!(
        assert_conflict(&projection, &wrong_kind),
        Conflict::SourceChanged
    );

    let mut wrong_nested_kind = loaded;
    let ComponentIntent::Set {
        target: selected, ..
    } = &mut wrong_nested_kind.components[0]
    else {
        unreachable!()
    };
    let crate::SourceDocumentOrigin::Named { universe, .. } = key("leaf").origin().clone() else {
        unreachable!()
    };
    selected.through[0].expected_base =
        SourceDocumentKey::named(universe, "leaf", SourceDocumentKind::Scene).unwrap();
    let rejected = selected.clone();
    let conflict = assert_conflict(&projection, &wrong_nested_kind);
    assert_eq!(conflict, Conflict::MissingTarget(rejected));
    let unresolved = UnresolvedOverlay {
        overlay: wrong_nested_kind,
        conflict,
    };
    let encoded = ron::to_string(&unresolved).unwrap();
    assert_eq!(
        ron::from_str::<UnresolvedOverlay>(&encoded).unwrap(),
        unresolved
    );
}

fn schemas() -> AuthoringSchemaBundle {
    let schema = ComponentSchema::new(
        ComponentTypeId::new("game.Link"),
        "Link",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueShape::optional(ComponentValueKind::EntityRef.into()),
        )
        .with_capabilities([
            ComponentCapability::Scene,
            ComponentCapability::Inspect,
            ComponentCapability::Edit,
            ComponentCapability::EntityRef,
        ]),
        ComponentFieldSchema::required(
            ComponentFieldId::new("note"),
            "Note",
            ComponentFieldPath::from_fields(["note"]),
            ComponentValueKind::String,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
    .with_authoring_initial_value(ComponentValue::map([
        ("target", ComponentValue::Null),
        ("note", ComponentValue::String("initial".into())),
    ]));
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.overlay-comparison").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn open(document: &SceneDocument) -> SceneAuthoringSession {
    let bundle = schemas();
    open_with_bundle(document, &bundle)
}

fn open_with_bundle(
    document: &SceneDocument,
    bundle: &AuthoringSchemaBundle,
) -> SceneAuthoringSession {
    let adapters = bundle
        .owners()
        .iter()
        .map(|owner| {
            AuthoringAdapterReceipt::from_schema_owner(owner, &Default::default()).unwrap()
        })
        .collect::<Vec<_>>();
    SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    AssetRef::path("scenes/comparison.scene.json").unwrap(),
                ),
                bundle,
                &adapters,
            )
            .unwrap()
            .publish(),
    )
}

fn overlay() -> Overlay {
    Overlay {
        source: key("outer"),
        additions: vec![],
        placements: vec![],
        suppressions: vec![],
        components: vec![],
    }
}

fn place(target: Target, parent: Parent, slot: Slot) -> Placement {
    Placement {
        target,
        parent,
        slot,
    }
}

fn children(document: &SceneDocument, parent: &SceneEntityId) -> Vec<SceneEntityId> {
    document
        .entities
        .iter()
        .find(|record| &record.id == parent)
        .unwrap()
        .children
        .clone()
}

fn prepare_document(projection: &Projection, overlay: &Overlay) -> (SceneDocument, TargetMap) {
    let base = open(projection.expanded.document());
    let revision = base.revision();
    let history = base.history_status();
    let (prepared, targets) = prepare(projection, &base, overlay).unwrap();
    let result = prepared
        .map(|edit| edit.document().clone())
        .unwrap_or_else(|| base.document().clone());
    assert_eq!(base.document(), projection.expanded.document());
    assert_eq!(base.revision(), revision);
    assert_eq!(base.history_status(), history);
    (result, targets)
}

fn assert_conflict(projection: &Projection, overlay: &Overlay) -> Conflict {
    let base = open(projection.expanded.document());
    let history = base.history_status();
    let revision = base.revision();
    let error = prepare(projection, &base, overlay).unwrap_err();
    assert_eq!(base.document(), projection.expanded.document());
    assert_eq!(base.history_status(), history);
    assert_eq!(base.revision(), revision);
    error
}

#[test]
fn desired_placement_rebases_when_the_real_exact_prefab_move_no_longer_applies() {
    let original = outer(&["socket", "nest_a", "nest_b", "tail"]);
    let from = ScenePlacement::current_prefab(&original, &id("tail")).unwrap();
    let patch = ScenePatchDocument::new([ScenePatchOperation::MoveEntity {
        entity: id("tail"),
        from,
        to: ScenePlacement::after(ScenePlacementOwner::Root, id("socket")),
    }]);
    let mut incumbent = instance("host_a", path("outer"));
    incumbent.prefab.as_mut().unwrap().overrides = patch;
    let incumbent = SceneDocument::from_ordered_roots([incumbent]);
    assert!(
        incumbent
            .expand_prefabs(&registry(), &resolver(original))
            .candidate
            .is_some()
    );
    let changed = outer(&["tail", "nest_a", "socket", "nest_b", "new_base"]);
    assert!(
        incumbent
            .expand_prefabs(&registry(), &resolver(changed.clone()))
            .candidate
            .is_none()
    );
    let projection = projection(changed, false);
    let mut intent = overlay();
    intent.placements.push(place(
        direct("tail"),
        Parent::Root,
        Slot::After(direct("socket")),
    ));
    let (effective, targets) = prepare_document(&projection, &intent);
    assert_eq!(
        children(&effective, &id("host_a")),
        ["nest_a", "socket", "tail", "nest_b", "new_base"]
            .map(|name| targets[&direct(name)].clone())
    );
    for (entity, provenance) in projection.expanded.provenance() {
        if provenance
            .projection_path()
            .is_some_and(|path| path.root_anchor() == &id("host_b"))
        {
            assert_eq!(
                effective
                    .entities
                    .iter()
                    .find(|record| &record.id == entity),
                projection
                    .expanded
                    .document()
                    .entities
                    .iter()
                    .find(|record| &record.id == entity)
            );
        }
    }
    let bytes = serde_json::to_vec(&intent).unwrap();
    assert!(
        !String::from_utf8(bytes.clone())
            .unwrap()
            .contains("prefab_")
    );
    assert_eq!(serde_json::from_slice::<Overlay>(&bytes).unwrap(), intent);
}

#[test]
fn normalized_placement_cohorts_have_authored_order_not_allocation_or_replay_order() {
    let projection = projection(outer(&["socket", "tail"]), false);
    let mut intent = overlay();
    intent.additions = vec![id("second"), id("first")];
    intent.placements = vec![
        place(added("first"), Parent::Root, Slot::Before(direct("tail"))),
        place(added("second"), Parent::Root, Slot::Before(direct("tail"))),
    ];
    for reverse_allocations in [false, true] {
        if reverse_allocations {
            intent.additions.reverse();
        }
        let (effective, targets) = prepare_document(&projection, &intent);
        assert_eq!(
            children(&effective, &id("host_a")),
            [
                targets[&direct("socket")].clone(),
                targets[&added("first")].clone(),
                targets[&added("second")].clone(),
                targets[&direct("tail")].clone()
            ]
        );
    }
    intent.placements.reverse();
    let (effective, targets) = prepare_document(&projection, &intent);
    assert_eq!(
        children(&effective, &id("host_a")),
        [
            targets[&direct("socket")].clone(),
            targets[&added("second")].clone(),
            targets[&added("first")].clone(),
            targets[&direct("tail")].clone()
        ]
    );
}

#[test]
fn parent_rotation_moves_the_new_ancestor_first_without_a_transient_cycle() {
    let projection = projection(outer(&["socket", "nest_a", "tail"]), false);
    let mut intent = overlay();
    intent.placements = vec![
        place(
            direct("nest_a"),
            Parent::Entity(nested("nest_a", "socket")),
            Slot::Last,
        ),
        place(nested("nest_a", "socket"), Parent::Root, Slot::First),
    ];
    let (effective, targets) = prepare_document(&projection, &intent);
    assert_eq!(
        children(&effective, &targets[&nested("nest_a", "socket")]),
        [targets[&direct("nest_a")].clone()]
    );
    assert_eq!(
        children(&effective, &targets[&direct("nest_a")]),
        [targets[&nested("nest_a", "tip")].clone()]
    );
}

#[test]
fn suppression_is_instance_relative_and_can_preserve_an_explicitly_reparented_child() {
    let projection = projection(outer(&["socket", "nest_a", "nest_b", "tail"]), false);
    let mut intent = overlay();
    intent
        .placements
        .push(place(nested("nest_a", "socket"), Parent::Root, Slot::Last));
    intent.suppressions.push(direct("nest_a"));
    let (effective, targets) = prepare_document(&projection, &intent);
    assert!(targets.contains_key(&nested("nest_a", "socket")));
    assert!(!targets.contains_key(&nested("nest_a", "tip")));
    assert!(!targets.contains_key(&direct("nest_a")));
    assert!(targets.contains_key(&nested("nest_b", "socket")));
    assert!(
        effective
            .entities
            .iter()
            .all(|record| record.id != projection.targets().unwrap()[&direct("nest_a")])
    );
}

#[test]
fn invalid_or_ambiguous_structure_retains_a_round_trippable_conflict_without_publication() {
    let projection = projection(outer(&["socket", "nest_a", "nest_b", "tail"]), false);
    let cases = [
        vec![place(
            direct("socket"),
            Parent::Root,
            Slot::Before(direct("absent")),
        )],
        vec![place(
            nested("nest_a", "socket"),
            Parent::Entity(direct("nest_a")),
            Slot::Before(nested("nest_b", "socket")),
        )],
        vec![
            place(direct("socket"), Parent::Root, Slot::After(direct("tail"))),
            place(direct("tail"), Parent::Root, Slot::After(direct("socket"))),
        ],
        vec![place(
            direct("nest_a"),
            Parent::Entity(nested("nest_a", "socket")),
            Slot::Last,
        )],
        vec![
            place(direct("socket"), Parent::Root, Slot::Last),
            place(direct("socket"), Parent::Root, Slot::First),
        ],
    ];
    for placements in cases {
        let mut intent = overlay();
        intent.placements = placements;
        let conflict = assert_conflict(&projection, &intent);
        let file = UnresolvedOverlay {
            overlay: intent,
            conflict,
        };
        let bytes = serde_json::to_vec(&file).unwrap();
        assert_eq!(
            serde_json::from_slice::<UnresolvedOverlay>(&bytes).unwrap(),
            file
        );
    }
    let mut intent = overlay();
    intent.suppressions = vec![direct("nest_a"), nested("nest_a", "socket")];
    assert_eq!(
        assert_conflict(&projection, &intent),
        Conflict::DuplicateIntent
    );
    intent.suppressions.reverse();
    assert_eq!(
        assert_conflict(&projection, &intent),
        Conflict::DuplicateIntent
    );
}

#[test]
fn removed_anchor_conflicts_can_be_saved_and_reprepared_when_the_source_returns() {
    let missing = projection(outer(&["socket"]), false);
    let mut intent = overlay();
    intent.placements.push(place(
        direct("socket"),
        Parent::Root,
        Slot::Before(direct("tail")),
    ));
    let conflict = assert_conflict(&missing, &intent);
    let file = UnresolvedOverlay {
        overlay: intent,
        conflict,
    };
    let saved = serde_json::to_vec(&file).unwrap();
    let loaded: UnresolvedOverlay = serde_json::from_slice(&saved).unwrap();
    let restored = projection(outer(&["tail", "socket"]), true);
    let (effective, targets) = prepare_document(&restored, &loaded.overlay);
    assert_eq!(
        children(&effective, &id("host_a")),
        [
            targets[&direct("socket")].clone(),
            targets[&direct("tail")].clone()
        ]
    );
}

#[test]
fn an_unchanged_projection_needs_no_edit_but_explicit_intent_remains_authored() {
    let projection = projection(outer(&["socket", "tail"]), false);
    let base = open(projection.expanded.document());
    let mut intent = overlay();
    intent.placements.push(place(
        direct("tail"),
        Parent::Root,
        Slot::After(direct("socket")),
    ));
    assert!(prepare(&projection, &base, &intent).unwrap().0.is_none());
    assert_eq!(intent.placements.len(), 1);
    intent.source = key("different");
    assert_eq!(
        assert_conflict(&projection, &intent),
        Conflict::SourceChanged
    );
}

fn projection_with_containing_children() -> Projection {
    let mut host = instance("host_a", path("outer"));
    host.children = vec![id("local_a"), id("local_b")];
    let mut local = SceneEntityRecord::new(id("local_a"));
    local.children.push(id("local_descendant"));
    let scene = SceneDocument::new(
        [id("host_a")],
        [
            host,
            local,
            SceneEntityRecord::new(id("local_b")),
            SceneEntityRecord::new(id("local_descendant")),
        ],
    );
    Projection::expand(
        id("host_a"),
        &scene,
        &registry(),
        &resolver(outer(&["socket", "tail"])),
    )
}

#[test]
fn an_empty_overlay_preserves_containing_owner_children_without_an_edit() {
    let projection = projection_with_containing_children();
    let base = open(projection.expanded.document());
    let (prepared, targets) = prepare(&projection, &base, &overlay()).unwrap();
    assert!(prepared.is_none());
    assert_eq!(targets.len(), 2);
    assert_eq!(base.document(), projection.expanded.document());
    assert_eq!(base.history_status().undo_depth, 0);
}

#[test]
fn projected_reordering_and_additions_preserve_containing_owner_records_and_order() {
    let projection = projection_with_containing_children();
    let mut intent = overlay();
    intent.placements = vec![place(direct("tail"), Parent::Root, Slot::First)];
    let (effective, targets) = prepare_document(&projection, &intent);
    assert_eq!(
        children(&effective, &id("host_a")),
        [
            targets[&direct("tail")].clone(),
            targets[&direct("socket")].clone(),
            id("local_a"),
            id("local_b")
        ]
    );
    intent.additions.push(id("addition"));
    intent
        .placements
        .push(place(added("addition"), Parent::Root, Slot::Last));
    let (effective, targets) = prepare_document(&projection, &intent);
    let projected = children(&effective, &id("host_a"))
        .into_iter()
        .filter(|child| targets.values().any(|entity| entity == child))
        .collect::<Vec<_>>();
    assert_eq!(
        projected,
        [
            targets[&direct("tail")].clone(),
            targets[&direct("socket")].clone(),
            targets[&added("addition")].clone()
        ]
    );
    let locals = children(&effective, &id("host_a"))
        .into_iter()
        .filter(|child| !targets.values().any(|entity| entity == child))
        .collect::<Vec<_>>();
    assert_eq!(locals, [id("local_a"), id("local_b")]);
    for name in ["local_a", "local_b", "local_descendant"] {
        assert_eq!(
            effective
                .entities
                .iter()
                .find(|record| record.id == id(name)),
            projection
                .expanded
                .document()
                .entities
                .iter()
                .find(|record| record.id == id(name))
        );
    }
}

fn link(target: Target, referent: Option<Target>, note: &str) -> ComponentIntent {
    ComponentIntent::Set {
        mode: ComponentSetMode::Add,
        target,
        component: ComponentTypeId::new("game.Link"),
        version: ComponentSchemaVersion::ONE,
        value: ComponentValue::map([
            (
                "target",
                referent.as_ref().map_or(ComponentValue::Null, |_| {
                    ComponentValue::EntityReference(nara_identity::EntityReference::SceneLocal {
                        entity: id("socket"),
                    })
                }),
            ),
            ("note", ComponentValue::String(note.into())),
        ]),
        bindings: referent
            .into_iter()
            .map(|target| ReferenceBinding {
                slot: ComponentFieldPath::from_fields(["target"]),
                expected: id("socket"),
                target,
            })
            .collect(),
    }
}

fn assert_link(
    document: &SceneDocument,
    entity: &SceneEntityId,
    referent: &SceneEntityId,
    note: &str,
) {
    let record = document
        .entities
        .iter()
        .find(|record| &record.id == entity)
        .unwrap();
    assert_eq!(
        record.components[&ComponentTypeId::new("game.Link")].value,
        ComponentValue::map([
            (
                "target",
                ComponentValue::EntityReference(nara_identity::EntityReference::SceneLocal {
                    entity: referent.clone()
                })
            ),
            ("note", ComponentValue::String(note.into())),
        ])
    );
}

fn link_source(note: &str, target: ComponentValue) -> PrefabDocument {
    let mut source = outer(&["socket", "nest_a", "nest_b", "tail"]);
    source
        .entities
        .iter_mut()
        .find(|record| record.id == id("socket"))
        .unwrap()
        .components
        .insert(
            ComponentTypeId::new("game.Link"),
            crate::SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    ("target", target),
                    ("note", ComponentValue::String(note.into())),
                ]),
            ),
        );
    source
}

fn property_intent(
    path: &str,
    intent: PropertyIntent,
    bindings: Vec<ReferenceBinding>,
) -> ComponentIntent {
    ComponentIntent::Properties {
        target: direct("socket"),
        component: ComponentTypeId::new("game.Link"),
        version: ComponentSchemaVersion::ONE,
        claims: vec![BoundPropertyClaim {
            claim: PropertyClaim {
                path: nara_reflect::ComponentPropertyPath::new(ComponentFieldId::new(path)),
                intent,
            },
            bindings,
        }],
    }
}

#[test]
fn property_claims_rebase_leaf_pins_without_freezing_uncovered_reference_siblings() {
    let new_base = projection(
        link_source(
            "new base",
            ComponentValue::EntityReference(nara_identity::EntityReference::SceneLocal {
                entity: id("tail"),
            }),
        ),
        true,
    );
    let mut intent = overlay();
    intent.components.push(property_intent(
        "note",
        PropertyIntent::Set(ComponentValue::String("old base".into())),
        vec![],
    ));
    let (effective, targets) = prepare_document(&new_base, &intent);
    assert_link(
        &effective,
        &targets[&direct("socket")],
        &targets[&direct("tail")],
        "old base",
    );
    let mut reverted = intent.clone();
    reverted.components.clear();
    let (inherited, targets) = prepare_document(&new_base, &reverted);
    assert_link(
        &inherited,
        &targets[&direct("socket")],
        &targets[&direct("tail")],
        "new base",
    );
    assert_eq!(intent.components.len(), 1);
}

#[test]
fn scalar_property_payloads_use_empty_slots_and_default_absence_cannot_borrow_scene_names() {
    let projection = projection(link_source("kept", ComponentValue::Null), false);
    let referent = nested("nest_b", "socket");
    let binding = ReferenceBinding {
        slot: ComponentFieldPath::empty(),
        expected: id("socket"),
        target: referent.clone(),
    };
    let mut intent = overlay();
    intent.components.push(property_intent(
        "target",
        PropertyIntent::Set(ComponentValue::EntityReference(
            nara_identity::EntityReference::SceneLocal {
                entity: id("socket"),
            },
        )),
        vec![binding.clone()],
    ));
    let (effective, targets) = prepare_document(&projection, &intent);
    assert_link(
        &effective,
        &targets[&direct("socket")],
        &targets[&referent],
        "kept",
    );
    intent.suppressions.push(referent.clone());
    assert_eq!(
        assert_conflict(&projection, &intent),
        Conflict::OwnerAdmission
    );
    intent.suppressions.clear();
    let mut catalog = schemas().owners()[0].current().clone();
    let field = catalog.components[0]
        .fields
        .iter_mut()
        .find(|field| field.id() == &ComponentFieldId::new("target"))
        .unwrap();
    field.required = false;
    field.default_value = Some(ComponentValue::EntityReference(
        nara_identity::EntityReference::SceneLocal {
            entity: id("socket"),
        },
    ));
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.overlay-comparison").unwrap(),
        catalog,
        None,
    )])
    .unwrap();
    let base = open_with_bundle(projection.expanded.document(), &bundle);
    intent.components = vec![property_intent("target", PropertyIntent::Absent, vec![])];
    assert_eq!(
        prepare(&projection, &base, &intent).unwrap_err(),
        Conflict::Reference
    );
    let ComponentIntent::Properties { claims, .. } = &mut intent.components[0] else {
        unreachable!()
    };
    claims[0].bindings.push(binding);
    let (candidate, targets) = prepare(&projection, &base, &intent).unwrap();
    assert_link(
        candidate.unwrap().document(),
        &targets[&direct("socket")],
        &targets[&referent],
        "kept",
    );
    assert_ne!(targets[&referent], id("socket"));
    assert_eq!(base.history_status().undo_depth, 0);
    intent.suppressions.push(referent);
    assert_eq!(
        prepare(&projection, &base, &intent).unwrap_err(),
        Conflict::OwnerAdmission
    );
    assert_eq!(base.document(), projection.expanded.document());
    assert_eq!(base.history_status().undo_depth, 0);
}

#[test]
fn component_modes_and_property_claims_cannot_silently_upsert_or_overlap() {
    let projection = projection(link_source("base", ComponentValue::Null), false);
    let mut intent = overlay();
    intent.components = vec![link(direct("socket"), None, "add")];
    assert_eq!(assert_conflict(&projection, &intent), Conflict::Component);
    let ComponentIntent::Set { mode, target, .. } = &mut intent.components[0] else {
        unreachable!()
    };
    *mode = ComponentSetMode::Replace;
    *target = direct("tail");
    assert_eq!(assert_conflict(&projection, &intent), Conflict::Component);
    intent.components = vec![property_intent(
        "note",
        PropertyIntent::Set(ComponentValue::String("pin".into())),
        vec![],
    )];
    intent.components.push(property_intent(
        "target",
        PropertyIntent::Set(ComponentValue::Null),
        vec![],
    ));
    assert_eq!(
        assert_conflict(&projection, &intent),
        Conflict::DuplicateIntent
    );
}

#[test]
fn absent_defaults_materialize_missing_ancestors_in_the_derived_candidate_only() {
    let component = ComponentTypeId::new("game.NestedDefault");
    for local_reference in [false, true] {
        let default = if local_reference {
            ComponentValue::EntityReference(nara_identity::EntityReference::SceneLocal {
                entity: id("socket"),
            })
        } else {
            ComponentValue::U64(7)
        };
        let field = ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["settings", "wire_value"]),
            if local_reference {
                ComponentValueShape::EntityRef
            } else {
                ComponentValueKind::U64.into()
            },
            default,
        )
        .with_capabilities(
            ComponentCapability::SCENE_AUTHORING
                .into_iter()
                .chain(local_reference.then_some(ComponentCapability::EntityRef)),
        );
        let schema = ComponentSchema::new(
            component.clone(),
            "Nested default",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([field]);
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("game.overlay-comparison").unwrap(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )])
        .unwrap();
        for original in [
            ComponentValue::map([] as [(&str, ComponentValue); 0]),
            ComponentValue::map([(
                "settings",
                ComponentValue::map([] as [(&str, ComponentValue); 0]),
            )]),
        ] {
            let mut entity = SceneEntityRecord::new(id("item"));
            entity.components.insert(
                component.clone(),
                crate::SceneComponentRecord::new(ComponentSchemaVersion::ONE, original),
            );
            let document = SceneDocument::from_ordered_roots([
                entity,
                SceneEntityRecord::new(id("socket")),
                SceneEntityRecord::new(id("projected_socket")),
            ]);
            let base = open_with_bundle(&document, &bundle);
            let revision = base.revision();
            let history = base.history_status();
            let targets = TargetMap::from([
                (direct("item"), id("item")),
                (direct("socket"), id("projected_socket")),
            ]);
            let mut intents = [ComponentIntent::Properties {
                target: direct("item"),
                component: component.clone(),
                version: ComponentSchemaVersion::ONE,
                claims: vec![BoundPropertyClaim {
                    claim: PropertyClaim {
                        path: nara_reflect::ComponentPropertyPath::new(ComponentFieldId::new(
                            "value",
                        )),
                        intent: PropertyIntent::Absent,
                    },
                    bindings: vec![],
                }],
            }];
            if local_reference {
                let mut rejected = document.clone();
                assert_eq!(
                    compile_components(&base, &mut rejected, &targets, &intents),
                    Err(Conflict::Reference)
                );
                assert_eq!(rejected, document);
                let ComponentIntent::Properties { claims, .. } = &mut intents[0] else {
                    unreachable!()
                };
                claims[0].bindings.push(ReferenceBinding {
                    slot: ComponentFieldPath::empty(),
                    expected: id("socket"),
                    target: direct("socket"),
                });
            }
            let mut working = document.clone();
            compile_components(&base, &mut working, &targets, &intents).unwrap();
            assert_eq!(
                working
                    .entities
                    .iter()
                    .find(|entity| entity.id == id("item"))
                    .unwrap()
                    .components[&component]
                    .value,
                ComponentValue::map([(
                    "settings",
                    ComponentValue::map([(
                        "wire_value",
                        if local_reference {
                            ComponentValue::EntityReference(
                                nara_identity::EntityReference::SceneLocal {
                                    entity: id("projected_socket"),
                                },
                            )
                        } else {
                            ComponentValue::U64(7)
                        },
                    )]),
                )])
            );
            assert!(
                matches!(&intents[0], ComponentIntent::Properties { claims, .. }
                if matches!(claims[0].claim.intent, PropertyIntent::Absent))
            );
            assert_eq!(base.document(), &document);
            assert_eq!(base.revision(), revision);
            assert_eq!(base.history_status(), history);
        }
    }
}

#[test]
fn component_values_bind_equal_local_spellings_to_inherited_added_and_repeated_nested_targets() {
    let projection = projection(outer(&["socket", "nest_a", "nest_b", "tail"]), false);
    let mut intent = overlay();
    intent.additions.push(id("socket"));
    intent.placements.push(place(
        added("socket"),
        Parent::Root,
        Slot::Before(direct("socket")),
    ));
    intent.components = vec![
        link(
            direct("socket"),
            Some(added("socket")),
            "inherited to added",
        ),
        link(
            added("socket"),
            Some(nested("nest_a", "socket")),
            "added to nested",
        ),
        link(
            nested("nest_a", "socket"),
            Some(nested("nest_b", "socket")),
            "nested to repeated",
        ),
    ];
    let saved = serde_json::to_vec(&intent).unwrap();
    let loaded: Overlay = serde_json::from_slice(&saved).unwrap();
    assert_eq!(loaded, intent);
    let (effective, targets) = prepare_document(&projection, &loaded);
    let identities = [
        direct("socket"),
        added("socket"),
        nested("nest_a", "socket"),
        nested("nest_b", "socket"),
    ]
    .map(|target| targets[&target].clone());
    assert_eq!(identities.iter().collect::<BTreeSet<_>>().len(), 4);
    assert_link(
        &effective,
        &identities[0],
        &identities[1],
        "inherited to added",
    );
    assert_link(
        &effective,
        &identities[1],
        &identities[2],
        "added to nested",
    );
    assert_link(
        &effective,
        &identities[2],
        &identities[3],
        "nested to repeated",
    );
}

#[test]
fn component_replacement_and_suppression_reuse_the_existing_owner_validation() {
    let mut source = outer(&["socket", "tail"]);
    source
        .entities
        .iter_mut()
        .find(|record| record.id == id("socket"))
        .unwrap()
        .components
        .insert(
            ComponentTypeId::new("game.Link"),
            crate::SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    ("target", ComponentValue::Null),
                    ("note", ComponentValue::String("base".into())),
                ]),
            ),
        );
    let projection = projection(source, false);
    let mut intent = overlay();
    intent
        .components
        .push(link(direct("socket"), Some(direct("tail")), "replacement"));
    let ComponentIntent::Set { mode, .. } = &mut intent.components[0] else {
        unreachable!()
    };
    *mode = ComponentSetMode::Replace;
    let (effective, targets) = prepare_document(&projection, &intent);
    assert_link(
        &effective,
        &targets[&direct("socket")],
        &targets[&direct("tail")],
        "replacement",
    );
    intent.components = vec![ComponentIntent::Suppress {
        target: direct("socket"),
        component: ComponentTypeId::new("game.Link"),
    }];
    let (effective, targets) = prepare_document(&projection, &intent);
    assert!(
        effective
            .entities
            .iter()
            .find(|record| record.id == targets[&direct("socket")])
            .unwrap()
            .components
            .is_empty()
    );
}

#[test]
fn the_final_invalid_component_or_dangling_binding_publishes_nothing_and_preserves_intent() {
    let projection = projection(outer(&["socket", "tail"]), false);
    let mut intent = overlay();
    intent.components = vec![
        link(direct("socket"), Some(direct("tail")), "valid first"),
        link(direct("tail"), None, "invalid last"),
    ];
    let ComponentIntent::Set { value, .. } = &mut intent.components[1] else {
        unreachable!()
    };
    *value = ComponentValue::map([
        ("target", ComponentValue::Null),
        ("note", ComponentValue::U64(3)),
    ]);
    assert_eq!(assert_conflict(&projection, &intent), Conflict::Component);
    let file = UnresolvedOverlay {
        overlay: intent,
        conflict: Conflict::Component,
    };
    assert_eq!(
        serde_json::from_slice::<UnresolvedOverlay>(&serde_json::to_vec(&file).unwrap()).unwrap(),
        file
    );

    let mut intent = overlay();
    intent
        .components
        .push(link(direct("socket"), Some(direct("tail")), "dangling"));
    intent.suppressions.push(direct("tail"));
    assert_eq!(
        assert_conflict(&projection, &intent),
        Conflict::OwnerAdmission
    );
    let ComponentIntent::Set { bindings, .. } = &mut intent.components[0] else {
        unreachable!()
    };
    bindings.clear();
    assert_eq!(assert_conflict(&projection, &intent), Conflict::Reference);
}

#[test]
fn default_introduced_references_cannot_bind_to_an_unrelated_containing_scene_entity() {
    let projection = projection(outer(&["socket", "tail"]), false);
    let mut catalog = schemas().owners()[0].current().clone();
    let target = catalog.components[0]
        .fields
        .iter_mut()
        .find(|field| field.id() == &ComponentFieldId::new("target"))
        .unwrap();
    target.required = false;
    target.default_value = Some(ComponentValue::EntityReference(
        nara_identity::EntityReference::SceneLocal {
            entity: id("socket"),
        },
    ));
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.overlay-comparison").unwrap(),
        catalog,
        None,
    )])
    .unwrap();
    let base = open_with_bundle(projection.expanded.document(), &bundle);
    let mut intent = overlay();
    intent.components = vec![ComponentIntent::Set {
        mode: ComponentSetMode::Add,
        target: direct("tail"),
        component: ComponentTypeId::new("game.Link"),
        version: ComponentSchemaVersion::ONE,
        value: ComponentValue::map([("note", ComponentValue::String("omitted target".into()))]),
        bindings: vec![],
    }];
    assert_eq!(
        prepare(&projection, &base, &intent).unwrap_err(),
        Conflict::Reference
    );
    let ComponentIntent::Set { bindings, .. } = &mut intent.components[0] else {
        unreachable!()
    };
    bindings.push(ReferenceBinding {
        slot: ComponentFieldPath::from_fields(["target"]),
        expected: id("socket"),
        target: direct("socket"),
    });
    let (prepared, targets) = prepare(&projection, &base, &intent).unwrap();
    assert_link(
        prepared.unwrap().document(),
        &targets[&direct("tail")],
        &targets[&direct("socket")],
        "omitted target",
    );
    assert_ne!(targets[&direct("socket")], id("socket"));
    assert_eq!(base.document(), projection.expanded.document());
    assert_eq!(base.history_status().undo_depth, 0);
}

#[test]
fn whole_component_intent_preserves_distinct_lawful_optional_container_values() {
    let projection = projection(outer(&["socket"]), false);
    let component = ComponentTypeId::new("game.Nested");
    let schema = ComponentSchema::new(component.clone(), "Nested", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::optional(
            ComponentFieldId::new("note"),
            "Note",
            ComponentFieldPath::from_fields(["group", "note"]),
            ComponentValueKind::String,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
        .with_authoring_initial_value(ComponentValue::map([(
            "group",
            ComponentValue::map([("note", ComponentValue::String("initial".into()))]),
        )]));
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.overlay-comparison").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let base = open_with_bundle(projection.expanded.document(), &bundle);
    let mut intent = overlay();
    intent.components = vec![ComponentIntent::Set {
        mode: ComponentSetMode::Add,
        target: direct("socket"),
        component: component.clone(),
        version: ComponentSchemaVersion::ONE,
        value: ComponentValue::Map(BTreeMap::new()),
        bindings: vec![],
    }];
    let (prepared, targets) = prepare(&projection, &base, &intent).unwrap();
    assert_eq!(
        prepared
            .unwrap()
            .document()
            .entities
            .iter()
            .find(|record| record.id == targets[&direct("socket")])
            .unwrap()
            .components[&component]
            .value,
        ComponentValue::Map(BTreeMap::new())
    );
    assert_eq!(base.history_status().undo_depth, 0);
    assert_eq!(base.document(), projection.expanded.document());

    let ComponentIntent::Set { value, .. } = &mut intent.components[0] else {
        unreachable!()
    };
    *value = ComponentValue::map([("group", ComponentValue::Map(BTreeMap::new()))]);
    let (prepared, targets) = prepare(&projection, &base, &intent).unwrap();
    let prepared = prepared.unwrap();
    let actual = &prepared
        .document()
        .entities
        .iter()
        .find(|record| record.id == targets[&direct("socket")])
        .unwrap()
        .components[&component]
        .value;
    let ComponentIntent::Set { value, .. } = &intent.components[0] else {
        unreachable!()
    };
    assert_eq!(actual, value);
}

#[test]
fn projection_admission_needs_neither_creation_templates_adapters_nor_edit_history() {
    let projection = projection(outer(&["socket", "tail"]), false);
    let mut catalog = schemas().owners()[0].current().clone();
    for schema in &mut catalog.components {
        schema.authoring_initial_value = None;
        schema.capabilities.remove(&ComponentCapability::Edit);
        for field in &mut schema.fields {
            field.capabilities.remove(&ComponentCapability::Edit);
        }
    }
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.overlay-comparison").unwrap(),
        catalog,
        None,
    )])
    .unwrap();
    let published = SceneDocumentCandidate::decode_json_bytes(
        projection
            .expanded
            .document()
            .to_json_string()
            .unwrap()
            .as_bytes(),
    )
    .unwrap()
    .admit_authoring(
        AuthoringDocumentSource::new(AssetRef::path("scenes/comparison.scene.json").unwrap()),
        &bundle,
        &[],
    )
    .unwrap()
    .publish();
    let base = SceneAuthoringSession::from_published_authoring_document_with_history_limits(
        published,
        crate::SceneAuthoringHistoryLimits::new(1, 1, 1, 1, 1, 1, 1).unwrap(),
    );
    let revision = base.revision();
    let history = base.history_status();
    let mut intent = overlay();
    intent.additions.push(id("socket"));
    intent
        .placements
        .push(place(added("socket"), Parent::Root, Slot::Last));
    intent
        .components
        .push(link(added("socket"), Some(direct("tail")), "full value"));
    let (candidate, targets) = prepare(&projection, &base, &intent).unwrap();
    let candidate = candidate.unwrap();
    assert_link(
        candidate.document(),
        &targets[&added("socket")],
        &targets[&direct("tail")],
        "full value",
    );
    let origin = &candidate.provenance()[&targets[&added("socket")]];
    assert_eq!(origin.local_entity_id(), &id("socket"));
    assert!(matches!(origin.projection_path().unwrap().steps(),
        [crate::PrefabProjectionStep::InstanceAddition { entity }] if entity == &id("socket")));
    assert_eq!(base.document(), projection.expanded.document());
    assert_eq!(base.revision(), revision);
    assert_eq!(base.history_status(), history);
}
