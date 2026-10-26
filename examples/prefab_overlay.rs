//! Pin one inherited leaf, retain sibling inheritance, then Revert through the Scene owner.

use nara::{
    core::SourceUniverseId,
    identity::EntityLookup,
    prelude::*,
    reflect::ComponentPropertyPath,
    scene::{
        ExpandedSceneCandidate, InMemoryPrefabSourceResolver, PrefabComponentIntent,
        PrefabEntityTarget, PrefabInstance, PrefabOverlay, PrefabPropertyClaim,
        PrefabPropertyIntent, PrefabSourceResolver, SceneAuthoringSession, SourceDocumentKey,
        SourceDocumentKind, register_scene_components,
    },
    sprite::register_sprite_components,
};

fn main() {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).expect("scene components should register once");
    register_sprite_components(&mut registry).expect("sprite components should register once");
    registry
        .freeze()
        .expect("component registry should freeze before scene expansion");

    let source = AssetRef::path("prefabs/enemy.ron").unwrap();
    let universe: SourceUniverseId = "f037335a-e03a-41c7-8fb1-310ff21dd9be".parse().unwrap();
    let enemy_source =
        SourceDocumentKey::named(universe, "example.enemy", SourceDocumentKind::Prefab).unwrap();
    let mut resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        enemy_source.clone(),
        enemy_prefab(1.0, 1.0),
    );
    // The resolver admits canonical identity; the path above is only a lookup request.
    let mut overrides = PrefabOverlay::new(resolver.resolve_prefab(&source).unwrap().key().clone());
    overrides
        .components
        .push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(scene_id("visual")),
            component: sprite_id(),
            version: v1(),
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("material.tint.r")),
                intent: PrefabPropertyIntent::Set(ComponentValue::f64(0.25).unwrap()),
                bindings: Vec::new(),
            }],
        });
    let declaration = PrefabInstance {
        source: source.clone(),
        overrides: Some(overrides),
    };
    let scene = SceneDocument::from_ordered_roots([SceneEntityRecord {
        id: scene_id("enemy"),
        children: Vec::new(),
        components: Default::default(),
        prefab: Some(declaration.clone()),
    }]);

    let mut owner = SceneAuthoringSession::new(scene);
    let candidate = expand(owner.document(), &registry, &resolver);
    let expanded = candidate.document();
    assert_eq!(expanded.entities.len(), 2);
    let visual_id = candidate
        .provenance()
        .iter()
        .find_map(|(id, source)| {
            (source.local_entity_id() == &scene_id("visual") && source.projection_path().is_some())
                .then_some(id)
        })
        .expect("the expanded visual retains its source identity");
    let mut world = World::new();
    let report = nara::scene::spawn_scene(
        &mut world,
        &registry,
        &candidate,
        &scene_occurrence("main/report"),
        None,
    );
    assert!(!report.diagnostics.has_errors());
    let visual = match report
        .instance
        .as_ref()
        .expect("successful scene spawn should publish an instance")
        .resolve(&world, visual_id)
    {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("expected resolved prefab entity, got {lookup:?}"),
    };
    let sprite = world.get::<Sprite>(visual).unwrap();
    assert_eq!(sprite.material.tint.r, 0.25);
    assert_eq!(sprite.material.tint.g, 1.0);

    // Only red is pinned. A later base edit to green remains inherited.
    resolver.insert(source, enemy_source, enemy_prefab(0.75, 0.5));
    let inherited = expand(owner.document(), &registry, &resolver);
    assert_tint(&inherited, 0.25, 0.5);

    // Revert removes authored intent. The exact patch belongs to the containing Scene's
    // history; no operation patch is stored inside the durable instance declaration.
    let reverted = PrefabInstance {
        overrides: None,
        ..declaration.clone()
    };
    let report = owner.apply_patch(
        &ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
            entity: scene_id("enemy"),
            expected: Some(declaration.clone()),
            value: Some(reverted),
        }]),
        &registry,
    );
    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(owner.history_status().undo_depth, 1);
    assert_tint(&expand(owner.document(), &registry, &resolver), 0.75, 0.5);
    assert!(owner.undo(&registry).applied);
    assert_eq!(
        owner.document().entities[0].prefab.as_ref(),
        Some(&declaration),
    );
    assert_tint(&expand(owner.document(), &registry, &resolver), 0.25, 0.5);

    // Save the containing source with its declarative pin, rather than the expanded projection.
    println!("{}", owner.document().to_json_string().unwrap());
}

fn expand(
    scene: &SceneDocument,
    registry: &ComponentRegistry,
    resolver: &InMemoryPrefabSourceResolver,
) -> ExpandedSceneCandidate {
    let expansion = scene.expand_prefabs(registry, resolver);
    assert!(
        !expansion.diagnostics.has_errors(),
        "{:?}",
        expansion.diagnostics,
    );
    expansion
        .candidate
        .unwrap()
        .bind_source(scene_source_key("main/source"))
}

fn assert_tint(candidate: &ExpandedSceneCandidate, red: f64, green: f64) {
    let visual = candidate
        .document()
        .entities
        .iter()
        .find(|entity| entity.components.contains_key(&sprite_id()))
        .unwrap();
    let tint = visual.components[&sprite_id()]
        .value
        .field("material")
        .unwrap()
        .field("tint")
        .unwrap();
    assert_eq!(tint.field("r").unwrap(), &ComponentValue::f64(red).unwrap());
    assert_eq!(
        tint.field("g").unwrap(),
        &ComponentValue::f64(green).unwrap()
    );
}

fn enemy_prefab(red: f64, green: f64) -> PrefabDocument {
    PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("visual")).with_component(
        sprite_id(),
        SceneComponentRecord::new(v1(), sprite_value(red, green)),
    )])
}

fn sprite_value(red: f64, green: f64) -> ComponentValue {
    ComponentValue::map([
        ("size", vec2_value(32.0, 32.0)),
        (
            "material",
            ComponentValue::map([
                ("image", ComponentValue::Null),
                ("sampler", sampler_value()),
                ("alpha_mode", ComponentValue::String("blend".to_string())),
                ("tint", color_value(red, green, 1.0, 1.0)),
            ]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
    ])
}

fn sampler_value() -> ComponentValue {
    ComponentValue::map([
        ("min_filter", ComponentValue::String("linear".to_string())),
        ("mag_filter", ComponentValue::String("linear".to_string())),
        (
            "mipmap_filter",
            ComponentValue::String("linear".to_string()),
        ),
        (
            "address_mode_u",
            ComponentValue::String("clamp_to_edge".to_string()),
        ),
        (
            "address_mode_v",
            ComponentValue::String("clamp_to_edge".to_string()),
        ),
    ])
}

fn vec2_value(x: f64, y: f64) -> ComponentValue {
    ComponentValue::map([
        ("x", ComponentValue::f64(x).unwrap()),
        ("y", ComponentValue::f64(y).unwrap()),
    ])
}

fn color_value(r: f64, g: f64, b: f64, a: f64) -> ComponentValue {
    ComponentValue::map([
        ("r", ComponentValue::f64(r).unwrap()),
        ("g", ComponentValue::f64(g).unwrap()),
        ("b", ComponentValue::f64(b).unwrap()),
        ("a", ComponentValue::f64(a).unwrap()),
    ])
}

fn scene_id(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}

fn v1() -> ComponentSchemaVersion {
    ComponentSchemaVersion(1)
}

fn sprite_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.sprite.Sprite")
}

fn scene_source_key(name: &str) -> nara::scene::SourceDocumentKey {
    nara::scene::SourceDocumentKey::named(
        "ae9c4204-2585-4d22-9da7-6400ebd89664".parse().unwrap(),
        name,
        nara::scene::SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn scene_occurrence(slot: &str) -> nara::scene::SceneOccurrenceKey {
    nara::scene::SceneOccurrenceKey::new("nara.scene.tests", slot).unwrap()
}
