use super::*;

mod input;
mod recoverable_binding;
mod retained_sources;

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentPropertyReadBudget,
    ComponentPropertyStep, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
    ComponentShapeMember, ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::{AuthoredSchemaUpgradeLimits, AuthoringAdapterReceipt},
    AuthoringDocumentSource, AuthoringFieldEdit, PrefabDocument, SceneAuthoringHistoryUsage,
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
};

use crate::workspace::{
    EditorDocumentKind, EditorDocumentTabModel, EditorPersistenceCommit, EditorSelectionSet,
    EditorWorkspaceCommand,
};

const COMPONENT: &str = "trial.enemy.EnemyStats";

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("trial.enemy").unwrap()
}

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn schema(next: bool) -> ComponentSchema {
    let health = if next {
        ComponentValueShape::Record {
            members: vec![
                ComponentShapeMember::new("milli", ComponentValueKind::I64)
                    .with_id(ComponentFieldId::new("milli")),
            ],
        }
    } else {
        ComponentValueShape::scalar(ComponentValueKind::I64)
    };
    ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Enemy stats",
        ComponentSchemaVersion(if next { 2 } else { 1 }),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("health"),
            "Health",
            ComponentFieldPath::from_fields(["wire_health"]),
            health,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("armor"),
            "Armor",
            ComponentFieldPath::from_fields(["armor"]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
}

fn basis(
    catalog: ComponentSchemaCatalog,
    previous: Option<ComponentSchemaCatalog>,
) -> AuthoringSchemaBasis {
    let bundle =
        AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(owner(), catalog, previous)])
            .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

struct Fixture {
    previous: AuthoringSchemaBasis,
    successor: AuthoringSchemaBasis,
    upgrade: AuthoredFieldUpgrade,
}

impl Fixture {
    fn new() -> Self {
        let old = ComponentSchemaCatalog {
            components: vec![schema(false)],
            ..Default::default()
        };
        let mut next = ComponentSchemaCatalog::successor_of(&old).unwrap();
        next.components.push(schema(true));
        let previous = basis(old.clone(), None);
        let successor = basis(next, Some(old));
        let upgrade = AuthoredFieldUpgrade::prepare(
            previous.clone(),
            successor.clone(),
            ComponentTypeId::new(COMPONENT),
            ComponentFieldId::new("health"),
            ComponentPropertyPath::new(ComponentFieldId::new("health")).with_steps([
                ComponentPropertyStep::Member(ComponentFieldId::new("milli")),
            ]),
            &mut ComponentPropertyReadBudget::new(
                ItemLimit::new(4096).unwrap(),
                ByteLimit::new(1024 * 1024).unwrap(),
            ),
        )
        .unwrap();
        Self {
            previous,
            successor,
            upgrade,
        }
    }

    fn open(
        &self,
    ) -> (
        EditorWorkspace,
        EditorPersistenceAuthority,
        [EditorDocumentId; 3],
    ) {
        let (mut workspace, authority) = EditorWorkspace::new_hosted();
        let documents = [
            EditorDocumentKind::Scene,
            EditorDocumentKind::Scene,
            EditorDocumentKind::Prefab,
        ]
        .map(|kind| open(&mut workspace, kind, &self.previous));
        for document in documents {
            set_armor(&mut workspace, document, 8, 1);
            set_armor(&mut workspace, document, 9, 1);
            replay(&mut workspace, document, true);
            select(&mut workspace, document, "enemy");
        }
        assert!(workspace.activate_document(documents[0]).applied);
        (workspace, authority, documents)
    }

    fn prepare(
        &self,
        workspace: &EditorWorkspace,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> PreparedEditorWorkspaceAuthoredSchemaUpgrade {
        let mut proposals = proposals(workspace);
        proposals.reverse();
        workspace
            .prepare_authored_schema_upgrade(&self.upgrade, proposals, work)
            .unwrap()
    }
}

fn document() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("enemy")).with_component(
            ComponentTypeId::new(COMPONENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    ("wire_health", ComponentValue::I64(105)),
                    ("armor", ComponentValue::I64(7)),
                ]),
            )
            .with_owner(owner()),
        ),
        SceneEntityRecord::new(id("opaque")).with_component(
            ComponentTypeId::new("missing.Opaque"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("retain this data".into()),
            ),
        ),
    ])
}

fn open(
    workspace: &mut EditorWorkspace,
    kind: EditorDocumentKind,
    basis: &AuthoringSchemaBasis,
) -> EditorDocumentId {
    let name = format!("source-{}", workspace.len());
    let source = document();
    match kind {
        EditorDocumentKind::Scene => {
            let path = format!("scenes/{name}.scene.json");
            let published =
                SceneDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
                    .unwrap()
                    .admit_authoring_with_basis(
                        AuthoringDocumentSource::new(AssetRef::path(&path).unwrap()),
                        basis,
                    )
                    .unwrap()
                    .publish();
            workspace
                .open_authoring_scene_document(
                    name,
                    EditorDocumentSourceBinding::new(path),
                    published,
                )
                .unwrap()
                .opened_document
                .unwrap()
        }
        EditorDocumentKind::Prefab => {
            let path = format!("prefabs/{name}.prefab.json");
            let source = PrefabDocument::new(source.placements, source.entities);
            let published =
                PrefabDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
                    .unwrap()
                    .admit_authoring_with_basis(
                        AuthoringDocumentSource::new(AssetRef::path(&path).unwrap()),
                        basis,
                    )
                    .unwrap()
                    .publish();
            workspace
                .open_authoring_prefab_document(
                    name,
                    EditorDocumentSourceBinding::new(path),
                    published,
                )
                .unwrap()
                .opened_document
                .unwrap()
        }
        EditorDocumentKind::Extension(_) => unreachable!(),
    }
}

fn source_key(
    binding: &EditorDocumentSourceBinding,
    kind: SourceDocumentKind,
) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "04deef6c-d2c5-4fcf-905c-2793204b9512".parse().unwrap(),
        binding.locator(),
        kind,
    )
    .unwrap()
}

fn upgraded(entities: &mut [SceneEntityRecord]) {
    let record = entities
        .iter_mut()
        .find(|entity| entity.id == id("enemy"))
        .unwrap()
        .components
        .get_mut(&ComponentTypeId::new(COMPONENT))
        .unwrap();
    let health = record
        .value
        .field_i64("wire_health")
        .unwrap()
        .checked_mul(1000)
        .unwrap();
    let armor = record.value.field_i64("armor").unwrap();
    record.version = ComponentSchemaVersion(2);
    record.value = ComponentValue::map([
        (
            "wire_health",
            ComponentValue::map([("milli", ComponentValue::I64(health))]),
        ),
        ("armor", ComponentValue::I64(armor)),
    ]);
}

fn proposals(workspace: &EditorWorkspace) -> Vec<EditorAuthoredSchemaUpgradeProposal> {
    workspace
        .documents
        .iter()
        .map(|(&document, slot)| match slot {
            EditorDocumentSlot::Scene(slot) => {
                let binding = slot.source_binding().unwrap().clone();
                let mut candidate = slot.session().document().clone();
                upgraded(&mut candidate.entities);
                EditorAuthoredSchemaUpgradeProposal::scene(
                    document,
                    source_key(&binding, SourceDocumentKind::Scene),
                    binding,
                    slot.revision(),
                    SceneDocumentCandidate::decode_json_str(&candidate.to_json_string().unwrap())
                        .unwrap(),
                )
            }
            EditorDocumentSlot::Prefab(slot) => {
                let binding = slot.source_binding().clone();
                let mut candidate = slot.session().document().clone();
                upgraded(&mut candidate.entities);
                EditorAuthoredSchemaUpgradeProposal::prefab(
                    document,
                    source_key(&binding, SourceDocumentKind::Prefab),
                    binding,
                    slot.revision(),
                    PrefabDocumentCandidate::decode_json_str(&candidate.to_json_string().unwrap())
                        .unwrap(),
                )
            }
            EditorDocumentSlot::Extension(_) => unreachable!(),
        })
        .collect()
}

fn frozen_proposals(
    input: &EditorWorkspaceAuthoredSchemaUpgradeInput,
) -> Vec<EditorAuthoredSchemaUpgradeProposal> {
    input
        .documents()
        .map(|document| {
            let binding = input.document_source_binding(document).unwrap().clone();
            let revision = input.document_revision(document).unwrap();
            if let Some(source) = input.scene_document(document) {
                let mut candidate = source.clone();
                upgraded(&mut candidate.entities);
                EditorAuthoredSchemaUpgradeProposal::scene(
                    document,
                    source_key(&binding, SourceDocumentKind::Scene),
                    binding,
                    revision,
                    SceneDocumentCandidate::decode_json_str(&candidate.to_json_string().unwrap())
                        .unwrap(),
                )
            } else {
                let mut candidate = input.prefab_document(document).unwrap().clone();
                upgraded(&mut candidate.entities);
                EditorAuthoredSchemaUpgradeProposal::prefab(
                    document,
                    source_key(&binding, SourceDocumentKind::Prefab),
                    binding,
                    revision,
                    PrefabDocumentCandidate::decode_json_str(&candidate.to_json_string().unwrap())
                        .unwrap(),
                )
            }
        })
        .collect()
}

fn armor_patch(value: i64, version: u32) -> ScenePatchDocument {
    AuthoringFieldEdit::single(
        id("enemy"),
        ComponentTypeId::new(COMPONENT),
        ComponentSchemaVersion(version),
        ComponentFieldId::new("armor"),
        ComponentValue::I64(value),
    )
    .unwrap()
    .to_patch()
}

fn set_armor(
    workspace: &mut EditorWorkspace,
    document: EditorDocumentId,
    value: i64,
    version: u32,
) {
    let basis = workspace.document_query_basis(document).unwrap();
    let prepared = workspace
        .prepare_authoring_edit(&basis, &armor_patch(value, version))
        .unwrap();
    let report = workspace.commit_authoring_edit(prepared);
    assert!(report.applied, "{:?}", report.diagnostics);
}

fn replay(workspace: &mut EditorWorkspace, document: EditorDocumentId, undo: bool) {
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
    let report = workspace.apply_authoring_command(command);
    assert!(report.applied, "{:?}", report.diagnostics);
}

fn select(workspace: &mut EditorWorkspace, document: EditorDocumentId, entity: &str) {
    assert!(
        workspace
            .apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
                document,
                entity: Some(id(entity)),
            })
            .applied
    );
}

fn digest(bytes: &[u8]) -> EditorDocumentDigest {
    let digest = nara_fs::ContentDigest::of_bytes(bytes);
    EditorDocumentDigest::new(digest.length(), *digest.as_bytes())
}

// These owner tests supply explicit observations; they do not claim actual filesystem proof.
fn observations(
    prepared: &PreparedEditorWorkspaceAuthoredSchemaUpgrade,
) -> Vec<EditorAuthoredUpgradeSavedSource> {
    prepared
        .documents()
        .map(|document| {
            let binding = prepared.source_binding(document).unwrap().clone();
            if let Some(source) = prepared.scene(document) {
                EditorAuthoredUpgradeSavedSource::new(
                    document,
                    source.source_key().clone(),
                    binding,
                    source.revision(),
                    source.content_state(),
                    digest(source.document().to_json_string().unwrap().as_bytes()),
                )
            } else {
                let source = prepared.prefab(document).unwrap();
                EditorAuthoredUpgradeSavedSource::new(
                    document,
                    source.source_key().clone(),
                    binding,
                    source.revision(),
                    source.content_state(),
                    digest(source.document().to_json_string().unwrap().as_bytes()),
                )
            }
        })
        .collect()
}

#[derive(Debug, PartialEq)]
enum SourceSnapshot {
    Scene(Arc<SceneDocument>),
    Prefab(Arc<PrefabDocument>),
}

#[derive(Debug, PartialEq)]
struct SlotSnapshot {
    tab: EditorDocumentTabModel,
    source: SourceSnapshot,
    address: usize,
    content: SceneAuthoringContentState,
    saved_content: SceneAuthoringContentState,
    digest: Option<EditorDocumentDigest>,
    selection: EditorSelectionSet,
    usage: SceneAuthoringHistoryUsage,
    upgrade_required: bool,
}

fn snapshot(workspace: &EditorWorkspace) -> Vec<SlotSnapshot> {
    workspace
        .authoring_model()
        .documents
        .into_iter()
        .map(|tab| {
            let (
                source,
                address,
                content,
                saved_content,
                digest,
                selection,
                usage,
                upgrade_required,
            ) = match &workspace.documents[&tab.document] {
                EditorDocumentSlot::Scene(slot) => {
                    let source = slot.session().shared_document();
                    let address = Arc::as_ptr(&source) as usize;
                    (
                        SourceSnapshot::Scene(source),
                        address,
                        slot.session().content_state(),
                        slot.saved_content_state(),
                        slot.saved_digest(),
                        slot.selection().clone(),
                        slot.session().history_usage(),
                        slot.session().source_upgrade_required(),
                    )
                }
                EditorDocumentSlot::Prefab(slot) => {
                    let source = slot.session().shared_document();
                    let address = Arc::as_ptr(&source) as usize;
                    (
                        SourceSnapshot::Prefab(source),
                        address,
                        slot.session().content_state(),
                        slot.saved_content_state(),
                        slot.saved_digest(),
                        slot.selection().clone(),
                        slot.session().history_usage(),
                        slot.session().source_upgrade_required(),
                    )
                }
                EditorDocumentSlot::Extension(_) => unreachable!(),
            };
            SlotSnapshot {
                tab,
                source,
                address,
                content,
                saved_content,
                digest,
                selection,
                usage,
                upgrade_required,
            }
        })
        .collect()
}

fn assert_refused(report: &EditorWorkspaceCommandReport) {
    assert!(!report.applied);
    assert!(report.diagnostics.has_errors(), "{report:?}");
}

#[test]
fn complete_dirty_set_installs_future_checkpoints_preserves_current_navigation_and_rejects_old_work()
 {
    let fixture = Fixture::new();
    let (mut workspace, mut authority, documents) = fixture.open();
    let old_queries = documents.map(|document| workspace.document_query_basis(document).unwrap());
    let old_edits = old_queries
        .iter()
        .map(|basis| {
            workspace
                .prepare_authoring_edit(basis, &armor_patch(10, 1))
                .unwrap()
        })
        .collect::<Vec<_>>();
    let old_saves = documents.map(|document| authority.capture(&workspace, document).unwrap());
    let before_preparation = snapshot(&workspace);
    assert!(before_preparation.iter().all(|slot| slot.tab.dirty
        && slot.tab.history.undo_depth == 1
        && slot.tab.history.redo_depth == 1));
    let mut work = AuthoredSchemaUpgradeWork::default();
    let prepared = fixture.prepare(&workspace, &mut work);
    assert!(prepared.usage().work_bytes > 0);
    assert_eq!(prepared.schema_basis(), &fixture.successor);
    assert_eq!(snapshot(&workspace), before_preparation);
    let future_sources = prepared
        .documents()
        .map(|document| {
            if let Some(source) = prepared.scene(document) {
                SourceSnapshot::Scene(source.shared_document())
            } else {
                SourceSnapshot::Prefab(prepared.prefab(document).unwrap().shared_document())
            }
        })
        .collect::<Vec<_>>();
    let mut saved = observations(&prepared);
    let future_checkpoints = saved
        .iter()
        .map(|saved| (saved.revision, saved.content_state, saved.digest))
        .collect::<Vec<_>>();
    // Navigation and titles may change during review; preserve the current values at adoption.
    assert!(workspace.activate_document(documents[2]).applied);
    for document in documents {
        select(&mut workspace, document, "opaque");
    }
    if let EditorDocumentSlot::Scene(slot) = workspace.documents.get_mut(&documents[1]).unwrap() {
        slot.title = "Current title".into();
    }
    assert!(
        workspace
            .apply_authoring_command(EditorWorkspaceCommand::MarkExternalChanged {
                document: documents[1]
            })
            .applied
    );
    let current = snapshot(&workspace);
    saved.reverse();
    authority
        .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut work)
        .unwrap()
        .commit();
    let after = snapshot(&workspace);
    for (((before, after), source), (revision, content, digest)) in current
        .iter()
        .zip(&after)
        .zip(future_sources)
        .zip(future_checkpoints)
    {
        assert_eq!(after.tab.document, before.tab.document);
        assert_eq!(after.tab.title, before.tab.title);
        assert_eq!(after.tab.source_binding, before.tab.source_binding);
        assert_eq!(after.tab.active, before.tab.active);
        assert_eq!(after.selection, before.selection);
        assert_eq!(after.source, source);
        let expected_address = match &source {
            SourceSnapshot::Scene(source) => Arc::as_ptr(source) as usize,
            SourceSnapshot::Prefab(source) => Arc::as_ptr(source) as usize,
        };
        assert_eq!(after.address, expected_address);
        assert_ne!(
            after.tab.revision.source_id(),
            before.tab.revision.source_id()
        );
        assert_eq!(after.tab.revision, revision);
        assert_eq!(after.tab.saved_revision, revision);
        assert_eq!(after.content, content);
        assert_eq!(after.saved_content, content);
        assert_eq!(after.digest, Some(digest));
        assert_eq!(
            after.tab.authoring_schema_basis.as_ref(),
            Some(&fixture.successor)
        );
        assert_eq!(after.tab.history, Default::default());
        assert_eq!(after.usage, Default::default());
        assert_eq!(after.tab.external_reload, EditorExternalReloadState::Clean);
        assert!(!after.tab.dirty);
        assert!(!after.upgrade_required);
    }
    for query in old_queries {
        assert!(workspace.validate_document_query_basis(&query).is_err());
    }
    for edit in old_edits {
        assert_refused(&workspace.commit_authoring_edit(edit));
    }
    for checkpoint in old_saves {
        assert_refused(&authority.commit(
            &mut workspace,
            EditorPersistenceCommit {
                checkpoint,
                digest: EditorDocumentDigest::new(19, [0x11; 32]),
            },
        ));
    }
    assert_eq!(snapshot(&workspace), after);
    for document in documents {
        let saved_content = workspace.documents[&document]
            .saved_revision()
            .content_state();
        set_armor(&mut workspace, document, 10, 2);
        assert_eq!(workspace.document_is_dirty(document), Some(true));
        replay(&mut workspace, document, true);
        assert_eq!(workspace.document_is_dirty(document), Some(false));
        let content = match &workspace.documents[&document] {
            EditorDocumentSlot::Scene(slot) => slot.session().content_state(),
            EditorDocumentSlot::Prefab(slot) => slot.session().content_state(),
            EditorDocumentSlot::Extension(_) => unreachable!(),
        };
        assert_eq!(content, saved_content);
    }
    assert_eq!(workspace.active_document(), Some(documents[2]));
}

#[test]
fn invalid_proposal_membership_kind_key_binding_revision_and_basis_preserve_both_history_branches()
{
    for change in [
        "missing",
        "duplicate-document",
        "key-kind",
        "duplicate-key",
        "binding",
        "revision",
        "basis",
        "candidate-kind",
    ] {
        let fixture = Fixture::new();
        let (workspace, _, documents) = fixture.open();
        let mut proposals = proposals(&workspace);
        match change {
            "missing" => {
                proposals.pop();
            }
            "duplicate-document" => proposals[2].document = documents[0],
            "key-kind" => {
                proposals[0].source_key =
                    source_key(&proposals[0].source_binding, SourceDocumentKind::Prefab)
            }
            "duplicate-key" => proposals[1].source_key = proposals[0].source_key.clone(),
            "binding" => {
                proposals[2].source_binding = EditorDocumentSourceBinding::new("prefabs/moved.json")
            }
            "revision" => proposals[2].base_revision = proposals[2].base_revision.next(),
            "candidate-kind" => {
                proposals[0].candidate = ProposalCandidate::Prefab(
                    PrefabDocumentCandidate::decode_json_str(
                        &PrefabDocument::default().to_json_string().unwrap(),
                    )
                    .unwrap(),
                )
            }
            "basis" => {}
            _ => unreachable!(),
        }
        let foreign = Fixture::new();
        let upgrade = if change == "basis" {
            &foreign.upgrade
        } else {
            &fixture.upgrade
        };
        let before = snapshot(&workspace);
        assert_refused(
            &workspace
                .prepare_authored_schema_upgrade(
                    upgrade,
                    proposals,
                    &mut AuthoredSchemaUpgradeWork::default(),
                )
                .unwrap_err(),
        );
        assert_eq!(snapshot(&workspace), before, "{change}");
    }
}

#[test]
fn frozen_scene_or_prefab_preparation_rejects_stale_live_set_without_partial_installation() {
    for change in [
        "scene-edit",
        "prefab-edit",
        "undo-redo",
        "open",
        "close",
        "replace",
        "scene-binding",
        "prefab-binding",
        "reopen",
    ] {
        let fixture = Fixture::new();
        let (mut workspace, mut authority, documents) = fixture.open();
        let mut work = AuthoredSchemaUpgradeWork::default();
        let input = workspace
            .capture_authored_schema_upgrade_input(&fixture.previous, &mut work)
            .unwrap();
        match change {
            "scene-edit" => set_armor(&mut workspace, documents[1], 10, 1),
            "prefab-edit" => set_armor(&mut workspace, documents[2], 10, 1),
            "undo-redo" => {
                replay(&mut workspace, documents[1], true);
                replay(&mut workspace, documents[1], false);
            }
            "open" => {
                open(&mut workspace, EditorDocumentKind::Scene, &fixture.previous);
            }
            "close" => {
                assert!(workspace.__discard_document(documents[2]).applied);
            }
            "replace" => {
                assert!(workspace.__discard_document(documents[2]).applied);
                open(
                    &mut workspace,
                    EditorDocumentKind::Prefab,
                    &fixture.previous,
                );
            }
            "scene-binding" => {
                if let EditorDocumentSlot::Scene(slot) =
                    workspace.documents.get_mut(&documents[1]).unwrap()
                {
                    slot.source_binding =
                        Some(EditorDocumentSourceBinding::new("scenes/moved.json"));
                }
            }
            "prefab-binding" => {
                if let EditorDocumentSlot::Prefab(slot) =
                    workspace.documents.get_mut(&documents[2]).unwrap()
                {
                    slot.source_binding = EditorDocumentSourceBinding::new("prefabs/moved.json");
                }
            }
            "reopen" => {
                let slot = workspace.scene(documents[1]).unwrap();
                let session = slot
                    .session()
                    .try_reopen_authoring_candidate(
                        SceneDocumentCandidate::decode_json_str(
                            &slot.session().document().to_json_string().unwrap(),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                let revision = slot.revision();
                authority
                    .publish_reopened_session(
                        &mut workspace,
                        documents[1],
                        revision,
                        session,
                        EditorDocumentDigest::new(19, [0x31; 32]),
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let before = snapshot(&workspace);
        // Preparation uses the captured sources even after their live Sessions change. The
        // actual complete-set validation and binder must still refuse the changed incumbents.
        let prepared = input
            .prepare(&fixture.upgrade, frozen_proposals(&input), &mut work)
            .unwrap();
        let saved = observations(&prepared);
        assert_refused(&prepared.validate(&workspace, &mut work).unwrap_err());
        assert_refused(
            authority
                .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut work)
                .unwrap_err()
                .report(),
        );
        assert_eq!(snapshot(&workspace), before, "{change}");
    }
}

#[test]
fn foreign_authority_or_workspace_cannot_bind_matching_raw_document_ids() {
    for foreign_workspace in [false, true] {
        let fixture = Fixture::new();
        let (mut original, _, _) = fixture.open();
        let (mut other, mut other_authority, _) = fixture.open();
        let mut work = AuthoredSchemaUpgradeWork::default();
        let prepared = fixture.prepare(&original, &mut work);
        let saved = observations(&prepared);
        let original_before = snapshot(&original);
        let other_before = snapshot(&other);
        let target = if foreign_workspace {
            &mut other
        } else {
            &mut original
        };
        assert_refused(
            other_authority
                .bind_authored_schema_upgrade(target, prepared, saved, &mut work)
                .unwrap_err()
                .report(),
        );
        assert_eq!(snapshot(&original), original_before);
        assert_eq!(snapshot(&other), other_before);
    }
}

#[test]
fn every_saved_observation_is_checked_before_any_source_or_saved_field_changes() {
    for change in [
        "missing",
        "duplicate",
        "document",
        "key",
        "binding",
        "revision",
        "content",
        "encoded-limit",
    ] {
        let fixture = Fixture::new();
        let (mut workspace, mut authority, documents) = fixture.open();
        let mut work = AuthoredSchemaUpgradeWork::default();
        let prepared = fixture.prepare(&workspace, &mut work);
        let mut saved = observations(&prepared);
        match change {
            "missing" => {
                saved.pop();
            }
            "duplicate" => saved[2].document = documents[0],
            "document" => saved[2].document = EditorDocumentId::from_raw(9001),
            "key" => {
                saved[2].source_key = source_key(
                    &EditorDocumentSourceBinding::new("other.prefab.json"),
                    SourceDocumentKind::Prefab,
                )
            }
            "binding" => {
                saved[2].source_binding = EditorDocumentSourceBinding::new("prefabs/moved.json")
            }
            "revision" => saved[2].revision = workspace.document_revision(documents[2]).unwrap(),
            "content" => {
                saved[2].content_state = workspace
                    .prefab(documents[2])
                    .unwrap()
                    .session()
                    .content_state()
            }
            "encoded-limit" => saved[2].digest = EditorDocumentDigest::new(u64::MAX, [0x51; 32]),
            _ => unreachable!(),
        }
        let before = snapshot(&workspace);
        assert_refused(
            authority
                .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut work)
                .unwrap_err()
                .report(),
        );
        assert_eq!(snapshot(&workspace), before, "{change}");
    }
}

#[test]
fn cancelling_preparation_or_dropping_all_bound_guards_preserves_real_undo_and_redo() {
    let fixture = Fixture::new();
    let (mut workspace, mut authority, documents) = fixture.open();
    let before = snapshot(&workspace);
    let mut work = AuthoredSchemaUpgradeWork::default();
    drop(fixture.prepare(&workspace, &mut work));
    assert_eq!(snapshot(&workspace), before);
    let prepared = fixture.prepare(&workspace, &mut work);
    let saved = observations(&prepared);
    drop(
        authority
            .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut work)
            .unwrap(),
    );
    assert_eq!(snapshot(&workspace), before);
    for document in documents {
        replay(&mut workspace, document, false);
        replay(&mut workspace, document, true);
        replay(&mut workspace, document, true);
        assert_eq!(workspace.document_is_dirty(document), Some(false));
        replay(&mut workspace, document, false);
        assert_eq!(workspace.document_is_dirty(document), Some(true));
    }
}

fn work_with_limits(work: usize, retained: usize) -> AuthoredSchemaUpgradeWork {
    AuthoredSchemaUpgradeWork::new(AuthoredSchemaUpgradeLimits::new(
        ByteLimit::new(work).unwrap(),
        ByteLimit::new(retained).unwrap(),
    ))
}

#[test]
fn preparation_validation_and_binding_collections_use_the_shared_allowance_before_installation() {
    let fixture = Fixture::new();
    let (mut workspace, mut authority, _) = fixture.open();
    let before = snapshot(&workspace);
    for (work, retained) in [(1, 256 * 1024 * 1024), (512 * 1024 * 1024, 1)] {
        assert_refused(
            &workspace
                .prepare_authored_schema_upgrade(
                    &fixture.upgrade,
                    proposals(&workspace),
                    &mut work_with_limits(work, retained),
                )
                .unwrap_err(),
        );
        assert_eq!(snapshot(&workspace), before);
    }
    let mut calibration = AuthoredSchemaUpgradeWork::default();
    let prepared = fixture.prepare(&workspace, &mut calibration);
    let preparation_work = calibration.usage().work_bytes;
    prepared.validate(&workspace, &mut calibration).unwrap();
    let validation_work = calibration.usage().work_bytes - preparation_work;
    drop(prepared);
    for extra in [1, validation_work + 1] {
        let mut work = work_with_limits(preparation_work + extra, 256 * 1024 * 1024);
        let prepared = fixture.prepare(&workspace, &mut work);
        let saved = observations(&prepared);
        assert_refused(
            authority
                .bind_authored_schema_upgrade(&mut workspace, prepared, saved, &mut work)
                .unwrap_err()
                .report(),
        );
        assert_eq!(snapshot(&workspace), before);
    }
}

#[test]
fn long_shared_source_key_prefixes_admit_ordering_work_before_any_successor() {
    let fixture = Fixture::new();
    let (workspace, _, _) = fixture.open();
    let before = snapshot(&workspace);
    let proposed = || {
        let mut values = proposals(&workspace);
        for (proposal, suffix) in values.iter_mut().zip(["z", "a", "m"]) {
            proposal.source_key = SourceDocumentKey::named(
                "04deef6c-d2c5-4fcf-905c-2793204b9512".parse().unwrap(),
                format!("{}-{suffix}", "shared".repeat(8192)),
                proposal.source_key.kind(),
            )
            .unwrap();
        }
        values.reverse();
        values
    };
    let key_bytes = proposed()
        .iter()
        .map(|proposal| proposal.source_key.retained_bytes())
        .sum::<usize>();
    // Preparation now also reserves the independent saved metadata needed for reversible
    // final binding. Admit those owners while still refusing the complete key-ordering scan.
    let mut short = work_with_limits(key_bytes * 4, 256 * 1024 * 1024);
    let report = workspace
        .prepare_authored_schema_upgrade(&fixture.upgrade, proposed(), &mut short)
        .unwrap_err();
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.fields().iter().any(|field| {
            field.key().as_str() == "preparation-stage"
                && field.display_value() == "workspace-source-key-ordering"
        })
    }));
    assert_eq!(snapshot(&workspace), before);

    let mut measured = AuthoredSchemaUpgradeWork::default();
    drop(
        workspace
            .prepare_authored_schema_upgrade(&fixture.upgrade, proposed(), &mut measured)
            .unwrap(),
    );
    let required = measured.usage();
    for (ceiling, succeeds) in [
        (required.work_bytes, true),
        (required.work_bytes - 1, false),
    ] {
        let mut work = work_with_limits(ceiling, 256 * 1024 * 1024);
        assert_eq!(
            workspace
                .prepare_authored_schema_upgrade(&fixture.upgrade, proposed(), &mut work)
                .is_ok(),
            succeeds
        );
        assert_eq!(snapshot(&workspace), before);
    }
}
