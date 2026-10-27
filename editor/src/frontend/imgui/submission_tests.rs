use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};

use dear_imgui_rs::{ConfigFlags, Context, Ui};
use nara::{
    scene::{
        SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePlacement,
        ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentQueryBasis,
        EditorNavigationAction,
    },
};

use super::{
    ImguiEditorFrontend, ImguiEditorPanel, ImguiEditorPanelContext, ImguiEditorSubmission,
    PanelActionDispatch, PanelActionNotDispatched, PanelSubmissionId,
};
use crate::{
    EditorApplication, EditorLaunchOptions, EditorNativeComposition,
    workspace::{
        EditorPanelDescriptor, EditorPanelInstanceId, EditorPanelMultiplicity,
        EditorPanelPlacement, EditorPanelTarget,
    },
};

#[derive(Default)]
pub(super) struct Evidence {
    count: usize,
    pub(super) paint: Option<Box<dyn FnMut(&mut ImguiEditorPanelContext<'_, '_>)>>,
    close_resolution: Option<nara::tooling::EditorCloseDecision>,
    queued: Vec<PanelSubmissionId>,
    pub(super) finished: Vec<(PanelSubmissionId, PanelActionDispatch)>,
}

struct Probe(Rc<RefCell<Evidence>>);

impl ImguiEditorPanel for Probe {
    fn show(&mut self, _ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        let mut evidence = self.0.borrow_mut();
        if let Some(paint) = &mut evidence.paint {
            paint(context);
            return;
        }
        let Some(document) = context.model().document().map(|document| document.document) else {
            return;
        };
        for slot in 0..evidence.count {
            let action = if slot == 1 && evidence.close_resolution.is_some() {
                EditorAction::WorkspaceIntent(
                    context
                        .model()
                        .frame()
                        .workspace_intent()
                        .decision_prompt()
                        .unwrap()
                        .resolve(evidence.close_resolution.unwrap()),
                )
            } else {
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document,
                    entity: None,
                })
            };
            evidence.queued.push(context.submit(action).unwrap());
        }
    }

    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        self.0.borrow_mut().finished.push((id, result));
    }
}

fn authoring_basis(harness: &Harness) -> EditorDocumentQueryBasis {
    let frame = harness.application.current_frame().unwrap();
    let tab = frame
        .workspace()
        .document(frame.workspace().active_document.unwrap())
        .unwrap();
    EditorDocumentQueryBasis {
        document: tab.document,
        revision: tab.revision,
        schema: tab.authoring_schema_basis.clone(),
    }
}

fn prepared_panel_patch() -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(SceneEntityId::new("panel-prepared").unwrap()),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }])
}

#[test]
fn panel_context_forwards_selection_independent_document_queries() {
    let mut harness = Harness::new(1);
    let expected = authoring_basis(&harness);
    let captured = Rc::new(RefCell::new(None));
    let output = Rc::clone(&captured);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let basis = context.document_query_basis(expected.document).unwrap();
        assert_eq!(basis, expected);
        let entities = context
            .query_document_entities(nara::tooling::EditorEntityQuery {
                position: nara::tooling::EditorEntityQueryPosition::Start(basis.clone()),
                limits: nara::tooling::EditorEntityQueryLimits::new(64, 64).unwrap(),
            })
            .unwrap();
        let entity = entities
            .entities
            .iter()
            .find(|entity| entity.as_str() == "preserved")
            .cloned()
            .unwrap();
        let components = context
            .query_document_authoring(nara::tooling::EditorAuthoringQuery {
                position: nara::tooling::EditorAuthoringQueryPosition::Start(
                    nara::tooling::EditorAuthoringQueryTarget {
                        basis,
                        entity: entity.clone(),
                        projection: nara::tooling::EditorAuthoringProjection::Components,
                    },
                ),
                limits: nara::tooling::EditorAuthoringQueryLimits::new(64, 64, 4096).unwrap(),
            })
            .unwrap();
        assert_eq!(components.target.entity, entity);
        assert!(matches!(
            components.rows.as_slice(),
            [nara::tooling::EditorAuthoringRow::Component {
                component,
                readiness: nara::scene::AuthoringRecordReadiness::UnknownSchema,
                ..
            }] if component.as_str() == "studio.unavailable.Payload"
        ));
        *output.borrow_mut() = Some(components.rows);
    }));
    let before = harness.application.current_frame().unwrap();
    drop(harness.draw());
    assert!(matches!(
        captured.borrow().as_ref(),
        Some(rows) if rows.len() == 1
    ));
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    assert!(harness.panels[0].1.borrow().queued.is_empty());
    assert!(harness.panels[0].1.borrow().finished.is_empty());
}

#[test]
fn workspace_panel_discovers_reads_and_commits_a_nested_property_on_a_nonactive_scene() {
    use nara::{
        reflect::{
            ComponentFieldId, ComponentPropertyPath, ComponentPropertyStep, ComponentSchemaVersion,
            ComponentTypeId, ComponentValue, PersistentComponentSchemaOwnerId,
        },
        scene::{AuthoringPropertyEdit, SceneComponentRecord},
        tooling::{
            EditorAuthoringMemberStatus, EditorAuthoringProjection, EditorAuthoringQuery,
            EditorAuthoringQueryLimits, EditorAuthoringQueryPosition, EditorAuthoringQueryTarget,
            EditorAuthoringRow, EditorDocumentMutation, EditorEntityQuery, EditorEntityQueryLimits,
            EditorEntityQueryPosition, EditorPropertyRead, EditorPropertyReadLimits,
            EditorPropertyTarget, EditorPropertyValue, PreparedEditorDocumentEdit,
        },
    };

    let mut harness = Harness::with_project(
        super::interaction_tests::InteractionProject::new_recursive(),
        1,
    );
    let scene_document = authoring_basis(&harness).document;
    let entity = SceneEntityId::new("prefab-anchor").unwrap();
    let component = ComponentTypeId::new(super::interaction_tests::CUSTOM_COMPONENT);
    let value = ComponentValue::map([
        ("optional", ComponentValue::Null),
        (
            "vector",
            ComponentValue::map([
                ("x", ComponentValue::f64(1.0).unwrap()),
                ("y", ComponentValue::f64(2.0).unwrap()),
            ]),
        ),
        ("list", ComponentValue::List(vec![ComponentValue::U64(3)])),
        (
            "map",
            ComponentValue::map([("first", ComponentValue::String("before".into()))]),
        ),
    ]);
    let frame = harness.application.current_frame().unwrap();
    let tab = frame.workspace().document(scene_document).unwrap();
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyScenePatch {
                    document: scene_document,
                    base_revision: tab.revision,
                    patch: ScenePatchDocument::new([ScenePatchOperation::AddComponent {
                        entity: entity.clone(),
                        component: component.clone(),
                        value: SceneComponentRecord::new(ComponentSchemaVersion::ONE, value)
                            .with_owner(
                                PersistentComponentSchemaOwnerId::try_new(
                                    "studio.interaction.components",
                                )
                                .unwrap(),
                            ),
                    }]),
                },
            )],
        ),
        Duration::ZERO,
    );
    let basis = authoring_basis(&harness);
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            harness.application.current_frame().unwrap().revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document: scene_document,
                    entity: Some(entity.clone()),
                },
            )],
        ),
        Duration::ZERO,
    );
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            harness.application.current_frame().unwrap().revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::OpenPrefabSource {
                    document: scene_document,
                    base_revision: basis.revision,
                    entity: entity.clone(),
                },
            )],
        ),
        Duration::ZERO,
    );
    harness
        .application
        .panel_workspace_mut()
        .layout_mut()
        .set_target(harness.panels[0].0, EditorPanelTarget::Workspace);

    let retained = Rc::new(RefCell::new(None::<PreparedEditorDocumentEdit>));
    let queued = Rc::new(RefCell::new(None::<PanelSubmissionId>));
    let retained_for_panel = Rc::clone(&retained);
    let queued_for_panel = Rc::clone(&queued);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        assert!(context.model().document().is_none());
        let basis = context.document_query_basis(scene_document).unwrap();
        let entities = context
            .query_document_entities(EditorEntityQuery {
                position: EditorEntityQueryPosition::Start(basis.clone()),
                limits: EditorEntityQueryLimits::new(64, 64).unwrap(),
            })
            .unwrap();
        assert!(entities.entities.contains(&entity));
        let components = context
            .query_document_authoring(EditorAuthoringQuery {
                position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                    basis: basis.clone(),
                    entity: entity.clone(),
                    projection: EditorAuthoringProjection::Components,
                }),
                limits: EditorAuthoringQueryLimits::new(64, 64, 4096).unwrap(),
            })
            .unwrap();
        assert!(matches!(
            components.rows.as_slice(),
            [EditorAuthoringRow::Component {
                component: actual,
                readiness: nara::scene::AuthoringRecordReadiness::Available,
                ..
            }] if actual == &component
        ));
        let roots = context
            .query_document_authoring(EditorAuthoringQuery {
                position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                    basis: basis.clone(),
                    entity: entity.clone(),
                    projection: EditorAuthoringProjection::Roots {
                        component: component.clone(),
                    },
                }),
                limits: EditorAuthoringQueryLimits::new(64, 64, 4096).unwrap(),
            })
            .unwrap();
        assert!(roots.rows.iter().any(|row| matches!(
            row,
            EditorAuthoringRow::Root { field, .. } if field.as_str() == "vector"
        )));
        let nested_property = ComponentPropertyPath::new(ComponentFieldId::new("vector"))
            .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new("x"))]);
        let members = context
            .query_document_authoring(EditorAuthoringQuery {
                position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                    basis: basis.clone(),
                    entity: entity.clone(),
                    projection: EditorAuthoringProjection::Members {
                        component: component.clone(),
                        root: ComponentFieldId::new("vector"),
                    },
                }),
                limits: EditorAuthoringQueryLimits::new(64, 64, 4096).unwrap(),
            })
            .unwrap();
        assert!(members.rows.iter().any(|row| matches!(
            row,
            EditorAuthoringRow::Member {
                property,
                kind: Some(nara::reflect::ComponentValueKind::F64),
                edit_capability: true,
                status: EditorAuthoringMemberStatus::Available,
            } if property == &nested_property
        )));
        let observation = context
            .read_document_property(EditorPropertyRead {
                target: EditorPropertyTarget {
                    basis: basis.clone(),
                    entity: entity.clone(),
                    component: component.clone(),
                    property: nested_property.clone(),
                },
                limits: EditorPropertyReadLimits::new(4096, 4, 64).unwrap(),
            })
            .unwrap();
        assert_eq!(
            observation.value,
            EditorPropertyValue::Present(ComponentValue::f64(1.0).unwrap())
        );

        if queued_for_panel.borrow().is_some() {
            return;
        }
        let edit = context
            .prepare_authoring_property_edit(
                &basis,
                &[AuthoringPropertyEdit {
                    entity: entity.clone(),
                    component: component.clone(),
                    component_version: ComponentSchemaVersion::ONE,
                    property: nested_property,
                    value: ComponentValue::f64(9.0).unwrap(),
                }],
            )
            .unwrap();
        if retained_for_panel.borrow().is_none() {
            *retained_for_panel.borrow_mut() = Some(edit);
        } else {
            let prepared = retained_for_panel.borrow_mut().take().unwrap();
            *queued_for_panel.borrow_mut() = Some(context.commit_authoring_edit(prepared).unwrap());
        }
    }));

    let before = harness.application.current_frame().unwrap();
    drop(harness.draw());
    assert!(retained.borrow().is_some());
    assert!(queued.borrow().is_none());
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    let candidate = harness.draw();
    assert!(queued.borrow().is_some());
    harness.submit(candidate);
    let after = harness.application.current_frame().unwrap();
    assert_eq!(
        after.workspace().active_document,
        before.workspace().active_document
    );
    assert_ne!(after.workspace().active_document, Some(scene_document));
    assert_ne!(
        after.workspace().document(scene_document).unwrap().revision,
        basis.revision
    );
    assert!(matches!(
        harness.panels[0].1.borrow().finished.as_slice(),
        [(_, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)))]
            if report.applied
    ));
}

#[test]
fn a_workspace_panel_can_retain_and_drop_preparation_for_a_nonactive_source_without_mutation() {
    let mut harness = Harness::new(1);
    let basis = authoring_basis(&harness);
    let source = harness._project.root.join("worlds/entry.scene.json");
    let saved = std::fs::read(&source).unwrap();
    let frame = harness.application.current_frame().unwrap();
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document: basis.document,
                    entity: Some(SceneEntityId::new("prefab-anchor").unwrap()),
                },
            )],
        ),
        Duration::ZERO,
    );
    let frame = harness.application.current_frame().unwrap();
    let opening = harness.application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::OpenPrefabSource {
                    document: basis.document,
                    base_revision: basis.revision,
                    entity: SceneEntityId::new("prefab-anchor").unwrap(),
                },
            )],
        ),
        Duration::ZERO,
    );
    assert!(
        matches!(
            opening.actions(),
            [crate::EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))]
                if report.applied
        ),
        "{opening:?}"
    );
    harness
        .application
        .panel_workspace_mut()
        .layout_mut()
        .set_target(harness.panels[0].0, EditorPanelTarget::Workspace);
    let before = harness.application.current_frame().unwrap();
    assert_ne!(before.workspace().active_document, Some(basis.document));
    let retained = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&retained);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        assert!(context.model().document().is_none());
        let queried_basis = context.document_query_basis(basis.document).unwrap();
        assert_eq!(queried_basis, basis);
        let entities = context
            .query_document_entities(nara::tooling::EditorEntityQuery {
                position: nara::tooling::EditorEntityQueryPosition::Start(queried_basis.clone()),
                limits: nara::tooling::EditorEntityQueryLimits::new(64, 64).unwrap(),
            })
            .unwrap();
        let entity = entities
            .entities
            .iter()
            .find(|entity| entity.as_str() == "preserved")
            .cloned()
            .unwrap();
        let components = context
            .query_document_authoring(nara::tooling::EditorAuthoringQuery {
                position: nara::tooling::EditorAuthoringQueryPosition::Start(
                    nara::tooling::EditorAuthoringQueryTarget {
                        basis: queried_basis,
                        entity,
                        projection: nara::tooling::EditorAuthoringProjection::Components,
                    },
                ),
                limits: nara::tooling::EditorAuthoringQueryLimits::new(64, 64, 4096).unwrap(),
            })
            .unwrap();
        assert!(matches!(
            components.rows.as_slice(),
            [nara::tooling::EditorAuthoringRow::Component { component, .. }]
                if component.as_str() == "studio.unavailable.Payload"
        ));
        let patch = prepared_panel_patch();
        let edit = context.prepare_authoring_edit(&basis, &patch).unwrap();
        assert_eq!(edit.document(), basis.document);
        assert_eq!(edit.patch(), &patch);
        assert_eq!(edit.inverse().operations.len(), 1);
        let mut draft = context.begin_authoring_construction(&basis).unwrap();
        let local = draft
            .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
            .unwrap();
        let construction = context
            .prepare_authoring_construction(basis.document, &draft)
            .unwrap();
        assert!(construction.entity(&local).is_some());
        assert_eq!(construction.patch().operations.len(), 1);
        assert_eq!(construction.inverse().operations.len(), 1);
        *captured.borrow_mut() = Some((edit, construction));
    }));
    drop(harness.draw());
    assert!(retained.borrow().is_some());
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(|_| {}));
    drop(harness.draw());
    assert!(retained.borrow().is_some());
    drop(retained.borrow_mut().take());
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    assert_eq!(std::fs::read(source).unwrap(), saved);
    assert!(harness.panels[0].1.borrow().finished.is_empty());
}

#[test]
fn panel_preparation_checks_the_complete_schema_basis_before_lowering_any_edit() {
    use nara::{project_host::EditorEditPreparationError, tooling::EditorDocumentQueryRejection};

    let mut harness = Harness::new(1);
    let mut basis = authoring_basis(&harness);
    assert!(basis.schema.take().is_some());
    let before = harness.application.current_frame().unwrap();
    let painted = Rc::new(RefCell::new(false));
    let captured = Rc::clone(&painted);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let errors = [
            context
                .prepare_authoring_edit(&basis, &prepared_panel_patch())
                .unwrap_err(),
            context
                .prepare_authoring_property_edit(&basis, &[])
                .unwrap_err(),
            context.begin_authoring_construction(&basis).unwrap_err(),
        ];
        for error in errors {
            assert_eq!(
                error,
                EditorEditPreparationError::QueryBasis(EditorDocumentQueryRejection::StaleSchema)
            );
        }
        *captured.borrow_mut() = true;
    }));
    drop(harness.draw());
    assert!(*painted.borrow());
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    assert!(harness.panels[0].1.borrow().finished.is_empty());
}

#[test]
fn a_prepared_panel_edit_commits_only_after_paint_and_delivers_its_original_submission() {
    let mut harness = Harness::new(1);
    let basis = authoring_basis(&harness);
    let before = harness.application.current_frame().unwrap();
    let depth = before
        .workspace()
        .document(basis.document)
        .unwrap()
        .history
        .undo_depth;
    let document = basis.document;
    let queued = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&queued);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let edit = context
            .prepare_authoring_edit(&basis, &prepared_panel_patch())
            .unwrap();
        *captured.borrow_mut() = Some(context.commit_authoring_edit(edit).unwrap());
    }));
    let candidate = harness.draw();
    assert!(queued.borrow().is_some());
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    assert!(harness.panels[0].1.borrow().finished.is_empty());
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            before.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(nara::scene::SceneEntityId::new("preserved").unwrap()),
                },
            )],
        ),
        Duration::ZERO,
    );
    assert_ne!(
        harness.application.current_frame().unwrap().revision(),
        before.revision(),
        "the consuming source-only command must survive an actual frame advance"
    );
    harness.submit(candidate);
    let after = harness.application.current_frame().unwrap();
    assert_eq!(
        after
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        depth + 1
    );
    let evidence = harness.panels[0].1.borrow();
    assert!(matches!(
        evidence.finished.as_slice(),
        [(id, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)))]
            if Some(*id) == *queued.borrow() && report.applied
    ));
}

#[test]
fn prepared_commit_mixed_with_navigation_keeps_whole_batch_frame_admission() {
    let mut harness = Harness::new(1);
    let basis = authoring_basis(&harness);
    let document = basis.document;
    let before = harness.application.current_frame().unwrap();
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let prepared = context
            .prepare_authoring_edit(&basis, &prepared_panel_patch())
            .unwrap();
        context.commit_authoring_edit(prepared).unwrap();
        context
            .submit(EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document,
                    entity: None,
                },
            ))
            .unwrap();
    }));
    let candidate = harness.draw();
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            before.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(nara::scene::SceneEntityId::new("preserved").unwrap()),
                },
            )],
        ),
        Duration::ZERO,
    );
    let advanced = harness.application.current_frame().unwrap();
    assert_ne!(advanced.revision(), before.revision());
    harness.submit(candidate);
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .documents,
        advanced.workspace().documents
    );
    let evidence = harness.panels[0].1.borrow();
    assert_eq!(evidence.finished.len(), 2);
    assert!(evidence.finished.iter().all(|(_, result)| matches!(
        result,
        PanelActionDispatch::NotDispatched(PanelActionNotDispatched::BatchRejected(
            nara::tooling::EditorActionRejection::StaleFrame { .. }
        ))
    )));
}

#[test]
fn panel_construction_delivers_the_owner_mapping_and_supports_a_followup_property_edit() {
    use nara::{
        reflect::{
            ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion, ComponentTypeId,
            ComponentValue,
        },
        scene::AuthoringPropertyEdit,
    };

    let mut harness = Harness::new(1);
    let basis = authoring_basis(&harness);
    let document = basis.document;
    let before = harness.application.current_frame().unwrap();
    let source = harness._project.root.join("worlds/entry.scene.json");
    let saved = std::fs::read(&source).unwrap();
    let mapping = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&mapping);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let mut draft = context.begin_authoring_construction(&basis).unwrap();
        let local = draft
            .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
            .unwrap();
        draft
            .add_component(
                &local,
                ComponentTypeId::new(super::interaction_tests::CUSTOM_COMPONENT),
            )
            .unwrap();
        let prepared = context
            .prepare_authoring_construction(basis.document, &draft)
            .unwrap();
        let entity = prepared.entity(&local).unwrap().clone();
        let id = context.commit_authoring_construction(prepared).unwrap();
        *captured.borrow_mut() = Some((id, local, entity));
    }));
    let candidate = harness.draw();
    assert!(mapping.borrow().is_some());
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    assert!(harness.panels[0].1.borrow().finished.is_empty());
    harness.submit(candidate);
    let (id, local, entity) = mapping.borrow_mut().take().unwrap();
    {
        let evidence = harness.panels[0].1.borrow();
        let [
            (
                actual,
                PanelActionDispatch::Dispatched(EditorActionOutcome::AuthoringConstruction(
                    committed,
                )),
            ),
        ] = evidence.finished.as_slice()
        else {
            panic!("construction must return its source-owner receipt");
        };
        assert_eq!(*actual, id);
        assert!(committed.report().applied);
        assert_eq!(committed.entity(&local), Some(&entity));
    }
    let basis = authoring_basis(&harness);
    let before_property = harness.application.current_frame().unwrap();
    let captured = Rc::new(RefCell::new(None));
    let queued = Rc::clone(&captured);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let edit = context
            .prepare_authoring_property_edit(
                &basis,
                &[AuthoringPropertyEdit {
                    entity: entity.clone(),
                    component: ComponentTypeId::new(super::interaction_tests::CUSTOM_COMPONENT),
                    component_version: ComponentSchemaVersion::ONE,
                    property: ComponentPropertyPath::new(ComponentFieldId::new("value")),
                    value: ComponentValue::I64(27),
                }],
            )
            .unwrap();
        *queued.borrow_mut() = Some(context.commit_authoring_edit(edit).unwrap());
    }));
    let candidate = harness.draw();
    assert!(captured.borrow().is_some());
    assert!(std::sync::Arc::ptr_eq(
        &before_property,
        &harness.application.current_frame().unwrap()
    ));
    harness.submit(candidate);
    let after = harness.application.current_frame().unwrap();
    assert_eq!(
        after
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        before
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth
            + 2
    );
    let evidence = harness.panels[0].1.borrow();
    assert_eq!(evidence.finished.len(), 2);
    assert!(matches!(
        &evidence.finished[1],
        (id, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)))
            if Some(*id) == *captured.borrow() && report.applied
    ));
    assert_eq!(std::fs::read(source).unwrap(), saved);
}

#[test]
fn panel_queue_refusal_preserves_both_owned_candidates_without_a_commit_callback() {
    let mut harness = Harness::new(1);
    let basis = authoring_basis(&harness);
    let before = harness.application.current_frame().unwrap();
    let retained = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&retained);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let patch = prepared_panel_patch();
        let edit = context.prepare_authoring_edit(&basis, &patch).unwrap();
        let mut draft = context.begin_authoring_construction(&basis).unwrap();
        let local = draft
            .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
            .unwrap();
        let construction = context
            .prepare_authoring_construction(basis.document, &draft)
            .unwrap();
        let entity = construction.entity(&local).unwrap().clone();
        for _ in 0..super::submission::MAX_PANEL_SUBMISSIONS {
            context
                .submit(EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document: basis.document,
                        entity: None,
                    },
                ))
                .unwrap();
        }
        let (error, edit) = context.commit_authoring_edit(edit).unwrap_err();
        assert_eq!(error, super::PanelSubmitError::Capacity);
        assert_eq!(edit.document(), basis.document);
        assert_eq!(edit.base_revision(), basis.revision);
        assert_eq!(edit.patch(), &patch);
        assert_eq!(edit.inverse().operations.len(), 1);
        let (error, construction) = context
            .commit_authoring_construction(construction)
            .unwrap_err();
        assert_eq!(error, super::PanelSubmitError::Capacity);
        assert_eq!(construction.document(), basis.document);
        assert_eq!(construction.base_revision(), basis.revision);
        assert_eq!(construction.entity(&local), Some(&entity));
        assert_eq!(construction.patch().operations.len(), 1);
        assert_eq!(construction.inverse().operations.len(), 1);
        *captured.borrow_mut() = Some((edit, construction));
    }));
    let candidate = harness.draw();
    assert!(retained.borrow().is_some());
    assert_eq!(
        candidate.ledger.submissions.len(),
        super::submission::MAX_PANEL_SUBMISSIONS
    );
    assert_eq!(
        candidate.batch.into_actions().len(),
        super::submission::MAX_PANEL_SUBMISSIONS
    );
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    drop(retained.borrow_mut().take());
    assert!(harness.panels[0].1.borrow().finished.is_empty());
}

#[test]
fn the_same_capacity_refused_edit_commits_once_in_a_fresh_paint_without_preparation() {
    let mut harness = Harness::new(1);
    let basis = authoring_basis(&harness);
    let document = basis.document;
    let revision = basis.revision;
    let before = harness.application.current_frame().unwrap();
    let source = harness._project.root.join("worlds/entry.scene.json");
    let saved = std::fs::read(&source).unwrap();
    let retained = Rc::new(RefCell::new(None));
    let refused = Rc::clone(&retained);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let edit = context
            .prepare_authoring_edit(&basis, &prepared_panel_patch())
            .unwrap();
        for _ in 0..super::submission::MAX_PANEL_SUBMISSIONS {
            context
                .submit(EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document,
                        entity: None,
                    },
                ))
                .unwrap();
        }
        let (error, edit) = context.commit_authoring_edit(edit).unwrap_err();
        assert_eq!(error, super::PanelSubmitError::Capacity);
        *refused.borrow_mut() = Some(edit);
    }));
    drop(harness.draw());
    assert!(retained.borrow().is_some());
    assert!(harness.panels[0].1.borrow().finished.is_empty());
    let queued = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&queued);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let edit = retained.borrow_mut().take().unwrap();
        assert_eq!(edit.base_revision(), revision);
        assert_eq!(edit.patch(), &prepared_panel_patch());
        *captured.borrow_mut() = Some(context.commit_authoring_edit(edit).unwrap());
    }));
    let retry = harness.draw();
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    assert!(
        harness.panels[0]
            .1
            .borrow()
            .finished
            .iter()
            .all(|(_, result)| *result
                == PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded))
    );
    harness.submit(retry);
    let after = harness.application.current_frame().unwrap();
    assert_eq!(
        after
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        before
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth
            + 1
    );
    let evidence = harness.panels[0].1.borrow();
    assert_eq!(
        evidence.finished.len(),
        super::submission::MAX_PANEL_SUBMISSIONS + 1
    );
    assert!(matches!(evidence.finished.last(),
        Some((id, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))))
            if Some(*id) == *queued.borrow() && report.applied
    ));
    assert_eq!(
        evidence
            .finished
            .iter()
            .filter(|(id, _)| Some(*id) == *queued.borrow())
            .count(),
        1
    );
    assert_eq!(std::fs::read(source).unwrap(), saved);
}

#[test]
fn the_same_capacity_refused_construction_keeps_its_preview_mapping_on_retry() {
    let mut harness = Harness::new(1);
    let basis = authoring_basis(&harness);
    let document = basis.document;
    let revision = basis.revision;
    let before = harness.application.current_frame().unwrap();
    let source = harness._project.root.join("worlds/entry.scene.json");
    let saved = std::fs::read(&source).unwrap();
    let retained = Rc::new(RefCell::new(None));
    let refused = Rc::clone(&retained);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let mut draft = context.begin_authoring_construction(&basis).unwrap();
        let local = draft
            .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
            .unwrap();
        let construction = context
            .prepare_authoring_construction(document, &draft)
            .unwrap();
        let preview = construction.entity(&local).unwrap().clone();
        for _ in 0..super::submission::MAX_PANEL_SUBMISSIONS {
            context
                .submit(EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document,
                        entity: None,
                    },
                ))
                .unwrap();
        }
        let (error, construction) = context
            .commit_authoring_construction(construction)
            .unwrap_err();
        assert_eq!(error, super::PanelSubmitError::Capacity);
        *refused.borrow_mut() = Some((construction, local, preview));
    }));
    drop(harness.draw());
    assert!(retained.borrow().is_some());
    assert!(harness.panels[0].1.borrow().finished.is_empty());
    let queued = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&queued);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let (construction, local, preview) = retained.borrow_mut().take().unwrap();
        assert_eq!(construction.base_revision(), revision);
        assert_eq!(construction.entity(&local), Some(&preview));
        let id = context.commit_authoring_construction(construction).unwrap();
        *captured.borrow_mut() = Some((id, local, preview));
    }));
    let retry = harness.draw();
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    assert!(
        harness.panels[0]
            .1
            .borrow()
            .finished
            .iter()
            .all(|(_, result)| *result
                == PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded))
    );
    harness.submit(retry);
    let after = harness.application.current_frame().unwrap();
    assert_eq!(
        after
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        before
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth
            + 1
    );
    let (id, local, preview) = queued.borrow_mut().take().unwrap();
    let evidence = harness.panels[0].1.borrow();
    assert_eq!(
        evidence.finished.len(),
        super::submission::MAX_PANEL_SUBMISSIONS + 1
    );
    let (
        actual,
        PanelActionDispatch::Dispatched(EditorActionOutcome::AuthoringConstruction(committed)),
    ) = evidence.finished.last().unwrap()
    else {
        panic!("the retried construction must publish its original owner mapping");
    };
    assert_eq!(*actual, id);
    assert_eq!(
        evidence
            .finished
            .iter()
            .filter(|(actual, _)| *actual == id)
            .count(),
        1
    );
    assert!(committed.report().applied);
    assert_eq!(committed.entity(&local), Some(&preview));
    assert_eq!(std::fs::read(source).unwrap(), saved);
}

fn queue_both_prepared_candidates(
    harness: &mut Harness,
) -> (ImguiEditorSubmission, [PanelSubmissionId; 2]) {
    let basis = authoring_basis(harness);
    let queued = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&queued);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let edit = context
            .prepare_authoring_edit(&basis, &prepared_panel_patch())
            .unwrap();
        let mut draft = context.begin_authoring_construction(&basis).unwrap();
        let local = draft
            .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
            .unwrap();
        let construction = context
            .prepare_authoring_construction(basis.document, &draft)
            .unwrap();
        assert!(construction.entity(&local).is_some());
        *captured.borrow_mut() = Some([
            context.commit_authoring_edit(edit).unwrap(),
            context.commit_authoring_construction(construction).unwrap(),
        ]);
    }));
    let candidate = harness.draw();
    let ids = queued.borrow_mut().take().unwrap();
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(|_| {}));
    (candidate, ids)
}

#[test]
fn superseding_both_consuming_candidates_refuses_each_original_submission_once() {
    let mut harness = Harness::new(1);
    let before = harness.application.current_frame().unwrap();
    let source = harness._project.root.join("worlds/entry.scene.json");
    let saved = std::fs::read(&source).unwrap();
    let (stale, ids) = queue_both_prepared_candidates(&mut harness);
    let fresh = harness.draw();
    assert_eq!(stale.batch.frame_revision(), fresh.batch.frame_revision());
    assert_eq!(
        harness.panels[0].1.borrow().finished,
        ids.map(|id| (
            id,
            PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded)
        ))
    );
    harness.submit(stale);
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    harness.submit(fresh);
    let after = harness.application.current_frame().unwrap();
    assert_eq!(after.workspace().documents, before.workspace().documents);
    assert_eq!(
        after.workspace().active_scene,
        before.workspace().active_scene
    );
    assert_eq!(harness.panels[0].1.borrow().finished.len(), 2);
    assert_eq!(std::fs::read(source).unwrap(), saved);
}

#[test]
fn both_consuming_candidates_are_filtered_before_hide_retarget_reset_or_restore() {
    for fence in ["hide", "retarget", "reset", "restore"] {
        let mut harness = Harness::new(1);
        let before = harness.application.current_frame().unwrap();
        let source = harness._project.root.join("worlds/entry.scene.json");
        let saved = std::fs::read(&source).unwrap();
        let (candidate, _) = queue_both_prepared_candidates(&mut harness);
        let id = harness.panels[0].0;
        let incarnation =
            std::sync::Arc::clone(&harness.frontend.presentation.widgets[&id].incarnation);
        match fence {
            "hide" => {
                harness
                    .application
                    .panel_workspace_mut()
                    .layout_mut()
                    .set_visible(id, false);
            }
            "retarget" => {
                harness
                    .application
                    .panel_workspace_mut()
                    .layout_mut()
                    .set_target(id, EditorPanelTarget::Workspace);
            }
            "reset" => harness.application.panel_workspace_mut().reset(),
            "restore" => {
                let layout = harness.application.panel_workspace().layout().clone();
                harness
                    .application
                    .panel_workspace_mut()
                    .restore(layout, "dear-imgui", 1, &[]);
            }
            _ => unreachable!(),
        }
        let dispatch = harness
            .frontend
            .dispatch_submission(&mut harness.application, candidate)
            .unwrap();
        assert_eq!(dispatch.original_slots(), &[None, None], "{fence}");
        assert!(
            !harness.frontend.presentation.widgets.contains_key(&id),
            "{fence}"
        );
        harness
            .application
            .panel_workspace_mut()
            .layout_mut()
            .reopen(id);
        let instance = harness
            .application
            .panel_workspace()
            .layout()
            .instance(id)
            .unwrap()
            .clone();
        // Install the registered replacement before settling the old dispatch. Its Probe
        // shares the evidence sink, so any old callback or construction mapping is observable.
        let panel = harness.frontend.presentation.factories[instance.kind()]();
        let widgets = super::workspace::ImguiPanelWidgets {
            panel: Some(panel),
            kind: instance.kind().to_owned(),
            document: instance
                .target()
                .document(&harness.application.current_frame().unwrap()),
            target: instance.target().clone(),
            ..Default::default()
        };
        let replacement_incarnation = std::sync::Arc::clone(&widgets.incarnation);
        assert!(
            !std::sync::Arc::ptr_eq(&incarnation, &replacement_incarnation),
            "{fence}"
        );
        harness.frontend.presentation.widgets.insert(id, widgets);
        harness
            .application
            .drive_editor_frame(Duration::ZERO, dispatch.actions_observed(), true);
        harness
            .frontend
            .finish_dispatch(&harness.application, &dispatch);
        assert!(harness.panels[0].1.borrow().finished.is_empty(), "{fence}");
        assert!(
            std::sync::Arc::ptr_eq(
                &replacement_incarnation,
                &harness.frontend.presentation.widgets[&id].incarnation
            ),
            "{fence}"
        );
        let after = harness.application.current_frame().unwrap();
        assert_eq!(
            after.workspace().documents,
            before.workspace().documents,
            "{fence}"
        );
        assert_eq!(
            after.workspace().active_scene,
            before.workspace().active_scene,
            "{fence}"
        );
        assert_eq!(std::fs::read(&source).unwrap(), saved, "{fence}");

        let replacement = harness.draw();
        let widgets = &harness.frontend.presentation.widgets[&id];
        assert!(
            !std::sync::Arc::ptr_eq(&incarnation, &widgets.incarnation),
            "{fence}"
        );
        harness.submit(replacement);
        assert!(harness.panels[0].1.borrow().finished.is_empty(), "{fence}");
        assert_eq!(std::fs::read(source).unwrap(), saved, "{fence}");
    }
}

#[test]
fn project_replacement_cannot_publish_or_deliver_either_old_consuming_candidate() {
    let mut harness = Harness::new(1);
    let old_generation = harness.application.workspace_generation();
    let old_source = harness._project.root.join("worlds/entry.scene.json");
    let old_saved = std::fs::read(&old_source).unwrap();
    let (candidate, _) = queue_both_prepared_candidates(&mut harness);
    let id = harness.panels[0].0;
    let incarnation =
        std::sync::Arc::clone(&harness.frontend.presentation.widgets[&id].incarnation);
    let project = super::interaction_tests::InteractionProject::new();
    let new_source = project.root.join("worlds/entry.scene.json");
    let new_saved = std::fs::read(&new_source).unwrap();
    assert!(
        harness
            .application
            .request_project_replacement(&project.root, Some("desktop".to_owned()))
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while harness.application.workspace_generation() == old_generation {
        assert!(
            std::time::Instant::now() < deadline,
            "project replacement timed out"
        );
        harness.application.advance_project_open();
        harness
            .application
            .drive_editor_frame(Duration::ZERO, false, true);
        std::thread::sleep(Duration::from_millis(1));
    }
    let before = harness.application.current_frame().unwrap();
    harness.submit(candidate);
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    let replacement = harness.draw();
    assert!(!std::sync::Arc::ptr_eq(
        &incarnation,
        &harness.frontend.presentation.widgets[&id].incarnation
    ));
    harness.submit(replacement);
    let after = harness.application.current_frame().unwrap();
    assert_eq!(after.workspace().documents, before.workspace().documents);
    assert_eq!(
        after.workspace().active_scene,
        before.workspace().active_scene
    );
    assert!(harness.panels[0].1.borrow().finished.is_empty());
    assert_eq!(std::fs::read(old_source).unwrap(), old_saved);
    assert_eq!(std::fs::read(new_source).unwrap(), new_saved);
    drop(harness);
}

#[test]
fn closing_filter_refuses_both_candidates_around_an_allowed_close_resolution() {
    use nara::tooling::{
        EditorCloseDecision, EditorDocumentMutation, EditorWorkspaceIntent,
        EditorWorkspaceIntentAction,
    };

    let mut harness = Harness::new(1);
    let basis = authoring_basis(&harness);
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            harness.application.current_frame().unwrap().revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyScenePatch {
                    document: basis.document,
                    base_revision: basis.revision,
                    patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                        entity: SceneEntityRecord::new(
                            SceneEntityId::new("closing-baseline").unwrap(),
                        ),
                        placement: ScenePlacement::last(ScenePlacementOwner::Root),
                    }]),
                },
            )],
        ),
        Duration::ZERO,
    );
    let basis = authoring_basis(&harness);
    let document = basis.document;
    let retained = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&retained);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let edit = context
            .prepare_authoring_edit(&basis, &prepared_panel_patch())
            .unwrap();
        let mut draft = context.begin_authoring_construction(&basis).unwrap();
        draft
            .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
            .unwrap();
        let construction = context
            .prepare_authoring_construction(document, &draft)
            .unwrap();
        *captured.borrow_mut() = Some((edit, construction));
    }));
    drop(harness.draw());
    let frame = harness.application.current_frame().unwrap();
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseDocument {
                    document,
                }),
            )],
        ),
        Duration::ZERO,
    );
    let before = harness.application.current_frame().unwrap();
    assert!(before.workspace_intent().decision_prompt().is_some());
    let source = harness._project.root.join("worlds/entry.scene.json");
    let saved = std::fs::read(&source).unwrap();
    let queued = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&queued);
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        let (edit, construction) = retained.borrow_mut().take().unwrap();
        let resolution = context
            .model()
            .frame()
            .workspace_intent()
            .decision_prompt()
            .unwrap()
            .resolve(EditorCloseDecision::Cancel);
        *captured.borrow_mut() = Some([
            context.commit_authoring_edit(edit).unwrap(),
            context
                .submit(EditorAction::WorkspaceIntent(resolution))
                .unwrap(),
            context.commit_authoring_construction(construction).unwrap(),
        ]);
    }));
    let candidate = harness.draw();
    let dispatch = harness
        .frontend
        .dispatch_submission(&mut harness.application, candidate)
        .unwrap();
    assert_eq!(dispatch.original_slots(), &[None, Some(0), None]);
    harness
        .application
        .drive_editor_frame(Duration::ZERO, dispatch.actions_observed(), true);
    harness
        .frontend
        .finish_dispatch(&harness.application, &dispatch);
    let after = harness.application.current_frame().unwrap();
    assert_eq!(after.workspace().documents, before.workspace().documents);
    assert_eq!(
        after.workspace().active_scene,
        before.workspace().active_scene
    );
    let ids = queued.borrow_mut().take().unwrap();
    let evidence = harness.panels[0].1.borrow();
    assert_eq!(
        evidence
            .finished
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        ids
    );
    assert_eq!(
        evidence.finished[0].1,
        PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing)
    );
    assert!(matches!(
        evidence.finished[1].1,
        PanelActionDispatch::Dispatched(EditorActionOutcome::WorkspaceIntent(_))
    ));
    assert_eq!(
        evidence.finished[2].1,
        PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing)
    );
    assert_eq!(std::fs::read(source).unwrap(), saved);
}

pub(super) struct Harness {
    _project: super::interaction_tests::InteractionProject,
    pub(super) application: EditorApplication,
    pub(super) frontend: ImguiEditorFrontend,
    context: Context,
    pub(super) panels: Vec<(EditorPanelInstanceId, Rc<RefCell<Evidence>>)>,
}

impl Harness {
    pub(super) fn new(count: usize) -> Self {
        Self::with_project(super::interaction_tests::InteractionProject::new(), count)
    }

    pub(super) fn with_project(
        project: super::interaction_tests::InteractionProject,
        count: usize,
    ) -> Self {
        let mut application =
            EditorApplication::new(EditorLaunchOptions::new(&project.root).with_profile("desktop"));
        application.native_shell_ready();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if application.current_frame().is_some() {
                break;
            }
            application.advance_project_open();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(application.current_frame().is_some());
        let mut composition = EditorNativeComposition::new();
        let mut evidence = Vec::new();
        for index in 0..count {
            let shared = Rc::new(RefCell::new(Evidence::default()));
            let factory = Rc::clone(&shared);
            let kind = format!("test.dispatch.{index}");
            composition
                .register_panel(
                    EditorPanelDescriptor::new(
                        &kind,
                        &kind,
                        EditorPanelMultiplicity::Multiple,
                        EditorPanelPlacement::Floating,
                        true,
                    ),
                    move || Probe(Rc::clone(&factory)),
                )
                .unwrap();
            evidence.push((kind, shared));
        }
        let frontend = composition.install_panels(&mut application).unwrap();
        let panels = evidence
            .into_iter()
            .map(|(kind, evidence)| {
                let id = application
                    .panel_workspace()
                    .layout()
                    .instances()
                    .iter()
                    .find(|panel| panel.kind() == kind)
                    .unwrap()
                    .id();
                (id, evidence)
            })
            .collect();
        let mut context = Context::create();
        context.set_ini_filename::<PathBuf>(None).unwrap();
        context
            .io_mut()
            .set_config_flags(ConfigFlags::DOCKING_ENABLE);
        context.io_mut().set_display_size([1800.0, 1100.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        let mut harness = Self {
            _project: project,
            application,
            frontend,
            context,
            panels,
        };
        for _ in 0..3 {
            let candidate = harness.draw();
            harness
                .frontend
                .submit_and_drive(&mut harness.application, candidate, Duration::ZERO);
        }
        harness
    }

    pub(super) fn draw(&mut self) -> ImguiEditorSubmission {
        let candidate = self
            .frontend
            .show(self.context.frame(), &mut self.application)
            .unwrap()
            .unwrap();
        let _ = self.context.render_legacy();
        candidate
    }

    pub(super) fn submit(&mut self, candidate: ImguiEditorSubmission) {
        self.frontend
            .submit_and_drive(&mut self.application, candidate, Duration::ZERO);
    }

    pub(super) fn click_control(&mut self, label: &str) {
        let rect = super::ui_interaction_tests::control_rect(label)
            .unwrap_or_else(|| panic!("the real ImGui control {label:?} was not painted"));
        let clip = super::ui_interaction_tests::control_clip(label).unwrap();
        let min = [rect.min[0].max(clip.min[0]), rect.min[1].max(clip.min[1])];
        let max = [rect.max[0].min(clip.max[0]), rect.max[1].min(clip.max[1])];
        assert!(max[0] > min[0] && max[1] > min[1], "{label:?} is clipped");
        let point = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
        self.context.io_mut().add_mouse_pos_event(point);
        let candidate = self.draw();
        self.submit(candidate);
        self.context
            .io_mut()
            .add_mouse_button_event(dear_imgui_rs::MouseButton::Left, true);
        let candidate = self.draw();
        self.submit(candidate);
        self.context
            .io_mut()
            .add_mouse_button_event(dear_imgui_rs::MouseButton::Left, false);
        let candidate = self.draw();
        self.submit(candidate);
        for _ in 0..3 {
            let candidate = self.draw();
            self.submit(candidate);
        }
    }

    fn close_dirty_document(&mut self) -> nara::tooling::EditorDocumentId {
        use nara::{
            scene::{
                SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
                ScenePlacement, ScenePlacementOwner,
            },
            tooling::{EditorDocumentMutation, EditorWorkspaceIntent, EditorWorkspaceIntentAction},
        };
        let frame = self.application.current_frame().unwrap();
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        self.application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [
                    EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
                        document: scene.document,
                        base_revision: scene.revision,
                        patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                            entity: SceneEntityRecord::new(
                                SceneEntityId::new("dispatch-dirty").unwrap(),
                            ),
                            placement: ScenePlacement::last(ScenePlacementOwner::Root),
                        }]),
                    }),
                    EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
                        EditorWorkspaceIntent::CloseDocument {
                            document: scene.document,
                        },
                    )),
                ],
            ),
            Duration::ZERO,
        );
        assert!(
            self.application
                .current_frame()
                .unwrap()
                .workspace_intent()
                .decision_prompt()
                .is_some()
        );
        scene.document
    }
}

#[test]
fn two_panels_receive_only_their_own_multiple_ordered_action_results_once() {
    let mut harness = Harness::new(2);
    for (_, evidence) in &harness.panels {
        evidence.borrow_mut().count = 2;
    }
    let candidate = harness.draw();
    for (_, evidence) in &harness.panels {
        assert_eq!(evidence.borrow().queued.len(), 2);
        assert!(evidence.borrow().finished.is_empty());
    }
    let frame = harness.application.current_frame().unwrap();
    // Direct dispatch has no frontend observation authority. Keep the current published frame
    // here so this assertion remains independent of navigation's exact-frame admission gate.
    let direct = harness
        .application
        .dispatch_editor_actions(EditorActionBatch::new(frame.revision(), []));
    assert!(!direct.actions_observed());
    assert!(std::sync::Arc::ptr_eq(
        &frame,
        &harness.application.current_frame().unwrap()
    ));
    assert!(
        harness
            .panels
            .iter()
            .all(|(_, evidence)| evidence.borrow().finished.is_empty())
    );
    harness.submit(candidate);
    let mut all = std::collections::BTreeSet::new();
    for (_, evidence) in &harness.panels {
        let evidence = evidence.borrow();
        assert_eq!(
            evidence.queued,
            evidence
                .finished
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>()
        );
        assert!(evidence.finished.iter().all(|(id, outcome)| all.insert(*id)
            && matches!(
                outcome,
                PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(_))
            )));
    }
    let frame = harness.application.current_frame().unwrap();
    // An unrelated direct batch cannot redeliver or settle frontend observations.
    harness
        .application
        .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO);
    assert!(
        harness
            .panels
            .iter()
            .all(|(_, evidence)| evidence.borrow().finished.len() == 2)
    );
}

#[test]
fn an_unrelated_application_drive_can_stale_a_candidate_without_settling_its_callbacks() {
    let mut harness = Harness::new(1);
    harness.panels[0].1.borrow_mut().count = 2;
    let candidate = harness.draw();
    let received = candidate.batch.frame_revision();
    harness
        .application
        .submit_and_drive(EditorActionBatch::new(received, []), Duration::ZERO);
    let expected = harness.application.current_frame().unwrap().revision();
    assert_ne!(received, expected);
    assert!(harness.panels[0].1.borrow().finished.is_empty());
    harness.submit(candidate);
    {
        let evidence = harness.panels[0].1.borrow();
        assert_eq!(evidence.finished.len(), 2);
        assert_eq!(
            evidence.queued,
            evidence
                .finished
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>()
        );
        assert!(evidence.finished.iter().all(|(_, outcome)| *outcome
            == PanelActionDispatch::NotDispatched(PanelActionNotDispatched::BatchRejected(
                nara::tooling::EditorActionRejection::StaleFrame { expected, received }
            ))));
    }
    let fresh = harness.draw();
    harness.submit(fresh);
    let evidence = harness.panels[0].1.borrow();
    assert_eq!(evidence.finished.len(), 4);
    assert!(evidence.finished[2..].iter().all(|(_, outcome)| matches!(
        outcome,
        PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(_))
    )));
}

#[test]
fn foreign_and_superseded_candidates_with_the_same_frame_cannot_dispatch() {
    let mut harness = Harness::new(1);
    harness.panels[0].1.borrow_mut().count = 2;
    let foreign = harness.draw();
    let revision = foreign.batch.frame_revision();
    let before = harness.application.current_frame().unwrap();
    let update = ImguiEditorFrontend::new().submit_and_drive(
        &mut harness.application,
        foreign,
        Duration::ZERO,
    );
    assert!(!update.actions_observed());
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    assert!(harness.panels[0].1.borrow().finished.is_empty());
    let stale = harness.draw();
    assert_eq!(revision, stale.batch.frame_revision());
    assert!(
        harness.panels[0]
            .1
            .borrow()
            .finished
            .iter()
            .all(|(_, result)| *result
                == PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded))
    );
    let current = harness.draw();
    assert_eq!(revision, current.batch.frame_revision());
    let finished = harness.panels[0].1.borrow().finished.len();
    harness.submit(stale);
    assert_eq!(harness.panels[0].1.borrow().finished.len(), finished);
    harness.submit(current);
    let evidence = harness.panels[0].1.borrow();
    assert_eq!(evidence.finished.len(), 6);
    assert!(
        evidence.finished[4..]
            .iter()
            .all(|(_, result)| matches!(result, PanelActionDispatch::Dispatched(_)))
    );
}

#[test]
fn closing_filter_keeps_original_action_slots_for_each_callback() {
    let mut harness = Harness::new(1);
    harness.close_dirty_document();
    {
        let mut evidence = harness.panels[0].1.borrow_mut();
        evidence.count = 3;
        evidence.close_resolution = Some(nara::tooling::EditorCloseDecision::Cancel);
    }
    let candidate = harness.draw();
    harness.submit(candidate);
    let evidence = harness.panels[0].1.borrow();
    assert_eq!(evidence.finished.len(), 3);
    assert_eq!(
        evidence.finished[0].1,
        PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing)
    );
    assert!(matches!(
        evidence.finished[1].1,
        PanelActionDispatch::Dispatched(EditorActionOutcome::WorkspaceIntent(_))
    ));
    assert_eq!(
        evidence.finished[2].1,
        PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing)
    );
    assert_eq!(
        evidence.queued,
        evidence
            .finished
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn retargeting_before_dispatch_retires_the_original_action_and_recipient() {
    let mut harness = Harness::new(1);
    let frame = harness.application.current_frame().unwrap();
    let document = frame.workspace().active_scene.as_ref().unwrap().document;
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(nara::scene::SceneEntityId::new("preserved").unwrap()),
                },
            )],
        ),
        Duration::ZERO,
    );
    harness.panels[0].1.borrow_mut().count = 1;
    let candidate = harness.draw();
    let id = harness.panels[0].0;
    let frame = harness.application.current_frame().unwrap();
    let target = EditorPanelTarget::Source {
        kind: crate::workspace::EditorPanelSourceKind::Scene,
        locator: "missing.scene.json".to_owned(),
    };
    harness
        .application
        .panel_workspace_mut()
        .layout_mut()
        .set_target(id, target);
    harness.submit(candidate);
    assert!(harness.panels[0].1.borrow().finished.is_empty());
    assert_eq!(
        frame.workspace().active_scene.as_ref().unwrap().selection,
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .selection
    );
    assert!(!harness.frontend.presentation.widgets.contains_key(&id));
}

#[test]
fn a_document_closed_during_drive_cannot_deliver_to_its_retired_panel() {
    let mut harness = Harness::new(1);
    let document = harness.close_dirty_document();
    {
        let mut evidence = harness.panels[0].1.borrow_mut();
        evidence.count = 2;
        evidence.close_resolution = Some(nara::tooling::EditorCloseDecision::Discard);
    }
    let candidate = harness.draw();
    assert_eq!(harness.panels[0].1.borrow().queued.len(), 2);
    harness.submit(candidate);
    assert!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .document(document)
            .is_none()
    );
    assert!(harness.panels[0].1.borrow().finished.is_empty());
    assert!(
        !harness
            .frontend
            .presentation
            .widgets
            .contains_key(&harness.panels[0].0)
    );
}

#[test]
fn queue_capacity_rejects_before_assigning_an_id_or_enqueuing() {
    use super::submission::{self, PanelOrigin, Recipient};
    let mut pending = Vec::new();
    let mut next = 0;
    let origin = PanelOrigin {
        instance: EditorPanelInstanceId(1),
        incarnation: Default::default(),
    };
    for slot in 0..submission::MAX_PANEL_SUBMISSIONS {
        assert!(
            submission::enqueue(
                &mut next,
                &mut pending,
                origin.clone(),
                slot,
                Recipient::Contribution
            )
            .is_ok()
        );
    }
    let old_next = next;
    assert_eq!(
        submission::enqueue(&mut next, &mut pending, origin, 0, Recipient::Contribution),
        Err(super::PanelSubmitError::Capacity)
    );
    assert_eq!(next, old_next);
    assert_eq!(pending.len(), submission::MAX_PANEL_SUBMISSIONS);
}
