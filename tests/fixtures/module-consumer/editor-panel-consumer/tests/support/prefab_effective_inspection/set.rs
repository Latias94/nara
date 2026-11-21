use super::*;

use nara::{
    reflect::{ComponentPropertyStep, ComponentShapeMember, ComponentValueShape},
    scene::{AuthoringFieldEdit, PrefabAddition},
};
use nara_tooling::EditorDocumentSourceBinding;

const SCORE_INCREMENT: u64 = 7;
const INITIAL_NOTE: &str = "base note";
const CHANGED_NOTE: &str = "next note";
const INITIAL_RANK: u64 = 10;
const CHANGED_RANK: u64 = 11;
const UI_TEXT: &str = "nara.ui.UiText";

fn set_schema() -> AuthoringSchemaBundle {
    let schema = schema();
    let owner = &schema.owners()[0];
    let mut catalog = owner.current().clone();
    let settings = catalog
        .components
        .iter_mut()
        .find(|component| component.id().as_str() == COMPONENT)
        .unwrap();
    settings.fields.extend([
        ComponentFieldSchema::required(
            ComponentFieldId::new("rank"),
            "Rank",
            ComponentFieldPath::from_fields(["wire_rank"]),
            ComponentValueKind::U64,
        )
        .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect]),
        ComponentFieldSchema::required(
            ComponentFieldId::new("profile"),
            "Profile",
            ComponentFieldPath::from_fields(["wire_profile"]),
            ComponentValueShape::record_members([ComponentShapeMember::new(
                "wire_level",
                ComponentValueKind::U64,
            )
            .with_id(ComponentFieldId::new("level"))]),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::optional(
            ComponentFieldId::new("optional"),
            "Optional score",
            ComponentFieldPath::from_fields(["wire_optional"]),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ]);
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.owner_id().clone(),
        catalog,
        None,
    )])
    .unwrap()
}

fn profile(level: u64) -> ComponentValue {
    ComponentValue::map([("wire_level", ComponentValue::U64(level))])
}

fn base_source(score: u64, note: &str, rank: u64) -> String {
    PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("base-root")).with_component(
        ComponentTypeId::new(COMPONENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([
                ("wire_score", ComponentValue::U64(score)),
                ("wire_note", ComponentValue::String(note.into())),
                ("wire_rank", ComponentValue::U64(rank)),
                ("wire_profile", profile(1)),
            ]),
        )
        .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
    )])
    .to_json_string()
    .unwrap()
}

fn with_claims(
    instance: &PrefabInstance,
    claims: impl IntoIterator<Item = (&'static str, ComponentValue)>,
) -> PrefabInstance {
    let mut instance = instance.clone();
    let mut overlay = PrefabOverlay::new(base_key());
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("base-root")),
        component: ComponentTypeId::new(COMPONENT),
        version: ComponentSchemaVersion::ONE,
        claims: claims
            .into_iter()
            .map(|(field, value)| PrefabPropertyClaim {
                path: property(field),
                intent: PrefabPropertyIntent::Set(value),
            })
            .collect(),
    });
    instance.overrides = Some(overlay);
    instance
}

fn set_source(
    kind: EditorDocumentKind,
    left: &PrefabInstance,
    right: &PrefabInstance,
) -> SceneDocument {
    let mut document = containing_source(left, right);
    if kind == EditorDocumentKind::Prefab {
        let unrelated = document
            .entities
            .iter_mut()
            .find(|entity| entity.id == id("unrelated"))
            .unwrap();
        unrelated.components.insert(
            ComponentTypeId::new(UI_TEXT),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(
                    "content",
                    ComponentValue::String("preserved UI text".into()),
                )]),
            )
            .with_owner(PersistentComponentSchemaOwnerId::try_new("nara.ui.components").unwrap()),
        );
    }
    document
}

fn workspace_action(app: &mut EditorApplication, action: EditorAction) {
    let result = submit(app, action);
    assert!(
        matches!(result.actions(), [
            EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)),
        ] if report.applied),
        "{result:?}"
    );
}

struct Fixture {
    panel: PanelDriver,
    app: Application,
    project: text_project::TextProject,
    document: EditorDocumentId,
    active: EditorDocumentId,
    source: &'static str,
    source_binding: EditorDocumentSourceBinding,
    original: String,
    initial_base: String,
    left: PrefabInstance,
    right: PrefabInstance,
}

impl Fixture {
    fn new(kind: EditorDocumentKind, source: &'static str) -> Self {
        let left = PrefabInstance::root(
            AssetRef::path("inspection-base.prefab.json").unwrap(),
            base_key(),
            PrefabEntityTarget::source(id("base-root")),
        );
        Self::with_left(text_project::TextProject::new(), kind, source, left)
    }

    fn with_left(
        project: text_project::TextProject,
        kind: EditorDocumentKind,
        source: &'static str,
        left: PrefabInstance,
    ) -> Self {
        let right = with_claims(
            &PrefabInstance::forest(left.source.clone()),
            [("profile", profile(3))],
        );
        let original = set_source(kind, &left, &right);
        let original = match kind {
            EditorDocumentKind::Scene => original.to_json_string(),
            EditorDocumentKind::Prefab => {
                PrefabDocument::new(original.placements, original.entities).to_json_string()
            }
            EditorDocumentKind::Extension(_) => unreachable!(),
        }
        .unwrap();
        let initial_base = base_source(INITIAL_SCORE, INITIAL_NOTE, INITIAL_RANK);
        let unrelated =
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("unrelated-root"))])
                .to_json_string()
                .unwrap();
        let startup = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))])
            .to_json_string()
            .unwrap();
        let schema = set_schema().to_json_string().unwrap();
        for (path, bytes) in [
            (source, original.as_str()),
            (BASE, initial_base.as_str()),
            (UNRELATED, unrelated.as_str()),
            (STARTUP, startup.as_str()),
            ("schema/authoring.bundle.json", schema.as_str()),
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
        let document = open_source(&mut app, source);
        assert_ne!(active, document);
        for action in [
            EditorNavigationAction::SelectEntity {
                document,
                entity: Some(id("right")),
            },
            EditorNavigationAction::SetActiveDocument { document: active },
            EditorNavigationAction::SelectEntity {
                document: active,
                entity: Some(id("active")),
            },
        ] {
            workspace_action(&mut app, EditorAction::Navigate(action));
        }
        let current = frame(&app);
        let tab = current.workspace().document(document).unwrap();
        assert_eq!(tab.kind, kind);
        let source_binding = tab.source_binding.clone().unwrap();
        let panel = PanelDriver::register(&mut app);
        Self {
            panel,
            app,
            project,
            document,
            active,
            source,
            source_binding,
            original,
            initial_base,
            left,
            right,
        }
    }

    fn inspect<const N: usize>(
        &mut self,
        anchor: &str,
        fields: [(&str, bool); N],
    ) -> (
        EditorPrefabInspectionId,
        [(EditorPrefabPropertyAddress, ComponentValue); N],
    ) {
        let inspection = start(&mut self.panel, &mut self.app, self.document, anchor);
        expect_phase(&mut self.app, inspection, Phase::Ready);
        let selected = self.panel.observe(&mut self.app, |context| {
            let snapshot = context.prefab_effective_observation(inspection).unwrap();
            let expected = address(anchor, "score");
            let entity = snapshot
                .targets()
                .find(|target| *target == &expected.entity)
                .expect("discovery must identify the selected occurrence");
            let component = snapshot
                .target(entity)
                .unwrap()
                .component(&expected.component)
                .unwrap();
            assert_eq!(
                component.fact().record(),
                &AuthoringPrefabRecordOrigin::SourceEntity {
                    source: PrefabDeclarationSource::Prefab(base_key()),
                    entity: id("base-root"),
                }
            );
            let roots = component.root_fields().unwrap();
            fields.map(|(field, claimed)| {
                let root = roots
                    .iter()
                    .find(|root| root.id().as_str() == field)
                    .unwrap();
                let address = EditorPrefabPropertyAddress {
                    entity: entity.clone(),
                    component: component.fact().component().clone(),
                    property: ComponentPropertyPath::new(root.id().clone()),
                };
                let mut budget = ComponentPropertyReadBudget::new(
                    ItemLimit::new(16).unwrap(),
                    ByteLimit::new(1024).unwrap(),
                );
                let property = component
                    .property_value(&address.property, &mut budget)
                    .unwrap();
                if claimed {
                    let claim = property
                        .revert_claim()
                        .expect("an explicit Set must be revertible");
                    assert_eq!(claim.path, address.property);
                    assert_eq!(claim.kind, AuthoringPrefabPropertyClaimKind::Set);
                    assert_eq!(claim.location.path(), &PrefabInstancePath::new(id(anchor)));
                    assert_eq!(
                        claim.location.source(),
                        &PrefabDeclarationSource::Containing
                    );
                    assert_eq!(property.controlling_claim(), Some(claim));
                } else {
                    assert!(property.controlling_claim().is_none());
                }
                let AuthoringPrefabPropertyValue::Present(value) = property.value else {
                    panic!("the selected field must be present: {property:?}");
                };
                let limits = ComponentValueMaterializationLimits::new(
                    ItemLimit::ONE,
                    ByteLimit::new(128).unwrap(),
                    DepthLimit::ONE,
                );
                (address, value.copy_value(limits).unwrap())
            })
        });
        (inspection, selected)
    }

    fn assert_live(&mut self, left: &PrefabInstance, dirty: bool, undo: usize, redo: usize) {
        let current = frame(&self.app);
        let workspace = current.workspace();
        assert_eq!(workspace.active_document, Some(self.active));
        assert_eq!(current.scene_preview_document(), Some(self.active));
        let active = workspace.active_scene.as_ref().unwrap();
        assert_eq!(active.selection.top_entity(), Some(&id("active")));
        let tab = workspace.document(self.document).unwrap();
        assert_eq!(
            (tab.dirty, tab.history.undo_depth, tab.history.redo_depth),
            (dirty, undo, redo)
        );
        assert_eq!(tab.source_binding.as_ref(), Some(&self.source_binding));
        let document = self.document;
        let right = &self.right;
        self.panel.observe(&mut self.app, |context| {
            let basis = context.document_query_basis(document).unwrap();
            for (anchor, expected) in [("left", left), ("right", right)] {
                let observed = context
                    .observe_prefab_instance(
                        &basis,
                        &PrefabInstancePath::new(id(anchor)),
                        Default::default(),
                    )
                    .unwrap();
                assert_eq!(observed.instance.as_ref(), Some(expected));
            }
        });
    }

    fn assert_unwritten(&self) {
        assert_eq!(
            fs::read(self.project.root.join(self.source)).unwrap(),
            self.original.as_bytes()
        );
        assert_eq!(
            fs::read(self.project.root.join(BASE)).unwrap(),
            self.initial_base.as_bytes()
        );
    }

    fn prepare_set(
        &mut self,
        inspection: EditorPrefabInspectionId,
        address: EditorPrefabPropertyAddress,
        value: ComponentValue,
        limits: PrefabDeclarationLimits,
    ) -> Outcome {
        self.panel.dispatch(&mut self.app, |_| Action::PrepareSet {
            inspection,
            address,
            value,
            limits,
        })
    }

    fn review_set(
        &mut self,
        inspection: EditorPrefabInspectionId,
        address: EditorPrefabPropertyAddress,
        value: ComponentValue,
        before: &PrefabInstance,
        after: &PrefabInstance,
    ) {
        let property = address.property.clone();
        let expected = PrefabPropertyIntent::Set(value.clone());
        assert_eq!(
            self.prepare_set(inspection, address, value, Default::default()),
            Outcome::Accepted {
                inspection,
                phase: Phase::PreparingEdit,
            }
        );
        expect_phase(&mut self.app, inspection, Phase::Review);
        let patch = instance_patch(before, after);
        assert_eq!(
            self.app
                .prefab_inspection_review(inspection)
                .unwrap()
                .patch(),
            &patch
        );
        let document = self.document;
        self.panel.observe(&mut self.app, |context| {
            let edit = context.prefab_inspection_review(inspection).unwrap();
            assert_eq!(edit.patch(), &patch);
            let preview = edit.preview();
            assert_eq!(
                preview.basis(),
                &context.document_query_basis(document).unwrap()
            );
            let page = preview
                .query_authoring(EditorAuthoringQuery {
                    position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                        basis: preview.basis().clone(),
                        entity: id("left"),
                        projection: EditorAuthoringProjection::PrefabPropertyClaims {
                            steps: vec![],
                        },
                    }),
                    limits: EditorAuthoringQueryLimits::new(16, 8, 4096).unwrap(),
                })
                .unwrap();
            assert!(page.next.is_none());
            let claim = page
                .rows
                .iter()
                .find_map(|row| match row {
                    EditorAuthoringRow::PrefabPropertyClaim {
                        claim,
                        property: actual,
                        has_value: true,
                        ..
                    } if actual == &property => Some(claim),
                    _ => None,
                })
                .expect("the prepared source query must expose the new Set");
            assert_eq!(
                preview
                    .read_prefab_property_claim(
                        claim,
                        EditorPropertyReadLimits::new(1024, 1, 128).unwrap(),
                    )
                    .unwrap(),
                expected,
            );
        });
    }

    fn commit(&mut self, inspection: EditorPrefabInspectionId) {
        assert_eq!(
            self.panel
                .dispatch(&mut self.app, |_| Action::Commit { inspection }),
            Outcome::Accepted {
                inspection,
                phase: Phase::Validating,
            },
        );
        expect_phase(&mut self.app, inspection, Phase::Terminal);
    }

    fn commit_set(&mut self, inspection: EditorPrefabInspectionId) {
        let before = basis(&self.app, self.document).revision;
        self.commit(inspection);
        let revision = basis(&self.app, self.document).revision;
        assert_ne!(revision, before);
        assert_eq!(
            frame(&self.app).prefab_inspection().result,
            Some(EditorPrefabInspectionResult::Set {
                inspection,
                document: self.document,
                revision,
            }),
        );
        acknowledge(&mut self.panel, &mut self.app, inspection);
    }
}

fn instance_patch(before: &PrefabInstance, after: &PrefabInstance) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
        entity: id("left"),
        expected: Some(before.clone()),
        value: Some(after.clone()),
    }])
}

#[test]
fn a_registered_panel_sets_and_pins_effective_fields_on_a_nonactive_scene() {
    set_round_trip(EditorDocumentKind::Scene, SCENE);
}

#[test]
fn a_registered_panel_sets_and_pins_effective_fields_on_a_nonactive_prefab() {
    set_round_trip(EditorDocumentKind::Prefab, PREFAB);
}

fn set_round_trip(kind: EditorDocumentKind, source: &'static str) {
    let mut fixture = Fixture::new(kind, source);
    let left = fixture.left.clone();
    let original_basis = basis(&fixture.app, fixture.document);

    let cancelled_value = ComponentValue::String("cancelled note".into());
    let cancelled_candidate = with_claims(&left, [("note", cancelled_value.clone())]);
    let (cancelled, [(note, _)]) = fixture.inspect("left", [("note", false)]);
    fixture.review_set(
        cancelled,
        note,
        cancelled_value,
        &left,
        &cancelled_candidate,
    );
    fixture.assert_live(&left, false, 0, 0);
    fixture.assert_unwritten();
    cancel(&mut fixture.panel, &mut fixture.app, cancelled);
    assert_eq!(basis(&fixture.app, fixture.document), original_basis);
    fixture.assert_live(&left, false, 0, 0);

    let (scoring, [(score, ComponentValue::U64(observed_score))]) =
        fixture.inspect("left", [("score", false)])
    else {
        panic!("the derived override needs a scalar score");
    };
    assert_eq!(observed_score, INITIAL_SCORE);
    let changed_score = ComponentValue::U64(observed_score + SCORE_INCREMENT);
    let scored = with_claims(&left, [("score", changed_score.clone())]);
    fixture.review_set(scoring, score, changed_score.clone(), &left, &scored);
    if kind == EditorDocumentKind::Prefab {
        fixture.panel.observe(&mut fixture.app, |context| {
            let preview = context.prefab_inspection_review(scoring).unwrap().preview();
            let page = preview
                .query_authoring(EditorAuthoringQuery {
                    position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                        basis: preview.basis().clone(),
                        entity: id("unrelated"),
                        projection: EditorAuthoringProjection::Components,
                    }),
                    limits: EditorAuthoringQueryLimits::new(16, 8, 4096).unwrap(),
                })
                .unwrap();
            assert_eq!(
                page.rows,
                [EditorAuthoringRow::Component {
                    component: ComponentTypeId::new(UI_TEXT),
                    version: ComponentSchemaVersion::ONE,
                    readiness: AuthoringRecordReadiness::UnknownSchema,
                }]
            );
        });
    }
    fixture.assert_live(&left, false, 0, 0);
    fixture.assert_unwritten();
    fixture.commit_set(scoring);

    let (pinning, [(note, inherited_note)]) = fixture.inspect("left", [("note", false)]);
    assert_eq!(inherited_note, ComponentValue::String(INITIAL_NOTE.into()));
    let pinned = with_claims(
        &left,
        [
            ("score", changed_score.clone()),
            ("note", inherited_note.clone()),
        ],
    );
    fixture.review_set(pinning, note, inherited_note.clone(), &scored, &pinned);
    fixture.assert_live(&scored, true, 1, 0);
    fixture.assert_unwritten();
    fixture.commit_set(pinning);
    fixture.assert_live(&pinned, true, 2, 0);
    assert_effective(
        &mut fixture,
        "right",
        INITIAL_SCORE,
        INITIAL_NOTE,
        INITIAL_RANK,
        false,
    );

    replay(&mut fixture.app, fixture.document, false);
    fixture.assert_live(&scored, true, 1, 1);
    replay(&mut fixture.app, fixture.document, true);
    fixture.assert_live(&pinned, true, 2, 0);
    fixture.assert_unwritten();

    workspace_action(
        &mut fixture.app,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
            document: fixture.document,
        }),
    );
    let current = frame(&fixture.app);
    let active = current.workspace().active_scene.as_ref().unwrap();
    assert_eq!(active.selection.top_entity(), Some(&id("right")));
    workspace_action(
        &mut fixture.app,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
            document: fixture.active,
        }),
    );

    save(&mut fixture.app, fixture.document);
    let saved = fs::read(fixture.project.root.join(source)).unwrap();
    assert_saved_source(kind, &saved, &set_source(kind, &pinned, &fixture.right));
    fixture.assert_live(&pinned, false, 2, 0);
    let before = basis(&fixture.app, fixture.document).revision;
    let reopened = persist(
        &mut fixture.app,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document: fixture.document,
                base_revision: before,
            },
        },
    );
    let after = basis(&fixture.app, fixture.document).revision;
    assert!(
        matches!(reopened, EditorPersistenceResult::Opened { document, revision, .. }
            if document == fixture.document && revision == after),
        "{reopened:?}"
    );
    assert_ne!(before.source_id(), after.source_id());
    fixture.assert_live(&pinned, false, 0, 0);

    let changed_base = base_source(CHANGED_SCORE, CHANGED_NOTE, CHANGED_RANK);
    fs::write(fixture.project.root.join(BASE), &changed_base).unwrap();
    assert_effective(
        &mut fixture,
        "right",
        CHANGED_SCORE,
        CHANGED_NOTE,
        CHANGED_RANK,
        false,
    );
    let (reverting, [(_, score), (note, pinned_note), (_, rank)]) =
        fixture.inspect("left", [("score", true), ("note", true), ("rank", false)]);
    assert_eq!(
        (score, pinned_note, rank),
        (
            changed_score,
            inherited_note,
            ComponentValue::U64(CHANGED_RANK)
        )
    );
    prepare_revert(
        &mut fixture.panel,
        &mut fixture.app,
        reverting,
        note,
        Default::default(),
    );
    assert_review(
        &mut fixture.panel,
        &mut fixture.app,
        reverting,
        &instance_patch(&pinned, &scored),
    );
    fixture.commit(reverting);
    assert_eq!(
        frame(&fixture.app).prefab_inspection().result,
        Some(EditorPrefabInspectionResult::Reverted {
            inspection: reverting,
            document: fixture.document,
            revision: basis(&fixture.app, fixture.document).revision,
        }),
    );
    acknowledge(&mut fixture.panel, &mut fixture.app, reverting);
    assert_effective(
        &mut fixture,
        "left",
        INITIAL_SCORE + SCORE_INCREMENT,
        CHANGED_NOTE,
        CHANGED_RANK,
        true,
    );
    fixture.assert_live(&scored, true, 1, 0);
    assert_eq!(fs::read(fixture.project.root.join(source)).unwrap(), saved);
    assert_eq!(
        fs::read(fixture.project.root.join(BASE)).unwrap(),
        changed_base.as_bytes()
    );
    assert!(fixture.app.close());
}

fn assert_effective(
    fixture: &mut Fixture,
    anchor: &str,
    score: u64,
    note: &str,
    rank: u64,
    score_claimed: bool,
) {
    let (inspection, [(_, actual_score), (_, actual_note), (_, actual_rank)]) = fixture.inspect(
        anchor,
        [("score", score_claimed), ("note", false), ("rank", false)],
    );
    assert_eq!(
        (actual_score, actual_note, actual_rank),
        (
            ComponentValue::U64(score),
            ComponentValue::String(note.into()),
            ComponentValue::U64(rank),
        ),
    );
    cancel(&mut fixture.panel, &mut fixture.app, inspection);
}

#[test]
fn a_registered_panel_creates_and_updates_the_nested_set_through_an_addition_alias() {
    let project = text_project::TextProject::new();
    let nested_path = "prefabs/inspection-nested.prefab.json";
    let nested_base = SourceDocumentKey::project_location(
        "24ed5e36-1b37-5fa3-9c42-232066b17916".parse().unwrap(),
        AssetSourcePath::new(nested_path).unwrap(),
        SourceDocumentKind::Prefab,
    );
    let inherited_score = 41;
    let nested_source = base_source(inherited_score, INITIAL_NOTE, INITIAL_RANK);
    fs::write(project.root.join(nested_path), &nested_source).unwrap();
    let mount = id("added-mount");
    let mut addition = PrefabAddition::new(mount.clone());
    addition.prefab = Some(PrefabInstance::root(
        AssetRef::path("inspection-nested.prefab.json").unwrap(),
        nested_base.clone(),
        PrefabEntityTarget::source(id("base-root")),
    ));
    let mut left = with_claims(
        &PrefabInstance::forest(AssetRef::path("inspection-base.prefab.json").unwrap()),
        [("note", ComponentValue::String("outer note".into()))],
    );
    let overlay = left.overrides.as_mut().unwrap();
    overlay.additions.push(addition);
    overlay.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::addition(mount.clone()),
        ScenePlacement::last(ScenePlacementParent::Root),
    ));

    let target = PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::InstanceAddition(mount.clone()),
            expected_base: nested_base,
        }],
        entity: PrefabEntityOrigin::SourceEntity(id("base-root")),
    };
    let mut alias_route = address("left", "score").entity;
    alias_route.entity = PrefabEntityOrigin::InstanceAddition(mount);
    let mut nested_route = address("left", "score").entity;
    nested_route.through.extend(target.through.iter().cloned());
    let routes = [&alias_route, &nested_route];
    let expected =
        |score| {
            let mut instance = left.clone();
            instance.overrides.as_mut().unwrap().components.push(
                PrefabComponentIntent::Properties {
                    target: target.clone(),
                    component: ComponentTypeId::new(COMPONENT),
                    version: ComponentSchemaVersion::ONE,
                    claims: vec![PrefabPropertyClaim {
                        path: property("score"),
                        intent: PrefabPropertyIntent::Set(ComponentValue::U64(score)),
                    }],
                },
            );
            instance
        };
    let mut fixture = Fixture::with_left(project, EditorDocumentKind::Scene, SCENE, left.clone());
    let pinned_score = inherited_score + SCORE_INCREMENT;
    let pinned = expected(pinned_score);
    let (creating, [alias, _]) =
        inspect_addition_aliases(&mut fixture, routes, inherited_score, None);
    fixture.review_set(
        creating,
        alias,
        ComponentValue::U64(pinned_score),
        &left,
        &pinned,
    );
    fixture.assert_live(&left, false, 0, 0);
    fixture.commit_set(creating);

    let slot = Some(PrefabOverlaySlot::Property {
        component_index: 1,
        claim_index: 0,
    });
    let changed_score = pinned_score + SCORE_INCREMENT;
    let changed = expected(changed_score);
    let (using_nested, [_, nested]) =
        inspect_addition_aliases(&mut fixture, routes, pinned_score, slot);
    fixture.review_set(
        using_nested,
        nested,
        ComponentValue::U64(changed_score),
        &pinned,
        &changed,
    );
    cancel(&mut fixture.panel, &mut fixture.app, using_nested);

    let (using_alias, [alias, _]) =
        inspect_addition_aliases(&mut fixture, routes, pinned_score, slot);
    fixture.review_set(
        using_alias,
        alias,
        ComponentValue::U64(changed_score),
        &pinned,
        &changed,
    );
    fixture.assert_live(&pinned, true, 1, 0);
    fixture.commit_set(using_alias);
    fixture.assert_live(&changed, true, 2, 0);
    let before_repeat = basis(&fixture.app, fixture.document);
    let (verifying, [alias, _]) =
        inspect_addition_aliases(&mut fixture, routes, changed_score, slot);
    assert_eq!(
        fixture.prepare_set(
            verifying,
            alias,
            ComponentValue::U64(changed_score),
            Default::default()
        ),
        Outcome::Accepted {
            inspection: verifying,
            phase: Phase::PreparingEdit
        }
    );
    assert_failure(
        &mut fixture.app,
        verifying,
        Rejection::AlreadySet,
        "scene.authoring-prefab-already-set",
    );
    acknowledge(&mut fixture.panel, &mut fixture.app, verifying);
    assert_eq!(basis(&fixture.app, fixture.document), before_repeat);
    fixture.assert_live(&changed, true, 2, 0);
    fixture.assert_unwritten();
    assert_eq!(
        fs::read_to_string(fixture.project.root.join(nested_path)).unwrap(),
        nested_source,
    );
    assert!(fixture.app.close());
}

fn inspect_addition_aliases(
    fixture: &mut Fixture,
    routes: [&PrefabEntityTarget; 2],
    expected_score: u64,
    expected_slot: Option<PrefabOverlaySlot>,
) -> (EditorPrefabInspectionId, [EditorPrefabPropertyAddress; 2]) {
    let inspection = start(
        &mut fixture.panel,
        &mut fixture.app,
        fixture.document,
        "left",
    );
    expect_phase(&mut fixture.app, inspection, Phase::Ready);
    let addresses = fixture.panel.observe(&mut fixture.app, |context| {
        let snapshot = context.prefab_effective_observation(inspection).unwrap();
        routes.map(|route| {
            let entity = snapshot.targets().find(|target| *target == route).unwrap();
            let component = ComponentTypeId::new(COMPONENT);
            let property = property("score");
            let mut budget = ComponentPropertyReadBudget::new(
                ItemLimit::new(16).unwrap(),
                ByteLimit::new(1024).unwrap(),
            );
            let read = snapshot
                .target(entity)
                .unwrap()
                .component(&component)
                .unwrap()
                .property_value(&property, &mut budget)
                .unwrap();
            assert_eq!(read.controlling_claim(), read.revert_claim());
            assert_eq!(
                read.revert_claim().map(|claim| {
                    assert_eq!(claim.kind, AuthoringPrefabPropertyClaimKind::Set);
                    assert_eq!(claim.location.path(), &PrefabInstancePath::new(id("left")));
                    claim.slot
                }),
                expected_slot,
            );
            let AuthoringPrefabPropertyValue::Present(value) = read.value else {
                panic!("the root-bound addition must expose the nested score: {read:?}");
            };
            let limits = ComponentValueMaterializationLimits::new(
                ItemLimit::ONE,
                ByteLimit::new(128).unwrap(),
                DepthLimit::ONE,
            );
            assert_eq!(
                value.copy_value(limits).unwrap(),
                ComponentValue::U64(expected_score),
            );
            EditorPrefabPropertyAddress {
                entity: entity.clone(),
                component,
                property,
            }
        })
    });
    (inspection, addresses)
}

#[test]
fn a_registered_panel_rejects_inadmissible_and_stale_effective_field_sets() {
    let mut fixture = Fixture::new(EditorDocumentKind::Scene, SCENE);
    let left = fixture.left.clone();
    let initial = basis(&fixture.app, fixture.document);
    let limits = PrefabDeclarationLimits::default();
    let (inspection, [(score, _)]) = fixture.inspect("left", [("score", false)]);
    let mut local = address("left", "text");
    local.component = ComponentTypeId::new(ANCHOR_COMPONENT);
    fixture.panel.observe(&mut fixture.app, |context| {
        let snapshot = context.prefab_effective_observation(inspection).unwrap();
        let component = snapshot
            .target(&local.entity)
            .unwrap()
            .component(&local.component)
            .unwrap();
        assert_eq!(
            component.fact().record(),
            &AuthoringPrefabRecordOrigin::SourceEntity {
                source: PrefabDeclarationSource::Containing,
                entity: id("left"),
            }
        );
    });
    for (address, value) in [
        (address("right", "score"), ComponentValue::U64(27)),
        (score.clone(), ComponentValue::String("wrong kind".into())),
        (address("left", "rank"), ComponentValue::U64(12)),
        (address("left", "optional"), ComponentValue::U64(12)),
        (address("left", "profile"), profile(3)),
        (local, ComponentValue::String("source record".into())),
    ] {
        assert_eq!(
            fixture.prepare_set(inspection, address, value, limits),
            Outcome::Rejected {
                inspection: Some(inspection),
                reason: Rejection::InvalidProperty,
            }
        );
        expect_phase(&mut fixture.app, inspection, Phase::Ready);
    }
    let tiny =
        PrefabDeclarationLimits::new(1, limits.max_logical_bytes(), limits.max_depth()).unwrap();
    assert_eq!(
        fixture.prepare_set(inspection, score, ComponentValue::U64(27), tiny),
        Outcome::Rejected {
            inspection: Some(inspection),
            reason: Rejection::BudgetExceeded,
        }
    );
    expect_phase(&mut fixture.app, inspection, Phase::Ready);
    cancel(&mut fixture.panel, &mut fixture.app, inspection);

    let overlap = start(
        &mut fixture.panel,
        &mut fixture.app,
        fixture.document,
        "right",
    );
    expect_phase(&mut fixture.app, overlap, Phase::Ready);
    let mut level = address("right", "profile");
    level
        .property
        .steps
        .push(ComponentPropertyStep::Member(ComponentFieldId::new(
            "level",
        )));
    assert_eq!(
        fixture.prepare_set(overlap, level, ComponentValue::U64(4), limits),
        Outcome::Accepted {
            inspection: overlap,
            phase: Phase::PreparingEdit,
        }
    );
    assert_failure(
        &mut fixture.app,
        overlap,
        Rejection::ConflictingClaim,
        "scene.authoring-prefab-conflicting-claim",
    );
    acknowledge(&mut fixture.panel, &mut fixture.app, overlap);

    let (reviewed, [(score, _)]) = fixture.inspect("left", [("score", false)]);
    let changed = ComponentValue::U64(27);
    let candidate = with_claims(&left, [("score", changed.clone())]);
    fixture.review_set(reviewed, score, changed, &left, &candidate);
    let changed_base = base_source(CHANGED_SCORE, CHANGED_NOTE, CHANGED_RANK);
    assert_eq!(changed_base.len(), fixture.initial_base.len());
    fs::write(fixture.project.root.join(BASE), &changed_base).unwrap();
    fixture.commit(reviewed);
    assert_failure(
        &mut fixture.app,
        reviewed,
        Rejection::StaleSource,
        "project.editor.authoring-source-changed",
    );
    acknowledge(&mut fixture.panel, &mut fixture.app, reviewed);
    assert_eq!(
        fixture.app.prefab_inspection_review(reviewed).unwrap_err(),
        Rejection::WrongInspection
    );
    fs::write(fixture.project.root.join(BASE), &fixture.initial_base).unwrap();

    let base_document = open_source(&mut fixture.app, BASE);
    workspace_action(
        &mut fixture.app,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
            document: fixture.active,
        }),
    );
    let (working, [(score, value)]) = fixture.inspect("left", [("score", false)]);
    assert_eq!(value, ComponentValue::U64(INITIAL_SCORE));
    let base_revision = basis(&fixture.app, base_document).revision;
    workspace_action(
        &mut fixture.app,
        EditorAction::Mutate(EditorDocumentMutation::ApplyAuthoringFieldEdit {
            document: base_document,
            base_revision,
            edit: AuthoringFieldEdit::single(
                id("base-root"),
                ComponentTypeId::new(COMPONENT),
                ComponentSchemaVersion::ONE,
                ComponentFieldId::new("score"),
                ComponentValue::U64(CHANGED_SCORE),
            )
            .unwrap(),
        }),
    );
    assert_eq!(
        fixture
            .app
            .prefab_effective_observation(working)
            .unwrap_err(),
        Rejection::StaleSource
    );
    assert_eq!(
        fixture.prepare_set(working, score, value, limits),
        Outcome::Rejected {
            inspection: Some(working),
            reason: Rejection::StaleSource,
        }
    );
    cancel(&mut fixture.panel, &mut fixture.app, working);
    assert_effective(
        &mut fixture,
        "left",
        CHANGED_SCORE,
        INITIAL_NOTE,
        INITIAL_RANK,
        false,
    );
    assert_eq!(basis(&fixture.app, fixture.document), initial);
    fixture.assert_live(&left, false, 0, 0);
    fixture.assert_unwritten();
    assert!(fixture.app.close());
}
