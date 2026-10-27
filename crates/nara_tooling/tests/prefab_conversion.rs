use std::{sync::Arc, thread};

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringSchemaBasis,
    PrefabDocument, PrefabDocumentCandidate, PrefabInstance, ResolvedPrefabSource,
    SceneAuthoringContentState, SceneAuthoringHistoryStatus, SceneAuthoringHistoryUsage,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
    ScenePatchDocument, ScenePatchOperation, SourceDocumentKey, SourceDocumentKind,
};
use nara_tooling::{
    EditorDocumentDigest, EditorDocumentId, EditorDocumentKind, EditorDocumentQueryBasis,
    EditorDocumentSourceBinding, EditorPersistenceAuthority, EditorPrefabConversionCommitError,
    EditorSelectionSet, EditorWorkspace, EditorWorkspaceCommand, EditorWorkspaceCommandReport,
    EditorWorkspaceModel, PreparedEditorPrefabConversion,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn source_ref() -> AssetRef {
    AssetRef::path("prefabs/conversion/base.prefab.json").unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.WorkspaceConversion")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.workspace-conversion").unwrap()
}

fn schema_basis() -> AuthoringSchemaBasis {
    let schema = ComponentSchema::new(component(), "Conversion", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("amount"),
            "Amount",
            ComponentFieldPath::from_fields(["amount"]),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn record(amount: u64) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("amount", ComponentValue::U64(amount))]),
    )
    .with_owner(owner())
}

fn open_document(
    workspace: &mut EditorWorkspace,
    kind: EditorDocumentKind,
    name: &str,
    document: SceneDocument,
    basis: &AuthoringSchemaBasis,
) -> EditorDocumentId {
    let report = match kind {
        EditorDocumentKind::Scene => {
            let path = format!("scenes/{name}.scene.json");
            let published =
                SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
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
        }
        EditorDocumentKind::Prefab => {
            let path = format!("prefabs/{name}.prefab.json");
            let document = PrefabDocument::new(document.roots, document.entities);
            let published =
                PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
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
        }
        EditorDocumentKind::Extension(_) => unreachable!("nominal source fixture"),
    };
    report.opened_document.unwrap()
}

#[derive(Debug, PartialEq)]
struct SourceState {
    document: SceneDocument,
    content: SceneAuthoringContentState,
    history: SceneAuthoringHistoryStatus,
    history_usage: SceneAuthoringHistoryUsage,
    selection: EditorSelectionSet,
}

fn source_state(workspace: &EditorWorkspace, document: EditorDocumentId) -> SourceState {
    if let Some(slot) = workspace.scene(document) {
        SourceState {
            document: slot.session().document().clone(),
            content: slot.session().content_state(),
            history: slot.session().history_status(),
            history_usage: slot.session().history_usage(),
            selection: slot.selection().clone(),
        }
    } else {
        let slot = workspace.prefab(document).unwrap();
        SourceState {
            document: slot.session().document().instantiate(),
            content: slot.session().content_state(),
            history: slot.session().history_status(),
            history_usage: slot.session().history_usage(),
            selection: slot.selection().clone(),
        }
    }
}

#[derive(Debug, PartialEq)]
struct WorkspaceState {
    model: EditorWorkspaceModel,
    target: SourceState,
    active: SourceState,
}

struct Fixture {
    workspace: EditorWorkspace,
    authority: EditorPersistenceAuthority,
    document: EditorDocumentId,
    active: EditorDocumentId,
    source: ResolvedPrefabSource,
}

impl Fixture {
    fn new(kind: EditorDocumentKind) -> Self {
        let basis = schema_basis();
        let dependency =
            PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("inherited")).with_component(component(), record(17))
            ]);
        let published =
            PrefabDocumentCandidate::decode_json_str(&dependency.to_json_string().unwrap())
                .unwrap()
                .admit_authoring_with_basis(AuthoringDocumentSource::new(source_ref()), &basis)
                .unwrap()
                .publish();
        assert!(
            !published.record_facts()[0]
                .adapter()
                .unwrap()
                .supports_native_preview()
        );
        let source = ResolvedPrefabSource::bound_authoring(
            Arc::new(published),
            SourceDocumentKey::named(
                "3d74bc55-a7c2-4342-bf24-1fb920c7fca9".parse().unwrap(),
                "workspace-conversion",
                SourceDocumentKind::Prefab,
            )
            .unwrap(),
        );
        let mut anchor = SceneEntityRecord::new(id("anchor")).with_children([id("local")]);
        anchor.prefab = Some(PrefabInstance {
            source: source_ref(),
            overrides: None,
        });
        let (mut workspace, authority) = EditorWorkspace::new_hosted();
        let document = open_document(
            &mut workspace,
            kind,
            "conversion",
            SceneDocument::new(
                [id("anchor"), id("peer")],
                [
                    anchor,
                    SceneEntityRecord::new(id("local")),
                    SceneEntityRecord::new(id("peer")).with_component(component(), record(2)),
                ],
            ),
            &basis,
        );
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
                    document,
                    entity: Some(id("peer")),
                })
                .applied
        );
        let active = open_document(
            &mut workspace,
            EditorDocumentKind::Scene,
            "active",
            SceneDocument::from_ordered_roots([
                SceneEntityRecord::new(id("active")).with_component(component(), record(5))
            ]),
            &basis,
        );
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
                    document: active,
                    entity: Some(id("active")),
                })
                .applied
        );
        Self {
            workspace,
            authority,
            document,
            active,
            source,
        }
    }

    fn basis(&self) -> EditorDocumentQueryBasis {
        self.workspace.document_query_basis(self.document).unwrap()
    }

    fn state(&self) -> WorkspaceState {
        WorkspaceState {
            model: self.workspace.authoring_model(),
            target: source_state(&self.workspace, self.document),
            active: source_state(&self.workspace, self.active),
        }
    }

    fn prepare(&self) -> PreparedEditorPrefabConversion {
        let basis = self.basis();
        let request = self
            .workspace
            .request_complete_prefab_conversion(&basis, &id("anchor"), Default::default())
            .unwrap();
        assert_eq!(request.basis(), &basis);
        assert_eq!(
            Some(request.document_kind()),
            self.workspace.document_kind(self.document)
        );
        let source = self.source.clone();
        let (prepared, reads) = thread::spawn(move || {
            let mut reads = Vec::new();
            let prepared = request
                .prepare_with_loader(None, |request, _| {
                    reads.push(request.clone());
                    assert_eq!(request, &source_ref());
                    Ok::<_, LoadFailure>(Some(source.clone()))
                })
                .unwrap();
            (prepared, reads)
        })
        .join()
        .unwrap();
        assert_eq!(reads, [source_ref()]);
        prepared
    }

    fn replay(&mut self, undo: bool) {
        let document = self.document;
        let base_revision = self.workspace.document_revision(document).unwrap();
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
        let report = self.workspace.apply_authoring_command(command);
        assert!(report.applied, "{:?}", report.diagnostics);
        assert_eq!(report.document, Some(document));
        assert_eq!(report.active_document, Some(self.active));
    }

    fn reopen(&mut self) {
        let revision = self.basis().revision;
        let digest = EditorDocumentDigest::new(1, [0x31; 32]);
        if let Some(slot) = self.workspace.scene(self.document) {
            let session = slot.session();
            let candidate = SceneDocumentCandidate::decode_json_str(
                &session.document().to_json_string().unwrap(),
            )
            .unwrap();
            let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
            self.authority
                .publish_reopened_session(
                    &mut self.workspace,
                    self.document,
                    revision,
                    reopened,
                    digest,
                )
                .unwrap();
        } else {
            let slot = self.workspace.prefab(self.document).unwrap();
            let binding = slot.source_binding().clone();
            let session = slot.session();
            let candidate = PrefabDocumentCandidate::decode_json_str(
                &session.document().to_json_string().unwrap(),
            )
            .unwrap();
            let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
            self.authority
                .publish_reopened_prefab_session(
                    &mut self.workspace,
                    self.document,
                    revision,
                    binding,
                    reopened,
                    digest,
                )
                .unwrap();
        }
    }
}

#[derive(Debug)]
struct LoadFailure {
    request: AssetRef,
    identity: Arc<()>,
}

fn assert_rejected(report: &EditorWorkspaceCommandReport, document: EditorDocumentId, code: &str) {
    assert!(!report.applied);
    assert_eq!(report.document, Some(document));
    assert!(report.patch_report.is_none());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == code),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn non_active_scene_and_prefab_conversion_review_commits_the_same_mapping_and_one_history_entry() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut fixture = Fixture::new(kind);
        let before = fixture.state();
        let basis = fixture.basis();
        let prepared = fixture.prepare();
        assert_eq!(fixture.state(), before);
        assert_eq!(prepared.basis(), &basis);
        assert_eq!(prepared.base_revision(), basis.revision);
        assert_eq!(prepared.document(), fixture.document);
        assert_eq!(prepared.document_kind(), kind);
        assert_eq!(prepared.anchor(), &id("anchor"));
        assert_eq!(prepared.entities().len(), 1);
        let mapping = prepared.entities().to_vec();
        let localized = mapping[0].entity().clone();
        assert_ne!(localized, id("inherited"));
        assert_eq!(mapping[0].provenance().local_entity_id(), &id("inherited"));
        assert_eq!(
            mapping[0]
                .provenance()
                .projection_path()
                .unwrap()
                .root_anchor(),
            &id("anchor")
        );
        let reviewed = match kind {
            EditorDocumentKind::Scene => {
                assert!(prepared.prefab_document().is_none());
                prepared.scene_document().unwrap().clone()
            }
            EditorDocumentKind::Prefab => {
                assert!(prepared.scene_document().is_none());
                prepared.prefab_document().unwrap().instantiate()
            }
            EditorDocumentKind::Extension(_) => unreachable!("nominal source fixture"),
        };
        let copied = reviewed
            .entities
            .iter()
            .find(|entity| entity.id == localized)
            .unwrap();
        assert_eq!(copied.components[&component()], record(17));
        let anchor = reviewed
            .entities
            .iter()
            .find(|entity| entity.id == id("anchor"))
            .unwrap();
        assert!(anchor.prefab.is_none());
        assert_eq!(anchor.children, [localized, id("local")]);
        let mut reads = Vec::new();
        let receipt = fixture
            .workspace
            .commit_complete_prefab_conversion_with_loader(prepared, |request| {
                reads.push(request.clone());
                Ok::<_, LoadFailure>(Some(fixture.source.clone()))
            })
            .unwrap();
        assert_eq!(reads, [source_ref()]);
        assert_eq!(receipt.entities(), mapping);
        assert!(receipt.report().applied);
        assert_eq!(receipt.report().document, Some(fixture.document));
        assert_eq!(receipt.report().active_document, Some(fixture.active));
        assert_eq!(receipt.report().dirty, Some(true));
        let (committed_mapping, report) = receipt.into_parts();
        assert_eq!(committed_mapping, mapping);
        let committed = fixture.state();
        assert_eq!(committed.target.document, reviewed);
        assert_eq!(report.revision, Some(fixture.basis().revision));
        assert_eq!(committed.target.history.undo_depth, 1);
        assert_eq!(committed.target.history.redo_depth, 0);
        assert_eq!(committed.target.selection, before.target.selection);
        assert_eq!(committed.active, before.active);
        assert_eq!(fixture.workspace.active_document(), Some(fixture.active));

        fixture.replay(true);
        let undone = fixture.state();
        assert_eq!(undone.target.document, before.target.document);
        assert_eq!(undone.target.content, before.target.content);
        assert_eq!(undone.target.history.undo_depth, 0);
        assert_eq!(undone.target.history.redo_depth, 1);
        assert_eq!(undone.target.selection, before.target.selection);
        assert_eq!(undone.active, before.active);
        assert_eq!(fixture.workspace.active_document(), Some(fixture.active));
        fixture.replay(false);
        let redone = fixture.state();
        assert_eq!(redone.target.document, reviewed);
        assert_eq!(redone.target.content, committed.target.content);
        assert_eq!(redone.target.history.undo_depth, 1);
        assert_eq!(redone.target.history.redo_depth, 0);
        assert_eq!(redone.target.selection, before.target.selection);
        assert_eq!(redone.active, before.active);
        assert_eq!(fixture.workspace.active_document(), Some(fixture.active));
        assert_eq!(reads, [source_ref()]);
    }
}

#[test]
fn conversion_request_rejects_foreign_incarnation_revision_and_exact_schema_before_loading() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let fixture = Fixture::new(kind);
        let foreign = Fixture::new(kind);
        assert_eq!(fixture.document, foreign.document);
        let basis = fixture.basis();
        let mut wrong_source = basis.clone();
        wrong_source.revision = fixture.workspace.document_revision(fixture.active).unwrap();
        let mut stale_revision = basis.clone();
        stale_revision.revision = stale_revision.revision.next();
        let mut missing_schema = basis.clone();
        missing_schema.schema = None;
        let mut foreign_schema = basis.clone();
        foreign_schema.schema = foreign.basis().schema;
        assert_ne!(foreign_schema.schema, basis.schema);
        let before = fixture.state();
        let mut reads = 0;
        for (rejected, code) in [
            (
                foreign.basis(),
                "tooling.workspace-stale-document-incarnation",
            ),
            (wrong_source, "tooling.workspace-stale-document-incarnation"),
            (stale_revision, "tooling.workspace-stale-document-revision"),
            (missing_schema, "tooling.workspace-stale-schema-basis"),
            (foreign_schema, "tooling.workspace-stale-schema-basis"),
        ] {
            let result = fixture
                .workspace
                .request_complete_prefab_conversion(&rejected, &id("anchor"), Default::default())
                .map(|request| {
                    request.prepare_with_loader(None, |_, _| {
                        reads += 1;
                        Ok::<_, LoadFailure>(Some(fixture.source.clone()))
                    })
                });
            assert_rejected(&result.unwrap_err(), fixture.document, code);
            assert_eq!(reads, 0);
            assert_eq!(fixture.state(), before);
        }
    }
}

#[test]
fn conversion_commit_rejects_wrong_workspace_reopen_and_undone_edits_before_loading() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        for invalidation in ["workspace", "reopen", "revision"] {
            let mut fixture = Fixture::new(kind);
            let original = fixture.state().target.document;
            let prepared = fixture.prepare();
            let mapping = prepared.entities().to_vec();
            let old_revision = prepared.base_revision();
            let code = match invalidation {
                "workspace" => {
                    let replacement = Fixture::new(kind);
                    assert_eq!(fixture.document, replacement.document);
                    fixture = replacement;
                    "tooling.workspace-stale-document-incarnation"
                }
                "reopen" => {
                    fixture.reopen();
                    assert_ne!(
                        fixture.basis().revision.source_id(),
                        old_revision.source_id()
                    );
                    "tooling.workspace-stale-document-incarnation"
                }
                "revision" => {
                    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
                        entity: id("peer"),
                        component: component(),
                        component_version: ComponentSchemaVersion::ONE,
                        field: ComponentFieldId::new("amount"),
                        value: ComponentValue::U64(3),
                    }]);
                    let edit = fixture
                        .workspace
                        .prepare_authoring_edit(&fixture.basis(), &patch)
                        .unwrap();
                    assert!(fixture.workspace.commit_authoring_edit(edit).applied);
                    fixture.replay(true);
                    assert_eq!(
                        fixture.basis().revision.source_id(),
                        old_revision.source_id()
                    );
                    assert_ne!(fixture.basis().revision, old_revision);
                    "tooling.workspace-stale-document-revision"
                }
                _ => unreachable!("explicit invalidation cases"),
            };
            let before = fixture.state();
            assert_eq!(before.target.document, original);
            let mut reads = 0;
            let result = fixture
                .workspace
                .commit_complete_prefab_conversion_with_loader(prepared, |_| {
                    reads += 1;
                    Ok::<_, LoadFailure>(Some(fixture.source.clone()))
                });
            let EditorPrefabConversionCommitError::Document(report) = result.unwrap_err() else {
                panic!("source basis rejection must precede dependency loading");
            };
            assert_rejected(&report, fixture.document, code);
            assert_eq!(reads, 0);
            assert_eq!(fixture.state(), before);
            assert!(mapping.iter().all(|entry| {
                before
                    .target
                    .document
                    .entities
                    .iter()
                    .all(|entity| &entity.id != entry.entity())
            }));
        }
    }
}

#[test]
fn conversion_commit_preserves_the_concrete_loader_error_and_all_workspace_state() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut fixture = Fixture::new(kind);
        let prepared = fixture.prepare();
        let before = fixture.state();
        let mapping = prepared.entities().to_vec();
        let identity = Arc::new(());
        let mut reads = Vec::new();
        let result = fixture
            .workspace
            .commit_complete_prefab_conversion_with_loader(prepared, |request| {
                reads.push(request.clone());
                Err(LoadFailure {
                    request: request.clone(),
                    identity: Arc::clone(&identity),
                })
            });
        let EditorPrefabConversionCommitError::Loader(error) = result.unwrap_err() else {
            panic!("the concrete loader error must survive Workspace routing");
        };
        assert_eq!(error.request, source_ref());
        assert!(Arc::ptr_eq(&error.identity, &identity));
        assert_eq!(reads, [source_ref()]);
        assert_eq!(fixture.state(), before);
        assert!(mapping.iter().all(|entry| {
            before
                .target
                .document
                .entities
                .iter()
                .all(|entity| &entity.id != entry.entity())
        }));
    }
}
