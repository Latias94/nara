use super::*;
use crate::SchemaReclassificationKind::DeclarationExtension;
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentFieldPath, ComponentFieldSchema, ComponentSchemaCatalog,
    ComponentValueKind,
};

fn id() -> SceneEntityId {
    SceneEntityId::new("root").unwrap()
}
fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.Reclassified")
}
fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.reclassification").unwrap()
}

fn schema() -> ComponentSchema {
    ComponentSchema::new(component(), "Reclassified", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::optional(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::String,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
}

fn basis(schema: ComponentSchema) -> AuthoringSchemaBasis {
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &BTreeSet::new()).unwrap();
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn empty_basis() -> AuthoringSchemaBasis {
    AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap()
}

fn document(value: ComponentValue) -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(id()).with_component(
        component(),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner()),
    )])
}

fn classify(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> AuthoringDocumentState {
    classify_scene_document(
        document,
        AuthoringDocumentSource::new(AssetRef::path("scenes/reclassification.scene.json").unwrap()),
        AuthoringDocumentKind::Scene,
        Arc::clone(&basis.0),
        SceneFileLimits::default(),
    )
    .unwrap()
}

#[test]
fn extension_keeps_every_full_declaration_owner_and_adapter() {
    let previous = basis(schema());
    let mutate = |edit: fn(&mut BTreeMap<ComponentTypeId, AuthoringSchemaEntry>)| {
        let mut schemas = previous.0.schemas.clone();
        edit(&mut schemas);
        AuthoringSchemaBasis(Arc::new(AuthoringClassificationBasis {
            owners: previous.0.owners.clone(),
            schemas,
        }))
    };
    assert!(previous.require_declaration_extension_of(&previous).is_ok());
    assert!(
        basis(schema())
            .require_declaration_extension_of(&previous)
            .is_ok()
    );
    for changed in [
        mutate(|schemas| {
            schemas.clear();
        }),
        mutate(|schemas| {
            schemas.get_mut(&component()).unwrap().owner =
                PersistentComponentSchemaOwnerId::try_new("other.owner").unwrap();
        }),
        mutate(|schemas| {
            schemas
                .get_mut(&component())
                .unwrap()
                .schema
                .aliases
                .push("Other".to_owned());
        }),
        mutate(|schemas| {
            schemas.get_mut(&component()).unwrap().schema.fields.clear();
        }),
        mutate(|schemas| {
            schemas.get_mut(&component()).unwrap().adapter = None;
        }),
        mutate(|schemas| {
            schemas
                .get_mut(&component())
                .unwrap()
                .adapter
                .as_mut()
                .unwrap()
                .adapter_generation = NonZeroU64::new(2).unwrap();
        }),
        mutate(|schemas| {
            schemas
                .get_mut(&component())
                .unwrap()
                .adapter
                .as_mut()
                .unwrap()
                .identity
                .version = 2;
        }),
    ] {
        assert!(changed.require_declaration_extension_of(&previous).is_err());
    }
}

#[test]
fn valid_unknown_becomes_available_without_copying_or_mutating_the_source() {
    let document = document(ComponentValue::map([(
        "value",
        ComponentValue::String("retained".to_owned()),
    )]));
    let source_ptr = document.entities.as_ptr();
    let previous = classify(&document, &empty_basis());
    assert_eq!(previous.unavailable_manifest.record_count(), 1);
    let successor = basis(schema());
    let mut admitted = 0usize;
    let next = previous
        .reclassify(
            &document,
            &successor,
            &DeclarationExtension,
            &mut |_, bytes| {
                admitted = admitted.checked_add(bytes.unwrap()).unwrap();
                assert!(
                    admitted < 1024 * 1024,
                    "small sources must not reserve file-limit maxima"
                );
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(next.readiness, AuthoringDocumentReadiness::Complete);
    assert_eq!(next.unavailable_manifest.record_count(), 0);
    assert_eq!(next.schema_basis(), successor);
    assert_eq!(next.source, previous.source);
    assert_eq!(document.entities.as_ptr(), source_ptr);
    assert!(admitted >= next.checked_retained_bytes().unwrap() + 2 * size_of::<usize>());
    let repeated = next
        .reclassify(&document, &successor, &DeclarationExtension, &mut |_, _| {
            Ok(())
        })
        .unwrap();
    assert_eq!(repeated.record_facts, next.record_facts);
    assert_eq!(repeated.unavailable_manifest, next.unavailable_manifest);
}

#[test]
fn tiny_allowance_refuses_before_value_validation_or_source_copy() {
    let document = document(ComponentValue::map([("value", ComponentValue::U64(42))]));
    let previous = classify(&document, &empty_basis());
    let successor = basis(schema());
    let mut stages = Vec::new();
    let result = previous.reclassify(
        &document,
        &successor,
        &DeclarationExtension,
        &mut |stage, bytes| {
            stages.push(stage);
            assert!(bytes.unwrap() > 1);
            Err(structure::authoring_structure_rejection(
                "test allowance refused",
            ))
        },
    );
    assert!(result.is_err());
    assert_eq!(stages, ["classification-source"]);
    let prefab = PrefabDocument {
        roots: document.roots.clone(),
        entities: document.entities.clone(),
    };
    stages.clear();
    assert!(
        previous
            .reclassify_prefab(
                &prefab,
                &successor,
                &DeclarationExtension,
                &mut |stage, _| {
                    stages.push(stage);
                    Err(structure::authoring_structure_rejection(
                        "test allowance refused",
                    ))
                }
            )
            .is_err()
    );
    assert_eq!(stages, ["source-cost-walk"]);
}

#[test]
fn newly_known_invalid_value_and_dangling_local_reference_refuse() {
    let invalid = document(ComponentValue::map([("value", ComponentValue::U64(42))]));
    let previous = classify(&invalid, &empty_basis());
    assert!(
        previous
            .reclassify(
                &invalid,
                &basis(schema()),
                &DeclarationExtension,
                &mut |_, _| Ok(())
            )
            .is_err()
    );
    let schema = ComponentSchema::new(component(), "Reference", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::EntityRef,
        )
        .with_capabilities([
            ComponentCapability::Scene,
            ComponentCapability::Inspect,
            ComponentCapability::Edit,
            ComponentCapability::EntityRef,
        ])]);
    let invalid = document(ComponentValue::map([(
        "value",
        ComponentValue::EntityReference(EntityReference::SceneLocal {
            entity: SceneEntityId::new("missing").unwrap(),
        }),
    )]));
    let previous = classify(&invalid, &empty_basis());
    assert!(
        previous
            .reclassify(
                &invalid,
                &basis(schema),
                &DeclarationExtension,
                &mut |_, _| Ok(())
            )
            .is_err()
    );
}

#[test]
fn optional_scalar_removal_generates_a_large_inverse_under_source_admission() {
    let payload = "x".repeat(64 * 1024 - 64);
    let source = document(ComponentValue::map([(
        "value",
        ComponentValue::String(payload.clone()),
    )]));
    let state = classify(&source, &basis(schema()));
    assert!(state.checked_retained_bytes().unwrap() < payload.len());
    let patch = ScenePatchDocument::new([ScenePatchOperation::RemoveField {
        entity: id(),
        component: component(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("value"),
    }]);
    let mut source_admitted = false;
    let prepared = prepare_scene_authoring_patch_with_admission(
        &source,
        &state,
        &patch,
        true,
        &mut |stage, bytes| {
            if stage == "complete-document" {
                assert!(bytes.unwrap() >= payload.len());
                source_admitted = true;
            }
            Ok(())
        },
    )
    .unwrap();
    assert!(source_admitted);
    let inverse = prepared.report.inverse.unwrap();
    assert!(
        matches!(&inverse.operations[0], ScenePatchOperation::SetField {
        value: ComponentValue::String(value), ..
    } if value == &payload)
    );
    let restored = prepare_scene_authoring_patch_with_admission(
        &prepared.document,
        &prepared.state,
        &inverse,
        true,
        &mut |_, _| Ok(()),
    )
    .unwrap();
    assert_eq!(restored.document, source);
}

#[test]
fn scalar_defaults_are_admitted_even_without_asset_facts() {
    let mut schema = schema();
    schema.fields[0].default_value = Some(ComponentValue::String("x".repeat(64 * 1024)));
    let source = document(ComponentValue::Map(BTreeMap::new()));
    let previous = classify(&source, &empty_basis());
    let successor = basis(schema);
    let mut stage = None;
    assert!(
        previous
            .reclassify(
                &source,
                &successor,
                &DeclarationExtension,
                &mut |name, bytes| {
                    if name == "classification-defaults"
                        && bytes.is_none_or(|bytes| bytes >= 32 * 1024)
                    {
                        stage = Some(name);
                        Err(structure::authoring_structure_rejection(
                            "default copy refused",
                        ))
                    } else {
                        Ok(())
                    }
                }
            )
            .is_err()
    );
    assert_eq!(stage, Some("classification-defaults"));
    assert!(
        source.entities[0].components[&component()]
            .value
            .get("value")
            .is_none()
    );
}

#[test]
fn fixed_scalar_overlay_claims_charge_schema_scans_and_refuse_in_the_selector_walk() {
    use crate::{
        PrefabComponentIntent, PrefabOverlay, SceneAuthoringSession, SchemaReclassificationLimits,
        SchemaReclassificationWork, SourceDocumentKey, SourceDocumentKind,
    };
    use nara_reflect::ComponentPropertyPath;

    let key = SourceDocumentKey::named(
        "941c649a-3d67-4f5b-a62c-689d5e09c0f0".parse().unwrap(),
        "selector-budget",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let mut overlay = PrefabOverlay::new(key);
    for index in 0..4 {
        overlay.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(
                SceneEntityId::new(format!("leaf-{index}")).unwrap(),
            ),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("value")),
                intent: PrefabPropertyIntent::Set(ComponentValue::String("retained".into())),
                bindings: Vec::new(),
            }],
        });
    }
    let mut root = SceneEntityRecord::new(id());
    root.prefab = Some(PrefabInstance {
        source: AssetRef::path("prefabs/selector-base.prefab.json").unwrap(),
        overrides: Some(overlay),
    });
    let source = SceneDocument::from_ordered_roots([root]);
    let previous = empty_basis();
    let session = SceneAuthoringSession::from_published_authoring_document(
        SceneAuthoringDocumentCandidate {
            state: classify(&source, &previous),
            document: source.clone(),
        }
        .publish(),
    );
    let narrow = basis(schema());
    let mut wide_schema = schema();
    let selected = std::mem::take(&mut wide_schema.fields);
    wide_schema.fields = (0..2048)
        .map(|index| {
            let name = format!("padding-{index:04}-{}", "x".repeat(32));
            ComponentFieldSchema::optional(
                ComponentFieldId::new(&name),
                "Padding",
                ComponentFieldPath::from_fields([name]),
                ComponentValueKind::String,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        })
        .chain(selected)
        .collect();
    let wide = basis(wide_schema);
    let mut narrow_work = SchemaReclassificationWork::default();
    let mut wide_work = SchemaReclassificationWork::default();
    drop(
        session
            .prepare_schema_reclassification(&narrow, &DeclarationExtension, &mut narrow_work)
            .unwrap(),
    );
    drop(
        session
            .prepare_schema_reclassification(&wide, &DeclarationExtension, &mut wide_work)
            .unwrap(),
    );
    assert!(wide_work.usage().work_bytes > narrow_work.usage().work_bytes);

    let limits = SchemaReclassificationLimits::new(
        ByteLimit::new(narrow_work.usage().work_bytes).unwrap(),
        ByteLimit::new(256 * 1024 * 1024).unwrap(),
    );
    let mut limited = SchemaReclassificationWork::new(limits);
    let refused = session
        .prepare_schema_reclassification(&wide, &DeclarationExtension, &mut limited)
        .unwrap_err();
    assert!(
        refused.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-schema-reclassification-limit"
                && diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "preparation-stage"
                        && field.display_value() == "overlay-property-selector"
                })
        }),
        "{:?}",
        refused.diagnostics
    );
    assert_eq!(session.document(), &source);
    assert_eq!(session.authoring_schema_basis(), Some(previous));
}
