use std::ptr;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_core::DepthLimit;
use nara_ecs::World;
use nara_ecs::identity::{
    EntityLookup, PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
};
use nara_reflect::{
    ComponentCodecError, ComponentRegistryBuilder, ComponentSchemaOwnerSnapshot,
    ComponentValueKind, ComponentValueMaterializationLimits,
};

use super::*;
use crate::{SceneComponentBindings, SceneOccurrenceKey};

#[derive(nara_ecs::Component)]
struct SharingPayload(ComponentValue);

fn sharing_component() -> ComponentTypeId {
    ComponentTypeId::new("game.SharingPayload")
}

fn sharing_schema(version: ComponentSchemaVersion) -> ComponentSchema {
    ComponentSchema::new(sharing_component(), "Sharing payload", version)
        .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("payload"),
                "Payload",
                ComponentFieldPath::from_fields(["payload"]),
                ComponentValueShape::list(ComponentValueKind::String.into()),
            )
            .with_capabilities([ComponentCapability::Scene]),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("revision"),
                "Revision",
                ComponentFieldPath::from_fields(["revision"]),
                ComponentValueKind::U64,
                ComponentValue::U64(7),
            )
            .with_capabilities([ComponentCapability::Scene]),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("target"),
                "Target",
                ComponentFieldPath::from_fields(["target"]),
                ComponentValueShape::optional(ComponentValueShape::EntityRef),
                ComponentValue::Null,
            )
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::EntityRef]),
        ])
}

fn sharing_registry(version: ComponentSchemaVersion) -> ComponentRegistrySnapshot {
    sharing_registry_with_migration(sharing_schema(version), |mut value| {
        value
            .set_path(
                &ComponentFieldPath::from_fields(["revision"]),
                ComponentValue::U64(11),
            )
            .unwrap();
        Ok(value)
    })
}

fn sharing_registry_with_migration(
    schema: ComponentSchema,
    migrate: impl Fn(ComponentValue) -> Result<ComponentValue, ComponentCodecError>
    + Send
    + Sync
    + 'static,
) -> ComponentRegistrySnapshot {
    let version = schema.version();
    let mut registry = ComponentRegistryBuilder::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new("game.overlay-links"),
        ComponentSchemaCatalog::default(),
        None,
    )
    .unwrap();
    registry
        .register_persistent_component_with_codec::<SharingPayload, _, _>(
            schema,
            |value| Ok(SharingPayload(value.to_owned())),
            |value| Ok(value.0.clone()),
        )
        .unwrap();
    if version != ComponentSchemaVersion::ONE {
        registry
            .register_component_migration(
                &sharing_component(),
                ComponentSchemaVersion::ONE,
                version,
                migrate,
            )
            .unwrap();
    }
    registry.freeze().unwrap()
}

fn sharing_value(target: ComponentValue) -> ComponentValue {
    ComponentValue::map([
        (
            "payload",
            ComponentValue::List(vec![ComponentValue::String("cell".repeat(16)); 1_024]),
        ),
        ("revision", ComponentValue::U64(7)),
        ("target", target),
    ])
}

fn sharing_document(value: ComponentValue) -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("host"))
        .with_component(sharing_component(), effective_record(value))])
}

fn compiled_record<'a>(
    report: &'a SceneCompilationReport,
    entity: &SceneEntityId,
) -> &'a CompiledSceneComponent {
    &expanded(report)
        .entities()
        .iter()
        .find(|record| record.id() == entity)
        .unwrap()
        .components()[&sharing_component()]
}

#[test]
fn current_explicit_payloads_share_storage_when_references_need_no_rewrite() {
    let registry = sharing_registry(ComponentSchemaVersion::ONE);
    let persistent = ComponentValue::EntityReference(EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("payload-sharing").unwrap(),
            PersistentRuntimeId::parse_str("34567890-3456-4456-8456-345678901abc").unwrap(),
        ),
    });
    for (target, project) in [
        (ComponentValue::Null, true),
        (persistent, true),
        (local(&id("host")), false),
    ] {
        let expected = sharing_value(target);
        let sources = InMemoryPrefabSourceResolver::new().with_prefab(
            request("base"),
            key("base"),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("host"))
                .with_component(sharing_component(), effective_record(expected.clone()))]),
        );
        let resolved = sources.resolve_prefab(&request("base")).unwrap();
        let document = if project {
            scene(PrefabOverlay::new(key("base")))
        } else {
            sharing_document(expected.clone())
        };
        let report = expand(&registry, &document, &sources, Default::default());
        let host = if project {
            projected(&report, "host")
        } else {
            id("host")
        };
        let source = if project {
            &resolved.document().entities[0].components[&sharing_component()]
        } else {
            &document.entities[0].components[&sharing_component()]
        };
        let compiled = compiled_record(&report, &host);
        assert_record(compiled, source);
        assert!(ptr::eq(
            source.value(),
            compiled.value().as_value().unwrap()
        ));
        assert!(!report.source_upgrade_required());
        drop(resolved);
        drop(sources);
        drop(document);
        assert_eq!(compiled_record(&report, &host).value(), &expected);
    }
}

#[test]
fn projected_local_reference_rewrite_separates_storage_and_preserves_the_raw_prefab() {
    let registry = sharing_registry(ComponentSchemaVersion::ONE);
    let original = sharing_value(local(&id("socket")));
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(
        request("base"),
        key("base"),
        PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("host"))
                .with_component(sharing_component(), effective_record(original.clone())),
            SceneEntityRecord::new(id("socket")),
        ]),
    );
    let resolved = sources.resolve_prefab(&request("base")).unwrap();
    let document = scene(PrefabOverlay::new(key("base")));
    let report = expand(&registry, &document, &sources, Default::default());
    let host = projected(&report, "host");
    let socket = projected(&report, "socket");
    assert_ne!(socket, id("socket"));
    let source = &resolved.document().entities[0].components[&sharing_component()];
    let compiled = compiled_record(&report, &host);
    assert!(!ptr::eq(
        source.value(),
        compiled.value().as_value().unwrap()
    ));
    assert_eq!(source.value(), &original);
    let mut expected = original.clone();
    expected
        .set_path(&ComponentFieldPath::from_fields(["target"]), local(&socket))
        .unwrap();
    assert_eq!(compiled.value(), &expected);
    drop(resolved);
    drop(sources);
    drop(document);
    assert_eq!(compiled_record(&report, &host).value(), &expected);
}

#[test]
fn public_deep_clones_and_value_edits_preserve_earlier_compiled_values() {
    let registry = sharing_registry(ComponentSchemaVersion::ONE);
    let sources = InMemoryPrefabSourceResolver::new();
    let original = sharing_value(ComponentValue::Null);
    let mut document = sharing_document(original.clone());
    let report = expand(&registry, &document, &sources, Default::default());
    let mut copied = document.clone();
    let source = &document.entities[0].components[&sharing_component()];
    let copy = &copied.entities[0].components[&sharing_component()];
    assert_eq!(source, copy);
    assert!(ptr::eq(
        source.value(),
        compiled_record(&report, &id("host"))
            .value()
            .as_value()
            .unwrap(),
    ));
    assert!(!ptr::eq(source.value(), copy.value()));
    let (Some(ComponentValue::List(source_items)), Some(ComponentValue::List(copy_items))) =
        (source.value().get("payload"), copy.value().get("payload"))
    else {
        panic!("expected the complete list payload in both public documents")
    };
    assert!(!ptr::eq(source_items.as_ptr(), copy_items.as_ptr()));
    let (ComponentValue::String(source_text), ComponentValue::String(copy_text)) =
        (&source_items[0], &copy_items[0])
    else {
        panic!("expected independently owned string leaves")
    };
    assert!(!ptr::eq(source_text.as_ptr(), copy_text.as_ptr()));

    let replacement = sharing_value(local(&id("host")));
    copied.entities[0]
        .components
        .get_mut(&sharing_component())
        .unwrap()
        .set_value(replacement.clone());
    let escaped = copied.entities[0]
        .components
        .remove(&sharing_component())
        .unwrap()
        .into_value();
    assert_eq!(escaped, replacement);
    assert_eq!(
        document.entities[0].components[&sharing_component()].value(),
        &original,
    );

    let mut edited = original.clone();
    edited
        .set_path(
            &ComponentFieldPath::from_fields(["revision"]),
            ComponentValue::U64(9),
        )
        .unwrap();
    document.entities[0]
        .components
        .get_mut(&sharing_component())
        .unwrap()
        .value_mut()
        .set_path(
            &ComponentFieldPath::from_fields(["revision"]),
            ComponentValue::U64(9),
        )
        .unwrap();
    assert_eq!(compiled_record(&report, &id("host")).value(), &original);
    let later = expand(&registry, &document, &sources, Default::default());
    let mut owned = document.entities[0]
        .components
        .remove(&sharing_component())
        .unwrap()
        .into_value();
    assert_eq!(owned, edited);
    owned
        .set_path(
            &ComponentFieldPath::from_fields(["revision"]),
            ComponentValue::U64(13),
        )
        .unwrap();
    assert_eq!(compiled_record(&report, &id("host")).value(), &original);
    assert_eq!(compiled_record(&later, &id("host")).value(), &edited);
}

#[test]
fn migration_and_default_insertion_preserve_sources_and_previous_compilations() {
    let current = sharing_registry(ComponentSchemaVersion::ONE);
    let next_version = ComponentSchemaVersion::new(2).unwrap();
    let next = sharing_registry(next_version);
    let sources = InMemoryPrefabSourceResolver::new();
    for migrate in [false, true] {
        let original = sharing_value(ComponentValue::Null);
        let mut document = sharing_document(original.clone());
        let previous = expand(&current, &document, &sources, Default::default());
        if !migrate {
            let ComponentValue::Map(fields) = document.entities[0]
                .components
                .get_mut(&sharing_component())
                .unwrap()
                .value_mut()
            else {
                panic!("expected source fields")
            };
            assert_eq!(fields.remove("revision"), Some(ComponentValue::U64(7)));
        }
        let before = document.clone();
        let registry = if migrate { &next } else { &current };
        let report = expand(registry, &document, &sources, Default::default());
        let mut expected = original.clone();
        if migrate {
            expected
                .set_path(
                    &ComponentFieldPath::from_fields(["revision"]),
                    ComponentValue::U64(11),
                )
                .unwrap();
        }
        let compiled = compiled_record(&report, &id("host"));
        assert_eq!(compiled.value(), &expected);
        assert_eq!(
            compiled.version(),
            if migrate {
                next_version
            } else {
                ComponentSchemaVersion::ONE
            },
        );
        assert_eq!(compiled.schema_owner(), Some(&owner()));
        assert_eq!(report.source_upgrade_required(), migrate);
        let source = document.entities[0].components[&sharing_component()].value();
        if migrate {
            assert!(!ptr::eq(source, compiled.value().as_value().unwrap()));
        } else {
            assert!(compiled.value().as_value().is_none());
            assert!(ptr::eq(
                source.get("payload").unwrap(),
                compiled.value().get("payload").unwrap().as_value().unwrap(),
            ));
        }
        assert_eq!(document, before);
        assert_eq!(compiled_record(&previous, &id("host")).value(), &original);
        assert_eq!(
            compiled_record(&previous, &id("host")).version(),
            ComponentSchemaVersion::ONE,
        );
        drop(document);
        assert_eq!(compiled_record(&previous, &id("host")).value(), &original);
        assert_eq!(compiled_record(&report, &id("host")).value(), &expected);
    }
}

#[test]
fn native_migration_defaults_reuse_list_storage_and_keep_original_budgets() {
    fn list_ptr(value: &ComponentValue) -> usize {
        let Some(ComponentValue::List(items)) = value.get("payload") else {
            panic!("expected retained list payload")
        };
        items.as_ptr() as usize
    }

    let migrations = Arc::new(AtomicUsize::new(0));
    let migrated_list = Arc::new(AtomicUsize::new(0));
    let version = ComponentSchemaVersion::new(2).unwrap();
    let schema =
        sharing_schema(version).with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("settings"),
            "Settings",
            ComponentFieldPath::from_fields(["settings", "values"]),
            ComponentValueShape::list(ComponentValueKind::U64.into()),
            ComponentValue::List(vec![ComponentValue::U64(42)]),
        )
        .with_capabilities([ComponentCapability::Scene])]);
    let registry = sharing_registry_with_migration(schema, {
        let migrations = Arc::clone(&migrations);
        let migrated_list = Arc::clone(&migrated_list);
        move |mut value| {
            migrations.fetch_add(1, Ordering::SeqCst);
            migrated_list.store(list_ptr(&value), Ordering::SeqCst);
            let ComponentValue::Map(fields) = &mut value else {
                panic!("expected migrating map payload")
            };
            fields.insert("revision".into(), ComponentValue::U64(11));
            assert_eq!(fields.remove("target"), Some(ComponentValue::Null));
            Ok(value)
        }
    });
    let original = sharing_value(ComponentValue::Null);
    let document = sharing_document(original.clone());
    let before = document.clone();
    let current = sharing_registry(ComponentSchemaVersion::ONE);
    let sources = InMemoryPrefabSourceResolver::new();
    let previous = expand(&current, &document, &sources, Default::default());
    let compile = |limits| {
        let mut loader = |_: &AssetRef, _: &mut crate::prefab::SceneCompilationWorkingSet| {
            Ok::<_, std::convert::Infallible>(None)
        };
        let mut context = crate::prefab::SceneCompilationContext::new(
            (&registry).into(),
            &mut loader,
            SceneCompilationOptions::default().with_limits(limits),
            None,
        );
        context.admit_terminal_work(7, 11, Some(13)).unwrap();
        let report = context.compile_scene_document(&document);
        (report, context.usage, context.reference_work)
    };
    let (report, usage, reference_work) = compile(Default::default());
    let compiled = compiled_record(&report, &id("host"));
    let mut expected = original.clone();
    let ComponentValue::Map(fields) = &mut expected else {
        panic!("expected effective map payload")
    };
    fields.insert("revision".into(), ComponentValue::U64(11));
    fields.insert(
        "settings".into(),
        ComponentValue::map([(
            "values",
            ComponentValue::List(vec![ComponentValue::U64(42)]),
        )]),
    );
    assert_eq!(compiled.value(), &expected);
    assert_eq!(compiled.version(), version);
    assert!(report.source_upgrade_required());
    assert_eq!(migrations.load(Ordering::SeqCst), 1);
    assert_eq!(
        list_ptr(compiled.value().as_value().unwrap()),
        migrated_list.load(Ordering::SeqCst)
    );
    let exact = SceneCompilationLimits::default()
        .with_working_bytes(ByteLimit::new(usage.checked_work_bytes().unwrap()).unwrap())
        .with_reference_work_nodes(ItemLimit::new(reference_work.0).unwrap())
        .with_reference_work_bytes(ByteLimit::new(reference_work.1).unwrap());
    let (fits, exact_usage, exact_work) = compile(exact);
    assert_eq!(compiled_record(&fits, &id("host")).value(), &expected);
    assert_eq!(exact_usage, usage);
    assert_eq!(exact_work, reference_work);
    assert_eq!(migrations.load(Ordering::SeqCst), 2);
    let shallow = ComponentValueMaterializationLimits::new(
        ItemLimit::new(4_096).unwrap(),
        ByteLimit::new(1024 * 1024).unwrap(),
        DepthLimit::new(3).unwrap(),
    );
    let depth_limit = exact.with_materialization_limits(shallow);
    let source_limit = exact.with_materialization_limits(shallow.with_nodes(ItemLimit::ONE));
    let short = exact.working_bytes().get() - 1;
    let working_limit = exact.with_working_bytes(ByteLimit::new(short).unwrap());
    for (limits, kind, maximum, calls) in [
        (depth_limit, "materialized-value-depth", 3, 1),
        (working_limit, "working-bytes", short, 1),
        (source_limit, "source-value-nodes", 1, 0),
    ] {
        migrations.store(0, Ordering::SeqCst);
        let (refused, refused_usage, refused_work) = compile(limits);
        assert!(refused.candidate.is_none());
        let diagnostic = refused
            .diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code().as_str() == "scene.prefab-expansion-budget-exceeded"
            })
            .unwrap();
        for (name, value) in [
            ("budget-kind", DiagnosticValueRef::Identifier(kind)),
            ("maximum", DiagnosticValueRef::Unsigned(maximum as u64)),
        ] {
            assert_eq!(field(diagnostic, name), value);
        }
        assert!(matches!(
            field(diagnostic, "observed"),
            DiagnosticValueRef::Unsigned(value) if value > maximum as u64
        ));
        assert_eq!(migrations.load(Ordering::SeqCst), calls);
        assert!(refused_work.0 >= 7 && refused_work.1 >= 11);
        assert!(refused_usage.scratch_bytes >= 13);
        if kind != "working-bytes" {
            assert_eq!(refused_usage.materialized_value_nodes, 0);
            assert_eq!(refused_usage.materialized_value_bytes, 0);
        }
        assert_eq!(document, before);
        assert_eq!(compiled_record(&previous, &id("host")).value(), &original);
        assert_eq!(compiled_record(&report, &id("host")).value(), &expected);
    }
}

#[test]
fn property_replacement_preserves_raw_prefab_and_the_previous_compiled_output() {
    let registry = sharing_registry(ComponentSchemaVersion::ONE);
    let original = sharing_value(ComponentValue::Null);
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(
        request("base"),
        key("base"),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("host"))
            .with_component(sharing_component(), effective_record(original.clone()))]),
    );
    let document = scene(PrefabOverlay::new(key("base")));
    let previous = expand(&registry, &document, &sources, Default::default());
    let before_document = document.clone();
    let resolved = sources.resolve_prefab(&request("base")).unwrap();
    let before_prefab = resolved.document().clone();
    let source = &resolved.document().entities[0].components[&sharing_component()];
    assert!(ptr::eq(
        source.value(),
        compiled_record(&previous, &projected(&previous, "host"))
            .value()
            .as_value()
            .unwrap(),
    ));
    let mut overlay = PrefabOverlay::new(key("base"));
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("host")),
        component: sharing_component(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("payload")),
            intent: PrefabPropertyIntent::Set(ComponentValue::List(vec![ComponentValue::String(
                "replacement".into(),
            )])),
        }],
    });
    let replacement_document = scene(overlay);
    let report = expand(
        &registry,
        &replacement_document,
        &sources,
        Default::default(),
    );
    let host = projected(&report, "host");
    let mut expected = original.clone();
    expected
        .set_path(
            &ComponentFieldPath::from_fields(["payload"]),
            ComponentValue::List(vec![ComponentValue::String("replacement".into())]),
        )
        .unwrap();
    assert_eq!(compiled_record(&report, &host).value(), &expected);
    assert_eq!(source.value(), &original);
    assert_eq!(resolved.document(), &before_prefab);
    assert_eq!(document, before_document);
    drop(resolved);
    drop(sources);
    drop(document);
    drop(replacement_document);
    assert_eq!(
        compiled_record(&previous, &projected(&previous, "host")).value(),
        &original,
    );
    assert_eq!(compiled_record(&report, &host).value(), &expected);
}

#[test]
fn data_preparation_keeps_old_values_after_candidate_drop_and_enforces_full_receiving_limits() {
    let data_owner = ComponentSchemaOwnerSnapshot::admit(
        owner(),
        ComponentSchemaCatalog {
            components: vec![sharing_schema(ComponentSchemaVersion::ONE)],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let bindings = SceneComponentBindings::new(
        ComponentRegistryBuilder::new().freeze().unwrap(),
        [data_owner],
    )
    .unwrap();
    let original = sharing_value(ComponentValue::Null);
    let mut document = SceneDocument::from_ordered_roots(["host", "peer"].map(|name| {
        SceneEntityRecord::new(id(name))
            .with_component(sharing_component(), effective_record(original.clone()))
    }));
    let before = document.clone();
    let report = bindings.compile_scene(&document, &InMemoryPrefabSourceResolver::new());
    expanded(&report);
    assert_eq!(document, before);
    let candidate = report.candidate.unwrap().bind_source(
        SourceDocumentKey::named(
            "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
            "payload-sharing.scene",
            SourceDocumentKind::Scene,
        )
        .unwrap(),
    );
    let old_value = candidate.document().entities()[0].components()[&sharing_component()]
        .value()
        .as_value()
        .unwrap() as *const ComponentValue;
    let cost = original.cost();
    let nodes = cost.nodes() * 2;
    let bytes = cost.logical_bytes() * 2;
    let limits = |nodes, bytes, depth| {
        ComponentValueMaterializationLimits::new(
            ItemLimit::new(nodes).unwrap(),
            ByteLimit::new(bytes).unwrap(),
            DepthLimit::new(depth).unwrap(),
        )
    };
    let occurrence = SceneOccurrenceKey::new("payload-sharing", "prepared").unwrap();
    let mut world = World::new();
    let sentinel = world.spawn_empty().id();
    let initialization = bindings
        .prepare_scene_with_limits(
            &mut world,
            &candidate,
            &occurrence,
            None,
            limits(nodes, bytes, 3),
        )
        .unwrap();
    drop(initialization);
    let baseline_entities = world.iter_entities().count();
    for (receiving, kind, maximum) in [
        (
            limits(nodes - 1, bytes, 3),
            "materialization-node-limit",
            cost.nodes() - 1,
        ),
        (
            limits(nodes, bytes - 1, 3),
            "materialization-byte-limit",
            cost.logical_bytes() - 1,
        ),
        (limits(nodes, bytes, 2), "materialization-depth-limit", 2),
    ] {
        let rejected = bindings
            .prepare_scene_with_limits(&mut world, &candidate, &occurrence, None, receiving)
            .expect_err("shared source storage cannot waive complete receiving admission");
        assert!(rejected.diagnostics.has_errors(), "{rejected:?}");
        let diagnostic = rejected
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code().as_str() == "scene.invalid-component-payload")
            .expect("receiving admission must retain the value-limit diagnostic");
        assert_eq!(
            field(diagnostic, "codec-error-kind"),
            DiagnosticValueRef::Identifier(kind),
        );
        assert_eq!(
            field(diagnostic, "maximum"),
            DiagnosticValueRef::Unsigned(maximum as u64),
        );
        assert!(matches!(
            field(diagnostic, "observed"),
            DiagnosticValueRef::Unsigned(observed) if observed > maximum as u64
        ));
        assert_eq!(world.iter_entities().count(), baseline_entities);
        assert!(world.iter_entities().all(|entity| {
            !entity.contains::<crate::SceneEntitySource>()
                && !entity.contains::<crate::SceneDataComponents>()
        }));
        assert!(world.get_entity(sentinel).is_ok());
    }
    let canceled = bindings
        .prepare_scene_with_limits(
            &mut world,
            &candidate,
            &occurrence,
            None,
            limits(nodes, bytes, 3),
        )
        .expect("the complete two-record allowance must be sufficient");
    let prepared = bindings
        .prepare_scene_with_limits(
            &mut world,
            &candidate,
            &occurrence,
            None,
            limits(nodes, bytes, 3),
        )
        .expect("each prepared owner must admit the complete value independently");
    document.entities[0]
        .components
        .get_mut(&sharing_component())
        .unwrap()
        .value_mut()
        .set_path(
            &ComponentFieldPath::from_fields(["revision"]),
            ComponentValue::U64(99),
        )
        .unwrap();
    document.entities[1]
        .components
        .get_mut(&sharing_component())
        .unwrap()
        .set_value(sharing_value(local(&id("host"))));
    for entity in candidate.document().entities() {
        assert_eq!(entity.components()[&sharing_component()].value(), &original);
    }
    drop(candidate);
    drop(document);
    drop(canceled);
    assert_eq!(world.iter_entities().count(), baseline_entities);
    assert!(world.get_entity(sentinel).is_ok());
    let report = prepared.commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    let instance = report
        .instance
        .expect("the independent preparation must commit");
    assert_eq!(instance.len(), 2);
    for name in ["host", "peer"] {
        let EntityLookup::Resolved(entity) = instance.resolve(&world, &id(name)) else {
            panic!("prepared data must publish its complete original entity set")
        };
        let data = bindings
            .data_component(&world, entity, &sharing_component())
            .unwrap()
            .expect("prepared data must survive the compiled candidate");
        assert_eq!(data.value(), &original);
        assert_eq!(data.version(), ComponentSchemaVersion::ONE);
        assert_eq!(data.schema_owner(), Some(&owner()));
        if name == "host" {
            assert!(ptr::eq(data.value().as_value().unwrap(), old_value));
        }
    }
    assert!(world.get_entity(sentinel).is_ok());
}
