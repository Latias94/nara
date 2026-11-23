use super::{set::instance_patch, *};

use nara::{
    identity::AuthoredEntityReference,
    reflect::{ComponentShapeMember, ComponentValueShape},
    scene::{
        AuthoringPrefabComponentOverrideKind as OverrideKind, PrefabComponentEdit,
        PrefabComponentSetMode as SetMode, SourceDocumentOrigin,
    },
};
use nara_tooling::{EditorPrefabComponentAddress, EditorPrefabComponentEditKind as EditKind};

#[path = "component_edit/contextual_defaults.rs"]
mod contextual_defaults;

const INHERITED: &str = "base-root";
const EMPTY: &str = "empty-root";
const SCHEMA: &str = "schema/authoring.bundle.json";
const TARGET_FIELD: &str = "target";
const TARGET_STORAGE: &str = "wire_target";

#[test]
fn a_registered_panel_edits_whole_prefab_components_on_a_nonactive_scene() {
    round_trip_components(EditorDocumentKind::Scene, SCENE);
}

#[test]
fn a_registered_panel_edits_whole_prefab_components_on_a_nonactive_prefab() {
    round_trip_components(EditorDocumentKind::Prefab, PREFAB);
}

#[test]
fn a_registered_panel_rejects_drift_in_a_prefab_newly_demanded_by_a_component_edit() {
    let mount = PrefabInstance::forest(AssetRef::path("inspection-base.prefab.json").unwrap());
    let mut fixture = ComponentFixture::new(
        EditorDocumentKind::Scene,
        SCENE,
        &mount,
        &settings(INITIAL_SCORE, Some(1)),
    );
    let document = fixture.document;
    let before = basis(&fixture.app, document);
    let dependency = fixture.project.root.join(UNRELATED);
    let write_dependency = |score| {
        let source =
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("unrelated-root"))
                .with_component(
                    ComponentTypeId::new(COMPONENT),
                    component_record(&settings(score, None)),
                )])
            .to_json_string()
            .unwrap();
        fs::write(&dependency, &source).unwrap();
        source
    };
    let initial = write_dependency(41);
    let (inspection, empty) = fixture.inspect("left", EMPTY, None, None);

    // This source is absent from the original selected capture and first demanded by the write.
    let reviewed = write_dependency(42);
    let base = base_key();
    let SourceDocumentOrigin::ProjectLocation { universe, .. } = base.origin() else {
        unreachable!();
    };
    let route = PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("unrelated")),
            expected_base: SourceDocumentKey::project_location(
                *universe,
                AssetSourcePath::new(UNRELATED).unwrap(),
                SourceDocumentKind::Prefab,
            ),
        }],
        entity: PrefabEntityOrigin::SourceEntity(id("unrelated-root")),
    };
    let mut addition = settings(51, Some(0));
    let ComponentValue::Map(fields) = &mut addition else {
        unreachable!();
    };
    fields.insert(TARGET_STORAGE.into(), authored_reference(route));
    let edited = with_intents(&mount, [set_intent(SetMode::Add, EMPTY, &addition)]);
    fixture.review(
        inspection,
        empty,
        set(SetMode::Add, &addition),
        &mount,
        &edited,
    );
    fixture.assert_live(&mount, false, 0, 0);

    let changed = write_dependency(43);
    assert_eq!(initial.len(), reviewed.len());
    assert_eq!(reviewed.len(), changed.len());
    assert_ne!(initial, reviewed);
    assert_ne!(reviewed, changed);
    fixture.request_commit(inspection);
    assert_failure(
        &mut fixture.app,
        inspection,
        Rejection::StaleSource,
        "project.editor.authoring-source-changed",
    );
    acknowledge(&mut fixture.panel, &mut fixture.app, inspection);
    assert_eq!(basis(&fixture.app, document), before);
    fixture.assert_live(&mount, false, 0, 0);
    assert_eq!(
        fs::read(&fixture.source).unwrap(),
        fixture.original.as_bytes()
    );
    assert_eq!(fs::read(dependency).unwrap(), changed.as_bytes());
    assert!(fixture.app.close());
}

fn round_trip_components(kind: EditorDocumentKind, source: &str) {
    let mount = PrefabInstance::forest(AssetRef::path("inspection-base.prefab.json").unwrap());
    let inherited = settings(INITIAL_SCORE, Some(1));
    let replacement = settings(37, None);
    let addition = settings(51, Some(0));
    let mut fixture = ComponentFixture::new(kind, source, &mount, &inherited);
    let document = fixture.document;
    let initial = basis(&fixture.app, document);

    let reverted = with_intents(&mount, []);
    let suppressed = with_intents(
        &mount,
        [PrefabComponentIntent::Suppress {
            target: PrefabEntityTarget::source(id(INHERITED)),
            component: ComponentTypeId::new(COMPONENT),
        }],
    );
    let replace_intent = set_intent(SetMode::Replace, INHERITED, &replacement);
    let replaced = with_intents(&mount, [replace_intent.clone()]);
    let edited = with_intents(
        &mount,
        [replace_intent, set_intent(SetMode::Add, EMPTY, &addition)],
    );

    let (cancelled, selected) = fixture.inspect("left", INHERITED, None, Some(&inherited));
    assert_eq!(
        fixture.prepare(cancelled, selected.clone(), set(SetMode::Add, &addition)),
        Outcome::Rejected {
            inspection: Some(cancelled),
            reason: Rejection::InvalidComponent,
        },
    );
    expect_phase(&mut fixture.app, cancelled, Phase::Ready);
    fixture.review(
        cancelled,
        selected,
        PrefabComponentEdit::Suppress,
        &mount,
        &suppressed,
    );
    fixture.assert_live(&mount, false, 0, 0);
    cancel(&mut fixture.panel, &mut fixture.app, cancelled);
    assert_eq!(basis(&fixture.app, document), initial);
    fixture.assert_live(&mount, false, 0, 0);
    assert_eq!(
        fs::read(&fixture.source).unwrap(),
        fixture.original.as_bytes()
    );

    let (inspection, selected) = fixture.inspect("left", INHERITED, None, Some(&inherited));
    fixture.review(
        inspection,
        selected,
        PrefabComponentEdit::Suppress,
        &mount,
        &suppressed,
    );
    fixture.commit(inspection, EditKind::Suppressed);
    fixture.assert_live(&suppressed, true, 1, 0);
    fixture.assert_effective("left", INHERITED, Some(OverrideKind::Suppressed), None);
    replay(&mut fixture.app, document, false);
    fixture.assert_live(&mount, false, 0, 1);
    replay(&mut fixture.app, document, true);
    fixture.assert_live(&suppressed, true, 1, 0);
    assert_eq!(
        fs::read(&fixture.source).unwrap(),
        fixture.original.as_bytes()
    );
    let saved_suppression = fixture.save(&suppressed, 1);
    fixture.reopen(&suppressed, &saved_suppression);

    let (inspection, absent) =
        fixture.inspect("left", INHERITED, Some(OverrideKind::Suppressed), None);
    fixture.review(
        inspection,
        absent,
        PrefabComponentEdit::Revert,
        &suppressed,
        &reverted,
    );
    fixture.assert_live(&suppressed, false, 0, 0);
    fixture.commit(inspection, EditKind::Reverted);
    fixture.assert_live(&reverted, true, 1, 0);

    let (inspection, restored) = fixture.inspect("left", INHERITED, None, Some(&inherited));
    fixture.review(
        inspection,
        restored,
        set(SetMode::Replace, &replacement),
        &reverted,
        &replaced,
    );
    fixture.assert_live(&reverted, true, 1, 0);
    fixture.commit(inspection, EditKind::Replaced);
    fixture.assert_live(&replaced, true, 2, 0);
    fixture.assert_effective(
        "left",
        INHERITED,
        Some(OverrideKind::Replaced),
        Some(&replacement),
    );

    let (inspection, empty) = fixture.inspect("left", EMPTY, None, None);
    fixture.review(
        inspection,
        empty,
        set(SetMode::Add, &addition),
        &replaced,
        &edited,
    );
    fixture.assert_live(&replaced, true, 2, 0);
    fixture.commit(inspection, EditKind::Added);
    fixture.assert_live(&edited, true, 3, 0);

    for (index, left) in [&replaced, &reverted, &suppressed].into_iter().enumerate() {
        replay(&mut fixture.app, document, false);
        fixture.assert_live(left, index != 2, 2 - index, index + 1);
    }
    for (index, left) in [&reverted, &replaced, &edited].into_iter().enumerate() {
        replay(&mut fixture.app, document, true);
        fixture.assert_live(left, true, index + 1, 2 - index);
    }
    assert_eq!(fs::read(&fixture.source).unwrap(), saved_suppression);
    let saved = fixture.save(&edited, 3);
    replay(&mut fixture.app, document, false);
    fixture.assert_live(&replaced, true, 2, 1);
    fixture.reopen(&edited, &saved);
    fixture.assert_effective(
        "left",
        INHERITED,
        Some(OverrideKind::Replaced),
        Some(&replacement),
    );
    fixture.assert_effective("left", EMPTY, Some(OverrideKind::Added), Some(&addition));
    fixture.assert_effective("right", INHERITED, None, Some(&inherited));
    fixture.assert_effective("right", EMPTY, None, None);
    fixture.assert_untouched_files();
    assert!(fixture.app.close());
}

struct ComponentFixture {
    panel: PanelDriver,
    app: Application,
    project: text_project::TextProject,
    document: EditorDocumentId,
    kind: EditorDocumentKind,
    source: PathBuf,
    initial_frame: std::sync::Arc<EditorFrameModel>,
    original: String,
    right: PrefabInstance,
    untouched: Vec<(&'static str, String)>,
}

impl ComponentFixture {
    fn new(
        kind: EditorDocumentKind,
        source: &str,
        mount: &PrefabInstance,
        inherited: &ComponentValue,
    ) -> Self {
        let base = PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id(INHERITED))
                .with_component(ComponentTypeId::new(COMPONENT), component_record(inherited)),
            SceneEntityRecord::new(id(EMPTY)),
        ]);
        Self::with_sources(
            kind,
            source,
            mount,
            [
                (BASE, base.to_json_string().unwrap()),
                (SCHEMA, component_schema(None).to_json_string().unwrap()),
            ],
        )
    }

    fn with_sources(
        kind: EditorDocumentKind,
        source: &str,
        mount: &PrefabInstance,
        inputs: impl IntoIterator<Item = (&'static str, String)>,
    ) -> Self {
        let project = text_project::TextProject::new();
        let original = containing_json(kind, mount, mount);
        let startup = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))]);
        let unrelated =
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("unrelated-root"))]);
        let untouched = [
            (STARTUP, startup.to_json_string().unwrap()),
            (UNRELATED, unrelated.to_json_string().unwrap()),
        ]
        .into_iter()
        .chain(inputs)
        .collect::<Vec<_>>();
        for (path, contents) in &untouched {
            fs::write(project.root.join(path), contents).unwrap();
        }
        let source_file = project.root.join(source);
        fs::write(&source_file, &original).unwrap();

        let mut app = Application(EditorApplication::new(EditorLaunchOptions::new(
            &project.root,
        )));
        app.native_shell_ready();
        wait(&mut app, |current| {
            !current.project_browser().has_pending_work()
        });
        let active = frame(&app).workspace().active_document.unwrap();
        let document = open_source(&mut app, source);
        assert_ne!(document, active);
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
            let result = submit(&mut app, EditorAction::Navigate(action));
            assert!(
                matches!(result.actions(), [
                EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)),
            ] if report.applied),
                "{result:?}"
            );
        }
        let initial_frame = frame(&app);
        assert_eq!(
            initial_frame.workspace().document(document).unwrap().kind,
            kind
        );
        let panel = PanelDriver::register(&mut app);
        Self {
            panel,
            app,
            project,
            document,
            kind,
            source: source_file,
            initial_frame,
            original,
            right: mount.clone(),
            untouched,
        }
    }

    fn inspect(
        &mut self,
        anchor: &str,
        entity: &str,
        expected_override: Option<OverrideKind>,
        expected_value: Option<&ComponentValue>,
    ) -> (EditorPrefabInspectionId, EditorPrefabComponentAddress) {
        let route = mounted_target(anchor, &PrefabEntityTarget::source(id(entity)));
        self.inspect_target(anchor, &route, expected_override, expected_value)
    }

    fn inspect_target(
        &mut self,
        anchor: &str,
        route: &PrefabEntityTarget,
        expected_override: Option<OverrideKind>,
        expected_value: Option<&ComponentValue>,
    ) -> (EditorPrefabInspectionId, EditorPrefabComponentAddress) {
        let inspection = start(&mut self.panel, &mut self.app, self.document, anchor);
        expect_phase(&mut self.app, inspection, Phase::Ready);
        let address = self.panel.observe(&mut self.app, |context| {
            let snapshot = context.prefab_effective_observation(inspection).unwrap();
            let target = snapshot
                .targets()
                .find(|target| *target == route)
                .unwrap_or_else(|| panic!("the selected mount must expose {route:?}"));
            let occurrence = snapshot.target(target).unwrap();
            let component = ComponentTypeId::new(COMPONENT);
            assert_eq!(occurrence.component_override(&component), expected_override);
            assert_eq!(
                occurrence.component_overrides().collect::<Vec<_>>(),
                expected_override.map(|kind| (&component, kind)).as_slice(),
            );
            let view = occurrence.component(&component);
            if let Some(expected) = expected_value {
                let view = view.unwrap();
                let mut budget = ComponentPropertyReadBudget::new(
                    ItemLimit::new(16).unwrap(),
                    ByteLimit::new(1024).unwrap(),
                );
                let limits = ComponentValueMaterializationLimits::new(
                    ItemLimit::new(8).unwrap(),
                    ByteLimit::new(256).unwrap(),
                    DepthLimit::new(4).unwrap(),
                );
                for (field, storage) in [
                    ("score", "wire_score"),
                    ("note", "wire_note"),
                    ("profile", "wire_profile"),
                ] {
                    let observed = view.property_value(&property(field), &mut budget).unwrap();
                    match (observed.value, expected.get(storage)) {
                        (AuthoringPrefabPropertyValue::Present(value), Some(expected)) => {
                            assert_eq!(&value.copy_value(limits).unwrap(), expected, "{field}");
                        }
                        (AuthoringPrefabPropertyValue::Missing, None) => {}
                        (actual, expected) => {
                            panic!("unexpected {field}: {actual:?}, expected {expected:?}");
                        }
                    }
                }
            } else {
                assert_eq!(view.unwrap_err(), AuthoringReadError::ComponentNotFound);
            }
            EditorPrefabComponentAddress {
                entity: target.clone(),
                component,
            }
        });
        (inspection, address)
    }

    fn assert_effective(
        &mut self,
        anchor: &str,
        entity: &str,
        expected_override: Option<OverrideKind>,
        expected_value: Option<&ComponentValue>,
    ) {
        let (inspection, _) = self.inspect(anchor, entity, expected_override, expected_value);
        cancel(&mut self.panel, &mut self.app, inspection);
    }

    fn prepare(
        &mut self,
        inspection: EditorPrefabInspectionId,
        address: EditorPrefabComponentAddress,
        edit: PrefabComponentEdit,
    ) -> Outcome {
        self.panel
            .dispatch(&mut self.app, |_| Action::PrepareComponentEdit {
                inspection,
                address,
                edit,
                limits: Default::default(),
            })
    }

    fn review(
        &mut self,
        inspection: EditorPrefabInspectionId,
        address: EditorPrefabComponentAddress,
        edit: PrefabComponentEdit,
        before: &PrefabInstance,
        after: &PrefabInstance,
    ) {
        let captured = basis(&self.app, self.document);
        let kind = self.kind;
        assert_eq!(
            self.prepare(inspection, address, edit),
            Outcome::Accepted {
                inspection,
                phase: Phase::PreparingEdit,
            }
        );
        expect_phase(&mut self.app, inspection, Phase::Review);
        self.panel.observe(&mut self.app, |context| {
            let prepared = context.prefab_inspection_review(inspection).unwrap();
            assert_eq!(prepared.document_kind(), kind);
            assert_eq!(prepared.preview().basis(), &captured);
            assert_eq!(prepared.patch(), &instance_patch(before, after));
            assert_eq!(prepared.inverse(), &instance_patch(after, before));
        });
    }

    fn request_commit(&mut self, inspection: EditorPrefabInspectionId) {
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

    fn commit(&mut self, inspection: EditorPrefabInspectionId, kind: EditKind) {
        let before = basis(&self.app, self.document).revision;
        self.request_commit(inspection);
        let revision = basis(&self.app, self.document).revision;
        assert_ne!(revision, before);
        assert_eq!(revision.source_id(), before.source_id());
        assert_eq!(
            frame(&self.app).prefab_inspection().result,
            Some(EditorPrefabInspectionResult::ComponentEdited {
                inspection,
                document: self.document,
                revision,
                kind,
            })
        );
        acknowledge(&mut self.panel, &mut self.app, inspection);
    }

    fn assert_live(&self, left: &PrefabInstance, dirty: bool, undo: usize, redo: usize) {
        let current = frame(&self.app);
        let before = self.initial_frame.workspace();
        let workspace = current.workspace();
        let active = before.active_document.unwrap();
        assert_eq!(workspace.active_document, Some(active));
        assert_eq!(current.scene_preview_document(), Some(active));
        assert_eq!(workspace.active_scene, before.active_scene);
        assert_eq!(workspace.document(active), before.document(active));
        assert_eq!(workspace.documents.len(), before.documents.len());
        let tab = workspace.document(self.document).unwrap();
        assert!(!tab.active);
        assert_eq!(
            tab.source_binding,
            before.document(self.document).unwrap().source_binding
        );
        assert_eq!(
            (tab.dirty, tab.history.undo_depth, tab.history.redo_depth),
            (dirty, undo, redo)
        );
        let basis = basis(&self.app, self.document);
        for (anchor, expected) in [("left", left), ("right", &self.right)] {
            let observed = self
                .app
                .observe_prefab_instance(
                    &basis,
                    &PrefabInstancePath::new(id(anchor)),
                    Default::default(),
                )
                .unwrap();
            assert_eq!(observed.instance(), Some(expected));
        }
    }

    fn save(&mut self, left: &PrefabInstance, undo: usize) -> Vec<u8> {
        save(&mut self.app, self.document);
        let bytes = fs::read(&self.source).unwrap();
        assert_saved_source(self.kind, &bytes, &containing_source(left, &self.right));
        self.assert_live(left, false, undo, 0);
        bytes
    }

    fn reopen(&mut self, left: &PrefabInstance, saved: &[u8]) {
        let before = basis(&self.app, self.document).revision;
        let result = persist(
            &mut self.app,
            EditorPersistenceAction::Reopen {
                target: EditorReopenTarget::OpenDocument {
                    document: self.document,
                    base_revision: before,
                },
            },
        );
        let after = basis(&self.app, self.document).revision;
        assert!(
            matches!(result, EditorPersistenceResult::Opened { document, revision, .. }
            if document == self.document && revision == after),
            "{result:?}"
        );
        assert_ne!(after.source_id(), before.source_id());
        self.assert_live(left, false, 0, 0);
        assert_eq!(fs::read(&self.source).unwrap(), saved);
    }

    fn assert_untouched_files(&self) {
        for (path, bytes) in &self.untouched {
            assert_eq!(
                fs::read(self.project.root.join(path)).unwrap(),
                bytes.as_bytes(),
                "{path}"
            );
        }
    }
}

fn containing_json(
    kind: EditorDocumentKind,
    left: &PrefabInstance,
    right: &PrefabInstance,
) -> String {
    let containing = containing_source(left, right);
    match kind {
        EditorDocumentKind::Scene => containing.to_json_string(),
        EditorDocumentKind::Prefab => {
            PrefabDocument::new(containing.placements, containing.entities).to_json_string()
        }
        EditorDocumentKind::Extension(_) => unreachable!(),
    }
    .unwrap()
}

fn mounted_target(anchor: &str, target: &PrefabEntityTarget) -> PrefabEntityTarget {
    let mut route = address(anchor, TARGET_FIELD).entity;
    route.through.extend(target.through.iter().cloned());
    route.entity = target.entity.clone();
    route
}

fn authored_reference(target: PrefabEntityTarget) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference::from(target),
    })
}

fn component_schema(target_default: Option<ComponentValue>) -> AuthoringSchemaBundle {
    let bundle = schema();
    let owner = &bundle.owners()[0];
    let mut catalog = owner.current().clone();
    let settings = catalog
        .components
        .iter_mut()
        .find(|component| component.id().as_str() == COMPONENT)
        .unwrap();
    settings.fields.extend([
        ComponentFieldSchema::optional(
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
        ComponentFieldSchema {
            default_value: target_default,
            ..ComponentFieldSchema::optional(
                ComponentFieldId::new(TARGET_FIELD),
                "Target",
                ComponentFieldPath::from_fields([TARGET_STORAGE]),
                ComponentValueShape::EntityRef,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef)
        },
    ]);
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.owner_id().clone(),
        catalog,
        None,
    )])
    .unwrap()
}

fn settings(score: u64, level: Option<u64>) -> ComponentValue {
    let mut fields = vec![
        ("wire_score", ComponentValue::U64(score)),
        (
            "wire_note",
            ComponentValue::String("complete record".into()),
        ),
    ];
    if let Some(level) = level {
        fields.push((
            "wire_profile",
            ComponentValue::map([("wire_level", ComponentValue::U64(level))]),
        ));
    }
    ComponentValue::map(fields)
}

fn component_record(value: &ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value.clone())
        .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap())
}

fn set(mode: SetMode, value: &ComponentValue) -> PrefabComponentEdit {
    PrefabComponentEdit::Set {
        mode,
        value: value.clone(),
    }
}

fn set_intent(mode: SetMode, entity: &str, value: &ComponentValue) -> PrefabComponentIntent {
    PrefabComponentIntent::Set {
        mode,
        target: PrefabEntityTarget::source(id(entity)),
        component: ComponentTypeId::new(COMPONENT),
        record: component_record(value),
    }
}

fn with_intents(
    mount: &PrefabInstance,
    components: impl IntoIterator<Item = PrefabComponentIntent>,
) -> PrefabInstance {
    let mut instance = mount.clone();
    let mut overlay = PrefabOverlay::new(base_key());
    overlay.components = components.into_iter().collect();
    instance.overrides = Some(overlay);
    instance
}
