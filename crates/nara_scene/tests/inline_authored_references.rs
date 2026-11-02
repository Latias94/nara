//! Complete production expansion keeps lexical payload and occurrence input environments distinct.

use std::collections::BTreeMap;

use nara_asset::AssetRef;
use nara_ecs::Component;
use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityReference, ReferenceInputId,
};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentPropertyPath, ComponentRegistryBuilder, ComponentRegistrySnapshot, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueShape,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabComponentIntent, PrefabDocument, PrefabEntityOrigin,
    PrefabEntityTarget, PrefabInstance, PrefabInstanceHop, PrefabOverlay, PrefabPropertyClaim,
    PrefabPropertyIntent, PrefabReferenceArguments, SceneComponentRecord, SceneDocument,
    SceneEntityId, SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
};

#[derive(Component)]
struct Link(EntityReference);

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
fn input() -> ReferenceInputId {
    ReferenceInputId::new("u").unwrap()
}
fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.InlineLink")
}
fn source(name: &str) -> AssetRef {
    AssetRef::path(format!("{name}.prefab.ron")).unwrap()
}
fn key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
        name,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}
fn authored(reference: AuthoredEntityReference) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::Authored { reference })
}
fn record(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", value)]),
    )
}
fn registry(default: ComponentValue) -> ComponentRegistrySnapshot {
    let mut registry = ComponentRegistryBuilder::new();
    registry
        .register_persistent_component_with_codec::<Link, _, _>(
            ComponentSchema::new(component(), "Inline link", ComponentSchemaVersion::ONE)
                .with_capabilities([ComponentCapability::Scene])
                .with_fields([ComponentFieldSchema::optional_with_default(
                    ComponentFieldId::new("target"),
                    "Target",
                    ComponentFieldPath::from_fields(["target"]),
                    ComponentValueShape::optional(ComponentValueShape::EntityRef),
                    default,
                )
                .with_capabilities([ComponentCapability::Scene, ComponentCapability::EntityRef])]),
            |value| Ok(Link(value.field_entity_reference("target")?.clone())),
            |value| {
                Ok(ComponentValue::map([(
                    "target",
                    ComponentValue::EntityReference(value.0.clone()),
                )]))
            },
        )
        .unwrap();
    registry.freeze().unwrap()
}
fn instance(
    name: &str,
    argument: Option<EntityReference>,
    overlay: Option<PrefabOverlay>,
) -> PrefabInstance {
    PrefabInstance {
        binding: nara_scene::PrefabInstanceBinding::Forest,
        source: source(name),
        arguments: argument.map(|argument| PrefabReferenceArguments {
            base: key(name),
            values: BTreeMap::from([(input(), argument)]),
        }),
        overrides: overlay,
    }
}
fn local(name: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(name) }
}

#[test]
fn payload_scope_is_lexical_while_explicit_routes_select_nested_input_environments() {
    let registry = registry(ComponentValue::Null);
    for choice in 0..4 {
        let mut q = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("host"))
            .with_component(
                component(),
                record(authored(AuthoredEntityReference::input(input()))),
            )]);
        q.inputs.insert(input());
        let replacement = match choice {
            0 => None,
            1 => Some(AuthoredEntityReference::input(input())),
            2 => Some(AuthoredEntityReference {
                through: vec![PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id("a")),
                    expected_base: key("q"),
                }],
                target: AuthoredReferenceTarget::Input(input()),
            }),
            _ => Some(AuthoredEntityReference::source(id("b"))),
        };
        let mut overlay = PrefabOverlay::new(key("q"));
        if let Some(reference) = replacement {
            overlay.components.push(PrefabComponentIntent::Properties {
                target: PrefabEntityTarget::source(id("host")),
                component: component(),
                version: ComponentSchemaVersion::ONE,
                claims: vec![PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
                    intent: PrefabPropertyIntent::Set(authored(reference)),
                }],
            });
        }
        let mut a = SceneEntityRecord::new(id("a"));
        a.prefab = Some(instance("q", Some(local("b")), Some(overlay)));
        let mut p = PrefabDocument::from_ordered_roots([a, SceneEntityRecord::new(id("b"))]);
        p.inputs.insert(input());
        let sources = InMemoryPrefabSourceResolver::new()
            .with_prefab(source("p"), key("p"), p)
            .with_prefab(source("q"), key("q"), q);
        let mut records = vec![SceneEntityRecord::new(id("c"))];
        for name in ["left", "right"] {
            let mut anchor = SceneEntityRecord::new(id(name));
            anchor.prefab = Some(instance("p", Some(local("c")), None));
            records.push(anchor);
        }
        let document = SceneDocument::from_ordered_roots(records);
        let before = document.clone();
        let report = document.compile(&registry, &sources);
        assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
        let origins = report.provenance().unwrap();
        let candidate = report.candidate.as_ref().unwrap();
        let mut targets = Vec::new();
        for host in candidate
            .entities()
            .iter()
            .filter(|entity| entity.components().contains_key(&component()))
        {
            let ComponentValue::EntityReference(EntityReference::SceneLocal { entity: target }) =
                host.components()[&component()]
                    .value()
                    .get("target")
                    .unwrap()
            else {
                panic!("expected lowered reference")
            };
            if choice == 1 {
                assert_eq!(target, &id("c"));
            } else {
                assert_eq!(origins[target].local_entity_id(), &id("b"));
                assert_eq!(
                    origins[host.id()].projection_path().unwrap().root_anchor(),
                    origins[target].projection_path().unwrap().root_anchor()
                );
            }
            targets.push(target.clone());
        }
        assert_eq!(targets.len(), 2);
        if choice != 1 {
            assert_ne!(targets[0], targets[1]);
        }
        assert_eq!(document, before);
    }
}

#[test]
fn self_routed_argument_refuses_a_cycle_but_overwritten_input_needs_no_argument() {
    let registry = registry(ComponentValue::Null);
    for cycle in [false, true] {
        let mut q = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("host"))
            .with_component(
                component(),
                record(authored(AuthoredEntityReference::input(input()))),
            )]);
        q.inputs.insert(input());
        let sources = InMemoryPrefabSourceResolver::new().with_prefab(source("q"), key("q"), q);
        let mut overlay = PrefabOverlay::new(key("q"));
        if !cycle {
            overlay.components.push(PrefabComponentIntent::Properties {
                target: PrefabEntityTarget::source(id("host")),
                component: component(),
                version: ComponentSchemaVersion::ONE,
                claims: vec![PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
                    intent: PrefabPropertyIntent::Set(ComponentValue::EntityReference(local(
                        "socket",
                    ))),
                }],
            });
        }
        let argument = cycle.then(|| EntityReference::Authored {
            reference: AuthoredEntityReference {
                through: vec![PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id("anchor")),
                    expected_base: key("q"),
                }],
                target: AuthoredReferenceTarget::Input(input()),
            },
        });
        let mut anchor = SceneEntityRecord::new(id("anchor"));
        anchor.prefab = Some(instance("q", argument, Some(overlay)));
        let report =
            SceneDocument::from_ordered_roots([anchor, SceneEntityRecord::new(id("socket"))])
                .compile(&registry, &sources);
        if cycle {
            assert!(report.candidate.is_none());
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str() == "scene.prefab-input-cycle")
            );
        } else {
            let candidate = report
                .candidate
                .as_ref()
                .unwrap_or_else(|| panic!("{:?}", report.diagnostics));
            let host = candidate
                .entities()
                .iter()
                .find(|entity| entity.components().contains_key(&component()))
                .unwrap();
            assert_eq!(
                host.components()[&component()].value().get("target"),
                Some(&ComponentValue::EntityReference(local("socket")))
            );
        }
    }
}

#[test]
fn hidden_contextual_default_is_retained_and_absent_uses_its_original_input_environment() {
    let registry = registry(ComponentValue::EntityReference(local("schema_template")));
    for expose in [false, true] {
        let record = record(ComponentValue::Null).with_contextual_default(
            ComponentFieldId::new("target"),
            authored(AuthoredEntityReference::input(input())),
        );
        let mut q = PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("host")).with_component(component(), record)
        ]);
        q.inputs.insert(input());
        let before = q.clone();
        let sources = InMemoryPrefabSourceResolver::new().with_prefab(source("q"), key("q"), q);
        let mut overlay = PrefabOverlay::new(key("q"));
        if expose {
            overlay.components.push(PrefabComponentIntent::Properties {
                target: PrefabEntityTarget::source(id("host")),
                component: component(),
                version: ComponentSchemaVersion::ONE,
                claims: vec![PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
                    intent: PrefabPropertyIntent::Absent,
                }],
            });
        }
        let mut anchor = SceneEntityRecord::new(id("anchor"));
        anchor.prefab = Some(instance(
            "q",
            expose.then(|| local("socket")),
            Some(overlay),
        ));
        let report =
            SceneDocument::from_ordered_roots([anchor, SceneEntityRecord::new(id("socket"))])
                .compile(&registry, &sources);
        let candidate = report
            .candidate
            .as_ref()
            .unwrap_or_else(|| panic!("{:?}", report.diagnostics));
        let host = candidate
            .entities()
            .iter()
            .find(|entity| entity.components().contains_key(&component()))
            .unwrap();
        assert_eq!(
            host.components()[&component()].value().get("target"),
            Some(&if expose {
                ComponentValue::EntityReference(local("socket"))
            } else {
                ComponentValue::Null
            })
        );
        // Compiled records structurally exclude contextual defaults.
        let retained =
            nara_scene::PrefabSourceResolver::resolve_prefab(&sources, &source("q")).unwrap();
        assert_eq!(retained.document(), &before);
    }
}
