use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_asset::AssetServer;
use nara_core::ItemLimit;
use nara_identity::EntityLookup;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistryBuilder, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerId,
    ComponentSchemaVersion, ComponentValue, ComponentValueKind, ComponentValueShape,
};

use super::*;
use crate::{
    InMemoryPrefabSourceResolver, SceneComponentRecord, SceneDocument, SceneEntityRecord,
    SourceDocumentKind,
};

#[derive(Component)]
struct ReceivedValue(ComponentValue);

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.CompiledReceiver")
}

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn schema(shape: impl Into<ComponentValueShape>) -> ComponentSchema {
    ComponentSchema::new(
        component(),
        "Compiled receiver",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        shape,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
}

fn registry(
    schema: ComponentSchema,
    owner: Option<&'static str>,
) -> (ComponentRegistrySnapshot, Arc<AtomicUsize>) {
    let mut builder = match owner {
        Some(owner) => ComponentRegistryBuilder::from_owner_catalog_candidate(
            ComponentSchemaOwnerId::new(owner),
            ComponentSchemaCatalog::default(),
            None,
        )
        .unwrap(),
        None => ComponentRegistryBuilder::new(),
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    builder
        .register_persistent_component_with_codec::<ReceivedValue, _, _>(
            schema,
            move |value| {
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(ReceivedValue(value.clone()))
            },
            |received| Ok(received.0.clone()),
        )
        .unwrap();
    (builder.freeze().unwrap(), calls)
}

fn entity(name: &str, value: ComponentValue) -> SceneEntityRecord {
    SceneEntityRecord::new(id(name)).with_component(
        component(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("value", value)]),
        ),
    )
}

fn compile(
    document: &SceneDocument,
    registry: &ComponentRegistrySnapshot,
) -> CompiledSceneCandidate {
    let report = document.compile(registry, &InMemoryPrefabSourceResolver::new());
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report.candidate.unwrap().bind_source(
        SourceDocumentKey::named(
            "bc5a779a-8452-488b-80a5-c0dcc55aee94".parse().unwrap(),
            "compiled-receiver.scene",
            SourceDocumentKind::Scene,
        )
        .unwrap(),
    )
}

fn occurrence() -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("compiled-receiver-tests", "main").unwrap()
}

#[test]
fn late_receiving_shape_failure_runs_zero_codecs_and_preserves_last_good_replacement() {
    let (source, source_calls) = registry(
        schema(ComponentValueShape::optional(
            ComponentValueKind::I64.into(),
        )),
        None,
    );
    let (receiving, receiving_calls) = registry(schema(ComponentValueKind::I64), None);
    let initial = compile(
        &SceneDocument::from_ordered_roots([entity("a", ComponentValue::I64(1))]),
        &source,
    );
    let mut world = World::new();
    world.insert_resource(AssetServer::new());
    let report = spawn_scene(&mut world, &receiving, &initial, &occurrence(), None);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let current = report.instance.unwrap();
    let EntityLookup::Resolved(old) = current.resolve(&world, &id("a")) else {
        panic!("the initial activation must resolve");
    };
    assert_eq!(receiving_calls.swap(0, Ordering::SeqCst), 1);
    let stats = world.resource::<WorldIdentityDomain>().stats();
    let assets = world.resource::<AssetServer>().clone();
    let population = world.entities().len();
    let old_source = world.get::<SceneEntitySource>(old).unwrap().clone();
    let invalid = compile(
        &SceneDocument::from_ordered_roots([
            entity("a", ComponentValue::I64(2)),
            entity("zz-invalid", ComponentValue::Null),
        ]),
        &source,
    );
    assert_eq!(source_calls.load(Ordering::SeqCst), 0);

    let rejected = prepare_scene_replacement(&mut world, &receiving, &invalid, &current, None)
        .expect_err("the complete receiving shape must pass before decoding");
    assert!(rejected.diagnostics.has_errors());
    assert_eq!(receiving_calls.load(Ordering::SeqCst), 0);
    let rejected = replace_expanded_scene_with_product(
        &mut world,
        &receiving,
        &invalid,
        None,
        &current,
        SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
        &[],
        |_| panic!("receiving admission must precede product configuration"),
    );
    assert!(rejected.instance.is_none());
    assert!(rejected.diagnostics.has_errors());
    assert_eq!(rejected.retired_entities(), 0);
    assert_eq!(receiving_calls.load(Ordering::SeqCst), 0);
    assert_eq!(world.entities().len(), population);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
    assert_eq!(world.resource::<AssetServer>(), &assets);
    assert_eq!(
        current.resolve(&world, &id("a")),
        EntityLookup::Resolved(old)
    );
    assert_eq!(world.get::<SceneEntitySource>(old), Some(&old_source));
    assert_eq!(
        world.get::<ReceivedValue>(old).unwrap().0,
        ComponentValue::map([("value", ComponentValue::I64(1))]),
    );

    let valid = compile(
        &SceneDocument::from_ordered_roots([
            entity("a", ComponentValue::I64(2)),
            entity("zz-valid", ComponentValue::I64(3)),
        ]),
        &source,
    );
    let prepared = prepare_scene_replacement(&mut world, &receiving, &valid, &current, None)
        .expect("the compatible receiving batch must prepare");
    assert_eq!(receiving_calls.load(Ordering::SeqCst), 2);
    let report = prepared.commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert_eq!(report.instance.unwrap().len(), 2);
    assert_eq!(receiving_calls.load(Ordering::SeqCst), 2);
    assert!(world.get_entity(old).is_err());
}

#[test]
fn compiled_actual_owner_must_match_receiving_owned_or_unowned_definition() {
    for (source_owner, receiving_owner) in [
        (None, None),
        (Some("test.compiled-owner"), Some("test.compiled-owner")),
        (None, Some("test.compiled-owner")),
        (Some("test.compiled-owner"), None),
        (Some("test.compiled-owner"), Some("test.other-owner")),
    ] {
        let (source, source_calls) = registry(schema(ComponentValueKind::I64), source_owner);
        let (receiving, receiving_calls) =
            registry(schema(ComponentValueKind::I64), receiving_owner);
        // The source omits an owner claim; strict compilation stamps the actual selected owner.
        let candidate = compile(
            &SceneDocument::from_ordered_roots([entity("a", ComponentValue::I64(1))]),
            &source,
        );
        assert_eq!(
            candidate.document().entities()[0].components()[&component()]
                .schema_owner()
                .map(|owner| owner.as_str()),
            source_owner,
        );
        assert_eq!(source_calls.load(Ordering::SeqCst), 0);
        let mut world = World::new();
        let original_entities = world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<std::collections::BTreeSet<_>>();
        let prepared = prepare_scene(&mut world, &receiving, &candidate, &occurrence(), None);
        if source_owner == receiving_owner {
            let prepared = prepared.expect("the matching receiving owner must prepare");
            assert_eq!(receiving_calls.load(Ordering::SeqCst), 1);
            let report = prepared.commit(&mut world);
            assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
            assert_eq!(report.instance.unwrap().len(), 1);
            assert_eq!(receiving_calls.load(Ordering::SeqCst), 1);
        } else {
            let report =
                prepared.expect_err("receiving ownership cannot reinterpret compiled data");
            assert!(report.diagnostics.has_errors());
            assert_eq!(receiving_calls.load(Ordering::SeqCst), 0);
            assert_eq!(
                world
                    .iter_entities()
                    .map(|entity| entity.id())
                    .collect::<std::collections::BTreeSet<_>>(),
                original_entities
            );
            assert!(!world.contains_resource::<WorldIdentityDomain>());
        }
    }
}

#[test]
fn added_receiving_default_requires_source_recompilation() {
    let (source, source_calls) = registry(schema(ComponentValueKind::I64), None);
    let mut receiving_schema = schema(ComponentValueKind::I64);
    receiving_schema.fields.push(
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("added"),
            "Added",
            ComponentFieldPath::from_fields(["added"]),
            ComponentValueKind::I64,
            ComponentValue::I64(7),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    );
    let (receiving, receiving_calls) = registry(receiving_schema, None);
    let document = SceneDocument::from_ordered_roots([entity("a", ComponentValue::I64(1))]);
    let old_candidate = compile(&document, &source);
    let expected_old = ComponentValue::map([("value", ComponentValue::I64(1))]);
    let mut world = World::new();
    let original_entities = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<std::collections::BTreeSet<_>>();

    let rejected = prepare_scene(&mut world, &receiving, &old_candidate, &occurrence(), None)
        .expect_err("activation cannot materialize a newly receiving default");
    assert!(rejected.diagnostics.has_errors());
    assert_eq!(receiving_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<std::collections::BTreeSet<_>>(),
        original_entities
    );
    assert!(!world.contains_resource::<WorldIdentityDomain>());
    assert_eq!(
        old_candidate.document().entities()[0].components()[&component()].value(),
        &expected_old,
    );

    let recompiled = compile(&document, &receiving);
    let expected_new = ComponentValue::map([
        ("value", ComponentValue::I64(1)),
        ("added", ComponentValue::I64(7)),
    ]);
    assert_eq!(source_calls.load(Ordering::SeqCst), 0);
    assert_eq!(receiving_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        recompiled.document().entities()[0].components()[&component()].value(),
        &expected_new,
    );
    let prepared = prepare_scene(&mut world, &receiving, &recompiled, &occurrence(), None)
        .expect("recompiled explicit values must prepare");
    assert_eq!(receiving_calls.load(Ordering::SeqCst), 1);
    let report = prepared.commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let current = report.instance.unwrap();
    let EntityLookup::Resolved(entity) = current.resolve(&world, &id("a")) else {
        panic!("the recompiled activation must resolve");
    };
    assert_eq!(world.get::<ReceivedValue>(entity).unwrap().0, expected_new);
    assert_eq!(receiving_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn receiving_version_requires_source_recompilation_without_activation_migration() {
    let (source, source_calls) = registry(schema(ComponentValueKind::I64), None);
    let mut receiving_schema = schema(ComponentValueKind::I64);
    receiving_schema.version = ComponentSchemaVersion(2);
    let calls = Arc::new(AtomicUsize::new(0));
    let migrations = Arc::new(AtomicUsize::new(0));
    let observed_calls = Arc::clone(&calls);
    let observed_migrations = Arc::clone(&migrations);
    let mut builder = ComponentRegistryBuilder::new();
    builder
        .register_persistent_component_with_codec::<ReceivedValue, _, _>(
            receiving_schema,
            move |value| {
                observed_calls.fetch_add(1, Ordering::SeqCst);
                Ok(ReceivedValue(value.clone()))
            },
            |received| Ok(received.0.clone()),
        )
        .unwrap();
    builder
        .register_component_migration(
            &component(),
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion(2),
            move |value| {
                observed_migrations.fetch_add(1, Ordering::SeqCst);
                Ok(value)
            },
        )
        .unwrap();
    let receiving = builder.freeze().unwrap();
    let document = SceneDocument::from_ordered_roots([entity("a", ComponentValue::I64(1))]);
    let old_candidate = compile(&document, &source);
    let mut world = World::new();
    let original_entities = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<std::collections::BTreeSet<_>>();

    let rejected = prepare_scene(&mut world, &receiving, &old_candidate, &occurrence(), None)
        .expect_err("activation cannot migrate a compiled value to a receiving version");
    assert!(rejected.diagnostics.has_errors());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(migrations.load(Ordering::SeqCst), 0);
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<std::collections::BTreeSet<_>>(),
        original_entities
    );
    assert!(!world.contains_resource::<WorldIdentityDomain>());

    let recompiled = compile(&document, &receiving);
    assert_eq!(
        recompiled.document().entities()[0].components()[&component()].version(),
        ComponentSchemaVersion(2),
    );
    assert_eq!(source_calls.load(Ordering::SeqCst), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let source_migrations = migrations.load(Ordering::SeqCst);
    assert!(source_migrations > 0);
    let prepared = prepare_scene(&mut world, &receiving, &recompiled, &occurrence(), None)
        .expect("the recompiled current version must prepare");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let report = prepared.commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert_eq!(report.instance.unwrap().len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(migrations.load(Ordering::SeqCst), source_migrations);
}

#[cfg(feature = "serde")]
#[test]
fn authoring_sealed_visibility_forest_requires_native_projection_before_any_codec() {
    use crate::{
        __private::{AuthoringAdapterReceipt, try_compile_authoring_scene_with_loader},
        AuthoringSchemaBasis, PrefabEntityTarget, SceneEntityPlacement, ScenePlacement,
        ScenePlacementParent,
    };
    use nara_diagnostic::DiagnosticValueRef;
    use nara_reflect::{__private::resolve_schema_provider, AuthoringSchemaBundle};

    let provider = crate::SCENE_COMPONENTS_SCHEMA_PROVIDER;
    let resolved = resolve_schema_provider(provider).unwrap();
    let bundle = AuthoringSchemaBundle::from_trusted_provider_definitions([provider]).unwrap();
    let adapter = AuthoringAdapterReceipt::from_trusted_provider(
        resolved.contribution_receipt(),
        "test.compiled-receiver-native",
        1,
        1,
        resolved
            .current()
            .catalog()
            .components()
            .iter()
            .map(|schema| schema.id().clone()),
    )
    .unwrap();
    let basis = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
    let visibility = ComponentTypeId::new("nara.scene.Visibility");
    let (receiving, calls) = registry(
        basis.schema(&visibility).unwrap().clone(),
        Some(crate::SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str()),
    );
    let visible = |name| {
        SceneEntityRecord::new(id(name)).with_component(
            visibility.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("visible".into()),
            ),
        )
    };
    let initial = compile(
        &SceneDocument::from_ordered_roots([visible("isolated")]),
        &receiving,
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let seal = |document: &SceneDocument| {
        let report = try_compile_authoring_scene_with_loader(
            document,
            &basis,
            Default::default(),
            None,
            |_, _| Ok::<_, std::convert::Infallible>(None),
        )
        .unwrap();
        assert!(
            !report.diagnostics().has_errors(),
            "{:?}",
            report.diagnostics()
        );
        let preview = report.into_native_preview().unwrap();
        assert!(preview.omissions.is_empty());
        assert!(
            !preview.diagnostics.has_errors(),
            "{:?}",
            preview.diagnostics
        );
        preview.candidate.bind_source(initial.source_key().clone())
    };
    let child = |name, parent| {
        SceneEntityPlacement::new(
            PrefabEntityTarget::source(id(name)),
            ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                id(parent),
            ))),
        )
    };
    let forest = SceneDocument::new(
        [
            SceneEntityPlacement::root(id("a-parent")),
            SceneEntityPlacement::root(id("z-parent")),
            SceneEntityPlacement::root(id("isolated")),
            child("y-child", "a-parent"),
            child("b-child", "z-parent"),
        ],
        [
            visible("a-parent"),
            SceneEntityRecord::new(id("z-parent")),
            visible("b-child"),
            SceneEntityRecord::new(id("y-child")),
            visible("isolated"),
        ],
    );
    let candidate = seal(&forest);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(candidate.document().entities()[1].id(), &id("b-child"));
    assert!(
        candidate.document().entities()[1]
            .components()
            .contains_key(&visibility)
    );
    let candidate_before = format!("{candidate:?}");
    let mut world = World::new();
    world.insert_resource(AssetServer::new());
    let report = spawn_scene(&mut world, &receiving, &initial, &occurrence(), None);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let current = report.instance.unwrap();
    let EntityLookup::Resolved(old) = current.resolve(&world, &id("isolated")) else {
        panic!("isolated Visibility must be supported");
    };
    assert_eq!(calls.swap(0, Ordering::SeqCst), 1);
    let old_source = world.get::<SceneEntitySource>(old).unwrap().clone();
    let stats = world.resource::<WorldIdentityDomain>().stats();
    let assets = world.resource::<AssetServer>().clone();
    let population = world.entities().len();

    let rejected = prepare_scene_replacement(&mut world, &receiving, &candidate, &current, None)
        .expect_err("Authoring sealing cannot authorize an unavailable Native projection");
    let diagnostic = rejected
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "scene.hierarchy-projection-unavailable")
        .expect("Native receiving must reject the unsupported projection");
    assert_eq!(
        diagnostic
            .fields()
            .iter()
            .find(|field| field.key().as_str() == "entity-id")
            .unwrap()
            .value(),
        DiagnosticValueRef::Identifier("b-child"),
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(format!("{candidate:?}"), candidate_before);
    assert_eq!(world.entities().len(), population);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
    assert_eq!(world.resource::<AssetServer>(), &assets);
    assert_eq!(
        current.resolve(&world, &id("isolated")),
        EntityLookup::Resolved(old)
    );
    assert_eq!(world.get::<SceneEntitySource>(old), Some(&old_source));
    assert_eq!(
        world.get::<ReceivedValue>(old).unwrap().0,
        ComponentValue::String("visible".into())
    );

    let mut without_visibility = forest;
    for entity in &mut without_visibility.entities {
        entity.components.clear();
    }
    let supported = seal(&without_visibility);
    let prepared = prepare_scene_replacement(&mut world, &receiving, &supported, &current, None)
        .expect("the same forest without Visibility must remain supported");
    let report = prepared.commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert_eq!(report.instance.unwrap().len(), 5);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn receiving_local_references_include_empty_entities_after_source_sorting_and_limit_rejection() {
    use crate::SceneEntityPlacement;
    use nara_reflect::EntityReference;

    let mut reference_schema = schema(ComponentValueShape::list(ComponentValueShape::EntityRef));
    reference_schema.fields[0] = reference_schema.fields[0]
        .clone()
        .with_capability(ComponentCapability::EntityRef);
    let (source, source_calls) = registry(reference_schema.clone(), None);
    let (receiving, calls) = registry(reference_schema, None);
    let bindings = SceneComponentBindings::new(receiving, []).unwrap();
    let references = ComponentValue::List(vec![
        ComponentValue::EntityReference(EntityReference::SceneLocal {
            entity: id("a-empty"),
        }),
        ComponentValue::EntityReference(EntityReference::SceneLocal {
            entity: id("z-empty"),
        }),
    ]);
    let document = SceneDocument {
        placements: ["z-empty", "m-reader", "a-empty"]
            .map(|name| SceneEntityPlacement::root(id(name)))
            .into(),
        entities: vec![
            SceneEntityRecord::new(id("z-empty")),
            entity("m-reader", references.clone()),
            SceneEntityRecord::new(id("a-empty")),
        ],
    };
    let before = document.clone();
    let candidate = compile(&document, &source);
    assert_eq!(document, before);
    assert_eq!(source_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        candidate
            .document()
            .entities()
            .iter()
            .map(|entity| entity.id().as_str())
            .collect::<Vec<_>>(),
        ["a-empty", "m-reader", "z-empty"]
    );
    let mut world = World::new();
    let sentinel = world.spawn_empty().id();
    let population = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<std::collections::BTreeSet<_>>();
    let limits = ComponentValueMaterializationLimits::default();
    let rejected = bindings
        .prepare_scene_with_limits(
            &mut world,
            &candidate,
            &occurrence(),
            None,
            ComponentValueMaterializationLimits::new(
                ItemLimit::ONE,
                limits.bytes(),
                limits.depth(),
            ),
        )
        .expect_err("source sealing cannot waive the receiving value allowance");
    assert!(rejected.diagnostics.has_errors());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<std::collections::BTreeSet<_>>(),
        population,
    );
    assert!(world.get_entity(sentinel).is_ok());
    assert!(!world.contains_resource::<WorldIdentityDomain>());

    let prepared = bindings
        .prepare_scene(&mut world, &candidate, &occurrence(), None)
        .expect("empty-component entities belong to the complete receiving namespace");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let report = prepared.commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let current = report.instance.unwrap();
    assert_eq!(
        current.entity_ids(),
        [id("a-empty"), id("m-reader"), id("z-empty")]
    );
    for entity_id in current.entity_ids() {
        let EntityLookup::Resolved(entity) = current.resolve(&world, entity_id) else {
            panic!("every entity, including empty reference targets, must resolve");
        };
        let source = world.get::<SceneEntitySource>(entity).unwrap();
        assert_eq!(source.instance_id, current.instance_id());
        assert_eq!(&source.entity_id, entity_id);
        assert_eq!(source.containing_source(), candidate.source_key());
        assert_eq!(source.occurrence(), &occurrence());
        assert_eq!(
            source.provenance(),
            &candidate.canonical_provenance()[entity_id]
        );
        if entity_id == &id("m-reader") {
            assert_eq!(
                world.get::<ReceivedValue>(entity).unwrap().0,
                ComponentValue::map([("value", references.clone())])
            );
        } else {
            assert!(world.get::<ReceivedValue>(entity).is_none());
        }
    }
    assert_eq!(source_calls.load(Ordering::SeqCst), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
