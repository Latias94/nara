//! Reference path and declaration-scope regressions through public Scene expansion.

use super::*;
use crate::PrefabExpansionFailureKind;
use nara_identity::{
    PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
};
use nara_reflect::{
    ComponentPropertyKey, ComponentPropertyStep, ComponentShapeMember, ComponentValueKind,
    collect_declared_asset_references_from_schema,
};

fn source_step(source: &str, entity: &str) -> PrefabProjectionStep {
    PrefabProjectionStep::SourceEntity {
        source_document: request(source),
        canonical_source: key(source),
        entity: id(entity),
    }
}

// Observe exact published provenance; production alone resolves authored targets.
fn at_path(report: &PrefabExpansionReport, steps: &[PrefabProjectionStep]) -> SceneEntityId {
    expanded(report);
    let mut matches = report.provenance().unwrap().iter().filter(|(_, origin)| {
        origin
            .projection_path()
            .is_some_and(|path| path.root_anchor() == &id("instance") && path.steps() == steps)
    });
    let entity = matches
        .next()
        .expect("expected published occurrence")
        .0
        .clone();
    assert!(matches.next().is_none(), "published path must be unique");
    entity
}

#[test]
fn surviving_inline_reference_checks_the_actual_nested_canonical_source_and_terminal() {
    let registry = registry(true);
    let mut nested = SceneEntityRecord::new(id("nested"));
    nested.prefab = Some(instance("leaf", None));
    let base = PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("host"))
            .with_component(component(), record(payload(&id("socket")))),
        SceneEntityRecord::new(id("socket")),
        nested,
    ]);

    for (actual_key, terminal, failure) in [
        ("leaf", "socket", None),
        (
            "replacement-leaf",
            "socket",
            Some(PrefabExpansionFailureKind::SourceChanged),
        ),
        (
            "leaf",
            "missing",
            Some(PrefabExpansionFailureKind::MissingTarget),
        ),
    ] {
        // The replacement keeps the same request and entity spelling, changing only its key.
        let sources = InMemoryPrefabSourceResolver::new()
            .with_prefab(request("base"), key("base"), base.clone())
            .with_prefab(
                request("leaf"),
                key(actual_key),
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("socket"))]),
            );
        let reference = AuthoredEntityReference {
            through: vec![
                PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id("instance")),
                    expected_base: key("base"),
                },
                PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id("nested")),
                    expected_base: key("leaf"),
                },
            ],
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(terminal))),
        };
        let mut overlay = PrefabOverlay::new(key("base"));
        overlay.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("host")),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
                intent: PrefabPropertyIntent::Set(authored(reference)),
            }],
        });
        let report = expand(&registry, &scene(overlay), &sources, Default::default());

        if let Some(expected) = failure {
            rejected(&report, "scene.prefab-compilation-failed");
            assert_eq!(report.failure.as_ref().unwrap().reason(), &expected);
        } else {
            let nested_socket = at_path(
                &report,
                &[source_step("base", "nested"), source_step("leaf", "socket")],
            );
            assert_ne!(
                nested_socket,
                at_path(&report, &[source_step("base", "socket")])
            );
            assert_eq!(
                stored(&report, "host"),
                &effective_record(payload(&nested_socket))
            );
        }
    }
}

#[test]
fn keyed_container_reorder_preserves_literal_map_keys_and_untouched_leaf_origins() {
    #[derive(nara_ecs::Component)]
    struct MixedLinks(ComponentValue);

    let component = ComponentTypeId::new("game.KeyedReferenceLeaves");
    let schema = ComponentSchema::new(
        component.clone(),
        "Keyed reference leaves",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("links"),
        "Links",
        ComponentFieldPath::from_fields(["wire_links"]),
        ComponentValueShape::keyed_list(
            ComponentValueShape::record_members([
                ComponentShapeMember::new("wire_id", ComponentValueKind::String)
                    .with_id(ComponentFieldId::new("row-id")),
                ComponentShapeMember::new(
                    "target",
                    ComponentValueShape::optional(ComponentValueShape::EntityRef),
                ),
                ComponentShapeMember::new(
                    "wire_others",
                    ComponentValueShape::map(ComponentValueShape::EntityRef),
                )
                .with_id(ComponentFieldId::new("others")),
                ComponentShapeMember::new("portrait", ComponentValueKind::AssetRef),
            ]),
            ComponentFieldId::new("row-id"),
        ),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef)
    .with_capability(ComponentCapability::AssetRef)]);
    let mut definitions = nara_reflect::ComponentRegistryBuilder::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new("game.overlay-links"),
        ComponentSchemaCatalog::default(),
        None,
    )
    .unwrap();
    definitions
        .register_persistent_component_with_codec::<MixedLinks, _, _>(
            schema.clone(),
            |value| Ok(MixedLinks(value.clone())),
            |value| Ok(value.0.clone()),
        )
        .unwrap();
    let registry = definitions.freeze().unwrap();
    let stable = AssetRef::stable_id("23456789-2345-4345-8345-234567890abc").unwrap();
    let portrait = ComponentValue::map([
        ("kind", ComponentValue::String("stable_id".into())),
        (
            "value",
            ComponentValue::String(stable.as_stable_id().unwrap().to_string()),
        ),
    ]);
    let persistent = ComponentValue::EntityReference(EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("overlay.fixture").unwrap(),
            PersistentRuntimeId::parse_str("34567890-3456-4456-8456-345678901abc").unwrap(),
        ),
    });
    let first = ComponentValue::map([
        ("wire_id", ComponentValue::String("a".into())),
        ("target", persistent),
        (
            "wire_others",
            ComponentValue::map([
                ("name.with.dot", local(&id("socket"))),
                ("untouched", local(&id("socket"))),
            ]),
        ),
        ("portrait", portrait.clone()),
    ]);
    let second = ComponentValue::map([
        ("wire_id", ComponentValue::String("b".into())),
        ("target", ComponentValue::Null),
        ("wire_others", empty()),
        ("portrait", portrait),
    ]);
    let base = PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("host")).with_component(
            component.clone(),
            effective_record(ComponentValue::map([(
                "wire_links",
                ComponentValue::List(vec![first.clone(), second.clone()]),
            )])),
        ),
        SceneEntityRecord::new(id("socket")),
    ]);

    for reordered in [false, true] {
        let mut replacement_first = first.clone();
        for name in ["name.with.dot", "untouched"] {
            replacement_first
                .set_path(
                    &ComponentFieldPath::from_fields(["wire_others", name]),
                    authored(AuthoredEntityReference {
                        through: vec![PrefabInstanceHop {
                            anchor: PrefabEntityOrigin::SourceEntity(id("nested")),
                            expected_base: key("base"),
                        }],
                        target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(
                            id("socket"),
                        )),
                    }),
                )
                .unwrap();
        }
        let rows = if reordered {
            vec![second.clone(), replacement_first]
        } else {
            vec![replacement_first, second.clone()]
        };
        let mut inner = PrefabOverlay::new(key("base"));
        inner.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("host")),
            component: component.clone(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("links")),
                intent: PrefabPropertyIntent::Set(ComponentValue::List(rows)),
            }],
        });
        let mut nested = SceneEntityRecord::new(id("nested"));
        nested.prefab = Some(instance("base", Some(inner)));
        let sources = InMemoryPrefabSourceResolver::new()
            .with_prefab(request("base"), key("base"), base.clone())
            .with_prefab(
                request("outer"),
                key("outer"),
                PrefabDocument::from_ordered_roots([nested]),
            );
        let mut outer = PrefabOverlay::new(key("outer"));
        add(&mut outer, PrefabAddition::new(id("socket")));
        outer.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget {
                through: vec![PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id("nested")),
                    expected_base: key("base"),
                }],
                entity: PrefabEntityOrigin::SourceEntity(id("host")),
            },
            component: component.clone(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("links")).with_steps([
                    ComponentPropertyStep::ListItem {
                        by: ComponentFieldId::new("row-id"),
                        key: ComponentPropertyKey::String("a".into()),
                    },
                    ComponentPropertyStep::Member(ComponentFieldId::new("others")),
                    ComponentPropertyStep::MapEntry("name.with.dot".into()),
                ]),
                intent: PrefabPropertyIntent::Set(authored(AuthoredEntityReference {
                    through: vec![PrefabInstanceHop {
                        anchor: PrefabEntityOrigin::SourceEntity(id("instance")),
                        expected_base: key("outer"),
                    }],
                    target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::InstanceAddition(
                        id("socket"),
                    )),
                })),
            }],
        });
        let mut anchor = SceneEntityRecord::new(id("instance"));
        anchor.prefab = Some(instance("outer", Some(outer)));
        let document = SceneDocument::from_ordered_roots([anchor]);
        let report = expand(&registry, &document, &sources, Default::default());

        {
            let inherited = at_path(
                &report,
                &[
                    source_step("outer", "nested"),
                    source_step("base", "socket"),
                ],
            );
            let added = at_path(
                &report,
                &[PrefabProjectionStep::InstanceAddition {
                    entity: id("socket"),
                }],
            );
            assert_ne!(inherited, added);
            let mut expected_first = first.clone();
            for (name, target) in [("name.with.dot", &added), ("untouched", &inherited)] {
                expected_first
                    .set_path(
                        &ComponentFieldPath::from_fields(["wire_others", name]),
                        local(target),
                    )
                    .unwrap();
            }
            let expected_rows = if reordered {
                vec![second.clone(), expected_first]
            } else {
                vec![expected_first, second.clone()]
            };
            let stored = &entity(&report, "host").components[&component];
            assert_eq!(
                stored,
                &effective_record(ComponentValue::map([(
                    "wire_links",
                    ComponentValue::List(expected_rows),
                )])),
            );
            let assets = collect_declared_asset_references_from_schema(
                &schema,
                schema.version(),
                &stored.value,
            )
            .unwrap();
            assert_eq!(assets.len(), 2);
            assert!(
                assets
                    .iter()
                    .all(|reference| reference.asset_ref() == &stable)
            );
        }
    }
}
