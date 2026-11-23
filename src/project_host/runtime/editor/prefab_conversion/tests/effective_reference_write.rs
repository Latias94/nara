use nara_asset::AssetSourcePath;
use nara_core::{ByteLimit, ItemLimit};
use nara_identity::{AuthoredEntityReference, EntityReference, ReferenceInputId};
use nara_reflect::{ComponentPropertyPath, ComponentPropertyReadBudget};
use nara_scene::{
    AuthoringPrefabPropertyValue, PrefabComponentEdit, PrefabComponentIntent,
    PrefabComponentSetMode, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstanceHop,
    PrefabOverlay, PrefabPropertyClaim, PrefabReferenceArguments, ScenePatchDocument,
    ScenePatchOperation, SourceDocumentKey,
};
use nara_tooling::{
    EditorPrefabComponentAddress, EditorPrefabComponentEditKind, EditorPrefabInspectionId,
    EditorPrefabInspectionView,
};

use super::*;

const LINK: &str = "prefabs/link.prefab.json";
const DESTINATION: &str = "prefabs/destination.prefab.json";

#[test]
fn effective_reference_write_revalidates_original_and_candidate_files() {
    check_reference_write_freshness(false);
}

#[test]
fn effective_reference_write_revalidates_original_and_candidate_working_copies() {
    check_reference_write_freshness(true);
}

#[test]
fn whole_component_edit_revalidates_the_source_first_demanded_by_its_reference() {
    let project = Project::new(1, false);
    write_endpoint(&project, DESTINATION, "new-leaf", 0);
    let source = SceneDocument::from_ordered_roots([
        mount("anchor", BASE),
        mount("new-target", DESTINATION),
    ]);
    project.write_source(SCENE, &source);
    let mut editor = project.editor();
    let document = open_source(&mut editor, SCENE);
    let before = source_state(&editor, document);
    let original_file = fs::read(project.root.join(SCENE)).unwrap();
    let address = EditorPrefabComponentAddress {
        entity: PrefabEntityTarget {
            through: vec![instance_hop("anchor", BASE)],
            entity: PrefabEntityOrigin::SourceEntity(id("leaf-0")),
        },
        component: payload_type(),
    };
    let reference = sibling_reference("new-target", DESTINATION, "new-leaf");
    let complete = ComponentValue::map([
        ("amount", ComponentValue::U64(35)),
        ("target", ComponentValue::EntityReference(reference.clone())),
    ]);
    for drift in [true, false] {
        let operation = inspect_link(&mut editor, document);
        assert_eq!(
            inspection(
                &mut editor,
                InspectAction::PrepareComponentEdit {
                    inspection: operation,
                    address: address.clone(),
                    edit: PrefabComponentEdit::Set {
                        mode: PrefabComponentSetMode::Replace,
                        value: complete.clone()
                    },
                    limits: Default::default(),
                }
            ),
            InspectOutcome::Accepted {
                inspection: operation,
                phase: InspectPhase::PreparingEdit
            },
        );
        let review = settled_inspection(&mut editor);
        assert_eq!(review.phase, InspectPhase::Review, "{review:?}");
        let edit = editor.prefab_inspection_review(operation).unwrap();
        let [
            ScenePatchOperation::SetPrefabInstance {
                value: Some(candidate),
                ..
            },
        ] = edit.patch().operations.as_slice()
        else {
            panic!("expected one instance declaration edit");
        };
        let [PrefabComponentIntent::Set { record, .. }] =
            candidate.overrides.as_ref().unwrap().components.as_slice()
        else {
            panic!("expected one complete component declaration");
        };
        assert_eq!(record.value(), &complete);
        assert_eq!(source_state(&editor, document), before);
        request_commit(&mut editor, operation);
        if drift {
            write_endpoint(&project, DESTINATION, "new-leaf", 1);
        }
        let terminal = settled_inspection(&mut editor);
        if drift {
            assert!(
                matches!(
                    terminal.result,
                    Some(InspectResult::Failed {
                        reason: InspectRejection::StaleSource,
                        ..
                    })
                ),
                "{terminal:?}"
            );
            assert_eq!(source_state(&editor, document), before);
            write_endpoint(&project, DESTINATION, "new-leaf", 0);
        } else {
            assert!(
                matches!(
                    terminal.result,
                    Some(InspectResult::ComponentEdited {
                        kind: EditorPrefabComponentEditKind::Replaced,
                        ..
                    })
                ),
                "{terminal:?}"
            );
            assert!(source_state(&editor, document).dirty);
        }
        assert_eq!(fs::read(project.root.join(SCENE)).unwrap(), original_file);
        acknowledge_inspection(&mut editor, operation);
    }
    let operation = inspect_link(&mut editor, document);
    assert_eq!(
        read_reference(
            &editor,
            operation,
            &EditorPrefabPropertyAddress {
                entity: address.entity,
                component: address.component,
                property: ComponentPropertyPath::new(ComponentFieldId::new("target")),
            }
        ),
        (ComponentValue::EntityReference(reference), None)
    );
    close(&mut editor);
}

fn check_reference_write_freshness(working_copies: bool) {
    let project = Project::new(1, false);
    let input = ReferenceInputId::new("destination").unwrap();
    let original_leaf = "leaf-0";
    let destination_leaf = "destination-leaf";
    let link_root = id("link-root");
    let old_target = sibling_reference("old-target", BASE, original_leaf);
    let new_target = sibling_reference("new-target", DESTINATION, destination_leaf);

    let mut link = payload(0);
    link.set_value(ComponentValue::map([
        ("amount", ComponentValue::U64(0)),
        (
            "target",
            ComponentValue::EntityReference(EntityReference::Authored {
                reference: AuthoredEntityReference::input(input.clone()),
            }),
        ),
    ]));
    let mut selected =
        PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(link_root.clone()).with_component(payload_type(), link)
        ]);
    selected.inputs.insert(input.clone());
    fs::write(project.root.join(LINK), selected.to_json_string().unwrap()).unwrap();
    write_endpoint(&project, DESTINATION, "saved-leaf", 0);

    let link_hop = instance_hop("anchor", LINK);
    let mut anchor = mount("anchor", LINK);
    anchor.prefab.as_mut().unwrap().arguments = Some(PrefabReferenceArguments {
        base: link_hop.expected_base.clone(),
        values: [(input, old_target.clone())].into(),
    });
    let original = anchor.prefab.clone().unwrap();
    let scene = SceneDocument::from_ordered_roots([
        anchor,
        mount("old-target", BASE),
        mount("new-target", DESTINATION),
    ]);
    project.write_source(SCENE, &scene);

    let property = ComponentPropertyPath::new(ComponentFieldId::new("target"));
    let address = EditorPrefabPropertyAddress {
        entity: PrefabEntityTarget {
            through: vec![link_hop.clone()],
            entity: PrefabEntityOrigin::SourceEntity(link_root.clone()),
        },
        component: payload_type(),
        property: property.clone(),
    };
    let intent = PrefabPropertyIntent::Set(ComponentValue::EntityReference(new_target.clone()));
    let mut overlay = PrefabOverlay::new(link_hop.expected_base);
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(link_root),
        component: payload_type(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: property,
            intent: intent.clone(),
        }],
    });
    let mut rebound = original.clone();
    rebound.overrides = Some(overlay);
    let expected_patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
        entity: id("anchor"),
        expected: Some(original),
        value: Some(rebound.clone()),
    }]);
    let mut expected_scene = scene.clone();
    expected_scene
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("anchor"))
        .unwrap()
        .prefab = Some(rebound);

    let mut editor = project.editor();
    let document = open_source(&mut editor, SCENE);
    let unrelated = open_source(&mut editor, OTHER);
    let original_copy = working_copies.then(|| open_source(&mut editor, BASE));
    let mut operation = inspect_link(&mut editor, document);
    let candidate_copy = if working_copies {
        let copy = open_source(&mut editor, DESTINATION);
        edit(&mut editor, copy, destination_leaf);
        Some(copy)
    } else {
        write_endpoint(&project, DESTINATION, destination_leaf, 0);
        None
    };
    let before = source_state(&editor, document);
    assert_eq!(before.document, scene);

    let prepare_review = |editor: &mut EditorProjectSession, operation| {
        assert_eq!(
            read_reference(editor, operation, &address),
            (ComponentValue::EntityReference(old_target.clone()), None)
        );
        assert_eq!(
            inspection(
                editor,
                InspectAction::PreparePropertyWrite {
                    inspection: operation,
                    address: address.clone(),
                    intent: intent.clone(),
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
        assert_eq!(review.document(), document);
        assert_eq!(review.base_revision(), before.revision);
        assert_eq!(review.patch(), &expected_patch);
        assert_eq!(source_state(editor, document), before);
    };

    for (path, leaf, copy) in [
        (BASE, original_leaf, original_copy),
        (DESTINATION, destination_leaf, candidate_copy),
    ] {
        prepare_review(&mut editor, operation);
        request_commit(&mut editor, operation);
        let file = project.root.join(path);
        let restore = if let Some(copy) = copy {
            edit(&mut editor, copy, "late-edit");
            None
        } else {
            let bytes = fs::read(&file).unwrap();
            write_endpoint(&project, path, leaf, 7);
            let changed = fs::read(&file).unwrap();
            assert_eq!(bytes.len(), changed.len(), "{path}");
            assert_ne!(bytes, changed, "{path}");
            Some(bytes)
        };
        let view = settled_inspection(&mut editor);
        assert_eq!(view.phase, InspectPhase::Terminal, "{path}: {view:?}");
        assert!(
            matches!(
                &view.result,
                Some(InspectResult::Failed { inspection, reason: InspectRejection::StaleSource, .. })
                    if *inspection == operation
            ),
            "{path}: {view:?}"
        );
        assert_eq!(source_state(&editor, document), before, "{path}");
        if let Some(bytes) = restore {
            fs::write(file, bytes).unwrap();
        }
        acknowledge_inspection(&mut editor, operation);
        operation = inspect_link(&mut editor, document);
    }

    prepare_review(&mut editor, operation);
    edit(&mut editor, unrelated, "unrelated-edit");
    let unrelated_before_commit = source_state(&editor, unrelated);
    request_commit(&mut editor, operation);
    let view = settled_inspection(&mut editor);
    let after = source_state(&editor, document);
    assert_eq!(
        view.result,
        Some(InspectResult::Written {
            inspection: operation,
            document,
            revision: after.revision,
            kind: AuthoringPrefabPropertyClaimKind::Set,
        })
    );
    assert_ne!(after.revision, before.revision);
    assert_eq!(after.document, expected_scene);
    assert_eq!(source_state(&editor, unrelated), unrelated_before_commit);
    acknowledge_inspection(&mut editor, operation);

    let operation = inspect_link(&mut editor, document);
    assert_eq!(
        read_reference(&editor, operation, &address),
        (
            ComponentValue::EntityReference(new_target),
            Some(AuthoringPrefabPropertyClaimKind::Set),
        )
    );
    close(&mut editor);
}

fn mount(anchor: &str, path: &str) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(id(anchor));
    record.prefab = Some(PrefabInstance::forest(
        AssetRef::path(path.strip_prefix("prefabs/").unwrap()).unwrap(),
    ));
    record
}

fn instance_hop(anchor: &str, path: &str) -> PrefabInstanceHop {
    PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
        expected_base: SourceDocumentKey::project_location(
            UNIVERSE.parse().unwrap(),
            AssetSourcePath::new(path).unwrap(),
            SourceDocumentKind::Prefab,
        ),
    }
}

fn sibling_reference(anchor: &str, path: &str, leaf: &str) -> EntityReference {
    EntityReference::Authored {
        reference: PrefabEntityTarget {
            through: vec![instance_hop(anchor, path)],
            entity: PrefabEntityOrigin::SourceEntity(id(leaf)),
        }
        .into(),
    }
}

fn write_endpoint(project: &Project, path: &str, leaf: &str, amount: u64) {
    project.write_source(
        path,
        &SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id(leaf)).with_component(payload_type(), payload(amount))
        ]),
    );
}

fn inspect_link(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
) -> EditorPrefabInspectionId {
    let basis = editor.document_query_basis(document).unwrap();
    let outcome = inspection(
        editor,
        InspectAction::Inspect {
            basis,
            anchor: id("anchor"),
        },
    );
    let InspectOutcome::Accepted {
        inspection: operation,
        phase: InspectPhase::Inspecting,
    } = outcome
    else {
        panic!("the selected Prefab inspection must start: {outcome:?}");
    };
    let view = settled_inspection(editor);
    assert_eq!(view.phase, InspectPhase::Ready, "{view:?}");
    operation
}

fn read_reference(
    editor: &EditorProjectSession,
    operation: EditorPrefabInspectionId,
    address: &EditorPrefabPropertyAddress,
) -> (ComponentValue, Option<AuthoringPrefabPropertyClaimKind>) {
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
    let AuthoringPrefabPropertyValue::Present(value) = property.value else {
        panic!("the EntityRef field must be present: {property:?}");
    };
    (
        value.copy_value(Default::default()).unwrap(),
        property.revert_claim().map(|claim| claim.kind),
    )
}

fn request_commit(editor: &mut EditorProjectSession, operation: EditorPrefabInspectionId) {
    assert_eq!(
        inspection(
            editor,
            InspectAction::Commit {
                inspection: operation,
            },
        ),
        InspectOutcome::Accepted {
            inspection: operation,
            phase: InspectPhase::Validating,
        }
    );
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
