//! Selector-sized reads feed a real non-active panel edit without copying its complete root.

use super::*;

use nara::{
    reflect::{
        ComponentPropertyError, ComponentPropertyKey, ComponentPropertyStep, ComponentShapeMember,
        ComponentValueShape,
    },
    scene::AuthoringReadError,
};

const COMPONENT: &str = "consumer.property.PanelSettings";
const SCORE_KEY: &str = "score.total[0]";
const ROW_KEY: &str = "chosen/stable";
const ORIGINAL: u64 = 41;

fn member(wire: &str, stable: &str, shape: impl Into<ComponentValueShape>) -> ComponentShapeMember {
    ComponentShapeMember::new(wire, shape).with_id(ComponentFieldId::new(stable))
}

fn schemas() -> AuthoringSchemaBundle {
    let shape = ComponentValueShape::record_members([
        member(
            "wire_rows",
            "rows",
            ComponentValueShape::keyed_list(
                ComponentValueShape::record_members([
                    member("wire_key", "id", ComponentValueKind::String),
                    member(
                        "wire_metrics",
                        "scores",
                        ComponentValueShape::map(ComponentValueKind::U64.into()),
                    ),
                ]),
                ComponentFieldId::new("id"),
            ),
        ),
        member("wire_padding", "unrelated", ComponentValueKind::String),
        member(
            "wire_optional",
            "optional",
            ComponentValueShape::optional(ComponentValueShape::record_members([member(
                "wire_score",
                "score",
                ComponentValueKind::U64,
            )])),
        ),
    ]);
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new(COMPONENT),
                    "Panel settings",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("settings"),
                        "Settings",
                        ComponentFieldPath::from_fields(["wire_settings"]),
                        shape,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                    ComponentFieldSchema::optional_with_default(
                        ComponentFieldId::new("defaults"),
                        "Defaults",
                        ComponentFieldPath::from_fields(["wire_defaults"]),
                        ComponentValueShape::record_members([member(
                            "wire_score",
                            "score",
                            ComponentValueKind::U64,
                        )]),
                        ComponentValue::map([("wire_score", ComponentValue::U64(13))]),
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                ]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn row(key: &str, score: u64) -> ComponentValue {
    ComponentValue::map([
        ("wire_key", ComponentValue::String(key.into())),
        (
            "wire_metrics",
            ComponentValue::map([
                (SCORE_KEY, ComponentValue::U64(score)),
                ("untouched", ComponentValue::U64(777)),
            ]),
        ),
    ])
}

fn component_value(score: u64) -> ComponentValue {
    ComponentValue::map([(
        "wire_settings",
        ComponentValue::map([
            (
                "wire_rows",
                ComponentValue::List(vec![row("distractor", 900), row(ROW_KEY, score)]),
            ),
            (
                "wire_padding",
                ComponentValue::String("untouched".repeat(2048)),
            ),
            ("wire_optional", ComponentValue::Null),
        ]),
    )])
}

fn score_path(key: &str) -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new("settings")).with_steps([
        ComponentPropertyStep::Member(ComponentFieldId::new("rows")),
        ComponentPropertyStep::ListItem {
            by: ComponentFieldId::new("id"),
            key: ComponentPropertyKey::String(key.into()),
        },
        ComponentPropertyStep::Member(ComponentFieldId::new("scores")),
        ComponentPropertyStep::MapEntry(SCORE_KEY.into()),
    ])
}

fn request(basis: EditorDocumentQueryBasis, property: ComponentPropertyPath) -> EditorPropertyRead {
    let target = EditorPropertyTarget {
        basis,
        entity: id("anchor"),
        component: ComponentTypeId::new(COMPONENT),
        property,
    };
    let metadata = target.entity.as_str().len()
        + target.component.as_str().len()
        + target.property.logical_bytes().unwrap();
    EditorPropertyRead {
        target,
        limits: EditorPropertyReadLimits::new(metadata, 1, 8)
            .unwrap()
            .with_scan_limits(128, 1024)
            .unwrap(),
    }
}

fn assert_scalar(app: &mut EditorApplication, document: EditorDocumentId, expected: u64) {
    let captured = basis(app, document);
    let observed = root_read(app, request(captured, score_path(ROW_KEY))).unwrap();
    assert_eq!(
        observed.value,
        EditorPropertyValue::Present(ComponentValue::U64(expected))
    );
    assert_eq!(observed.value_nodes, 1);
    assert_eq!(observed.value_logical_bytes, 8);
}

#[derive(Default)]
struct PanelEvidence {
    observation: Option<EditorPropertyObservation>,
    held_frames: usize,
    submissions: Vec<PanelSubmissionId>,
    completions: Vec<(PanelSubmissionId, PanelActionDispatch)>,
}

struct PropertyPanel {
    request: Option<EditorPropertyRead>,
    prepared: Option<PreparedEditorDocumentEdit>,
    stale: Option<PreparedEditorDocumentEdit>,
    evidence: Rc<RefCell<PanelEvidence>>,
}

impl ImguiEditorPanel for PropertyPanel {
    fn show(&mut self, _: &dear_imgui_rs::Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        if let Some(request) = self.request.take() {
            let mut whole_root = request.clone();
            whole_root.target.property.steps.clear();
            assert_eq!(
                context.read_document_property(whole_root),
                Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
            );
            let observed = context.read_document_property(request.clone()).unwrap();
            assert_eq!(observed.target, request.target);
            assert_eq!(observed.value_nodes, 1);
            assert_eq!(observed.value_logical_bytes, 8);
            let EditorPropertyValue::Present(ComponentValue::U64(original)) = observed.value else {
                panic!("the keyed row must return its actual scalar before preparation");
            };
            assert_eq!(original, ORIGINAL);
            let edits = [AuthoringPropertyEdit {
                entity: observed.target.entity.clone(),
                component: observed.target.component.clone(),
                component_version: ComponentSchemaVersion::ONE,
                property: observed.target.property.clone(),
                value: ComponentValue::U64(original.checked_add(2).unwrap()),
            }];
            let cancelled = context
                .prepare_authoring_property_edit(&observed.target.basis, &edits)
                .unwrap();
            assert_eq!(cancelled.document(), observed.target.basis.document);
            assert_eq!(cancelled.base_revision(), observed.target.basis.revision);
            assert_eq!(cancelled.patch().operations.len(), 1);
            assert_eq!(cancelled.inverse().operations.len(), 1);
            drop(cancelled);
            self.prepared = Some(
                context
                    .prepare_authoring_property_edit(&observed.target.basis, &edits)
                    .unwrap(),
            );
            self.stale = Some(
                context
                    .prepare_authoring_property_edit(&observed.target.basis, &edits)
                    .unwrap(),
            );
            self.evidence.borrow_mut().observation = Some(observed);
            return;
        }
        if self.prepared.is_some() {
            let mut evidence = self.evidence.borrow_mut();
            if evidence.held_frames < 2 {
                evidence.held_frames += 1;
                return;
            }
            for candidate in [self.prepared.take().unwrap(), self.stale.take().unwrap()] {
                let submission = context.commit_authoring_edit(candidate).unwrap();
                evidence.submissions.push(submission);
            }
        }
    }

    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        let mut evidence = self.evidence.borrow_mut();
        assert!(evidence.submissions.contains(&id));
        assert!(
            !evidence
                .completions
                .iter()
                .any(|(completed, _)| *completed == id)
        );
        evidence.completions.push((id, result));
    }
}

fn assert_navigation(
    app: &EditorApplication,
    document: EditorDocumentId,
    initial: &EditorFrameModel,
) {
    let current = frame(app);
    assert_eq!(
        current.workspace().documents.len(),
        initial.workspace().documents.len()
    );
    assert_eq!(
        current.workspace().active_document,
        initial.workspace().active_document
    );
    assert_ne!(current.workspace().active_document, Some(document));
    assert_eq!(
        current.workspace().active_scene,
        initial.workspace().active_scene
    );
    assert_eq!(
        current.scene_preview_document(),
        initial.scene_preview_document()
    );
    let tab = current.workspace().document(document).unwrap();
    let original = initial.workspace().document(document).unwrap();
    assert_eq!(tab.kind, original.kind);
    assert_eq!(tab.source_binding, original.source_binding);
    assert_eq!(tab.authoring_schema_basis, original.authoring_schema_basis);
    for retained in initial
        .workspace()
        .documents
        .iter()
        .filter(|tab| tab.document != document)
    {
        assert_eq!(
            current.workspace().document(retained.document),
            Some(retained)
        );
    }
}

#[test]
fn a_panel_reads_a_keyed_scalar_then_commits_it_without_copying_or_activating_its_root() {
    for (kind, source) in [
        (EditorDocumentKind::Scene, "scenes/property-read.scene.json"),
        (
            EditorDocumentKind::Prefab,
            "prefabs/property-read.prefab.json",
        ),
    ] {
        let project = text_project::TextProject::new();
        fs::write(
            project.root.join("schema/authoring.bundle.json"),
            schemas().to_json_string().unwrap(),
        )
        .unwrap();
        let startup = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))])
            .to_json_string()
            .unwrap();
        fs::write(project.root.join("scenes/startup.scene.json"), &startup).unwrap();
        let records = [SceneEntityRecord::new(id("anchor"))
            .with_component(
                ComponentTypeId::new(COMPONENT),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, component_value(ORIGINAL))
                    .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
            )
            .with_component(
                ComponentTypeId::new("consumer.Unknown"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion(7),
                    ComponentValue::map([(
                        "opaque",
                        ComponentValue::String("preserve exactly".into()),
                    )]),
                ),
            )];
        let contents = match kind {
            EditorDocumentKind::Scene => SceneDocument::from_ordered_roots(records)
                .to_json_string()
                .unwrap(),
            EditorDocumentKind::Prefab => PrefabDocument::from_ordered_roots(records)
                .to_json_string()
                .unwrap(),
            _ => unreachable!(),
        };
        fs::write(project.root.join(source), &contents).unwrap();
        let mut app = Application(EditorApplication::new(EditorLaunchOptions::new(
            &project.root,
        )));
        app.native_shell_ready();
        wait(&mut app, |frame| {
            !frame.project_browser().has_pending_work()
        });
        let active = frame(&app).workspace().active_document.unwrap();
        let opening = submit(
            &mut app,
            EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
                relative_path: source.into(),
            }),
        );
        let [
            EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(
                EditorProjectBrowserActionOutcome::Preparing {
                    request: open_request,
                },
            )),
        ] = opening.actions()
        else {
            panic!("named source Open was not admitted: {opening:?}");
        };
        let open_request = *open_request;
        wait(&mut app, |frame| frame.document_open().result().is_some());
        let EditorDocumentOpenResult::Opened {
            request: completed,
            document,
        } = frame(&app).document_open().result().unwrap()
        else {
            panic!("the property source did not open");
        };
        assert_eq!(completed, open_request);
        let acknowledged = submit(
            &mut app,
            EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen {
                request: open_request,
            }),
        );
        assert!(matches!(acknowledged.actions(), [
            EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(
                EditorProjectBrowserActionOutcome::OpenAcknowledged { request: accepted }
            ))
        ] if *accepted == open_request));
        for action in [
            EditorNavigationAction::SetActiveDocument { document: active },
            EditorNavigationAction::SelectEntity {
                document: active,
                entity: Some(id("active")),
            },
        ] {
            assert!(
                matches!(submit(&mut app, EditorAction::Navigate(action)).actions(), [
                EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))
            ] if report.applied)
            );
        }
        let initial = frame(&app);
        assert_eq!(initial.workspace().document(document).unwrap().kind, kind);
        assert!(
            initial
                .workspace()
                .document(document)
                .unwrap()
                .source_binding
                .is_some()
        );
        let captured = basis(&app, document);
        let selected = request(captured.clone(), score_path(ROW_KEY));
        let mut whole_root = selected.clone();
        whole_root.target.property.steps.clear();
        assert_eq!(
            root_read(&mut app, whole_root),
            Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
        );
        let observed = root_read(&mut app, selected.clone()).unwrap();
        assert_eq!(observed.target, selected.target);
        assert_eq!(
            observed.value,
            EditorPropertyValue::Present(ComponentValue::U64(ORIGINAL))
        );
        assert_eq!(observed.value_nodes, 1);
        assert_eq!(observed.value_logical_bytes, 8);
        let metadata = selected.target.entity.as_str().len()
            + selected.target.component.as_str().len()
            + selected.target.property.logical_bytes().unwrap();
        assert_eq!(observed.metadata_bytes, metadata);
        assert!(observed.scanned_entries > 1);
        assert!(observed.scanned_logical_bytes > 1);
        assert!(
            observed.scanned_logical_bytes <= 1024,
            "the unrelated large value is not read"
        );
        let mut short_metadata = selected.clone();
        short_metadata.limits = EditorPropertyReadLimits::new(metadata - 1, 1, 8).unwrap();
        assert_eq!(
            root_read(&mut app, short_metadata),
            Err(EditorDocumentQueryRejection::MetadataBudgetExceeded)
        );

        // Give copied output ample space: only the independent scan budget must refuse these reads.
        for (entries, bytes) in [
            (observed.scanned_entries - 1, observed.scanned_logical_bytes),
            (observed.scanned_entries, observed.scanned_logical_bytes - 1),
        ] {
            let mut exhausted = selected.clone();
            exhausted.limits = EditorPropertyReadLimits::new(metadata, 128, 64 * 1024)
                .unwrap()
                .with_scan_limits(entries, bytes)
                .unwrap();
            assert_eq!(
                root_read(&mut app, exhausted),
                Err(EditorDocumentQueryRejection::Source(
                    AuthoringReadError::Property(ComponentPropertyError::ReadBudgetExceeded)
                ))
            );
        }
        assert_eq!(
            root_read(&mut app, request(captured.clone(), score_path("absent"))),
            Err(EditorDocumentQueryRejection::Source(
                AuthoringReadError::Property(ComponentPropertyError::ListItemNotFound {
                    step: 1,
                    key: ComponentPropertyKey::String("absent".into()),
                })
            ))
        );
        let absent = ComponentPropertyPath::new(ComponentFieldId::new("settings")).with_steps([
            ComponentPropertyStep::Member(ComponentFieldId::new("optional")),
            ComponentPropertyStep::Member(ComponentFieldId::new("score")),
        ]);
        assert_eq!(
            root_read(&mut app, request(captured.clone(), absent)),
            Err(EditorDocumentQueryRejection::Source(
                AuthoringReadError::Property(ComponentPropertyError::OptionalAbsent { step: 1 })
            ))
        );
        let defaulted = ComponentPropertyPath::new(ComponentFieldId::new("defaults")).with_steps([
            ComponentPropertyStep::Member(ComponentFieldId::new("score")),
        ]);
        assert_eq!(
            root_read(&mut app, request(captured.clone(), defaulted))
                .unwrap()
                .value,
            EditorPropertyValue::Defaulted(ComponentValue::U64(13))
        );
        assert_eq!(
            frame(&app).workspace().documents,
            initial.workspace().documents
        );

        let evidence = Rc::new(RefCell::new(PanelEvidence::default()));
        let shared = Rc::clone(&evidence);
        let panel_request = selected.clone();
        let mut composition = EditorNativeComposition::new();
        composition
            .register_panel(
                EditorPanelDescriptor::new(
                    "consumer.selected-property",
                    "Selected property",
                    EditorPanelMultiplicity::Single,
                    EditorPanelPlacement::Floating,
                    true,
                ),
                move || PropertyPanel {
                    request: Some(panel_request.clone()),
                    prepared: None,
                    stale: None,
                    evidence: Rc::clone(&shared),
                },
            )
            .unwrap();
        let mut frontend = composition.install_panels(&mut app).unwrap();
        let mut context = dear_imgui_rs::Context::create();
        context.set_ini_filename::<PathBuf>(None).unwrap();
        context.io_mut().set_display_size([1200.0, 800.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .io_mut()
            .set_config_flags(dear_imgui_rs::ConfigFlags::DOCKING_ENABLE);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        for _ in 0..8 {
            let draw = context.begin_frame();
            let candidate = frontend.show(draw.ui(), &mut app).unwrap();
            drop(draw);
            assert_eq!(
                frame(&app).workspace().documents,
                initial.workspace().documents,
                "synchronous read, cancellation, held candidate and enqueue publish nothing"
            );
            assert!(evidence.borrow().completions.is_empty());
            assert_eq!(
                fs::read_to_string(project.root.join(source)).unwrap(),
                contents
            );
            if let Some(candidate) = candidate {
                frontend.submit_and_drive(&mut app, candidate, Duration::ZERO);
            } else {
                assert_scalar(&mut app, document, ORIGINAL);
            }
            assert_navigation(&app, document, &initial);
            if evidence.borrow().completions.len() == 2 {
                break;
            }
        }
        {
            let retained = evidence.borrow();
            assert_eq!(retained.held_frames, 2);
            assert_eq!(retained.observation.as_ref(), Some(&observed));
            assert_eq!(retained.submissions.len(), 2);
            assert_ne!(retained.submissions[0], retained.submissions[1]);
            let [
                (
                    accepted_id,
                    PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(accepted)),
                ),
                (stale_id, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(stale))),
            ] = retained.completions.as_slice()
            else {
                panic!("the panel did not receive both exact consuming results");
            };
            assert_eq!(*accepted_id, retained.submissions[0]);
            assert_eq!(*stale_id, retained.submissions[1]);
            assert!(accepted.applied, "{:?}", accepted.diagnostics);
            assert_eq!(accepted.document, Some(document));
            assert_eq!(accepted.active_document, Some(active));
            assert!(!stale.applied);
            assert_eq!(stale.document, Some(document));
            assert!(stale.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "tooling.workspace-stale-document-revision"
            }));
        }
        drop(context);
        drop(frontend);
        let edited = frame(&app);
        let tab = edited.workspace().document(document).unwrap();
        assert!(tab.dirty);
        assert_eq!(tab.history.undo_depth, 1);
        assert_eq!(tab.history.redo_depth, 0);
        assert_eq!(tab.revision.source_id(), captured.revision.source_id());
        assert_scalar(&mut app, document, ORIGINAL + 2);
        assert_eq!(
            root_read(&mut app, selected.clone()),
            Err(EditorDocumentQueryRejection::StaleRevision)
        );
        assert_navigation(&app, document, &initial);
        for (redo, value) in [(false, ORIGINAL), (true, ORIGINAL + 2)] {
            let base_revision = basis(&app, document).revision;
            let mutation = if redo {
                EditorDocumentMutation::Redo {
                    document,
                    base_revision,
                }
            } else {
                EditorDocumentMutation::Undo {
                    document,
                    base_revision,
                }
            };
            assert!(
                matches!(submit(&mut app, EditorAction::Mutate(mutation)).actions(), [
                EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))
            ] if report.applied)
            );
            assert_scalar(&mut app, document, value);
            let current = frame(&app);
            let tab = current.workspace().document(document).unwrap();
            assert_eq!(tab.history.undo_depth, usize::from(redo));
            assert_eq!(tab.history.redo_depth, usize::from(!redo));
            assert_eq!(tab.revision.source_id(), captured.revision.source_id());
            assert_navigation(&app, document, &initial);
        }
        let saving = basis(&app, document);
        assert!(matches!(
            submit(
                &mut app,
                EditorAction::Persistence(EditorPersistenceAction::Save {
                    document,
                    base_revision: saving.revision,
                })
            )
            .actions(),
            [EditorApplicationActionDispatch::Dispatched(
                EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
            )]
        ));
        settle_persistence(&mut app);
        assert!(
            matches!(frame(&app).persistence().result(), Some(EditorPersistenceResult::Saved {
            document: saved, revision, ..
        }) if saved == document && revision == saving.revision)
        );
        assert!(!frame(&app).workspace().document(document).unwrap().dirty);
        let saved = fs::read(project.root.join(source)).unwrap();
        let mut expected: serde_json::Value = serde_json::from_str(&contents).unwrap();
        expected["payload"]["entities"][0]["components"][COMPONENT]["value"] =
            serde_json::to_value(component_value(ORIGINAL + 2)).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&saved).unwrap(),
            expected,
            "only the selected score changes; the other row, large subtree and unknown record survive"
        );
        assert_navigation(&app, document, &initial);
        submit(
            &mut app,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
        );
        assert!(matches!(
            submit(
                &mut app,
                EditorAction::Persistence(EditorPersistenceAction::Reopen {
                    target: EditorReopenTarget::OpenDocument {
                        document,
                        base_revision: saving.revision,
                    },
                })
            )
            .actions(),
            [EditorApplicationActionDispatch::Dispatched(
                EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
            )]
        ));
        settle_persistence(&mut app);
        assert!(
            matches!(frame(&app).persistence().result(), Some(EditorPersistenceResult::Opened {
            document: opened, ..
        }) if opened == document)
        );
        let reopened = basis(&app, document);
        assert_ne!(reopened.revision.source_id(), saving.revision.source_id());
        assert_eq!(
            root_read(&mut app, selected),
            Err(EditorDocumentQueryRejection::StaleIncarnation)
        );
        assert_scalar(&mut app, document, ORIGINAL + 2);
        assert_eq!(fs::read(project.root.join(source)).unwrap(), saved);
        assert_eq!(
            fs::read_to_string(project.root.join("scenes/startup.scene.json")).unwrap(),
            startup
        );
        assert_navigation(&app, document, &initial);
        assert!(
            app.close(),
            "the property-read project did not physically retire"
        );
    }
}
