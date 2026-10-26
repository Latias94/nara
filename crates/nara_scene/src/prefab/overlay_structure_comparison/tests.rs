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
    Target::Inherited(
        steps
            .iter()
            .map(|(source, entity)| SourceEntity {
                source: SourceIdentity::of(&key(source)),
                entity: id(entity),
            })
            .collect(),
    )
}

fn direct(entity: &str) -> Target {
    inherited(&[("outer", entity)])
}
fn nested(anchor: &str, entity: &str) -> Target {
    inherited(&[("outer", anchor), ("leaf", entity)])
}
fn added(entity: &str) -> Target {
    Target::Added(id(entity))
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
    let report = scene.expand_prefabs(&registry(), &resolver(outer));
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    Projection {
        anchor: id("host_a"),
        source: SourceIdentity::of(&key("outer")),
        expanded: report.candidate.unwrap(),
    }
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
        source: SourceIdentity::of(&key("outer")),
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
            .all(|record| record.id != projection.targets()[&direct("nest_a")])
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
    intent.source = SourceIdentity::of(&key("different"));
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
    let report = scene.expand_prefabs(&registry(), &resolver(outer(&["socket", "tail"])));
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    Projection {
        anchor: id("host_a"),
        source: SourceIdentity::of(&key("outer")),
        expanded: report.candidate.unwrap(),
    }
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
fn whole_component_intent_is_never_silently_changed_by_optional_container_lowering() {
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
        target: direct("socket"),
        component: component.clone(),
        version: ComponentSchemaVersion::ONE,
        value: ComponentValue::Map(BTreeMap::new()),
        bindings: vec![],
    }];
    assert_eq!(
        prepare(&projection, &base, &intent).unwrap_err(),
        Conflict::Component
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
