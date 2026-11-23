use super::{
    set::{instance_patch, with_claim_intents},
    *,
};
use nara_tooling::{
    EditorPersistenceRejection, EditorPersistenceRequestKind, EditorPrefabApplyDestination,
};

#[path = "apply/nested_input.rs"]
mod nested_input;
#[path = "apply/structured.rs"]
mod structured;

const INSTANCE_NOTE: &str = "keep this local note";

#[test]
fn a_registered_panel_applies_a_score_override_from_a_nonactive_scene() {
    apply_round_trip(EditorDocumentKind::Scene, SCENE);
}

#[test]
fn a_registered_panel_applies_a_score_override_from_a_nonactive_prefab() {
    apply_round_trip(EditorDocumentKind::Prefab, PREFAB);
}

fn apply_round_trip(kind: EditorDocumentKind, source: &'static str) {
    let mut fixture = ApplyFixture::new(kind, source);
    let original = fixture.stored_sources();
    let containing_before = basis(&fixture.app, fixture.containing);
    let destination_before = basis(&fixture.app, fixture.destination);
    fixture.assert_effective("right", INITIAL_SCORE, false);

    let inspection = fixture.review(INITIAL_SCORE);
    fixture.assert_containing(true, (false, 0, 0));
    fixture.assert_destination(INITIAL_SCORE, (false, 0, 0));
    assert_eq!(basis(&fixture.app, fixture.containing), containing_before);
    assert_eq!(basis(&fixture.app, fixture.destination), destination_before);
    assert_eq!(fixture.stored_sources(), original);

    let published = fixture.commit_destination(inspection, true);
    assert_ne!(published.revision, destination_before.revision);
    fixture.require_destination_save(inspection, published);
    assert_eq!(basis(&fixture.app, fixture.containing), containing_before);
    fixture.assert_containing(true, (false, 0, 0));
    fixture.assert_destination(CHANGED_SCORE, (true, 1, 0));
    assert_eq!(fixture.stored_sources(), original);

    save(&mut fixture.app, fixture.destination);
    let saved_prefix = fixture.stored_sources();
    assert_eq!(saved_prefix.0, original.0);
    assert_stored_destination(&saved_prefix.1);
    assert_eq!(basis(&fixture.app, fixture.containing), containing_before);
    fixture.assert_containing(true, (false, 0, 0));
    fixture.assert_destination(CHANGED_SCORE, (false, 1, 0));

    fixture.finish(inspection, published);
    assert_ne!(
        basis(&fixture.app, fixture.containing).revision,
        containing_before.revision
    );
    fixture.assert_containing(false, (true, 1, 0));
    fixture.assert_destination(CHANGED_SCORE, (false, 1, 0));
    assert_eq!(fixture.stored_sources(), saved_prefix);

    replay(&mut fixture.app, fixture.containing, false);
    fixture.assert_containing(true, (false, 0, 1));
    fixture.assert_destination(CHANGED_SCORE, (false, 1, 0));
    replay(&mut fixture.app, fixture.destination, false);
    fixture.assert_destination(INITIAL_SCORE, (true, 0, 1));
    fixture.assert_effective("left", CHANGED_SCORE, true);
    fixture.assert_effective("right", INITIAL_SCORE, false);
    replay(&mut fixture.app, fixture.destination, true);
    fixture.assert_destination(CHANGED_SCORE, (false, 1, 0));
    fixture.assert_containing(true, (false, 0, 1));
    replay(&mut fixture.app, fixture.containing, true);
    fixture.assert_containing(false, (true, 1, 0));
    assert_eq!(fixture.stored_sources(), saved_prefix);

    fixture.save_containing(&saved_prefix.1);
    reopen(&mut fixture.app, fixture.destination);
    reopen(&mut fixture.app, fixture.containing);
    fixture.assert_containing(false, (false, 0, 0));
    fixture.assert_destination(CHANGED_SCORE, (false, 0, 0));
    fixture.assert_effective("left", CHANGED_SCORE, false);
    fixture.assert_effective("right", CHANGED_SCORE, false);
    assert!(fixture.app.close());
}

#[test]
fn a_registered_panel_retries_apply_after_cancelling_at_the_saved_destination() {
    let mut fixture = ApplyFixture::new(EditorDocumentKind::Scene, SCENE);
    let original = fixture.stored_sources();
    let containing_before = basis(&fixture.app, fixture.containing);
    let destination_before = basis(&fixture.app, fixture.destination);

    let (inspection, address) = fixture.inspect("left", CHANGED_SCORE, true);
    assert_eq!(
        fixture.prepare(inspection, address, fixture.active),
        Outcome::Rejected {
            inspection: Some(inspection),
            reason: Rejection::InvalidDestination,
        }
    );
    expect_phase(&mut fixture.app, inspection, Phase::Ready);
    cancel(&mut fixture.panel, &mut fixture.app, inspection);
    let reviewed = fixture.review(INITIAL_SCORE);
    cancel(&mut fixture.panel, &mut fixture.app, reviewed);
    assert_eq!(basis(&fixture.app, fixture.containing), containing_before);
    assert_eq!(basis(&fixture.app, fixture.destination), destination_before);
    fixture.assert_containing(true, (false, 0, 0));
    fixture.assert_destination(INITIAL_SCORE, (false, 0, 0));
    assert_eq!(fixture.stored_sources(), original);

    let inspection = fixture.review(INITIAL_SCORE);
    let published = fixture.commit_destination(inspection, true);
    save(&mut fixture.app, fixture.destination);
    let saved_prefix = fixture.stored_sources();
    assert_eq!(saved_prefix.0, original.0);
    assert_stored_destination(&saved_prefix.1);
    assert_eq!(
        fixture
            .panel
            .dispatch(&mut fixture.app, |_| Action::Cancel { inspection }),
        Outcome::Accepted {
            inspection,
            phase: Phase::Terminal,
        }
    );
    expect_phase(&mut fixture.app, inspection, Phase::Terminal);
    assert_eq!(
        frame(&fixture.app).prefab_inspection().result,
        Some(EditorPrefabInspectionResult::ApplyStopped {
            inspection,
            destination: published,
            failure: None,
            diagnostics: Default::default(),
        })
    );
    acknowledge(&mut fixture.panel, &mut fixture.app, inspection);
    assert_eq!(basis(&fixture.app, fixture.containing), containing_before);
    fixture.assert_containing(true, (false, 0, 0));
    fixture.assert_destination(CHANGED_SCORE, (false, 1, 0));
    assert_eq!(fixture.stored_sources(), saved_prefix);

    reopen(&mut fixture.app, fixture.containing);
    fixture.assert_containing(true, (false, 0, 0));
    fixture.assert_effective("right", CHANGED_SCORE, false);
    let retry = fixture.review(CHANGED_SCORE);
    assert_ne!(retry, inspection);
    let reused = fixture.commit_destination(retry, false);
    assert_eq!(reused.revision, published.revision);
    fixture.finish(retry, reused);
    fixture.assert_containing(false, (true, 1, 0));
    fixture.assert_destination(CHANGED_SCORE, (false, 1, 0));
    assert_eq!(fixture.stored_sources(), saved_prefix);
    fixture.save_containing(&saved_prefix.1);
    assert!(fixture.app.close());
}

#[test]
fn a_registered_panel_keeps_the_pin_when_the_apply_destination_changes_on_disk() {
    let mut fixture = ApplyFixture::new(EditorDocumentKind::Prefab, PREFAB);
    let initial = fixture.stored_sources();
    let containing_before = basis(&fixture.app, fixture.containing);
    let inspection = fixture.review(INITIAL_SCORE);
    let published = fixture.commit_destination(inspection, true);

    let destination_path = fixture.project.root.join(BASE);
    let external = base(CHANGED_SCORE + 1)
        .to_json_string()
        .unwrap()
        .into_bytes();
    assert_eq!(external.len(), initial.1.len());
    assert_ne!(external, initial.1);
    fs::write(&destination_path, &external).unwrap();
    assert_eq!(
        persist(
            &mut fixture.app,
            EditorPersistenceAction::Save {
                document: published.document,
                base_revision: published.revision,
            },
        ),
        EditorPersistenceResult::Rejected {
            request: EditorPersistenceRequestKind::Save,
            document: Some(published.document),
            reason: EditorPersistenceRejection::TargetChanged,
        }
    );
    expect_phase(&mut fixture.app, inspection, Phase::AwaitingDestinationSave);
    assert_eq!(
        frame(&fixture.app).prefab_inspection().apply_destination,
        Some(published)
    );
    assert_eq!(basis(&fixture.app, fixture.containing), containing_before);
    fixture.assert_containing(true, (false, 0, 0));
    fixture.assert_destination(CHANGED_SCORE, (true, 1, 0));
    let refused_prefix = fixture.stored_sources();
    assert_eq!(refused_prefix.0, initial.0);
    assert_eq!(refused_prefix.1, external);

    fs::write(&destination_path, &initial.1).unwrap();
    save(&mut fixture.app, fixture.destination);
    let saved_prefix = fixture.stored_sources();
    assert_eq!(saved_prefix.0, initial.0);
    assert_stored_destination(&saved_prefix.1);
    assert_eq!(external.len(), saved_prefix.1.len());
    assert_ne!(external, saved_prefix.1);
    fs::write(&destination_path, &external).unwrap();

    fixture.request_finish(inspection);
    let current = frame(&fixture.app);
    let result = &current.prefab_inspection().result;
    let Some(EditorPrefabInspectionResult::ApplyStopped {
        inspection: actual,
        destination,
        failure: Some(Rejection::StaleSource),
        diagnostics,
    }) = result
    else {
        panic!("the changed destination must stop Apply: {result:?}");
    };
    assert_eq!(*actual, inspection);
    assert_eq!(*destination, published);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "project.editor.authoring-source-changed"
        }),
        "{diagnostics:?}"
    );
    acknowledge(&mut fixture.panel, &mut fixture.app, inspection);
    assert_eq!(basis(&fixture.app, fixture.containing), containing_before);
    fixture.assert_containing(true, (false, 0, 0));
    fixture.assert_destination(CHANGED_SCORE, (false, 1, 0));
    assert_eq!(fixture.stored_sources(), (initial.0, external));
    assert!(fixture.app.close());
}

struct ApplyFixture {
    panel: PanelDriver,
    app: Application,
    project: text_project::TextProject,
    kind: EditorDocumentKind,
    source: &'static str,
    active: EditorDocumentId,
    containing: EditorDocumentId,
    destination: EditorDocumentId,
    pinned: PrefabInstance,
    cleared: PrefabInstance,
    sibling: PrefabInstance,
}

impl ApplyFixture {
    fn new(kind: EditorDocumentKind, source: &'static str) -> Self {
        let project = text_project::TextProject::new();
        let asset = AssetRef::path("inspection-base.prefab.json").unwrap();
        let instance = PrefabInstance::root(
            asset.clone(),
            base_key(),
            PrefabEntityTarget::source(id("base-root")),
        );
        let note = PrefabPropertyIntent::Set(ComponentValue::String(INSTANCE_NOTE.into()));
        let pinned = with_claim_intents(
            &instance,
            [
                (
                    "score",
                    PrefabPropertyIntent::Set(ComponentValue::U64(CHANGED_SCORE)),
                ),
                ("note", note.clone()),
            ],
        );
        let cleared = with_claim_intents(&instance, [("note", note)]);
        let sibling = PrefabInstance::forest(asset);
        write_containing_source(&project, kind, source, two_mount_source(&pinned, &sibling));
        for (path, bytes) in [
            (BASE, base(INITIAL_SCORE).to_json_string().unwrap()),
            (
                "schema/authoring.bundle.json",
                schema().to_json_string().unwrap(),
            ),
        ] {
            fs::write(project.root.join(path), bytes).unwrap();
        }
        Self::open(project, kind, source, pinned, cleared, sibling)
    }

    fn open(
        project: text_project::TextProject,
        kind: EditorDocumentKind,
        source: &'static str,
        pinned: PrefabInstance,
        cleared: PrefabInstance,
        sibling: PrefabInstance,
    ) -> Self {
        let startup = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))]);
        fs::write(
            project.root.join(STARTUP),
            startup.to_json_string().unwrap(),
        )
        .unwrap();

        let mut app = Application(EditorApplication::new(EditorLaunchOptions::new(
            &project.root,
        )));
        app.native_shell_ready();
        wait(&mut app, |current| {
            !current.project_browser().has_pending_work()
        });
        let active = frame(&app).workspace().active_document.unwrap();
        let containing = open_source(&mut app, source);
        let destination = open_source(&mut app, BASE);
        for action in [
            EditorNavigationAction::SetActiveDocument { document: active },
            EditorNavigationAction::SelectEntity {
                document: active,
                entity: Some(id("active")),
            },
        ] {
            let result = submit(&mut app, EditorAction::Navigate(action));
            assert!(
                matches!(result.actions(), [
                    EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)),
                ] if report.applied),
                "{result:?}"
            );
        }
        assert_ne!(containing, destination);
        let panel = PanelDriver::register(&mut app);
        Self {
            panel,
            app,
            project,
            kind,
            source,
            active,
            containing,
            destination,
            pinned,
            cleared,
            sibling,
        }
    }

    fn inspect(
        &mut self,
        anchor: &str,
        score: u64,
        pinned: bool,
    ) -> (EditorPrefabInspectionId, EditorPrefabPropertyAddress) {
        inspect_score(
            &mut self.panel,
            &mut self.app,
            self.containing,
            anchor,
            score,
            pinned,
        )
    }

    fn assert_effective(&mut self, anchor: &str, score: u64, pinned: bool) {
        let (inspection, _) = self.inspect(anchor, score, pinned);
        cancel(&mut self.panel, &mut self.app, inspection);
    }

    fn prepare(
        &mut self,
        inspection: EditorPrefabInspectionId,
        address: EditorPrefabPropertyAddress,
        destination: EditorDocumentId,
    ) -> Outcome {
        self.panel
            .dispatch(&mut self.app, |context| Action::PrepareApply {
                inspection,
                address,
                destination: context.document_query_basis(destination).unwrap(),
                limits: Default::default(),
            })
    }

    fn review(&mut self, destination_score: u64) -> EditorPrefabInspectionId {
        let (inspection, address) = self.inspect("left", CHANGED_SCORE, true);
        self.prepare_apply_review(inspection, address);
        let containing_basis = basis(&self.app, self.containing);
        let destination_basis = basis(&self.app, self.destination);
        let patch = instance_patch(&self.pinned, &self.cleared);
        let inverse = instance_patch(&self.cleared, &self.pinned);
        self.panel.observe(&mut self.app, |context| {
            let review = context.prefab_apply_review(inspection).unwrap();
            assert_eq!(review.destination, &destination_basis);
            assert_eq!(review.containing.document_kind(), self.kind);
            assert_eq!(review.containing.patch(), &patch);
            assert_eq!(review.containing.inverse(), &inverse);
            let containing = review.containing.preview();
            assert_eq!(containing.basis(), &containing_basis);
            assert_remaining_note(containing);
            if destination_score != CHANGED_SCORE {
                let edit = review.destination_edit.expect("the base score must change");
                assert_eq!(edit.document_kind(), EditorDocumentKind::Prefab);
                assert_eq!(edit.patch(), &score_patch(CHANGED_SCORE));
                assert_eq!(edit.inverse(), &score_patch(destination_score));
                let destination = edit.preview();
                assert_eq!(destination.basis(), &destination_basis);
                assert_eq!(
                    destination
                        .read_property(score_read(destination_basis.clone()))
                        .unwrap()
                        .value,
                    EditorPropertyValue::Present(ComponentValue::U64(CHANGED_SCORE))
                );
            } else {
                assert!(review.destination_edit.is_none());
            }
        });
        inspection
    }

    fn prepare_apply_review(
        &mut self,
        inspection: EditorPrefabInspectionId,
        address: EditorPrefabPropertyAddress,
    ) {
        assert_eq!(
            self.prepare(inspection, address, self.destination),
            Outcome::Accepted {
                inspection,
                phase: Phase::PreparingEdit,
            }
        );
        expect_phase(&mut self.app, inspection, Phase::Review);
    }

    fn commit(&mut self, inspection: EditorPrefabInspectionId, phase: Phase) {
        assert_eq!(
            self.panel
                .dispatch(&mut self.app, |_| Action::Commit { inspection }),
            Outcome::Accepted {
                inspection,
                phase: Phase::Validating,
            }
        );
        expect_phase(&mut self.app, inspection, phase);
    }

    fn commit_destination(
        &mut self,
        inspection: EditorPrefabInspectionId,
        changed: bool,
    ) -> EditorPrefabApplyDestination {
        self.commit(inspection, Phase::AwaitingDestinationSave);
        let published = frame(&self.app)
            .prefab_inspection()
            .apply_destination
            .unwrap();
        assert_eq!(
            published,
            EditorPrefabApplyDestination {
                document: self.destination,
                revision: basis(&self.app, self.destination).revision,
                changed,
            }
        );
        published
    }

    fn require_destination_save(
        &mut self,
        inspection: EditorPrefabInspectionId,
        destination: EditorPrefabApplyDestination,
    ) {
        assert_eq!(
            self.panel
                .dispatch(&mut self.app, |_| Action::FinishApply { inspection }),
            Outcome::Rejected {
                inspection: Some(inspection),
                reason: Rejection::DestinationSaveRequired,
            }
        );
        expect_phase(&mut self.app, inspection, Phase::AwaitingDestinationSave);
        assert_eq!(
            frame(&self.app).prefab_inspection().apply_destination,
            Some(destination)
        );
    }

    fn request_finish(&mut self, inspection: EditorPrefabInspectionId) {
        assert_eq!(
            self.panel
                .dispatch(&mut self.app, |_| Action::FinishApply { inspection }),
            Outcome::Accepted {
                inspection,
                phase: Phase::Validating,
            }
        );
        expect_phase(&mut self.app, inspection, Phase::Terminal);
    }

    fn finish(
        &mut self,
        inspection: EditorPrefabInspectionId,
        destination: EditorPrefabApplyDestination,
    ) {
        self.request_finish(inspection);
        assert_eq!(
            frame(&self.app).prefab_inspection().result,
            Some(EditorPrefabInspectionResult::Applied {
                inspection,
                document: self.containing,
                revision: basis(&self.app, self.containing).revision,
                destination,
            })
        );
        assert_eq!(
            basis(&self.app, self.destination).revision,
            destination.revision
        );
        acknowledge(&mut self.panel, &mut self.app, inspection);
    }

    fn assert_containing(&mut self, pinned: bool, history: (bool, usize, usize)) {
        let current = frame(&self.app);
        assert_eq!(current.workspace().active_document, Some(self.active));
        assert_eq!(current.scene_preview_document(), Some(self.active));
        let active = current.workspace().active_scene.as_ref().unwrap();
        assert_eq!(active.selection.top_entity(), Some(&id("active")));
        assert_history(&current, self.containing, self.kind, history);
        let left = if pinned { &self.pinned } else { &self.cleared };
        let document = self.containing;
        self.panel.observe(&mut self.app, |context| {
            let basis = context.document_query_basis(document).unwrap();
            for (anchor, expected) in [("left", left), ("right", &self.sibling)] {
                let observation = context
                    .observe_prefab_instance(
                        &basis,
                        &PrefabInstancePath::new(id(anchor)),
                        Default::default(),
                    )
                    .unwrap();
                assert_eq!(observation.instance(), Some(expected));
            }
        });
    }

    fn assert_destination(&mut self, score: u64, history: (bool, usize, usize)) {
        self.assert_destination_history(history);
        let document = self.destination;
        self.panel.observe(&mut self.app, |context| {
            let basis = context.document_query_basis(document).unwrap();
            let score = EditorPropertyValue::Present(ComponentValue::U64(score));
            assert_eq!(
                context
                    .read_document_property(score_read(basis))
                    .unwrap()
                    .value,
                score
            );
        });
    }

    fn assert_destination_history(&self, expected: (bool, usize, usize)) {
        assert_history(
            &frame(&self.app),
            self.destination,
            EditorDocumentKind::Prefab,
            expected,
        );
    }

    fn stored_sources(&self) -> (Vec<u8>, Vec<u8>) {
        (
            fs::read(self.project.root.join(self.source)).unwrap(),
            fs::read(self.project.root.join(BASE)).unwrap(),
        )
    }

    fn save_containing(&mut self, saved_destination: &[u8]) {
        save(&mut self.app, self.containing);
        let (containing, destination) = self.stored_sources();
        assert_saved_source(
            self.kind,
            &containing,
            &two_mount_source(&self.cleared, &self.sibling),
        );
        assert_eq!(destination.as_slice(), saved_destination);
    }
}

fn write_containing_source(
    project: &text_project::TextProject,
    kind: EditorDocumentKind,
    source: &str,
    containing: SceneDocument,
) {
    let bytes = match kind {
        EditorDocumentKind::Scene => containing.to_json_string(),
        EditorDocumentKind::Prefab => {
            PrefabDocument::new(containing.placements, containing.entities).to_json_string()
        }
        EditorDocumentKind::Extension(_) => unreachable!(),
    }
    .unwrap();
    fs::write(project.root.join(source), bytes).unwrap();
}

fn two_mount_source(left: &PrefabInstance, right: &PrefabInstance) -> SceneDocument {
    let mut left_anchor = SceneEntityRecord::new(id("left")).with_component(
        ComponentTypeId::new(ANCHOR_COMPONENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("wire_text", ComponentValue::String("left anchor".into()))]),
        )
        .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
    );
    left_anchor.prefab = Some(left.clone());
    let mut right_anchor = SceneEntityRecord::new(id("right"));
    right_anchor.prefab = Some(right.clone());
    SceneDocument::from_ordered_roots([left_anchor, right_anchor])
}

fn score_patch(score: u64) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id("base-root"),
        component: ComponentTypeId::new(COMPONENT),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("score"),
        value: ComponentValue::U64(score),
    }])
}

fn score_read(basis: EditorDocumentQueryBasis) -> EditorPropertyRead {
    EditorPropertyRead {
        target: EditorPropertyTarget {
            basis,
            entity: id("base-root"),
            component: ComponentTypeId::new(COMPONENT),
            property: property("score"),
        },
        limits: EditorPropertyReadLimits::new(1024, 1, 128).unwrap(),
    }
}

fn assert_remaining_note(preview: EditorPreparedDocumentPreview<'_>) {
    let page = preview
        .query_authoring(EditorAuthoringQuery {
            position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                basis: preview.basis().clone(),
                entity: id("left"),
                projection: EditorAuthoringProjection::PrefabPropertyClaims { steps: vec![] },
            }),
            limits: EditorAuthoringQueryLimits::new(8, 8, 4096).unwrap(),
        })
        .unwrap();
    assert!(page.next.is_none());
    let [
        EditorAuthoringRow::PrefabPropertyClaim {
            claim,
            property: retained,
            ..
        },
    ] = page.rows.as_slice()
    else {
        panic!(
            "the candidate must retain exactly the local note claim: {:?}",
            page.rows
        );
    };
    assert_eq!(retained, &property("note"));
    assert_eq!(
        preview
            .read_prefab_property_claim(claim, EditorPropertyReadLimits::new(1024, 1, 128).unwrap())
            .unwrap(),
        PrefabPropertyIntent::Set(ComponentValue::String(INSTANCE_NOTE.into()))
    );
}

fn assert_history(
    frame: &EditorFrameModel,
    document: EditorDocumentId,
    kind: EditorDocumentKind,
    expected: (bool, usize, usize),
) {
    let tab = frame.workspace().document(document).unwrap();
    assert!(!tab.active);
    assert_eq!(tab.kind, kind);
    assert!(tab.source_binding.is_some());
    assert_eq!(
        (tab.dirty, tab.history.undo_depth, tab.history.redo_depth),
        expected
    );
}

fn assert_stored_destination(bytes: &[u8]) {
    let stored = PrefabDocumentCandidate::decode_json_bytes(bytes)
        .unwrap()
        .into_composition_source()
        .unwrap();
    assert_eq!(stored, base(CHANGED_SCORE));
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
        matches!(result, EditorPersistenceResult::Opened { document: opened, revision: actual, .. }
            if opened == document && actual == revision),
        "{result:?}"
    );
    assert_ne!(revision.source_id(), base_revision.source_id());
}
