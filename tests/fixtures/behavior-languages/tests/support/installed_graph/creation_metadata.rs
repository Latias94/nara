//! Existing-owner evolution consumes actual managed artifacts and the public Session owners.

use nara_asset::AssetRef;
use nara_engine::{
    project_host::{
        EditorSchemaOwnerAdoptionError, EditorSchemaOwnerAdoptionId,
        EditorSchemaOwnerAdoptionOutcome, EditorSchemaOwnerAdoptionPhase,
        EditorSourceToolManagedHost,
    },
    reflect::ComponentSchemaOwnerChange,
};
use nara_reflect::{
    AuthoringSchemaBundle, ComponentFieldId, ComponentPropertyPath,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    PrefabDocument, PrefabInstance, SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneEntityId,
    ScenePatchDocument, ScenePatchOperation,
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorAuthoringProjection,
    EditorAuthoringQuery, EditorAuthoringQueryLimits, EditorAuthoringQueryPosition,
    EditorAuthoringQueryTarget, EditorAuthoringRow, EditorDocumentKind, EditorDocumentOpenResult,
    EditorDocumentQueryRejection, EditorDocumentTabModel, EditorEntityQuery,
    EditorEntityQueryLimits, EditorEntityQueryPosition, EditorNavigationAction,
    EditorProjectBrowserAction, EditorProjectBrowserActionOutcome, EditorPropertyRead,
    EditorPropertyReadLimits, EditorPropertyTarget, EditorPropertyValue, EditorSelectionSet,
};

use super::*;

const BUNDLE: &str = "schema/authoring.bundle.json";
const SECOND: &str = "scenes/second.scene.json";
const PREFAB: &str = "prefabs/enemy.prefab.json";

struct Fixture {
    project: Project,
    root: DirectoryCapability,
    v1: EditorSourceToolInstalledEntry,
}

impl Fixture {
    fn new() -> Self {
        let builtins = AuthoringSchemaBundle::from_trusted_provider_definitions([
            SCENE_COMPONENTS_SCHEMA_PROVIDER,
        ])
        .unwrap();
        let mut scene = original();
        scene.entities[0].components.insert(
            ComponentTypeId::new(STATS),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    ("wire_health", ComponentValue::I64(100)),
                    ("wire_armor", ComponentValue::I64(10)),
                ]),
            )
            .with_owner(PersistentComponentSchemaOwnerId::try_new(PACKAGE_A).unwrap()),
        );
        let prefab = PrefabDocument::from_ordered_roots(scene.entities.clone());
        let mut anchor = SceneEntityRecord::new(id("instance"));
        anchor.prefab = Some(PrefabInstance {
            source: AssetRef::path("enemy.prefab.json").unwrap(),
            overrides: None,
        });
        scene = SceneDocument::from_ordered_roots(scene.entities.into_iter().chain([anchor]));
        let project = Project::new(&builtins, &scene, "json");
        fs::write(project.root().join(SECOND), scene.to_json_string().unwrap()).unwrap();
        fs::write(
            project.root().join(PREFAB),
            prefab.to_json_string().unwrap(),
        )
        .unwrap();
        let root = project_root(&project);
        let (_source, template) = installed_variant("V1");
        let v1 = install(&root, template.release(), "creation-v1", PACKAGE_A, None);
        Self { project, root, v1 }
    }

    fn install(&self, variant: &str, destination: &str) -> EditorSourceToolInstalledEntry {
        let (_source, template) = installed_variant(variant);
        install(&self.root, template.release(), destination, PACKAGE_A, None)
    }

    fn admit_v1(&self, editor: &mut EditorProjectSession) {
        let before = self.bytes(BUNDLE);
        let bundle = AuthoringSchemaBundle::from_json_bytes(&before).unwrap();
        assert!(bundle.owner(&owner()).is_none());
        let operation = prepare(editor, &self.v1, ComponentSchemaOwnerChange::Add);
        assert_eq!(self.bytes(BUNDLE), before);
        adopt(editor, operation);
        assert_owner(self, 1, None);
        assert_no_execution(editor);
    }

    fn bytes(&self, path: &str) -> Vec<u8> {
        fs::read(self.project.root().join(path)).unwrap()
    }

    fn sources(&self) -> [Vec<u8>; 3] {
        [
            fs::read(self.project.scene_path()).unwrap(),
            self.bytes(SECOND),
            self.bytes(PREFAB),
        ]
    }
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(PACKAGE_A).unwrap()
}

fn wait(editor: &mut EditorProjectSession, ready: impl Fn(&EditorProjectSession) -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !ready(editor) {
        assert!(
            Instant::now() < deadline,
            "Creation-metadata operation did not settle"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn settled_schema(editor: &mut EditorProjectSession) {
    wait(editor, |editor| {
        matches!(
            editor.schema_owner_adoption_view().phase,
            EditorSchemaOwnerAdoptionPhase::Review
                | EditorSchemaOwnerAdoptionPhase::Terminal
                | EditorSchemaOwnerAdoptionPhase::RequiresReconcile
        ) && !editor.current_frame().schema_owner_adoption_work_pending()
    });
}

fn prepare(
    editor: &mut EditorProjectSession,
    installed: &EditorSourceToolInstalledEntry,
    change: ComponentSchemaOwnerChange,
) -> EditorSchemaOwnerAdoptionId {
    let operation = editor
        .prepare_installed_schema_owner(
            &installed.installed.directory,
            installed.installed.receipt,
            change,
        )
        .unwrap();
    settled_schema(editor);
    let view = editor.schema_owner_adoption_view();
    assert_eq!(view.operation, Some(operation));
    assert_eq!(view.change, change);
    assert_eq!(
        view.phase,
        EditorSchemaOwnerAdoptionPhase::Review,
        "{view:?}"
    );
    assert_eq!(view.owner.as_deref(), Some(PACKAGE_A));
    assert_eq!(view.outcome, None);
    assert_eq!(view.degraded_roots, 0);
    assert!(editor.schema_owner_adoption_publication().is_none());
    for document in editor
        .current_frame()
        .workspace()
        .documents
        .iter()
        .filter(|document| document.kind == EditorDocumentKind::Scene)
    {
        assert!(
            !editor
                .schema_owner_adoption_diagnostics(Some(document.document))
                .unwrap()
                .has_errors()
        );
        assert!(
            editor
                .schema_owner_adoption_composition_failure(Some(document.document))
                .is_none()
        );
    }
    assert!(
        !editor
            .schema_owner_adoption_diagnostics(None)
            .unwrap()
            .has_errors()
    );
    assert!(
        editor
            .schema_owner_adoption_composition_failure(None)
            .is_none()
    );
    operation
}

fn adopt(editor: &mut EditorProjectSession, operation: EditorSchemaOwnerAdoptionId) {
    editor.adopt_schema_owner(operation).unwrap();
    settled_schema(editor);
    let view = editor.schema_owner_adoption_view();
    assert_eq!(view.operation, Some(operation));
    assert_eq!(
        view.phase,
        EditorSchemaOwnerAdoptionPhase::Terminal,
        "{view:?}"
    );
    assert_eq!(
        view.outcome,
        Some(EditorSchemaOwnerAdoptionOutcome::Adopted)
    );
    assert!(editor.schema_owner_adoption_publication().is_some());
    for document in editor
        .current_frame()
        .workspace()
        .documents
        .iter()
        .filter(|document| document.kind == EditorDocumentKind::Scene)
    {
        assert!(
            !editor
                .schema_owner_adoption_diagnostics(Some(document.document))
                .unwrap()
                .has_errors()
        );
    }
    editor.acknowledge_schema_owner_adoption(operation).unwrap();
}

fn dispatch(editor: &mut EditorProjectSession, action: EditorAction) -> EditorActionOutcome {
    let revision = editor.current_frame().revision();
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(revision, [action]))
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    report.into_outcomes().into_iter().next().unwrap()
}

fn open_source(editor: &mut EditorProjectSession, path: &str) -> EditorDocumentId {
    wait(editor, |editor| {
        editor
            .current_frame()
            .project_browser()
            .entry(path)
            .is_some()
    });
    let opening = dispatch(
        editor,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
            relative_path: path.into(),
        }),
    );
    let EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Preparing {
        request,
    }) = opening
    else {
        panic!("Public Browser Open refused {path}: {opening:?}");
    };
    wait(editor, |editor| {
        editor.current_frame().document_open().result().is_some()
    });
    let result = editor.current_frame().document_open().result().unwrap();
    let EditorDocumentOpenResult::Opened {
        request: completed,
        document,
    } = result
    else {
        panic!("Public Browser Open failed: {result:?}");
    };
    assert_eq!(completed, request);
    assert!(matches!(
        dispatch(editor, EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen {
            request,
        })),
        EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::OpenAcknowledged {
            request: acknowledged,
        }) if acknowledged == request
    ));
    document
}

fn activate(editor: &mut EditorProjectSession, document: EditorDocumentId) {
    crate::authoring_project::applied(dispatch(
        editor,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document }),
    ));
}

fn replay(editor: &mut EditorProjectSession, document: EditorDocumentId, redo: bool) {
    let previous = editor.current_frame().workspace().active_document.unwrap();
    if previous != document {
        activate(editor, document);
    }
    crate::authoring_project::replay(editor, document, redo);
    if previous != document {
        activate(editor, previous);
    }
}

fn enable(
    editor: &mut EditorProjectSession,
    fixture: &Fixture,
    installed: &EditorSourceToolInstalledEntry,
    host: &EditorSourceToolManagedHost,
) -> EditorSourceToolSelectionView {
    let prepared = editor
        .prepare_installed_source_tool_selection(
            fixture.root.share(),
            &installed.installed.directory,
            installed.installed.receipt,
            host,
        )
        .unwrap();
    assert_eq!(prepared.selection(), &installed.installed);
    let basis = editor.source_tool_selection_view().basis;
    let attempt = editor
        .enable_installed_source_tool(&basis, prepared)
        .unwrap();
    let selection = settle_selection(editor);
    assert!(matches!(selection.operation.as_ref(), Some(operation)
        if operation.attempt == attempt
            && operation.outcome == Some(EditorSourceToolSelectionOutcome::Enabled)));
    assert_eq!(selection.active_origin.as_ref(), Some(&installed.installed));
    assert!(selection.desired.as_ref().unwrap().enabled);
    selection
}

fn disable(editor: &mut EditorProjectSession) -> EditorSourceToolSelectionView {
    let basis = editor.source_tool_selection_view().basis;
    let attempt = editor.disable_installed_source_tool(&basis).unwrap();
    let selection = settle_selection(editor);
    assert!(matches!(selection.disable.as_ref(), Some(operation)
        if operation.attempt == attempt
            && operation.outcome == Some(EditorSourceToolSelectionOutcome::Disabled)));
    settle_tool(editor);
    assert_no_execution(editor);
    assert!(!selection.desired.as_ref().unwrap().enabled);
    assert!(!editor.source_tool_view().has_pending_work());
    selection
}

fn name_patch(value: &str) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id("enemy"),
        component: ComponentTypeId::new(NAME),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("value"),
        value: ComponentValue::String(value.into()),
    }])
}

fn set_name(editor: &mut EditorProjectSession, document: EditorDocumentId, value: &str) {
    let basis = editor.document_query_basis(document).unwrap();
    let prepared = editor
        .prepare_authoring_edit(&basis, &name_patch(value))
        .unwrap();
    crate::authoring_project::applied(editor.commit_authoring_edit(prepared));
    editor.drive_editor_frame(Duration::ZERO);
}

fn set_instance(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    expected: Option<PrefabInstance>,
    value: Option<PrefabInstance>,
) {
    let basis = editor.document_query_basis(document).unwrap();
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
        entity: id("guide"),
        expected,
        value,
    }]);
    let prepared = editor.prepare_authoring_edit(&basis, &patch).unwrap();
    crate::authoring_project::applied(editor.commit_authoring_edit(prepared));
    editor.drive_editor_frame(Duration::ZERO);
}

/// Read-only snapshots exclude Schema classification, the intended change under review.
#[derive(Debug, PartialEq)]
struct SourceState {
    tab: EditorDocumentTabModel,
    entities: Vec<EntityState>,
}

#[derive(Debug, PartialEq)]
struct EntityState {
    entity: SceneEntityId,
    components: Vec<EditorAuthoringRow>,
    properties: Vec<(ComponentTypeId, ComponentFieldId, EditorPropertyValue)>,
}

fn state(editor: &EditorProjectSession, document: EditorDocumentId) -> SourceState {
    let mut tab = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .clone();
    tab.authoring_schema_basis = None;
    let basis = editor.document_query_basis(document).unwrap();
    let page = editor
        .query_document_entities(EditorEntityQuery {
            position: EditorEntityQueryPosition::Start(basis.clone()),
            limits: EditorEntityQueryLimits::new(16, 16).unwrap(),
        })
        .unwrap();
    assert!(page.next.is_none());
    let entities = page
        .entities
        .into_iter()
        .map(|entity| {
            let query = |projection| {
                let page = editor
                    .query_document_authoring(EditorAuthoringQuery {
                        position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                            basis: basis.clone(),
                            entity: entity.clone(),
                            projection,
                        }),
                        limits: EditorAuthoringQueryLimits::new(16, 16, 4096).unwrap(),
                    })
                    .unwrap();
                assert!(page.next.is_none());
                page.rows
            };
            let components = query(EditorAuthoringProjection::Components);
            let mut properties = Vec::new();
            for row in &components {
                let EditorAuthoringRow::Component { component, .. } = row else {
                    unreachable!()
                };
                for root in query(EditorAuthoringProjection::Roots {
                    component: component.clone(),
                }) {
                    let EditorAuthoringRow::Root { field, .. } = root else {
                        unreachable!()
                    };
                    let observation = editor
                        .read_document_property(EditorPropertyRead {
                            target: EditorPropertyTarget {
                                basis: basis.clone(),
                                entity: entity.clone(),
                                component: component.clone(),
                                property: ComponentPropertyPath::new(field.clone()),
                            },
                            limits: EditorPropertyReadLimits::new(4096, 256, 16 * 1024).unwrap(),
                        })
                        .unwrap();
                    properties.push((component.clone(), field, observation.value));
                }
            }
            EntityState {
                entity,
                components,
                properties,
            }
        })
        .collect();
    SourceState { tab, entities }
}

fn selection(editor: &mut EditorProjectSession, document: EditorDocumentId) -> EditorSelectionSet {
    activate(editor, document);
    editor
        .current_frame()
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .selection
        .clone()
}

fn assert_owner(
    fixture: &Fixture,
    generation: u64,
    predecessor: Option<&nara_reflect::ComponentSchemaCatalog>,
) {
    let bundle = AuthoringSchemaBundle::from_json_bytes(&fixture.bytes(BUNDLE)).unwrap();
    let admitted = bundle.owner(&owner()).unwrap();
    assert_eq!(admitted.current().generation(), generation);
    assert_eq!(admitted.predecessor(), predecessor);
}

fn assert_initial(
    editor: &EditorProjectSession,
    document: EditorDocumentId,
    health: i64,
    armor: i64,
) {
    let basis = editor.document_query_basis(document).unwrap();
    assert_eq!(
        basis
            .schema
            .as_ref()
            .unwrap()
            .schema(&ComponentTypeId::new(STATS))
            .unwrap()
            .authoring_initial_value(),
        Some(&ComponentValue::map([
            ("wire_health", ComponentValue::I64(health)),
            ("wire_armor", ComponentValue::I64(armor)),
        ]))
    );
}

fn assert_preserved(
    editor: &EditorProjectSession,
    documents: &[EditorDocumentId],
    before: &[SourceState],
) {
    for (&document, expected) in documents.iter().zip(before) {
        assert_eq!(&state(editor, document), expected);
    }
}

#[test]
fn csharp_installed_creation_metadata_preserves_dirty_scene_prefab_history_and_separate_enablement()
{
    let fixture = Fixture::new();
    let mut editor = fixture.project.open();
    settle_selection(&mut editor);
    let startup = editor.current_frame().workspace().active_document.unwrap();
    let second = open_source(&mut editor, SECOND);
    let prefab = open_source(&mut editor, PREFAB);
    let documents = [startup, second, prefab];
    fixture.admit_v1(&mut editor);
    let v1_bundle = AuthoringSchemaBundle::from_json_bytes(&fixture.bytes(BUNDLE)).unwrap();
    let predecessor = v1_bundle.owner(&owner()).unwrap().current().clone();
    let host = approved_managed_host();
    enable(&mut editor, &fixture, &fixture.v1, &host);
    let old_activation = editor.source_tool_view().active.unwrap().id().clone();
    for (index, &document) in documents.iter().enumerate() {
        activate(&mut editor, document);
        assert_initial(&editor, document, 100, 10);
        let request = review(
            &mut editor,
            document,
            EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            5,
        );
        accept(&mut editor, document, &request);
        set_name(&mut editor, document, &format!("working-{index}"));
        set_name(&mut editor, document, &format!("redo-{index}"));
        replay(&mut editor, document, false);
        crate::authoring_project::applied(dispatch(
            &mut editor,
            EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                document,
                entity: Some(id("enemy")),
            }),
        ));
        assert_history(&editor, document, 2, 1);
        assert_stats(&editor, document, 105);
    }
    let selections = documents.map(|document| selection(&mut editor, document));
    activate(&mut editor, startup);
    assert!(
        editor
            .current_frame()
            .workspace()
            .document(second)
            .unwrap()
            .dirty
    );
    assert!(
        editor
            .current_frame()
            .workspace()
            .document(prefab)
            .unwrap()
            .dirty
    );
    assert!(
        !editor
            .current_frame()
            .workspace()
            .document(second)
            .unwrap()
            .active
    );
    assert!(
        !editor
            .current_frame()
            .workspace()
            .document(prefab)
            .unwrap()
            .active
    );

    let v2 = fixture.install("CREATION_V2", "creation-v2");
    let before = documents.map(|document| state(&editor, document));
    let original_sources = fixture.sources();
    let original_bundle = fixture.bytes(BUNDLE);
    let old_bases = documents.map(|document| editor.document_query_basis(document).unwrap());
    let old_prepared = old_bases
        .iter()
        .map(|basis| {
            editor
                .prepare_authoring_edit(basis, &name_patch("stale-proposal"))
                .unwrap()
        })
        .collect::<Vec<_>>();
    let retained_review = review(
        &mut editor,
        startup,
        EditorSourceToolIntent::Edit {
            entity: id("enemy"),
        },
        40,
    );

    // A current V1 route refuses metadata evolution without changing either durable owner.
    let selection_bytes = fixture.bytes(RECORD_PATH);
    let active_selection = editor.source_tool_selection_view();
    let operation = editor
        .prepare_installed_schema_owner(
            &v2.installed.directory,
            v2.installed.receipt,
            ComponentSchemaOwnerChange::CreationMetadata,
        )
        .unwrap();
    settled_schema(&mut editor);
    assert_eq!(
        editor.schema_owner_adoption_view().outcome,
        Some(EditorSchemaOwnerAdoptionOutcome::Failed(
            EditorSchemaOwnerAdoptionError::ActiveTool
        ))
    );
    assert!(editor.schema_owner_adoption_publication().is_none());
    assert_eq!(editor.source_tool_selection_view(), active_selection);
    assert_eq!(
        editor.source_tool_view().active.unwrap().id(),
        &old_activation
    );
    assert_eq!(
        editor.source_tool_review().unwrap().request(),
        &retained_review
    );
    assert_preserved(&editor, &documents, &before);
    assert_eq!(fixture.bytes(BUNDLE), original_bundle);
    assert_eq!(fixture.bytes(RECORD_PATH), selection_bytes);
    assert_eq!(fixture.sources(), original_sources);
    editor.acknowledge_schema_owner_adoption(operation).unwrap();

    // The invocation remains admitted until the actual task owner observes its return.
    let pending = editor
        .request_source_tool(
            &old_activation,
            &old_bases[0],
            EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(
        editor.prepare_installed_schema_owner(
            &v2.installed.directory,
            v2.installed.receipt,
            ComponentSchemaOwnerChange::CreationMetadata,
        ),
        Err(EditorSchemaOwnerAdoptionError::Busy)
    );
    assert_eq!(fixture.bytes(BUNDLE), original_bundle);
    assert_preserved(&editor, &documents, &before);
    let disabled = disable(&mut editor);
    assert_eq!(
        disabled.desired.as_ref().unwrap().installed,
        fixture.v1.installed
    );
    let disabled_bytes = fixture.bytes(RECORD_PATH);

    let operation = prepare(
        &mut editor,
        &v2,
        ComponentSchemaOwnerChange::CreationMetadata,
    );
    assert_eq!(editor.schema_owner_adoption_view().documents, 3);
    assert_preserved(&editor, &documents, &before);
    assert_eq!(fixture.bytes(BUNDLE), original_bundle);
    assert_eq!(fixture.sources(), original_sources);
    adopt(&mut editor, operation);
    assert_preserved(&editor, &documents, &before);
    assert_eq!(
        editor.current_frame().workspace().active_document,
        Some(startup)
    );
    assert_eq!(fixture.sources(), original_sources);
    assert_eq!(fixture.bytes(RECORD_PATH), disabled_bytes);
    assert_owner(&fixture, 2, Some(&predecessor));
    assert_no_execution(&editor);

    let successor_basis = editor
        .document_query_basis(startup)
        .unwrap()
        .schema
        .unwrap();
    for (index, &document) in documents.iter().enumerate() {
        let basis = editor.document_query_basis(document).unwrap();
        assert_eq!(basis.revision, old_bases[index].revision);
        assert_eq!(basis.schema.as_ref(), Some(&successor_basis));
        assert_ne!(basis.schema, old_bases[index].schema);
        assert_initial(&editor, document, 250, 25);
        assert_stats(&editor, document, 105);
        assert_eq!(
            editor.read_document_property(EditorPropertyRead {
                target: EditorPropertyTarget {
                    basis: old_bases[index].clone(),
                    entity: id("enemy"),
                    component: ComponentTypeId::new(STATS),
                    property: ComponentPropertyPath::new(ComponentFieldId::new("health")),
                },
                limits: EditorPropertyReadLimits::new(1024, 16, 1024).unwrap(),
            }),
            Err(EditorDocumentQueryRejection::StaleSchema)
        );
    }
    for (document, expected) in documents.into_iter().zip(&selections) {
        assert_eq!(&selection(&mut editor, document), expected);
    }
    activate(&mut editor, startup);
    for prepared in old_prepared {
        let EditorActionOutcome::Workspace(report) = editor.commit_authoring_edit(prepared) else {
            panic!("The original prepared proposal must return its owner rejection");
        };
        assert!(!report.applied, "{report:?}");
    }
    for request in [&retained_review, &pending] {
        assert!(matches!(
            editor.accept_source_tool_review(request),
            Err(EditorSourceToolError::Disabled)
        ));
    }
    assert!(
        editor
            .request_source_tool(
                &old_activation,
                &old_bases[0],
                EditorSourceToolIntent::Edit {
                    entity: id("enemy")
                },
                BTreeMap::new()
            )
            .is_err()
    );
    assert_preserved(&editor, &documents, &before);

    // Replay both retained directions through each original Scene/Prefab history owner.
    for (index, &document) in documents.iter().enumerate() {
        replay(&mut editor, document, false);
        replay(&mut editor, document, false);
        assert!(
            !editor
                .current_frame()
                .workspace()
                .document(document)
                .unwrap()
                .dirty
        );
        assert_stats(&editor, document, 100);
        replay(&mut editor, document, true);
        replay(&mut editor, document, true);
        replay(&mut editor, document, true);
        assert_value(
            &editor,
            document,
            "enemy",
            NAME,
            "value",
            ComponentValue::String(format!("redo-{index}")),
        );
        replay(&mut editor, document, false);
        let mut after = state(&editor, document);
        assert_ne!(after.tab.revision, before[index].tab.revision);
        after.tab.revision = before[index].tab.revision;
        assert_eq!(after, before[index]);
    }

    // Creation consumes the new data template while execution is still disabled.
    let basis = editor.document_query_basis(startup).unwrap();
    let mut draft = editor.begin_authoring_construction(&basis).unwrap();
    draft
        .add_component(id("guide"), ComponentTypeId::new(STATS))
        .unwrap();
    let prepared = editor
        .prepare_authoring_construction(&basis, &draft)
        .unwrap();
    let committed = editor.commit_authoring_construction(prepared).unwrap();
    assert!(committed.report().applied);
    for (field, value) in [("health", 250), ("armor", 25)] {
        assert_value(
            &editor,
            startup,
            "guide",
            STATS,
            field,
            ComponentValue::I64(value),
        );
    }
    replay(&mut editor, startup, false);
    let after_undo = state(&editor, startup);
    assert!(!after_undo.entities.iter().find(|entity| entity.entity == id("guide")).unwrap()
        .components.iter().any(|row| matches!(row,
            EditorAuthoringRow::Component { component, .. } if component == &ComponentTypeId::new(STATS))));
    replay(&mut editor, startup, true);
    for &document in &documents {
        save(&mut editor, document);
    }
    let saved_sources = fixture.sources();
    let adopted_bundle = fixture.bytes(BUNDLE);
    close(&mut editor);
    drop(editor);

    // Schema generation 2 and disabled V1 selection are separately durable, before any V2 probe.
    let mut editor = fixture.project.open();
    let observed = settle_selection(&mut editor);
    assert_no_execution(&editor);
    assert_eq!(observed.desired, disabled.desired);
    assert_eq!(fixture.bytes(RECORD_PATH), disabled_bytes);
    assert_eq!(fixture.bytes(BUNDLE), adopted_bundle);
    assert_eq!(fixture.sources(), saved_sources);
    assert_owner(&fixture, 2, Some(&predecessor));
    let startup = editor.current_frame().workspace().active_document.unwrap();
    let second = open_source(&mut editor, SECOND);
    let prefab = open_source(&mut editor, PREFAB);
    for document in [startup, second, prefab] {
        assert_initial(&editor, document, 250, 25);
        assert_stats(&editor, document, 105);
        assert_history(&editor, document, 0, 0);
    }
    assert_value(
        &editor,
        startup,
        "guide",
        STATS,
        "health",
        ComponentValue::I64(250),
    );
    let enabled = enable(&mut editor, &fixture, &v2, &host);
    assert_ne!(
        editor.source_tool_view().active.unwrap().id(),
        &old_activation
    );
    assert_eq!(fixture.bytes(BUNDLE), adopted_bundle);
    for (entity, initial, expected) in [("enemy", 105, 119), ("guide", 250, 264)] {
        let request = review(
            &mut editor,
            startup,
            EditorSourceToolIntent::Edit { entity: id(entity) },
            7,
        );
        accept(&mut editor, startup, &request);
        assert_value(
            &editor,
            startup,
            entity,
            STATS,
            "health",
            ComponentValue::I64(expected),
        );
        replay(&mut editor, startup, false);
        assert_value(
            &editor,
            startup,
            entity,
            STATS,
            "health",
            ComponentValue::I64(initial),
        );
        replay(&mut editor, startup, true);
        assert_value(
            &editor,
            startup,
            entity,
            STATS,
            "health",
            ComponentValue::I64(expected),
        );
    }
    assert_history(&editor, startup, 2, 0);
    save(&mut editor, startup);
    close(&mut editor);
    drop(editor);

    let mut editor = fixture.project.open();
    let reopened = settle_selection(&mut editor);
    assert_no_execution(&editor);
    assert_eq!(reopened.desired, enabled.desired);
    assert_eq!(fixture.bytes(BUNDLE), adopted_bundle);
    let startup = editor.current_frame().workspace().active_document.unwrap();
    assert_initial(&editor, startup, 250, 25);
    assert_value(
        &editor,
        startup,
        "enemy",
        STATS,
        "health",
        ComponentValue::I64(119),
    );
    assert_value(
        &editor,
        startup,
        "guide",
        STATS,
        "health",
        ComponentValue::I64(264),
    );
    assert_value(
        &editor,
        startup,
        "guide",
        STATS,
        "armor",
        ComponentValue::I64(25),
    );
    assert_history(&editor, startup, 0, 0);
    close(&mut editor);
}

#[test]
fn csharp_installed_creation_metadata_rejects_wrong_predecessor_and_stored_schema_changes_without_publication()
 {
    let fixture = Fixture::new();
    let mut editor = fixture.project.open();
    settle_selection(&mut editor);
    fixture.admit_v1(&mut editor);
    let document = editor.current_frame().workspace().active_document.unwrap();
    set_name(&mut editor, document, "retained working state");
    let original_state = state(&editor, document);
    let original_basis = editor.document_query_basis(document).unwrap();
    let original_sources = fixture.sources();
    let original_bundle = fixture.bytes(BUNDLE);
    let original_selection = editor.source_tool_selection_view();
    for (variant, change) in [
        (
            "CREATION_WRONG_PREDECESSOR",
            ComponentSchemaOwnerChange::CreationMetadata,
        ),
        (
            "CREATION_UNSUPPORTED",
            ComponentSchemaOwnerChange::CreationMetadata,
        ),
        ("CREATION_V2", ComponentSchemaOwnerChange::Add),
    ] {
        let installed = fixture.install(variant, &variant.to_ascii_lowercase());
        let operation = editor
            .prepare_installed_schema_owner(
                &installed.installed.directory,
                installed.installed.receipt,
                change,
            )
            .unwrap();
        settled_schema(&mut editor);
        let view = editor.schema_owner_adoption_view();
        assert_eq!(view.change, change);
        assert_eq!(
            view.phase,
            EditorSchemaOwnerAdoptionPhase::Terminal,
            "{view:?}"
        );
        assert_eq!(
            view.outcome,
            Some(EditorSchemaOwnerAdoptionOutcome::Failed(
                EditorSchemaOwnerAdoptionError::Schema
            ))
        );
        assert!(editor.schema_owner_adoption_publication().is_none());
        assert_eq!(state(&editor, document), original_state);
        assert_eq!(
            editor.document_query_basis(document).unwrap(),
            original_basis
        );
        assert_eq!(fixture.bytes(BUNDLE), original_bundle);
        assert_eq!(fixture.sources(), original_sources);
        assert_eq!(editor.source_tool_selection_view(), original_selection);
        assert!(!fixture.project.root().join(RECORD_PATH).exists());
        assert_no_execution(&editor);
        editor.acknowledge_schema_owner_adoption(operation).unwrap();
    }
    save(&mut editor, document);
    close(&mut editor);
}

#[test]
fn csharp_installed_creation_metadata_captures_selected_dirty_prefab_instead_of_broken_saved_dependency()
 {
    let fixture = Fixture::new();
    let mut editor = fixture.project.open();
    settle_selection(&mut editor);
    fixture.admit_v1(&mut editor);
    let startup = editor.current_frame().workspace().active_document.unwrap();
    let prefab = open_source(&mut editor, PREFAB);
    let missing = PrefabInstance {
        source: AssetRef::path("missing-on-disk.prefab.json").unwrap(),
        overrides: None,
    };
    set_instance(&mut editor, prefab, None, Some(missing.clone()));
    save(&mut editor, prefab);
    let broken_disk = fixture.bytes(PREFAB);
    assert!(
        !fixture
            .project
            .root()
            .join("prefabs/missing-on-disk.prefab.json")
            .exists()
    );
    set_instance(&mut editor, prefab, Some(missing), None);
    set_name(&mut editor, prefab, "dirty dependency repair");
    set_name(&mut editor, prefab, "retained redo branch");
    replay(&mut editor, prefab, false);
    activate(&mut editor, startup);
    let before = [state(&editor, startup), state(&editor, prefab)];
    assert!(before[1].tab.dirty);
    assert!(!before[1].tab.active);
    assert_history(&editor, prefab, 3, 1);
    let v2 = fixture.install("CREATION_V2", "creation-capture-v2");

    // The saved Prefab cannot compose. Only the retained dirty repair can produce this review.
    let operation = prepare(
        &mut editor,
        &v2,
        ComponentSchemaOwnerChange::CreationMetadata,
    );
    assert_eq!(editor.schema_owner_adoption_view().documents, 2);
    assert_preserved(&editor, &[startup, prefab], &before);
    assert_eq!(fixture.bytes(PREFAB), broken_disk);
    adopt(&mut editor, operation);
    assert_preserved(&editor, &[startup, prefab], &before);
    assert_eq!(fixture.bytes(PREFAB), broken_disk);
    assert_eq!(
        editor.current_frame().workspace().active_document,
        Some(startup)
    );
    assert_no_execution(&editor);
    for document in [startup, prefab] {
        assert_initial(&editor, document, 250, 25);
    }
    replay(&mut editor, prefab, true);
    assert_value(
        &editor,
        prefab,
        "enemy",
        NAME,
        "value",
        ComponentValue::String("retained redo branch".into()),
    );
    replay(&mut editor, prefab, false);
    replay(&mut editor, prefab, false);
    replay(&mut editor, prefab, false);
    assert!(
        !editor
            .current_frame()
            .workspace()
            .document(prefab)
            .unwrap()
            .dirty
    );
    replay(&mut editor, prefab, true);
    replay(&mut editor, prefab, true);
    let mut restored = state(&editor, prefab);
    restored.tab.revision = before[1].tab.revision;
    assert_eq!(restored, before[1]);
    save(&mut editor, prefab);
    assert_ne!(fixture.bytes(PREFAB), broken_disk);
    close(&mut editor);
}

#[test]
fn csharp_installed_creation_metadata_failed_v2_probe_keeps_new_schema_and_disabled_v1_on_fresh_open()
 {
    let fixture = Fixture::new();
    let v2 = fixture.install("CREATION_INVALID", "creation-invalid");
    let mut editor = fixture.project.open();
    settle_selection(&mut editor);
    fixture.admit_v1(&mut editor);
    let host = approved_managed_host();
    enable(&mut editor, &fixture, &fixture.v1, &host);
    let disabled = disable(&mut editor);
    let selection_bytes = fixture.bytes(RECORD_PATH);
    let operation = prepare(
        &mut editor,
        &v2,
        ComponentSchemaOwnerChange::CreationMetadata,
    );
    adopt(&mut editor, operation);
    let bundle_bytes = fixture.bytes(BUNDLE);
    let document = editor.current_frame().workspace().active_document.unwrap();
    let source = state(&editor, document);
    let basis = editor.document_query_basis(document).unwrap();
    let prepared = editor
        .prepare_installed_source_tool_selection(
            fixture.root.share(),
            &v2.installed.directory,
            v2.installed.receipt,
            &host,
        )
        .unwrap();
    let attempt = editor
        .enable_installed_source_tool(&disabled.basis, prepared)
        .unwrap();
    wait(&mut editor, |editor| {
        let view = editor.source_tool_selection_view();
        view.operation
            .as_ref()
            .is_some_and(|operation| operation.attempt == attempt && operation.outcome.is_some())
            && view.phase == EditorSourceToolSelectionPhase::Idle
            && !editor.source_tool_view().has_pending_work()
    });
    let failed = editor.source_tool_selection_view();
    assert_eq!(
        failed.error,
        Some(EditorSourceToolSelectionError::ProbeFailed)
    );
    assert!(matches!(failed.operation.as_ref(), Some(operation)
        if operation.attempt == attempt && operation.outcome == Some(
            EditorSourceToolSelectionOutcome::Failed(EditorSourceToolSelectionError::ProbeFailed))));
    assert_eq!(
        editor.source_tool_view().attempt_error,
        Some(EditorSourceToolError::Declaration)
    );
    assert_eq!(failed.desired, disabled.desired);
    assert_no_execution(&editor);
    assert_eq!(state(&editor, document), source);
    assert_eq!(editor.document_query_basis(document).unwrap(), basis);
    assert_eq!(fixture.bytes(BUNDLE), bundle_bytes);
    assert_eq!(fixture.bytes(RECORD_PATH), selection_bytes);
    assert_initial(&editor, document, 250, 25);
    assert_stats(&editor, document, 100);
    close(&mut editor);
    drop(editor);

    let mut editor = fixture.project.open();
    let observed = settle_selection(&mut editor);
    assert_no_execution(&editor);
    assert_eq!(observed.desired, disabled.desired);
    assert_eq!(fixture.bytes(BUNDLE), bundle_bytes);
    assert_eq!(fixture.bytes(RECORD_PATH), selection_bytes);
    let document = editor.current_frame().workspace().active_document.unwrap();
    assert_initial(&editor, document, 250, 25);
    assert_stats(&editor, document, 100);
    close(&mut editor);
}
