use std::{collections::BTreeMap, sync::Arc};

use nara_asset::AssetRef;
use nara_core::DocumentContentState;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentRegistryBuilder, ComponentRegistrySnapshot,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentReadiness, AuthoringDocumentSource,
    AuthoringFieldEdit, AuthoringSchemaBasis, PrefabDocument, PrefabDocumentCandidate,
    PrefabEntityTarget, SceneAuthoringHistoryUsage, SceneComponentRecord, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, ScenePlacement, ScenePlacementParent,
    UnavailableRecordManifest,
};

use crate::{
    EditorDocumentId, EditorDocumentSourceBinding, EditorDocumentTabModel, EditorSelectionSet,
    EditorWorkspace, EditorWorkspaceCommand, EditorWorkspaceCommandReport,
    PrefabCompositionCommand, PrefabCompositionEntityDraft, PrefabCompositionOutcome,
};

const HEALTH: &str = "trial.composition.Health";
const TITLE: &str = "trial.composition.Title";
const NO_TEMPLATE: &str = "trial.composition.NoTemplate";
const READ_ONLY: &str = "trial.composition.ReadOnly";
const UNKNOWN: &str = "unavailable.Opaque";
const VERSION: ComponentSchemaVersion = ComponentSchemaVersion(3);

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("trial.composition").unwrap()
}

fn schema(component: &str, storage: &str, kind: ComponentValueKind) -> ComponentSchema {
    ComponentSchema::new(ComponentTypeId::new(component), component, VERSION)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields([storage]),
            kind,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
}

fn health(value: i64) -> SceneComponentRecord {
    SceneComponentRecord::new(
        VERSION,
        ComponentValue::map([("wire_health", ComponentValue::I64(value))]),
    )
}

fn title(value: &str) -> SceneComponentRecord {
    SceneComponentRecord::new(
        VERSION,
        ComponentValue::map([("wire_title", ComponentValue::String(value.into()))]),
    )
}

fn data_basis() -> AuthoringSchemaBasis {
    let mut read_only = schema(READ_ONLY, "wire_title", ComponentValueKind::String)
        .with_authoring_initial_value(title("fixed").value);
    read_only.fields[0].capabilities = [ComponentCapability::Scene, ComponentCapability::Inspect]
        .into_iter()
        .collect();
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![
                schema(HEALTH, "wire_health", ComponentValueKind::I64)
                    .with_authoring_initial_value(health(5).value),
                schema(TITLE, "wire_title", ComponentValueKind::String)
                    .with_authoring_initial_value(title("New entity").value),
                schema(NO_TEMPLATE, "wire_title", ComponentValueKind::String),
                read_only,
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    assert!(adapter.contribution().is_none());
    assert_eq!(adapter.components().len(), 4);
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn open() -> (EditorWorkspace, EditorDocumentId, ComponentRegistrySnapshot) {
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    for component in [HEALTH, TITLE, NO_TEMPLATE, READ_ONLY] {
        assert!(registry.schema(&ComponentTypeId::new(component)).is_none());
    }
    let source = PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("anchor"))
            .with_component(ComponentTypeId::new(HEALTH), health(11).with_owner(owner())),
        SceneEntityRecord::new(id("opaque")).with_component(
            ComponentTypeId::new(UNKNOWN),
            SceneComponentRecord::new(
                ComponentSchemaVersion(7),
                ComponentValue::map([(
                    "untouched",
                    ComponentValue::String("preserve independent opaque data".into()),
                )]),
            )
            .with_owner(PersistentComponentSchemaOwnerId::try_new("unavailable.owner").unwrap()),
        ),
    ]);
    let path = "prefabs/source-composition.prefab.json";
    let published = PrefabDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path(path).unwrap()),
            &data_basis(),
        )
        .unwrap()
        .publish();
    let mut workspace = EditorWorkspace::new();
    let document = workspace
        .open_authoring_prefab_document(
            "Source composition",
            EditorDocumentSourceBinding::new(path),
            published,
        )
        .unwrap()
        .opened_document
        .unwrap();
    let slot = workspace.prefab(document).unwrap();
    assert_eq!(
        slot.authoring_readiness(),
        Some(AuthoringDocumentReadiness::Degraded)
    );
    assert_eq!(slot.unavailable_record_manifest().unwrap().total_count(), 1);
    assert!(
        workspace
            .apply_command(
                &registry,
                EditorWorkspaceCommand::SelectEntity {
                    document,
                    entity: Some(id("anchor")),
                },
            )
            .applied
    );
    (workspace, document, registry)
}

#[derive(Debug, PartialEq)]
struct Snapshot {
    source: Arc<PrefabDocument>,
    tab: EditorDocumentTabModel,
    content: DocumentContentState,
    history_usage: SceneAuthoringHistoryUsage,
    selection: EditorSelectionSet,
    unavailable: UnavailableRecordManifest,
}

fn snapshot(workspace: &EditorWorkspace, document: EditorDocumentId) -> Snapshot {
    let slot = workspace.prefab(document).unwrap();
    Snapshot {
        source: slot.session().shared_document(),
        tab: workspace
            .authoring_model()
            .document(document)
            .unwrap()
            .clone(),
        content: slot.session().content_state(),
        history_usage: slot.session().history_usage(),
        selection: slot.selection().clone(),
        unavailable: slot.unavailable_record_manifest().unwrap().clone(),
    }
}

fn create(
    workspace: &mut EditorWorkspace,
    registry: &ComponentRegistrySnapshot,
    document: EditorDocumentId,
    components: BTreeMap<ComponentTypeId, SceneComponentRecord>,
) -> EditorWorkspaceCommandReport {
    let base_revision = workspace.document_revision(document).unwrap();
    workspace.apply_command(
        registry,
        EditorWorkspaceCommand::ComposePrefab {
            document,
            base_revision,
            command: PrefabCompositionCommand::CreateEntity {
                draft: PrefabCompositionEntityDraft::new(components).unwrap(),
                placement: ScenePlacement::last(ScenePlacementParent::Entity(
                    PrefabEntityTarget::source(id("anchor")),
                )),
            },
        },
    )
}

fn replay(
    workspace: &mut EditorWorkspace,
    registry: &ComponentRegistrySnapshot,
    document: EditorDocumentId,
    undo: bool,
) {
    let base_revision = workspace.document_revision(document).unwrap();
    let command = if undo {
        EditorWorkspaceCommand::Undo {
            document,
            base_revision,
        }
    } else {
        EditorWorkspaceCommand::Redo {
            document,
            base_revision,
        }
    };
    let report = workspace.apply_command(registry, command);
    assert!(report.applied, "{:?}", report.diagnostics);
}

#[test]
fn source_prefab_recipe_creates_exact_data_records_in_one_history_entry() {
    let (mut workspace, document, registry) = open();
    let before = snapshot(&workspace, document);
    let desired = BTreeMap::from([
        (
            ComponentTypeId::new(HEALTH),
            health(105).with_owner(owner()),
        ),
        (ComponentTypeId::new(TITLE), title("Enemy captain")),
    ]);
    let report = create(&mut workspace, &registry, document, desired.clone());
    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(report.document, Some(document));
    let Some(PrefabCompositionOutcome::Created { entity }) = report.prefab_composition_outcome
    else {
        panic!("successful source composition must return its created entity");
    };
    assert!(
        before
            .source
            .entities
            .iter()
            .all(|record| record.id != entity)
    );

    let mut expected = before.source.as_ref().clone();
    expected.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::source(entity.clone()),
        ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
            id("anchor"),
        ))),
    ));
    let mut created = SceneEntityRecord::new(entity.clone());
    created.components = desired
        .into_iter()
        .map(|(component, record)| (component, record.with_owner(owner())))
        .collect();
    expected.entities.push(created);
    expected.canonicalize();
    let after = snapshot(&workspace, document);
    assert_eq!(after.source.as_ref(), &expected);
    assert_eq!(after.unavailable.entries(), before.unavailable.entries());
    assert_eq!(
        after.unavailable.source_placements(),
        after.source.placements
    );
    assert_eq!(after.selection.top_entity(), Some(&entity));
    assert_eq!(after.selection.len(), 1);
    assert_eq!(report.revision, Some(after.tab.revision));
    assert_ne!(after.tab.revision, before.tab.revision);
    assert_eq!(after.tab.history.undo_depth, 1);
    assert_eq!(after.tab.history.redo_depth, 0);
    assert_eq!(after.history_usage.records, 1);

    replay(&mut workspace, &registry, document, true);
    let undone = snapshot(&workspace, document);
    assert_eq!(undone.source, before.source);
    assert_eq!(undone.content, before.content);
    assert_eq!(undone.unavailable, before.unavailable);
    assert_eq!(undone.tab.history.undo_depth, 0);
    assert_eq!(undone.tab.history.redo_depth, 1);
    assert!(undone.selection.is_empty());

    replay(&mut workspace, &registry, document, false);
    let redone = snapshot(&workspace, document);
    assert_eq!(redone.source, after.source);
    assert_eq!(redone.content, after.content);
    assert_eq!(redone.unavailable, after.unavailable);
    assert_eq!(redone.tab.history.undo_depth, 1);
    assert_eq!(redone.tab.history.redo_depth, 0);
    assert_eq!(redone.history_usage.records, 1);
}

#[test]
fn empty_source_prefab_recipe_creates_an_empty_entity_and_replays_exactly() {
    let (mut workspace, document, registry) = open();
    let before = snapshot(&workspace, document);
    let report = create(&mut workspace, &registry, document, BTreeMap::new());
    assert!(report.applied, "{:?}", report.diagnostics);
    let Some(PrefabCompositionOutcome::Created { entity }) = report.prefab_composition_outcome
    else {
        panic!("an empty source recipe still returns its created entity");
    };
    let after = snapshot(&workspace, document);
    let created = after
        .source
        .entities
        .iter()
        .find(|record| record.id == entity)
        .unwrap();
    assert_eq!(created, &SceneEntityRecord::new(entity.clone()));
    assert_eq!(
        after.source.entities.len(),
        before.source.entities.len() + 1
    );
    assert_eq!(
        after
            .source
            .local_hierarchy(Default::default())
            .unwrap()
            .roots(),
        before
            .source
            .local_hierarchy(Default::default())
            .unwrap()
            .roots(),
    );
    assert_eq!(after.selection.top_entity(), Some(&entity));
    assert_eq!(after.unavailable.entries(), before.unavailable.entries());
    assert_eq!(
        after.unavailable.source_placements(),
        after.source.placements
    );
    assert_eq!(after.tab.history.undo_depth, 1);
    assert_eq!(after.history_usage.records, 1);

    replay(&mut workspace, &registry, document, true);
    assert_eq!(snapshot(&workspace, document).source, before.source);
    replay(&mut workspace, &registry, document, false);
    assert_eq!(snapshot(&workspace, document).source, after.source);
}

#[test]
fn invalid_source_prefab_recipes_preserve_source_selection_and_both_history_branches() {
    let (mut workspace, document, registry) = open();
    let original = snapshot(&workspace, document);
    let mut checkpoints = Vec::new();
    for value in [30, 31] {
        let base_revision = workspace.document_revision(document).unwrap();
        let report = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::ApplyAuthoringFieldEdit {
                document,
                base_revision,
                edit: AuthoringFieldEdit::single(
                    id("anchor"),
                    ComponentTypeId::new(HEALTH),
                    VERSION,
                    ComponentFieldId::new("value"),
                    ComponentValue::I64(value),
                )
                .unwrap(),
            },
        );
        assert!(report.applied, "{:?}", report.diagnostics);
        checkpoints.push(snapshot(&workspace, document).source);
    }
    replay(&mut workspace, &registry, document, true);
    let before = snapshot(&workspace, document);
    assert_eq!(before.tab.history.undo_depth, 1);
    assert_eq!(before.tab.history.redo_depth, 1);
    assert_eq!(before.selection.top_entity(), Some(&id("anchor")));

    let mut wrong_version = title("Enemy captain");
    wrong_version.version = ComponentSchemaVersion(4);
    let foreign_owner = title("Enemy captain")
        .with_owner(PersistentComponentSchemaOwnerId::try_new("foreign.owner").unwrap());
    let extra_value = SceneComponentRecord::new(
        VERSION,
        ComponentValue::map([
            ("wire_title", ComponentValue::String("Enemy captain".into())),
            (
                "opaque",
                ComponentValue::String("must not be dropped".into()),
            ),
        ]),
    );
    for (case, component, record) in [
        ("missing template", NO_TEMPLATE, title("Enemy captain")),
        ("wrong version", TITLE, wrong_version),
        ("foreign owner", TITLE, foreign_owner),
        ("extra opaque value", TITLE, extra_value),
        ("read-only field change", READ_ONLY, title("changed")),
        ("unknown component", UNKNOWN, title("Enemy captain")),
        (
            "missing required value",
            TITLE,
            SceneComponentRecord::new(VERSION, ComponentValue::Map(BTreeMap::new())),
        ),
        (
            "invalid field kind",
            TITLE,
            SceneComponentRecord::new(
                VERSION,
                ComponentValue::map([("wire_title", ComponentValue::I64(99))]),
            ),
        ),
    ] {
        // The first component is valid so a later refusal must discard the whole proposal.
        let report = create(
            &mut workspace,
            &registry,
            document,
            BTreeMap::from([
                (ComponentTypeId::new(HEALTH), health(105)),
                (ComponentTypeId::new(component), record),
            ]),
        );
        assert!(!report.applied, "{case}");
        assert!(
            report.diagnostics.has_errors(),
            "{case}: {:?}",
            report.diagnostics
        );
        assert!(report.prefab_composition_outcome.is_none(), "{case}");
        assert_eq!(report.document, Some(document), "{case}");
        let after = snapshot(&workspace, document);
        assert_eq!(after, before, "{case}");
        assert!(Arc::ptr_eq(&after.source, &before.source), "{case}");
    }

    replay(&mut workspace, &registry, document, true);
    assert_eq!(snapshot(&workspace, document).source, original.source);
    replay(&mut workspace, &registry, document, false);
    assert_eq!(snapshot(&workspace, document).source, checkpoints[0]);
    replay(&mut workspace, &registry, document, false);
    assert_eq!(snapshot(&workspace, document).source, checkpoints[1]);
    let redone = snapshot(&workspace, document);
    assert_eq!(redone.tab.history.undo_depth, 2);
    assert_eq!(redone.tab.history.redo_depth, 0);
    assert_eq!(redone.history_usage.records, 2);
}
