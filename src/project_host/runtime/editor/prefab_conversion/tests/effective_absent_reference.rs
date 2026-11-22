use nara_asset::AssetSourcePath;
use nara_core::{ByteLimit, ItemLimit};
use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityReference, ReferenceInputId,
};
use nara_reflect::{ComponentPropertyPath, ComponentPropertyReadBudget};
use nara_scene::{
    AuthoringPrefabProperty, AuthoringPrefabPropertyValue, PrefabComponentIntent,
    PrefabDeclarationSource, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstanceHop,
    PrefabInstancePath, PrefabOverlay, PrefabOverlaySlot, PrefabPropertyClaim,
    PrefabReferenceArguments, ScenePatchDocument, ScenePatchOperation, SourceDocumentKey,
};
use nara_tooling::{EditorPrefabInspectionId, EditorPrefabInspectionView};

use super::*;

const SELECTED: &str = "prefabs/link.prefab.json";

#[test]
fn effective_absent_drops_demand_but_fences_original_source() {
    check_absent_reference_source_fences(ReferenceDemandChange::Removed);
}

#[test]
fn effective_absent_adds_demand_and_fences_candidate_source() {
    check_absent_reference_source_fences(ReferenceDemandChange::Added);
}

enum ReferenceDemandChange {
    Removed,
    Added,
}

fn check_absent_reference_source_fences(demand_change: ReferenceDemandChange) {
    let input = ReferenceInputId::new("destination").unwrap();
    let destination = EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("outside")),
                expected_base: prefab_key(BASE),
            }],
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("leaf-0"))),
        },
    };
    let input_value = ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference::input(input.clone()),
    });
    let field = ComponentFieldId::new("target");
    let (project, component, link, initial_value) = match demand_change {
        ReferenceDemandChange::Removed => {
            let mut link = payload(0);
            link.set_value(ComponentValue::map([
                ("amount", ComponentValue::U64(0)),
                ("target", input_value),
            ]));
            (
                Project::new(1, false),
                payload_type(),
                link,
                ComponentValue::EntityReference(destination.clone()),
            )
        }
        ReferenceDemandChange::Added => {
            let (schema, link) = contextual_link(&field, input_value);
            let component = schema.id().clone();
            (
                Project::with_additional_schema(1, false, Some(schema)),
                component,
                link,
                ComponentValue::Null,
            )
        }
    };
    let mut selected =
        PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("link-root")).with_component(component.clone(), link)
        ]);
    selected.inputs.insert(input.clone());
    let selected_file = project.root.join(SELECTED);
    let selected_bytes = selected.to_json_string().unwrap();
    fs::write(&selected_file, &selected_bytes).unwrap();

    let selected_key = prefab_key(SELECTED);
    let mut original = PrefabInstance::forest(AssetRef::path("link.prefab.json").unwrap());
    original.arguments = Some(PrefabReferenceArguments {
        base: selected_key.clone(),
        values: [(input, destination.clone())].into(),
    });
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(original.clone());
    let mut outside = SceneEntityRecord::new(id("outside"));
    outside.prefab = Some(PrefabInstance::forest(
        AssetRef::path("base.prefab.json").unwrap(),
    ));
    let scene = SceneDocument::from_ordered_roots([anchor.clone(), outside.clone()]);
    project.write_source(SCENE, &scene);
    let scene_file = project.root.join(SCENE);
    let scene_bytes = fs::read(&scene_file).unwrap();
    let dependency_file = project.root.join(BASE);
    let dependency_bytes = fs::read(&dependency_file).unwrap();

    let property = ComponentPropertyPath::new(field);
    let address = EditorPrefabPropertyAddress {
        entity: PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("anchor")),
                expected_base: selected_key.clone(),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id("link-root")),
        },
        component: component.clone(),
        property: property.clone(),
    };
    let mut overlay = PrefabOverlay::new(selected_key);
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("link-root")),
        component,
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: property,
            intent: PrefabPropertyIntent::Absent,
        }],
    });
    let mut absent = original.clone();
    absent.overrides = Some(overlay);
    let patch = instance_patch(&original, &absent);
    let inverse = instance_patch(&absent, &original);
    anchor.prefab = Some(absent);
    let expected_scene = SceneDocument::from_ordered_roots([anchor, outside]);

    let mut editor = project.editor();
    let document = open_source(&mut editor, SCENE);
    let before = source_state(&editor, document);
    assert_eq!(before.document, scene);
    let prepare_review = |editor: &mut EditorProjectSession, change_dependency: bool| {
        let (operation, property) = observe_property(editor, document, &address);
        let AuthoringPrefabPropertyValue::Present(value) = property.value else {
            panic!("the selected field must start with an explicit value: {property:?}");
        };
        assert_eq!(value.copy_value(Default::default()).unwrap(), initial_value);
        assert!(property.controlling_claim().is_none());
        if change_dependency {
            project.write_base(2);
        }
        assert_eq!(
            inspection(
                editor,
                InspectAction::PreparePropertyWrite {
                    inspection: operation,
                    address: address.clone(),
                    intent: PrefabPropertyIntent::Absent,
                    limits: Default::default(),
                },
            ),
            InspectOutcome::Accepted {
                inspection: operation,
                phase: InspectPhase::PreparingEdit,
            }
        );
        let view = settled_inspection(editor);
        assert_eq!(view.phase, InspectPhase::Review, "{view:?}");
        assert_eq!(view.inspection, Some(operation));
        let review = editor.prefab_inspection_review(operation).unwrap();
        assert_eq!(review.base_revision(), before.revision);
        assert_eq!(review.patch(), &patch);
        assert_eq!(review.inverse(), &inverse);
        assert_eq!(source_state(editor, document), before);
        operation
    };
    let commit = |editor: &mut EditorProjectSession, operation| {
        assert_eq!(
            inspection(
                editor,
                InspectAction::Commit {
                    inspection: operation
                }
            ),
            InspectOutcome::Accepted {
                inspection: operation,
                phase: InspectPhase::Validating,
            }
        );
    };

    let adds_demand = matches!(demand_change, ReferenceDemandChange::Added);
    let stale = prepare_review(&mut editor, adds_demand);
    let reviewed_bytes = fs::read(&dependency_file).unwrap();
    assert_eq!(reviewed_bytes != dependency_bytes, adds_demand);
    commit(&mut editor, stale);
    if adds_demand {
        fs::write(&dependency_file, &dependency_bytes).unwrap();
    } else {
        project.write_base(2);
    }
    assert_ne!(fs::read(&dependency_file).unwrap(), reviewed_bytes);
    let view = settled_inspection(&mut editor);
    assert!(
        matches!(
            &view.result,
            Some(InspectResult::Failed { inspection, reason: InspectRejection::StaleSource, .. })
                if *inspection == stale
        ),
        "{view:?}"
    );
    assert_eq!(source_state(&editor, document), before);
    if !adds_demand {
        fs::write(&dependency_file, &dependency_bytes).unwrap();
    }
    acknowledge_inspection(&mut editor, stale);

    let clean = prepare_review(&mut editor, false);
    commit(&mut editor, clean);
    let view = settled_inspection(&mut editor);
    let after = source_state(&editor, document);
    assert_eq!(
        view.result,
        Some(InspectResult::Written {
            inspection: clean,
            document,
            revision: after.revision,
            kind: AuthoringPrefabPropertyClaimKind::Absent,
        })
    );
    assert_eq!(after.document, expected_scene);
    assert_ne!(after.revision, before.revision);
    assert!(after.dirty);
    assert_eq!(after.history.undo_depth, before.history.undo_depth + 1);
    assert_eq!(after.history.redo_depth, 0);
    acknowledge_inspection(&mut editor, clean);

    let (_, effective) = observe_property(&mut editor, document, &address);
    match demand_change {
        ReferenceDemandChange::Removed => {
            assert!(matches!(
                effective.value,
                AuthoringPrefabPropertyValue::Missing
            ));
        }
        ReferenceDemandChange::Added => {
            let AuthoringPrefabPropertyValue::Defaulted(value) = effective.value else {
                panic!("Absent must reveal the contextual default: {effective:?}");
            };
            assert_eq!(
                value.copy_value(Default::default()).unwrap(),
                ComponentValue::EntityReference(destination)
            );
        }
    }
    let claim = effective.revert_claim().unwrap();
    assert_eq!(effective.controlling_claim(), Some(claim));
    assert_eq!(claim.kind, AuthoringPrefabPropertyClaimKind::Absent);
    assert_eq!(claim.path, address.property);
    assert_eq!(
        claim.location.source(),
        &PrefabDeclarationSource::Containing
    );
    assert_eq!(
        claim.location.path(),
        &PrefabInstancePath::new(id("anchor"))
    );
    assert_eq!(
        claim.slot,
        PrefabOverlaySlot::Property {
            component_index: 0,
            claim_index: 0,
        }
    );
    assert_eq!(fs::read(&scene_file).unwrap(), scene_bytes);
    assert_eq!(fs::read(&selected_file).unwrap(), selected_bytes.as_bytes());
    assert_eq!(fs::read(&dependency_file).unwrap(), dependency_bytes);
    close(&mut editor);
}

#[test]
fn effective_absent_first_demands_the_containing_prefab_working_copy() {
    let field = ComponentFieldId::new("target");
    let input = ReferenceInputId::new("destination").unwrap();
    let input_value = ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference::input(input.clone()),
    });
    let (schema, link) = contextual_link(&field, input_value);
    let component = schema.id().clone();
    let project = Project::with_additional_schema(1, false, Some(schema));

    let link_root = id("link-root");
    let mut selected =
        PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(link_root.clone()).with_component(component.clone(), link)
        ]);
    selected.inputs.insert(input.clone());
    let selected_file = project.root.join(SELECTED);
    let selected_bytes = selected.to_json_string().unwrap();
    fs::write(&selected_file, &selected_bytes).unwrap();

    let containing_key = prefab_key(CONTAINER);
    let working_copy_leaf = "working-copy-leaf";
    let destination = EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("outside")),
                expected_base: containing_key.clone(),
            }],
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(
                working_copy_leaf,
            ))),
        },
    };
    let selected_key = prefab_key(SELECTED);
    let mut original = PrefabInstance::forest(AssetRef::path("link.prefab.json").unwrap());
    original.arguments = Some(PrefabReferenceArguments {
        base: selected_key.clone(),
        values: [(input, destination.clone())].into(),
    });
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(original.clone());

    let mut finite = PrefabOverlay::new(containing_key);
    finite.suppressions.extend([
        PrefabEntityTarget::source(id("anchor")),
        PrefabEntityTarget::source(id("outside")),
    ]);
    let mut self_instance =
        PrefabInstance::forest(AssetRef::path("container.prefab.json").unwrap());
    self_instance.overrides = Some(finite);
    let mut outside = SceneEntityRecord::new(id("outside"));
    outside.prefab = Some(self_instance);
    project.write_source(
        CONTAINER,
        &SceneDocument::from_ordered_roots([
            anchor,
            outside,
            SceneEntityRecord::new(id("saved-leaf")),
        ]),
    );
    let containing_file = project.root.join(CONTAINER);
    let saved_bytes = fs::read(&containing_file).unwrap();
    let mut editor = project.editor();
    let document = open_source(&mut editor, CONTAINER);
    edit(&mut editor, document, working_copy_leaf);
    let before = source_state(&editor, document);
    assert!(before.dirty);
    assert_eq!(fs::read(&containing_file).unwrap(), saved_bytes);

    let property = ComponentPropertyPath::new(field);
    let address = EditorPrefabPropertyAddress {
        entity: PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("anchor")),
                expected_base: selected_key.clone(),
            }],
            entity: PrefabEntityOrigin::SourceEntity(link_root.clone()),
        },
        component: component.clone(),
        property: property.clone(),
    };
    let mut overlay = PrefabOverlay::new(selected_key);
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(link_root),
        component,
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: property,
            intent: PrefabPropertyIntent::Absent,
        }],
    });
    let mut absent = original.clone();
    absent.overrides = Some(overlay);

    let (operation, observed) = observe_property(&mut editor, document, &address);
    let AuthoringPrefabPropertyValue::Present(value) = observed.value else {
        panic!("the explicit Null must hide the self reference: {observed:?}");
    };
    assert_eq!(
        value.copy_value(Default::default()).unwrap(),
        ComponentValue::Null
    );
    assert!(observed.controlling_claim().is_none());
    assert_eq!(
        inspection(
            &mut editor,
            InspectAction::PreparePropertyWrite {
                inspection: operation,
                address: address.clone(),
                intent: PrefabPropertyIntent::Absent,
                limits: Default::default(),
            },
        ),
        InspectOutcome::Accepted {
            inspection: operation,
            phase: InspectPhase::PreparingEdit,
        }
    );
    let view = settled_inspection(&mut editor);
    assert_eq!(view.phase, InspectPhase::Review, "{view:?}");
    let reviewed = editor.prefab_inspection_review(operation).unwrap();
    assert_eq!(reviewed.document(), document);
    assert_eq!(reviewed.base_revision(), before.revision);
    assert_eq!(reviewed.patch(), &instance_patch(&original, &absent));
    assert_eq!(reviewed.inverse(), &instance_patch(&absent, &original));
    assert_eq!(source_state(&editor, document), before);

    assert_eq!(
        inspection(
            &mut editor,
            InspectAction::Commit {
                inspection: operation,
            },
        ),
        InspectOutcome::Accepted {
            inspection: operation,
            phase: InspectPhase::Validating,
        }
    );
    let view = settled_inspection(&mut editor);
    let after = source_state(&editor, document);
    assert_eq!(
        view.result,
        Some(InspectResult::Written {
            inspection: operation,
            document,
            revision: after.revision,
            kind: AuthoringPrefabPropertyClaimKind::Absent,
        })
    );
    let mut expected = before.document.clone();
    expected
        .entities
        .iter_mut()
        .find(|entity| entity.id.as_str() == "anchor")
        .unwrap()
        .prefab = Some(absent);
    assert_eq!(after.document, expected);
    assert_ne!(after.revision, before.revision);
    assert_eq!(after.history.undo_depth, before.history.undo_depth + 1);
    assert_eq!(after.history.redo_depth, 0);
    acknowledge_inspection(&mut editor, operation);

    let (_, effective) = observe_property(&mut editor, document, &address);
    let AuthoringPrefabPropertyValue::Defaulted(value) = effective.value else {
        panic!("the contextual default must resolve through the working copy: {effective:?}");
    };
    assert_eq!(
        value.copy_value(Default::default()).unwrap(),
        ComponentValue::EntityReference(destination)
    );
    let claim = effective.revert_claim().unwrap();
    assert_eq!(effective.controlling_claim(), Some(claim));
    assert_eq!(claim.kind, AuthoringPrefabPropertyClaimKind::Absent);
    assert_eq!(claim.path, address.property);
    assert_eq!(
        claim.location.path(),
        &PrefabInstancePath::new(id("anchor"))
    );
    assert_eq!(
        claim.location.source(),
        &PrefabDeclarationSource::Containing
    );
    assert_eq!(fs::read(&containing_file).unwrap(), saved_bytes);
    assert_eq!(fs::read(&selected_file).unwrap(), selected_bytes.as_bytes());
    close(&mut editor);
}

fn contextual_link(
    field: &ComponentFieldId,
    input_value: ComponentValue,
) -> (ComponentSchema, SceneComponentRecord) {
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.HostConversionContextualLink"),
        "Host conversion contextual link",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::optional_with_default(
        field.clone(),
        "Target",
        ComponentFieldPath::from_fields(["target"]),
        ComponentValueKind::EntityRef,
        ComponentValue::EntityReference(EntityReference::SceneLocal {
            entity: id("link-root"),
        }),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef)]);
    let link = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", ComponentValue::Null)]),
    )
    .with_owner(payload_owner())
    .with_contextual_default(field.clone(), input_value);
    (schema, link)
}

fn instance_patch(expected: &PrefabInstance, value: &PrefabInstance) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
        entity: id("anchor"),
        expected: Some(expected.clone()),
        value: Some(value.clone()),
    }])
}

fn prefab_key(path: &str) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        UNIVERSE.parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        SourceDocumentKind::Prefab,
    )
}

fn observe_property<'a>(
    editor: &'a mut EditorProjectSession,
    document: EditorDocumentId,
    address: &EditorPrefabPropertyAddress,
) -> (EditorPrefabInspectionId, AuthoringPrefabProperty<'a>) {
    let basis = editor.document_query_basis(document).unwrap();
    let admitted = inspection(
        editor,
        InspectAction::Inspect {
            basis,
            anchor: id("anchor"),
        },
    );
    let InspectOutcome::Accepted {
        inspection: operation,
        phase: InspectPhase::Inspecting,
    } = admitted
    else {
        panic!("the selected Prefab inspection must start: {admitted:?}");
    };
    let view = settled_inspection(editor);
    assert_eq!(view.phase, InspectPhase::Ready, "{view:?}");
    let mut budget = ComponentPropertyReadBudget::new(
        ItemLimit::new(16).unwrap(),
        ByteLimit::new(1024).unwrap(),
    );
    let property = editor
        .prefab_effective_observation(operation)
        .unwrap()
        .target(&address.entity)
        .unwrap()
        .component(&address.component)
        .unwrap()
        .property_value(&address.property, &mut budget)
        .unwrap();
    (operation, property)
}

fn settled_inspection(editor: &mut EditorProjectSession) -> EditorPrefabInspectionView {
    editor.drive_editor_frame(Duration::ZERO);
    wait(editor, |editor| {
        !editor
            .current_frame()
            .prefab_inspection()
            .phase
            .work_pending()
    });
    editor.current_frame().prefab_inspection().clone()
}

fn acknowledge_inspection(editor: &mut EditorProjectSession, operation: EditorPrefabInspectionId) {
    assert_eq!(
        inspection(
            editor,
            InspectAction::Acknowledge {
                inspection: operation,
            },
        ),
        InspectOutcome::Accepted {
            inspection: operation,
            phase: InspectPhase::Idle,
        }
    );
}
