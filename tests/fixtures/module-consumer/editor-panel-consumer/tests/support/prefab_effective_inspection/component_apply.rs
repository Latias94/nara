use std::cell::Cell;

use nara::scene::{
    AuthoringPrefabComponentOverrideKind as Kind, PrefabComponentSetMode, SourceDocumentOrigin,
};
use nara_tooling::{
    EditorPlayState, EditorPrefabApplyDestination, EditorPrefabComponentAddress,
    EditorWorkspaceIntent, EditorWorkspaceIntentAction, EditorWorkspaceIntentRejection,
    EditorWorkspaceIntentRequestResult,
};

use super::{set::instance_patch, *};

const MARKER: &str = "history-marker";
const INITIAL_MARKER_SCORE: u64 = 30;
const SAVED_MARKER_SCORE: u64 = 31;

/// Hands control to a process test after the containing output is accepted, before another drive.
pub(super) fn at_first_added_component_apply_output(
    stop: impl FnOnce(&std::path::Path, &[Vec<u8>; 2], &[Vec<u8>; 2]),
) {
    let mut fixture = Fixture::new(EditorDocumentKind::Scene, SCENE, Kind::Added);
    let originals = fixture.files();
    let (inspection, outputs) = fixture.review(&originals);
    assert_eq!(
        fixture.request_commit(inspection),
        Outcome::Accepted {
            inspection,
            phase: Phase::Validating,
        }
    );
    wait(&mut fixture.app, |current| {
        let view = current.prefab_inspection();
        assert_eq!(view.inspection, Some(inspection));
        match (view.phase, view.component_apply_published) {
            (Phase::Validating, None) | (Phase::PublishingComponentApply, Some(0)) => false,
            (Phase::PublishingComponentApply, Some(1)) => true,
            _ => panic!("expected the first component Apply output: {view:?}"),
        }
    });
    let [containing, destination] = fixture.files();
    assert_eq!(containing, outputs[0]);
    assert_eq!(destination, originals[1]);
    stop(&fixture.project.root, &originals, &outputs);
}

#[test]
fn a_registered_panel_applies_an_added_component_from_a_nonactive_scene() {
    round_trip(EditorDocumentKind::Scene, SCENE, Kind::Added);
}

#[test]
fn a_registered_panel_applies_a_replaced_component_from_a_nonactive_prefab() {
    round_trip(EditorDocumentKind::Prefab, PREFAB, Kind::Replaced);
}

#[test]
fn a_registered_panel_applies_a_suppressed_component_from_a_nonactive_scene() {
    round_trip(EditorDocumentKind::Scene, SCENE, Kind::Suppressed);
}

#[test]
fn a_registered_panel_cancels_component_apply_and_refuses_destination_drift() {
    let mut fixture = Fixture::new(EditorDocumentKind::Prefab, PREFAB, Kind::Replaced);
    let originals = fixture.files();
    let manifest_path = fixture.project.root.join("nara.toml");
    let original_manifest = fs::read(&manifest_path).unwrap();
    let bases = fixture.bases();

    let (inspection, _) = fixture.review(&originals);
    cancel(&mut fixture.panel, &mut fixture.app, inspection);
    assert_eq!(fixture.files(), originals);
    assert_eq!(fixture.bases(), bases);
    fixture.assert_histories((false, 1, 1));
    assert_eq!(fs::read(&manifest_path).unwrap(), original_manifest);

    let (inspection, _) = fixture.review(&originals);
    let mut external = originals[1].clone();
    let old_note = b"inherited record";
    let offset = external
        .windows(old_note.len())
        .position(|bytes| bytes == old_note)
        .unwrap();
    external[offset] = b'I';
    fs::write(fixture.project.root.join(BASE), &external).unwrap();
    match fixture.request_commit(inspection) {
        Outcome::Accepted {
            inspection: actual,
            phase: Phase::Validating,
        } => {
            assert_eq!(actual, inspection);
            expect_phase(&mut fixture.app, inspection, Phase::Terminal);
            let current = frame(&fixture.app);
            let view = current.prefab_inspection();
            assert!(
                matches!(
                    &view.result,
                    Some(EditorPrefabInspectionResult::Failed {
                        inspection: actual,
                        reason: Rejection::StaleSource,
                        ..
                    }) if *actual == inspection
                ),
                "{view:?}"
            );
            acknowledge(&mut fixture.panel, &mut fixture.app, inspection);
        }
        Outcome::Rejected {
            inspection: Some(actual),
            reason: Rejection::StaleSource,
        } => {
            assert_eq!(actual, inspection);
            expect_phase(&mut fixture.app, inspection, Phase::Review);
            cancel(&mut fixture.panel, &mut fixture.app, inspection);
        }
        outcome => panic!("destination drift must refuse confirmation: {outcome:?}"),
    }
    assert_eq!(fixture.files(), [originals[0].clone(), external]);
    assert_eq!(fixture.bases(), bases);
    fixture.assert_histories((false, 1, 1));
    assert_eq!(fs::read(&manifest_path).unwrap(), original_manifest);
    assert!(fixture.app.close());
}

fn round_trip(document_kind: EditorDocumentKind, source_path: &'static str, kind: Kind) {
    let mut fixture = Fixture::new(document_kind, source_path, kind);
    let originals = fixture.files();
    let bases = fixture.bases();
    let original_record = fixture.inherited.clone();
    let result_record = fixture.promoted.clone();
    fixture.assert_inherited("right", original_record.as_ref());
    let (inspection, outputs) = fixture.review(&originals);
    assert_eq!(fixture.bases(), bases);
    fixture.commit(inspection, &bases);
    fixture.assert_histories((false, 0, 0));
    assert_eq!(fixture.files(), outputs);
    for anchor in ["left", "right"] {
        fixture.assert_inherited(anchor, result_record.as_ref());
    }

    for document in [fixture.containing, fixture.destination] {
        fixture.assert_marker(document, SAVED_MARKER_SCORE);
        let checkpoint = basis(&fixture.app, document).revision;
        edit_marker(&mut fixture.app, document, 73);
        assert_eq!(
            basis(&fixture.app, document).revision.source_id(),
            checkpoint.source_id()
        );
        fixture.assert_marker(document, 73);
        fixture.assert_history(document, (true, 1, 0));
        replay(&mut fixture.app, document, false);
        fixture.assert_marker(document, SAVED_MARKER_SCORE);
        fixture.assert_history(document, (false, 0, 1));
        reopen(&mut fixture.app, document);
        fixture.assert_history(document, (false, 0, 0));
        fixture.assert_marker(document, SAVED_MARKER_SCORE);
    }
    for anchor in ["left", "right"] {
        fixture.assert_inherited(anchor, result_record.as_ref());
    }
    assert_eq!(fixture.files(), outputs);
    assert!(fixture.app.close());
}

struct Fixture {
    panel: PanelDriver,
    app: Application,
    project: text_project::TextProject,
    document_kind: EditorDocumentKind,
    source_path: &'static str,
    active: EditorDocumentId,
    containing: EditorDocumentId,
    destination: EditorDocumentId,
    kind: Kind,
    overridden: PrefabInstance,
    cleared: PrefabInstance,
    sibling: PrefabInstance,
    inherited: Option<SceneComponentRecord>,
    promoted: Option<SceneComponentRecord>,
}

impl Fixture {
    fn new(document_kind: EditorDocumentKind, source_path: &'static str, kind: Kind) -> Self {
        let project = text_project::TextProject::new();
        let inherited = (kind != Kind::Added).then(|| record(INITIAL_SCORE, "inherited record"));
        let promoted =
            (kind != Kind::Suppressed).then(|| record(CHANGED_SCORE, "whole promoted record"));
        let asset = AssetRef::path("inspection-base.prefab.json").unwrap();
        let target = PrefabEntityTarget::source(id("base-root"));
        let sibling = PrefabInstance::forest(asset);
        let mut cleared = sibling.clone();
        let declaration = match kind {
            Kind::Added | Kind::Replaced => PrefabComponentIntent::Set {
                mode: if kind == Kind::Added {
                    PrefabComponentSetMode::Add
                } else {
                    PrefabComponentSetMode::Replace
                },
                target,
                component: ComponentTypeId::new(COMPONENT),
                record: promoted.clone().unwrap(),
            },
            Kind::Suppressed => PrefabComponentIntent::Suppress {
                target,
                component: ComponentTypeId::new(COMPONENT),
            },
            Kind::Properties => unreachable!("this fixture promotes whole components"),
        };
        cleared.overrides = Some(PrefabOverlay::new(base_key()));
        let mut overridden = cleared.clone();
        overridden
            .overrides
            .as_mut()
            .unwrap()
            .components
            .push(declaration);
        let scene = containing_document(&overridden, &sibling, INITIAL_MARKER_SCORE);
        let containing_json = match document_kind {
            EditorDocumentKind::Scene => scene.to_json_string(),
            EditorDocumentKind::Prefab => {
                PrefabDocument::new(scene.placements, scene.entities).to_json_string()
            }
            EditorDocumentKind::Extension(_) => unreachable!(),
        }
        .unwrap();
        let startup = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))]);
        let schema_path = "schema/authoring.bundle.json";
        let runtime_bundle = AuthoringSchemaBundle::from_json_bytes_with_limits(
            &fs::read(project.root.join(schema_path)).unwrap(),
            Default::default(),
        )
        .unwrap();
        let authoring_bundle = schema();
        let bundle = AuthoringSchemaBundle::from_owner_snapshots(
            runtime_bundle
                .owners()
                .iter()
                .chain(authoring_bundle.owners())
                .cloned(),
        )
        .unwrap();
        for (path, bytes) in [
            (source_path, containing_json),
            (
                BASE,
                destination_document_at(inherited.as_ref(), INITIAL_MARKER_SCORE)
                    .to_json_string()
                    .unwrap(),
            ),
            (STARTUP, startup.to_json_string().unwrap()),
            (schema_path, bundle.to_json_string().unwrap()),
        ] {
            fs::write(project.root.join(path), bytes).unwrap();
        }
        let mut app = Application(EditorApplication::new(EditorLaunchOptions::new(
            &project.root,
        )));
        app.native_shell_ready();
        wait(&mut app, |current| {
            !current.project_browser().has_pending_work()
        });
        let active = frame(&app).workspace().active_document.unwrap();
        let containing = open_source(&mut app, source_path);
        let destination = open_source(&mut app, BASE);
        let activation = submit(
            &mut app,
            EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: active }),
        );
        assert!(
            matches!(activation.actions(), [
                EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)),
            ] if report.applied),
            "{activation:?}"
        );
        for document in [containing, destination] {
            edit_marker(&mut app, document, SAVED_MARKER_SCORE);
            edit_marker(&mut app, document, SAVED_MARKER_SCORE + 1);
            replay(&mut app, document, false);
            save(&mut app, document);
        }
        let panel = PanelDriver::register(&mut app);
        let fixture = Self {
            panel,
            app,
            project,
            document_kind,
            source_path,
            active,
            containing,
            destination,
            kind,
            overridden,
            cleared,
            sibling,
            inherited,
            promoted,
        };
        fixture.assert_histories((false, 1, 1));
        fixture
    }

    fn bases(&self) -> [EditorDocumentQueryBasis; 2] {
        [
            basis(&self.app, self.containing),
            basis(&self.app, self.destination),
        ]
    }

    fn files(&self) -> [Vec<u8>; 2] {
        [self.source_path, BASE].map(|path| fs::read(self.project.root.join(path)).unwrap())
    }

    fn inspect(
        &mut self,
        anchor: &str,
        kind: Option<Kind>,
        expected: Option<&SceneComponentRecord>,
    ) -> (EditorPrefabInspectionId, EditorPrefabComponentAddress) {
        let inspection = start(&mut self.panel, &mut self.app, self.containing, anchor);
        expect_phase(&mut self.app, inspection, Phase::Ready);
        let address = self.panel.observe(&mut self.app, |context| {
            let snapshot = context.prefab_effective_observation(inspection).unwrap();
            let route = address(anchor, "score");
            let occurrence = snapshot.target(&route.entity).unwrap();
            let declaration = occurrence.component_override(&route.component);
            assert_eq!(declaration.map(|item| item.kind()), kind);
            let origin = AuthoringPrefabRecordOrigin::SourceEntity {
                source: PrefabDeclarationSource::Prefab(base_key()),
                entity: id("base-root"),
            };
            if let Some(declaration) = declaration {
                let underlying =
                    matches!(kind, Some(Kind::Replaced | Kind::Suppressed)).then_some(&origin);
                assert_eq!(declaration.underlying_record(), underlying);
            }
            let view = occurrence.component(&route.component);
            if let Some(expected) = expected {
                let view = view.unwrap();
                if kind.is_none() {
                    assert_eq!(view.fact().record(), &origin);
                }
                let mut budget = ComponentPropertyReadBudget::new(
                    ItemLimit::new(8).unwrap(),
                    ByteLimit::new(1024).unwrap(),
                );
                let limits = ComponentValueMaterializationLimits::new(
                    ItemLimit::ONE,
                    ByteLimit::new(128).unwrap(),
                    DepthLimit::ONE,
                );
                for (field, storage) in [("score", "wire_score"), ("note", "wire_note")] {
                    let observed = view.property_value(&property(field), &mut budget).unwrap();
                    assert!(observed.controlling_claim().is_none());
                    let AuthoringPrefabPropertyValue::Present(value) = observed.value else {
                        panic!("the effective component must retain {field}");
                    };
                    assert_eq!(
                        &value.copy_value(limits).unwrap(),
                        expected.value().get(storage).unwrap()
                    );
                }
            } else {
                assert_eq!(view.unwrap_err(), AuthoringReadError::ComponentNotFound);
            }
            EditorPrefabComponentAddress {
                entity: route.entity,
                component: route.component,
            }
        });
        (inspection, address)
    }

    fn assert_inherited(&mut self, anchor: &str, value: Option<&SceneComponentRecord>) {
        let (inspection, _) = self.inspect(anchor, None, value);
        cancel(&mut self.panel, &mut self.app, inspection);
    }

    fn review(&mut self, originals: &[Vec<u8>; 2]) -> (EditorPrefabInspectionId, [Vec<u8>; 2]) {
        let promoted = self.promoted.clone();
        let (inspection, address) = self.inspect("left", Some(self.kind), promoted.as_ref());
        let destination = self.destination;
        assert_eq!(
            self.panel
                .dispatch(&mut self.app, |context| Action::PrepareComponentApply {
                    inspection,
                    destination_route: address.entity.clone(),
                    address,
                    destination: context.document_query_basis(destination).unwrap(),
                    limits: Default::default(),
                }),
            Outcome::Accepted {
                inspection,
                phase: Phase::PreparingEdit,
            }
        );
        expect_phase(&mut self.app, inspection, Phase::Review);
        let bases = self.bases();
        let kind = self.kind;
        let source_path = self.source_path;
        let document_kind = self.document_kind;
        let keys = [containing_key(document_kind, source_path), base_key()];
        let patches = [
            instance_patch(&self.overridden, &self.cleared),
            destination_patch(kind, promoted.as_ref()),
        ];
        let outputs = self.panel.observe(&mut self.app, |context| {
            let review = context.prefab_component_apply_review(inspection).unwrap();
            assert_eq!(review.kind, kind);
            for (index, file) in review.files.iter().enumerate() {
                assert_eq!(file.basis, &bases[index]);
                assert_eq!(file.source, &keys[index]);
                assert_eq!(file.project_path, [source_path, BASE][index]);
                assert_eq!(file.original_bytes, originals[index].as_slice());
                assert_ne!(file.output_bytes, file.original_bytes);
                assert_eq!(file.patch, &patches[index]);
            }
            review.files.map(|file| file.output_bytes.to_vec())
        });
        assert_saved_source(
            document_kind,
            &outputs[0],
            &containing_document(&self.cleared, &self.sibling, SAVED_MARKER_SCORE),
        );
        assert_eq!(
            PrefabDocumentCandidate::decode_json_bytes(&outputs[1])
                .unwrap()
                .into_composition_source()
                .unwrap(),
            destination_document(promoted.as_ref()),
        );
        assert_eq!(&self.files(), originals);
        assert_eq!(self.bases(), bases);
        self.assert_histories((false, 1, 1));
        (inspection, outputs)
    }

    fn request_commit(&mut self, inspection: EditorPrefabInspectionId) -> Outcome {
        let current = frame(&self.app);
        assert_eq!(current.persistence(), EditorPersistenceView::Idle);
        assert_eq!(current.play().state(), EditorPlayState::Empty);
        self.panel
            .dispatch(&mut self.app, |_| Action::Commit { inspection })
    }

    fn commit(
        &mut self,
        inspection: EditorPrefabInspectionId,
        bases: &[EditorDocumentQueryBasis; 2],
    ) {
        assert_eq!(
            self.request_commit(inspection),
            Outcome::Accepted {
                inspection,
                phase: Phase::Validating,
            }
        );
        wait(&mut self.app, |current| {
            let view = current.prefab_inspection();
            assert!(view.phase.work_pending(), "{view:?}");
            view.component_apply_published == Some(1)
        });
        let close = submit(
            &mut self.app,
            EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
                EditorWorkspaceIntent::CloseDocument {
                    document: self.destination,
                },
            )),
        );
        assert!(
            matches!(
                close.actions(),
                [EditorApplicationActionDispatch::Dispatched(
                    EditorActionOutcome::WorkspaceIntent(
                        EditorWorkspaceIntentRequestResult::Rejected(
                            EditorWorkspaceIntentRejection::Busy
                        )
                    )
                )]
            ),
            "{close:?}"
        );
        let published = Cell::new(Some(1));
        wait(&mut self.app, |current| {
            let view = current.prefab_inspection();
            assert_eq!(view.inspection, Some(inspection));
            match view.phase {
                Phase::Validating => false,
                Phase::PublishingComponentApply => {
                    let count = view.component_apply_published.unwrap();
                    assert!(count <= 2);
                    assert!(published.get().is_none_or(|previous| previous <= count));
                    published.set(Some(count));
                    false
                }
                Phase::Terminal => true,
                _ => panic!("unexpected component Apply publication phase: {view:?}"),
            }
        });
        let current = frame(&self.app);
        let current_bases = self.bases();
        assert_eq!(
            current.prefab_inspection().result,
            Some(EditorPrefabInspectionResult::ComponentApplied {
                inspection,
                document: self.containing,
                revision: current_bases[0].revision,
                destination: EditorPrefabApplyDestination {
                    document: self.destination,
                    revision: current_bases[1].revision,
                    changed: true,
                },
                kind: self.kind,
            })
        );
        assert_eq!(published.get(), Some(2));
        for (after, before) in current_bases.iter().zip(bases) {
            assert_ne!(after.revision.source_id(), before.revision.source_id());
            assert_eq!(after.schema, before.schema);
        }
        acknowledge(&mut self.panel, &mut self.app, inspection);
    }

    fn assert_history(&self, document: EditorDocumentId, expected: (bool, usize, usize)) {
        let current = frame(&self.app);
        assert_eq!(current.workspace().active_document, Some(self.active));
        assert_eq!(current.scene_preview_document(), Some(self.active));
        let tab = current.workspace().document(document).unwrap();
        assert!(!tab.active);
        assert_eq!(
            (tab.dirty, tab.history.undo_depth, tab.history.redo_depth),
            expected
        );
    }

    fn assert_histories(&self, expected: (bool, usize, usize)) {
        for document in [self.containing, self.destination] {
            self.assert_history(document, expected);
        }
    }

    fn assert_marker(&mut self, document: EditorDocumentId, score: u64) {
        self.panel.observe(&mut self.app, |context| {
            let value = context
                .read_document_property(EditorPropertyRead {
                    target: EditorPropertyTarget {
                        basis: context.document_query_basis(document).unwrap(),
                        entity: id(MARKER),
                        component: ComponentTypeId::new(COMPONENT),
                        property: property("score"),
                    },
                    limits: EditorPropertyReadLimits::new(1024, 1, 128).unwrap(),
                })
                .unwrap()
                .value;
            assert_eq!(
                value,
                EditorPropertyValue::Present(ComponentValue::U64(score))
            );
        });
    }
}

fn containing_key(kind: EditorDocumentKind, source: &str) -> SourceDocumentKey {
    let base = base_key();
    let SourceDocumentOrigin::ProjectLocation { universe, .. } = base.origin() else {
        unreachable!();
    };
    let kind = match kind {
        EditorDocumentKind::Scene => SourceDocumentKind::Scene,
        EditorDocumentKind::Prefab => SourceDocumentKind::Prefab,
        EditorDocumentKind::Extension(_) => unreachable!(),
    };
    SourceDocumentKey::project_location(*universe, AssetSourcePath::new(source).unwrap(), kind)
}

fn containing_document(left: &PrefabInstance, right: &PrefabInstance, score: u64) -> SceneDocument {
    let mut left_anchor = SceneEntityRecord::new(id("left"));
    left_anchor.prefab = Some(left.clone());
    let mut right_anchor = SceneEntityRecord::new(id("right"));
    right_anchor.prefab = Some(right.clone());
    SceneDocument::from_ordered_roots([left_anchor, right_anchor, marker(score)])
}

fn destination_document(value: Option<&SceneComponentRecord>) -> PrefabDocument {
    destination_document_at(value, SAVED_MARKER_SCORE)
}

fn destination_document_at(value: Option<&SceneComponentRecord>, score: u64) -> PrefabDocument {
    let mut root = SceneEntityRecord::new(id("base-root"));
    if let Some(value) = value {
        root = root.with_component(ComponentTypeId::new(COMPONENT), value.clone());
    }
    PrefabDocument::from_ordered_roots([root, marker(score)])
}

fn marker(score: u64) -> SceneEntityRecord {
    SceneEntityRecord::new(id(MARKER)).with_component(
        ComponentTypeId::new(COMPONENT),
        record(score, "unchanged marker"),
    )
}

fn record(score: u64, note: &str) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            ("wire_score", ComponentValue::U64(score)),
            ("wire_note", ComponentValue::String(note.into())),
        ]),
    )
    .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap())
}

fn destination_patch(kind: Kind, record: Option<&SceneComponentRecord>) -> ScenePatchDocument {
    let entity = id("base-root");
    let component = ComponentTypeId::new(COMPONENT);
    ScenePatchDocument::new([match kind {
        Kind::Added => ScenePatchOperation::AddComponent {
            entity,
            component,
            value: record.unwrap().clone(),
        },
        Kind::Replaced => ScenePatchOperation::ReplaceComponent {
            entity,
            component,
            value: record.unwrap().clone(),
        },
        Kind::Suppressed => ScenePatchOperation::RemoveComponent { entity, component },
        Kind::Properties => unreachable!("this fixture promotes whole components"),
    }])
}

fn edit_marker(app: &mut EditorApplication, document: EditorDocumentId, score: u64) {
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id(MARKER),
        component: ComponentTypeId::new(COMPONENT),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("score"),
        value: ComponentValue::U64(score),
    }]);
    let edit = app
        .prepare_authoring_edit(&basis(app, document), &patch)
        .unwrap();
    let result = app.commit_authoring_edit(edit, Duration::ZERO);
    assert!(
        matches!(result.actions(), [
            EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))
        ] if report.applied && report.document == Some(document)),
        "{result:?}"
    );
}

fn reopen(app: &mut EditorApplication, document: EditorDocumentId) {
    let base_revision = basis(app, document).revision;
    let result = persist(
        app,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision,
            },
        },
    );
    let revision = basis(app, document).revision;
    assert!(
        matches!(result, EditorPersistenceResult::Opened { document: actual, revision: reopened, .. }
            if actual == document && reopened == revision),
        "{result:?}"
    );
    assert_ne!(revision.source_id(), base_revision.source_id());
}
