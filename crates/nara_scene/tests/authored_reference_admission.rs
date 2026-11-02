//! Authored intent stays in Source; every Native entry admits a complete flat value batch.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_diagnostic::DiagnosticValueRef;
use nara_ecs::{Component, World};
use nara_identity::{AuthoredEntityReference, EntityLookup, EntityReference, ReferenceInputId};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistryBuilder, ComponentRegistrySnapshot, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueMaterializationLimits, ComponentValueShape,
    PersistentComponentSchemaOwnerId, PreparedComponentCandidate,
};
use nara_scene::{
    ExpandedSceneCandidate, SceneComponentBindings, SceneComponentRecord, SceneDataComponentError,
    SceneDocument, SceneEntityId, SceneEntityRecord, SceneOccurrenceKey, SourceDocumentKey,
    SourceDocumentKind,
};

#[derive(Component)]
struct First;

#[derive(Component)]
struct Last;

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn first_id() -> ComponentTypeId {
    ComponentTypeId::new("a.First")
}

fn last_id() -> ComponentTypeId {
    ComponentTypeId::new("z.Last")
}

fn authored() -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference::source(id("outside")),
    })
}

fn input() -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference::input(ReferenceInputId::new("target").unwrap()),
    })
}

fn link_schema(default: ComponentValue) -> ComponentSchema {
    ComponentSchema::new(last_id(), "Last", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueShape::EntityRef,
            default,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef)])
}

fn registry(schema: ComponentSchema) -> (ComponentRegistrySnapshot, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = ComponentRegistryBuilder::new();
    let first_calls = Arc::clone(&calls);
    registry
        .register_persistent_component_codec::<First, _, _>(
            ComponentSchema::new(first_id(), "First", ComponentSchemaVersion::ONE)
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("value"),
                    "Value",
                    ComponentFieldPath::empty(),
                    ComponentValueKind::Bool,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            move |value| {
                first_calls.fetch_add(1, Ordering::SeqCst);
                if value != &ComponentValue::Bool(true) {
                    return Err(ComponentCodecError::Message("First must be true".into()));
                }
                Ok(PreparedComponentCandidate::insert(First))
            },
            |_| Ok(ComponentValue::Bool(true)),
        )
        .unwrap();
    let last_calls = Arc::clone(&calls);
    registry
        .register_persistent_component_codec::<Last, _, _>(
            schema,
            move |_| {
                // Deliberately permissive: the engine must establish flatness before this
                // hand-written codec, rather than relying on a derived reference decoder.
                last_calls.fetch_add(1, Ordering::SeqCst);
                Ok(PreparedComponentCandidate::insert(Last))
            },
            |_| Ok(ComponentValue::map([("target", ComponentValue::Null)])),
        )
        .unwrap();
    (registry.freeze().unwrap(), calls)
}

fn record(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value)
}

fn batch(value: ComponentValue, separate_entities: bool) -> SceneDocument {
    let mut first = SceneEntityRecord::new(id("a"))
        .with_component(first_id(), record(ComponentValue::Bool(true)));
    if separate_entities {
        SceneDocument::from_ordered_roots([
            first,
            SceneEntityRecord::new(id("z")).with_component(last_id(), record(value)),
        ])
    } else {
        first.components.insert(last_id(), record(value));
        SceneDocument::from_ordered_roots([first])
    }
}

#[test]
fn native_batch_rejects_later_authored_values_and_defaults_before_first_codec() {
    for separate_entities in [false, true] {
        for value in [
            ComponentValue::map([("target", authored())]),
            ComponentValue::map([("target", input())]),
            ComponentValue::Map(Default::default()),
        ] {
            let (registry, calls) = registry(link_schema(authored()));
            let document = batch(value, separate_entities);
            let before = document.clone();
            let diagnostics = document.validate(&registry);
            assert!(diagnostics.has_errors(), "{diagnostics:?}");
            assert!(diagnostics.iter().any(|diagnostic| {
                diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "codec-error-kind"
                        && field.value()
                            == DiagnosticValueRef::Identifier("unresolved-authored-reference")
                })
            }));
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            assert_eq!(document, before);
        }
    }
}

#[test]
fn explicit_null_or_flat_reference_hides_authored_default() {
    for value in [
        ComponentValue::Null,
        ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id("a") }),
    ] {
        let (registry, calls) = registry(link_schema(authored()));
        let document = batch(ComponentValue::map([("target", value)]), false);
        let diagnostics = document.validate(&registry);
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}

#[test]
fn native_rejects_contextual_default_metadata_even_when_effective_values_are_flat() {
    for value in [
        ComponentValue::Null,
        ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id("a") }),
    ] {
        let (registry, calls) = registry(link_schema(authored()));
        let mut document = batch(ComponentValue::map([("target", value)]), true);
        document.entities[1]
            .components
            .get_mut(&last_id())
            .unwrap()
            .contextual_defaults
            .insert(ComponentFieldId::new("target"), input());
        let before = document.clone();
        let diagnostics = document.validate(&registry);
        assert!(diagnostics.has_errors(), "{diagnostics:?}");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(document, before);
    }
}

#[test]
fn native_rejects_authored_leaves_in_opaque_and_undeclared_contents_before_any_codec() {
    let schema = ComponentSchema::new(last_id(), "Opaque", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("payload"),
            "Payload",
            ComponentFieldPath::from_fields(["payload"]),
            ComponentValueShape::opaque(ComponentValueKind::Map),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    for field in ["payload", "retained"] {
        let (registry, calls) = registry(schema.clone());
        let mut value = ComponentValue::map([("payload", ComponentValue::Map(Default::default()))]);
        let ComponentValue::Map(fields) = &mut value else {
            unreachable!()
        };
        fields.insert(
            field.into(),
            ComponentValue::map([("nested", ComponentValue::List(vec![input()]))]),
        );
        let diagnostics = batch(value, false).validate(&registry);
        assert!(diagnostics.has_errors(), "{diagnostics:?}");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn complete_effective_batch_budget_is_checked_before_codec_work() {
    let (registry, calls) = registry(link_schema(ComponentValue::Null));
    let bindings = SceneComponentBindings::new(registry, []).unwrap();
    let document = batch(ComponentValue::Map(Default::default()), false);
    // The complete effective values are one Bool plus a Map with one Null member.
    for (nodes, bytes, accepted) in [(3, 7, true), (2, 7, false), (3, 6, false)] {
        calls.store(0, Ordering::SeqCst);
        let diagnostics = bindings.validate_scene(
            &document,
            None,
            ComponentValueMaterializationLimits::new(
                ItemLimit::new(nodes).unwrap(),
                ByteLimit::new(bytes).unwrap(),
                DepthLimit::new(4).unwrap(),
            ),
        );
        assert_eq!(!diagnostics.has_errors(), accepted, "{diagnostics:?}");
        assert_eq!(calls.load(Ordering::SeqCst), if accepted { 2 } else { 0 });
    }
}

fn owner_id() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.authored-admission").unwrap()
}

fn data_bindings(
    registry: ComponentRegistrySnapshot,
    schema: ComponentSchema,
) -> SceneComponentBindings {
    let owner = ComponentSchemaOwnerSnapshot::admit(
        owner_id(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    SceneComponentBindings::new(registry, [owner]).unwrap()
}

#[test]
fn later_data_member_also_blocks_earlier_native_codec() {
    let (native, calls) = registry(link_schema(ComponentValue::Null));
    // The data member uses an independent admitted owner, with no codec for that definition.
    let mut schema = link_schema(authored());
    schema.id = ComponentTypeId::new("zz.Data");
    schema.aliases = vec!["Data".into()];
    let data_id = schema.id.clone();
    let bindings = data_bindings(native, schema);
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("a"))
            .with_component(first_id(), record(ComponentValue::Bool(true))),
        SceneEntityRecord::new(id("z")).with_component(
            data_id,
            record(ComponentValue::Map(Default::default())).with_owner(owner_id()),
        ),
    ]);
    let diagnostics = bindings.validate_scene(
        &document,
        None,
        ComponentValueMaterializationLimits::default(),
    );
    assert!(diagnostics.has_errors(), "{diagnostics:?}");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn data_setter_rejects_authored_values_and_defaults_without_replacing_prior_member() {
    let bindings = data_bindings(
        ComponentRegistryBuilder::new().freeze().unwrap(),
        link_schema(authored()),
    );
    let prior = ComponentValue::map([("target", ComponentValue::Null)]);
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("a"))
        .with_component(last_id(), record(prior.clone()).with_owner(owner_id()))]);
    let source = ExpandedSceneCandidate::plain(
        SourceDocumentKey::named(
            "c1c7d65e-6cb3-4e7f-9247-a03e4bb75694".parse().unwrap(),
            "authored-admission.scene",
            SourceDocumentKind::Scene,
        )
        .unwrap(),
        Arc::new(document),
    );
    let mut world = World::new();
    let spawned = bindings
        .prepare_scene(
            &mut world,
            &source,
            &SceneOccurrenceKey::new("game.admission", "main").unwrap(),
            None,
        )
        .unwrap()
        .commit(&mut world);
    assert!(!spawned.diagnostics.has_errors(), "{spawned:?}");
    let instance = spawned.instance.unwrap();
    let EntityLookup::Resolved(entity) = instance.resolve(&world, &id("a")) else {
        panic!("the admitted data entity must be live");
    };
    for value in [
        ComponentValue::map([("target", authored())]),
        ComponentValue::map([("target", input())]),
        ComponentValue::Map(Default::default()),
    ] {
        let error = bindings
            .set_data_component(
                &mut world,
                entity,
                &last_id(),
                value,
                None,
                ComponentValueMaterializationLimits::default(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            SceneDataComponentError::InvalidValue {
                source: ComponentCodecError::UnresolvedAuthoredReference,
                ..
            }
        ));
        assert_eq!(
            bindings
                .data_component(&world, entity, &last_id())
                .unwrap()
                .unwrap()
                .value,
            prior,
        );
    }
}

#[cfg(feature = "serde")]
#[test]
fn source_json_and_ron_preserve_authored_intent_without_codec_but_native_refuses() {
    use nara_scene::SceneDocumentCandidate;

    for value in [authored(), input()] {
        let (registry, calls) = registry(link_schema(ComponentValue::Null));
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("a"))
            .with_component(last_id(), record(ComponentValue::map([("target", value)])))]);
        for candidate in [
            SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap()).unwrap(),
            SceneDocumentCandidate::decode_ron_str(&document.to_ron_string().unwrap()).unwrap(),
        ] {
            let published = candidate.publish(&registry).unwrap();
            assert_eq!(published.document(), &document);
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            let diagnostics = published.document().validate(&registry);
            assert!(diagnostics.has_errors(), "{diagnostics:?}");
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
    }
}

#[cfg(feature = "serde")]
#[test]
fn source_requires_explicit_context_for_source_dependent_schema_defaults_even_when_hidden() {
    use nara_scene::SceneDocumentCandidate;

    for default in [
        authored(),
        ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id("a") }),
    ] {
        for value in [
            ComponentValue::Map(Default::default()),
            ComponentValue::map([("target", ComponentValue::Null)]),
        ] {
            let (registry, calls) = registry(link_schema(default.clone()));
            let document = SceneDocument::from_ordered_roots([
                SceneEntityRecord::new(id("a")).with_component(last_id(), record(value))
            ]);
            for candidate in [
                SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
                    .unwrap(),
                SceneDocumentCandidate::decode_ron_str(&document.to_ron_string().unwrap()).unwrap(),
            ] {
                let error = candidate.publish(&registry).unwrap_err();
                assert!(error.diagnostics().has_errors());
                assert_eq!(calls.load(Ordering::SeqCst), 0);
                assert!(
                    document.entities[0].components[&last_id()]
                        .contextual_defaults
                        .is_empty()
                );
            }
        }
    }
}

#[cfg(feature = "serde")]
#[test]
fn source_keeps_context_free_null_persistent_and_scalar_default_fallbacks() {
    use nara_scene::SceneDocumentCandidate;

    let persistent = ComponentValue::EntityReference(EntityReference::Persistent {
        entity: nara_identity::PersistentRuntimeReference::new(
            nara_identity::PersistentRuntimeNamespaceId::new("game.saved").unwrap(),
            nara_identity::PersistentRuntimeId::parse_str("00000000-0000-4000-8000-000000000051")
                .unwrap(),
        ),
    });
    let scalar = ComponentSchema::new(last_id(), "Last", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueKind::U64,
            ComponentValue::U64(7),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    for schema in [
        link_schema(ComponentValue::Null),
        link_schema(persistent),
        scalar,
    ] {
        let (registry, calls) = registry(schema);
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("a"))
            .with_component(last_id(), record(ComponentValue::Map(Default::default())))]);
        let published =
            SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
                .unwrap()
                .publish(&registry)
                .unwrap();
        assert_eq!(published.document(), &document);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(
            published.document().entities[0].components[&last_id()]
                .contextual_defaults
                .is_empty()
        );
    }
}

#[cfg(feature = "serde")]
fn small_source_candidates(document: &SceneDocument) -> [nara_scene::SceneDocumentCandidate; 2] {
    use nara_scene::{SceneDocumentCandidate, SceneFileLimits};

    let limits = SceneFileLimits::default()
        .with_component_value_nodes(ItemLimit::new(32).unwrap())
        .with_component_value_bytes(ByteLimit::new(512).unwrap());
    [
        SceneDocumentCandidate::decode_json_bytes_with_limits(
            document.to_json_string().unwrap().as_bytes(),
            limits,
        )
        .unwrap(),
        SceneDocumentCandidate::decode_ron_bytes_with_limits(
            document.to_ron_string().unwrap().as_bytes(),
            limits,
        )
        .unwrap(),
    ]
}

#[cfg(feature = "serde")]
#[test]
fn source_limits_ignore_hidden_scalar_schema_defaults_but_bound_selected_defaults() {
    let schema = ComponentSchema::new(last_id(), "Last", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("label"),
            "Label",
            ComponentFieldPath::from_fields(["label"]),
            ComponentValueKind::String,
            ComponentValue::String("s".repeat(1024 * 1024)),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let (registry, calls) = registry(schema);
    for explicit in [true, false] {
        let value = if explicit {
            ComponentValue::map([("label", ComponentValue::String("x".into()))])
        } else {
            ComponentValue::Map(Default::default())
        };
        let document = SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("a")).with_component(last_id(), record(value))
        ]);
        for candidate in small_source_candidates(&document) {
            let before = calls.load(Ordering::SeqCst);
            let result = candidate.publish(&registry);
            if explicit {
                let published = result.unwrap();
                assert_eq!(published.document(), &document);
                assert_eq!(calls.load(Ordering::SeqCst), before + 1);
            } else {
                assert!(result.unwrap_err().diagnostics().has_errors());
                assert_eq!(calls.load(Ordering::SeqCst), before);
            }
        }
    }
}

#[cfg(feature = "serde")]
#[test]
fn source_limits_classify_mixed_schema_defaults_without_charging_hidden_payloads() {
    let persistent = ComponentValue::EntityReference(EntityReference::Persistent {
        entity: nara_identity::PersistentRuntimeReference::new(
            nara_identity::PersistentRuntimeNamespaceId::new("game.saved").unwrap(),
            nara_identity::PersistentRuntimeId::parse_str("00000000-0000-4000-8000-000000000052")
                .unwrap(),
        ),
    });
    for (reference, requires_context) in [
        (ComponentValue::Null, false),
        (persistent, false),
        (
            ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id("a") }),
            true,
        ),
        (authored(), true),
    ] {
        let schema = ComponentSchema::new(last_id(), "Last", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("settings"),
                "Settings",
                ComponentFieldPath::from_fields(["settings"]),
                ComponentValueShape::record([
                    (
                        "target".into(),
                        ComponentValueShape::optional(ComponentValueShape::EntityRef),
                    ),
                    ("label".into(), ComponentValueKind::String.into()),
                ]),
                ComponentValue::map([
                    ("target", reference),
                    ("label", ComponentValue::String("s".repeat(1024 * 1024))),
                ]),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef)]);
        let (registry, calls) = registry(schema);
        for explicit in [
            ComponentValue::Null,
            ComponentValue::map([
                ("target", ComponentValue::Null),
                ("label", ComponentValue::String("x".into())),
            ]),
        ] {
            for contextual in [false, true] {
                let mut component = record(ComponentValue::map([("settings", explicit.clone())]));
                if contextual {
                    component = component.with_contextual_default(
                        ComponentFieldId::new("settings"),
                        ComponentValue::Null,
                    );
                }
                let document = SceneDocument::from_ordered_roots([
                    SceneEntityRecord::new(id("a")).with_component(last_id(), component)
                ]);
                for candidate in small_source_candidates(&document) {
                    let before = calls.load(Ordering::SeqCst);
                    let result = candidate.publish(&registry);
                    if requires_context && !contextual {
                        assert!(result.unwrap_err().diagnostics().has_errors());
                        assert_eq!(calls.load(Ordering::SeqCst), before);
                    } else {
                        let published = result.unwrap();
                        assert_eq!(published.document(), &document);
                        assert_eq!(calls.load(Ordering::SeqCst), before + 1);
                    }
                }
            }
        }
    }
}

#[cfg(feature = "serde")]
#[test]
fn source_schema_default_context_scan_does_not_expand_shared_reference_paths() {
    let persistent = ComponentValue::EntityReference(EntityReference::Persistent {
        entity: nara_identity::PersistentRuntimeReference::new(
            nara_identity::PersistentRuntimeNamespaceId::new("game.saved").unwrap(),
            nara_identity::PersistentRuntimeId::parse_str("00000000-0000-4000-8000-000000000053")
                .unwrap(),
        ),
    });
    // The admitted payload is below 16,384 nodes / 4 MiB. Expanding 9,000 reference paths
    // would copy several GiB of this shared key; context classification needs no path storage.
    let default = ComponentValue::map([(
        "k".repeat(512 * 1024),
        ComponentValue::List(vec![persistent; 9_000]),
    )]);
    let schema = ComponentSchema::new(last_id(), "Last", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueShape::map(ComponentValueShape::list(ComponentValueShape::EntityRef)),
            default,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef)]);
    let (registry, calls) = registry(schema);
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("a"))
        .with_component(
            last_id(),
            record(ComponentValue::map([("target", ComponentValue::Null)])),
        )]);
    for candidate in small_source_candidates(&document) {
        let before = calls.load(Ordering::SeqCst);
        let published = candidate.publish(&registry).unwrap();
        assert_eq!(published.document(), &document);
        assert_eq!(calls.load(Ordering::SeqCst), before + 1);
    }
}

#[cfg(feature = "serde")]
#[test]
fn source_round_trip_preserves_hidden_contextual_defaults_and_selects_actual_overrides() {
    use nara_scene::SceneDocumentCandidate;

    for (default, hidden, codec_calls) in [
        (input(), false, 0),
        (input(), true, 1),
        (
            ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id("a") }),
            false,
            1,
        ),
    ] {
        let value = if hidden {
            ComponentValue::map([("target", ComponentValue::Null)])
        } else {
            ComponentValue::Map(Default::default())
        };
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("a"))
            .with_component(
                last_id(),
                record(value).with_contextual_default(ComponentFieldId::new("target"), default),
            )]);
        for candidate in [
            SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap()).unwrap(),
            SceneDocumentCandidate::decode_ron_str(&document.to_ron_string().unwrap()).unwrap(),
        ] {
            let (registry, calls) = registry(link_schema(authored()));
            let published = candidate.publish(&registry).unwrap();
            assert_eq!(published.document(), &document);
            assert_eq!(calls.load(Ordering::SeqCst), codec_calls);
            let diagnostics = published.document().validate(&registry);
            assert!(diagnostics.has_errors(), "{diagnostics:?}");
            assert_eq!(calls.load(Ordering::SeqCst), codec_calls);
        }
    }
}

#[test]
fn source_patch_accepts_authored_intent_and_flat_payloads_keep_business_checks() {
    use nara_scene::{ScenePatchDocument, ScenePatchOperation};

    let (registry, calls) = registry(link_schema(ComponentValue::Null));
    let mut document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("a"))
        .with_component(
            last_id(),
            record(ComponentValue::map([("target", authored())])),
        )]);
    let patch = ScenePatchDocument::new([ScenePatchOperation::ReplaceComponent {
        entity: id("a"),
        component: last_id(),
        value: record(ComponentValue::map([("target", input())])),
    }]);
    let report = patch.apply_to_scene(&mut document, &registry);
    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        document.entities[0].components[&last_id()]
            .value
            .get("target"),
        Some(&input()),
    );

    let before = document.clone();
    let invalid = ScenePatchDocument::new([ScenePatchOperation::AddComponent {
        entity: id("a"),
        component: first_id(),
        value: record(ComponentValue::Bool(false)),
    }]);
    let report = invalid.apply_to_scene(&mut document, &registry);
    assert!(!report.applied);
    assert!(report.diagnostics.has_errors());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(document, before);
}
