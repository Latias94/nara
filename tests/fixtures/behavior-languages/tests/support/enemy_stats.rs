//! A concrete trusted tool consumer, independent of a window or drawing toolkit.

use std::{cell::RefCell, fmt, rc::Rc, sync::Arc, time::Duration};

use nara_engine::project_host::{EditorEditPreparationError, EditorProjectSession};
use nara_reflect::{
    ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, resolve_component_property,
};
use nara_scene::{
    ConstructionEntity, LocalConstructionEntity, SceneComponentRecord, SceneDocument,
    SceneEntityId, SceneEntityRecord, ScenePlacement, ScenePlacementOwner,
};
use nara_tooling::{
    CommittedEditorDocumentConstruction, EditorActionOutcome, EditorDocumentId,
    EditorDocumentQueryBasis, EditorDocumentQueryRejection, EditorEntityQuery,
    EditorEntityQueryLimits, EditorEntityQueryPosition, EditorPropertyRead,
    EditorPropertyReadLimits, EditorPropertyTarget, EditorPropertyValue,
    PreparedEditorDocumentConstruction,
};

use super::{
    authoring_project::{
        Project, assert_history, assert_value, close, replay, save, schema_bundle,
    },
    dotnet::{DotnetProgram, EnemyStatsProposal, EnemyStatsRequest},
    scene_runtime::{Definition, Runtime, id},
};

const STATS: &str = "trial.enemy.EnemyStats";
const NAME: &str = "nara.scene.Name";

fn kind(value: &str) -> ComponentTypeId {
    ComponentTypeId::new(value)
}

fn property(field: &str) -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new(field))
}

#[derive(Debug)]
enum ToolError {
    Inactive,
    StalePublication,
    StaleRequest,
    InvalidObservation,
    SchemaMismatch,
    InvalidProposal,
    Managed(String),
    Owner(Box<EditorEditPreparationError>),
    Draft(String),
    Commit(Box<EditorActionOutcome>),
}

impl fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Managed(error) | Self::Draft(error) => formatter.write_str(error),
            Self::Owner(error) => error.fmt(formatter),
            Self::Commit(outcome) => write!(formatter, "{outcome:?}"),
            error => write!(formatter, "{error:?}"),
        }
    }
}

impl From<EditorEditPreparationError> for ToolError {
    fn from(error: EditorEditPreparationError) -> Self {
        Self::Owner(Box::new(error))
    }
}

enum Command {
    Attach(SceneEntityId),
    Edit(SceneEntityId),
    Spawn(String),
}

struct Proposal {
    publication: Rc<()>,
    request: Rc<()>,
    basis: EditorDocumentQueryBasis,
    command: Command,
    output: EnemyStatsProposal,
}

struct ReviewedEdit {
    publication: Rc<()>,
    request: Rc<()>,
    basis: EditorDocumentQueryBasis,
    title: String,
    created: Option<LocalConstructionEntity>,
    prepared: PreparedEditorDocumentConstruction,
}

/// Owns only tool routing. Document candidates, IDs, inverse and history remain Session-owned.
struct EnemyStatsTool {
    program: Option<DotnetProgram>,
    publication: Rc<()>,
    request: RefCell<Rc<()>>,
    definition: Arc<Definition>,
}

impl EnemyStatsTool {
    fn new(program: DotnetProgram) -> Self {
        let catalog = program.enemy_stats_catalog().unwrap();
        let definition = Definition::from_catalog("trial.enemy-stats", catalog).unwrap();
        Self {
            program: Some(program),
            publication: Rc::new(()),
            request: RefCell::new(Rc::new(())),
            definition,
        }
    }

    fn validate_request(&self, request: &Rc<()>) -> Result<(), ToolError> {
        if !Rc::ptr_eq(&self.request.borrow(), request) {
            return Err(ToolError::StaleRequest);
        }
        Ok(())
    }

    fn validate_publication(&self, publication: &Rc<()>) -> Result<(), ToolError> {
        if self.program.is_none() {
            return Err(ToolError::Inactive);
        }
        if !Rc::ptr_eq(&self.publication, publication) {
            return Err(ToolError::StalePublication);
        }
        Ok(())
    }

    fn propose(
        &self,
        editor: &EditorProjectSession,
        document: EditorDocumentId,
        command: Command,
        bonus: i64,
        fault: &str,
    ) -> Result<Proposal, ToolError> {
        let program = self.program.as_ref().ok_or(ToolError::Inactive)?;
        let request = Rc::new(());
        *self.request.borrow_mut() = Rc::clone(&request);
        let basis = editor.document_query_basis(document).map_err(|error| {
            ToolError::Owner(Box::new(EditorEditPreparationError::QueryBasis(error)))
        })?;
        editor.validate_authoring_query_basis(&basis)?;
        let schema = self.definition.owner.schema(&kind(STATS)).unwrap();
        let admitted = basis.schema.as_ref().ok_or(ToolError::SchemaMismatch)?;
        if admitted.component_schema_owner(&kind(STATS)) != Some(self.definition.owner.owner_id())
            || admitted.schema(&kind(STATS)) != Some(schema)
        {
            return Err(ToolError::SchemaMismatch);
        }
        let read = |field: &str| -> Result<i64, ToolError> {
            let value = match &command {
                Command::Edit(entity) => editor
                    .read_document_property(EditorPropertyRead {
                        target: EditorPropertyTarget {
                            basis: basis.clone(),
                            entity: entity.clone(),
                            component: kind(STATS),
                            property: property(field),
                        },
                        limits: EditorPropertyReadLimits::new(4096, 64, 4096).unwrap(),
                    })
                    .map(|observation| observation.value)
                    .map_err(|error| {
                        ToolError::Owner(Box::new(EditorEditPreparationError::QueryBasis(error)))
                    })?,
                Command::Attach(_) | Command::Spawn(_) => EditorPropertyValue::Present(
                    resolve_component_property(
                        schema,
                        schema.version(),
                        schema.authoring_initial_value().unwrap(),
                        &property(field),
                    )
                    .unwrap()
                    .value
                    .clone(),
                ),
            };
            let EditorPropertyValue::Present(ComponentValue::I64(value)) = value else {
                return Err(ToolError::InvalidObservation);
            };
            Ok(value)
        };
        let input = EnemyStatsRequest {
            operation: match &command {
                Command::Attach(_) => "attach",
                Command::Edit(_) => "edit",
                Command::Spawn(_) => "spawn",
            }
            .into(),
            name: match &command {
                Command::Spawn(name) => name.clone(),
                _ => String::new(),
            },
            health: read("health")?,
            armor: read("armor")?,
            bonus,
            fault: fault.into(),
        };
        let output = program
            .propose_enemy_stats(&input)
            .map_err(ToolError::Managed)?;
        Ok(Proposal {
            publication: Rc::clone(&self.publication),
            request,
            basis,
            command,
            output,
        })
    }

    fn prepare(
        &self,
        editor: &EditorProjectSession,
        proposal: Proposal,
    ) -> Result<ReviewedEdit, ToolError> {
        self.validate_publication(&proposal.publication)?;
        editor.validate_authoring_query_basis(&proposal.basis)?;
        self.validate_request(&proposal.request)?;
        let output = proposal.output;
        if output.writes.is_empty() || output.writes.len() > 8 {
            return Err(ToolError::InvalidProposal);
        }
        let mut bytes = output.title.len().saturating_add(output.name.len());
        let mut nodes: usize = 0;
        for write in &output.writes {
            bytes = bytes.saturating_add(write.field.len());
            let cost = write
                .value
                .cost_up_to(
                    128_usize.saturating_sub(nodes),
                    4096_usize.saturating_sub(bytes),
                )
                .ok_or(ToolError::InvalidProposal)?;
            nodes += cost.nodes();
            bytes = bytes.saturating_add(cost.logical_bytes());
        }
        if bytes > 4096 {
            return Err(ToolError::InvalidProposal);
        }
        let mut draft = editor
            .begin_authoring_construction(proposal.basis.document, proposal.basis.revision)?;
        let created = match &proposal.command {
            Command::Spawn(_) => Some(
                draft
                    .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
                    .map_err(|report| ToolError::Draft(format!("{report:?}")))?,
            ),
            _ => None,
        };
        let entity = match &proposal.command {
            Command::Attach(entity) | Command::Edit(entity) => {
                ConstructionEntity::Existing(entity.clone())
            }
            Command::Spawn(_) => ConstructionEntity::Local(created.clone().unwrap()),
        };
        if !matches!(proposal.command, Command::Edit(_)) {
            draft
                .add_component(entity.clone(), kind(STATS))
                .map_err(|report| ToolError::Draft(format!("{report:?}")))?;
        }
        if created.is_some() {
            draft
                .add_component(entity.clone(), kind(NAME))
                .map_err(|report| ToolError::Draft(format!("{report:?}")))?;
            draft
                .set_property(
                    entity.clone(),
                    kind(NAME),
                    property("value"),
                    ComponentValue::String(output.name),
                )
                .map_err(|report| ToolError::Draft(format!("{report:?}")))?;
        }
        for write in output.writes {
            draft
                .set_property(
                    entity.clone(),
                    kind(STATS),
                    property(&write.field),
                    write.value,
                )
                .map_err(|report| ToolError::Draft(format!("{report:?}")))?;
        }
        let prepared = editor.prepare_authoring_construction(proposal.basis.document, &draft)?;
        Ok(ReviewedEdit {
            publication: proposal.publication,
            request: proposal.request,
            basis: proposal.basis,
            title: output.title,
            created,
            prepared,
        })
    }

    fn commit(
        &self,
        editor: &mut EditorProjectSession,
        reviewed: ReviewedEdit,
    ) -> Result<CommittedEditorDocumentConstruction, ToolError> {
        self.validate_publication(&reviewed.publication)?;
        editor.validate_authoring_query_basis(&reviewed.basis)?;
        self.validate_request(&reviewed.request)?;
        editor
            .commit_authoring_construction(reviewed.prepared)
            .map_err(|outcome| ToolError::Commit(Box::new(outcome)))
    }

    fn replace(
        &mut self,
        candidate: DotnetProgram,
    ) -> Result<DotnetProgram, (DotnetProgram, String)> {
        let catalog = match candidate.enemy_stats_catalog() {
            Ok(catalog) => catalog,
            Err(error) => return Err((candidate, error)),
        };
        if catalog != *self.definition.owner.current() {
            return Err((
                candidate,
                "body replacement requires the same complete EnemyStats catalog".into(),
            ));
        }
        self.publication = Rc::new(());
        Ok(self.program.replace(candidate).unwrap())
    }

    fn disable(&mut self) -> DotnetProgram {
        self.publication = Rc::new(());
        self.program.take().unwrap()
    }
}

fn original() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("enemy")).with_component(
            kind(NAME),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Guard".into()),
            ),
        ),
        SceneEntityRecord::new(id("guide")).with_component(
            kind(NAME),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Guide".into()),
            ),
        ),
    ])
}

fn edit(
    tool: &EnemyStatsTool,
    editor: &EditorProjectSession,
    document: EditorDocumentId,
    command: Command,
    bonus: i64,
) -> ReviewedEdit {
    let proposal = tool.propose(editor, document, command, bonus, "").unwrap();
    tool.prepare(editor, proposal).unwrap()
}

fn assert_stats(
    editor: &EditorProjectSession,
    document: EditorDocumentId,
    entity: &str,
    health: i64,
) {
    for (field, value) in [("health", health), ("armor", 10)] {
        assert_value(
            editor,
            document,
            entity,
            STATS,
            field,
            ComponentValue::I64(value),
        );
    }
}

fn attach(tool: &EnemyStatsTool, editor: &mut EditorProjectSession, document: EditorDocumentId) {
    let reviewed = edit(tool, editor, document, Command::Attach(id("enemy")), 0);
    let committed = tool.commit(editor, reviewed).unwrap();
    assert!(committed.report().applied, "{:?}", committed.report());
    assert_eq!(committed.report().document, Some(document));
    editor.drive_editor_frame(Duration::ZERO);
}

#[test]
fn csharp_enemy_stats_review_commit_history_persistence_and_rust_data_access() {
    let tool = EnemyStatsTool::new(DotnetProgram::load_default());
    for extension in ["json", "ron"] {
        let project = Project::new(
            &schema_bundle(&tool.definition.owner),
            &original(),
            extension,
        );
        let before = std::fs::read(project.scene_path()).unwrap();
        let mut editor = project.open();
        let document = editor.current_frame().workspace().active_document.unwrap();
        attach(&tool, &mut editor, document);
        assert_stats(&editor, document, "enemy", 100);
        assert_history(&editor, document, 1, 0);

        let basis = editor.document_query_basis(document).unwrap();
        let reviewed = edit(&tool, &editor, document, Command::Edit(id("enemy")), 25);
        assert!(!reviewed.title.is_empty());
        assert_eq!(reviewed.prepared.base_revision(), basis.revision);
        let patch = reviewed.prepared.patch().clone();
        let inverse = reviewed.prepared.inverse().clone();
        assert!(!patch.operations.is_empty());
        assert!(!inverse.operations.is_empty());
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_stats(&editor, document, "enemy", 100);
        assert_history(&editor, document, 1, 0);
        let committed = tool.commit(&mut editor, reviewed).unwrap();
        assert!(committed.report().applied);
        assert_eq!(committed.report().document, Some(document));
        assert_eq!(
            committed
                .report()
                .patch_report
                .as_ref()
                .unwrap()
                .inverse
                .as_ref(),
            Some(&inverse)
        );
        editor.drive_editor_frame(Duration::ZERO);
        assert_stats(&editor, document, "enemy", 125);
        assert_history(&editor, document, 2, 0);
        replay(&mut editor, document, false);
        assert_stats(&editor, document, "enemy", 100);
        replay(&mut editor, document, true);
        assert_stats(&editor, document, "enemy", 125);

        let reviewed = edit(
            &tool,
            &editor,
            document,
            Command::Spawn("Reinforcement".into()),
            50,
        );
        let local = reviewed.created.clone().unwrap();
        let prepared_id = reviewed.prepared.entity(&local).unwrap().clone();
        assert_history(&editor, document, 2, 0);
        let committed = tool.commit(&mut editor, reviewed).unwrap();
        assert!(committed.report().applied);
        assert_eq!(committed.entity(&local), Some(&prepared_id));
        editor.drive_editor_frame(Duration::ZERO);
        assert_history(&editor, document, 3, 0);
        assert_stats(&editor, document, prepared_id.as_str(), 150);
        replay(&mut editor, document, false);
        assert_history(&editor, document, 2, 1);
        let page = editor
            .query_document_entities(EditorEntityQuery {
                position: EditorEntityQueryPosition::Start(
                    editor.document_query_basis(document).unwrap(),
                ),
                limits: EditorEntityQueryLimits::new(16, 16).unwrap(),
            })
            .unwrap();
        assert!(page.next.is_none());
        assert_eq!(page.entities, vec![id("enemy"), id("guide")]);
        assert_stats(&editor, document, "enemy", 125);
        assert_value(
            &editor,
            document,
            "enemy",
            NAME,
            "value",
            ComponentValue::String("Guard".into()),
        );
        assert_value(
            &editor,
            document,
            "guide",
            NAME,
            "value",
            ComponentValue::String("Guide".into()),
        );
        replay(&mut editor, document, true);
        assert_stats(&editor, document, prepared_id.as_str(), 150);
        assert_value(
            &editor,
            document,
            "enemy",
            NAME,
            "value",
            ComponentValue::String("Guard".into()),
        );
        assert_value(
            &editor,
            document,
            "guide",
            NAME,
            "value",
            ComponentValue::String("Guide".into()),
        );
        assert_eq!(std::fs::read(project.scene_path()).unwrap(), before);
        save(&mut editor, document);
        let saved = project.read();
        assert_eq!(saved.entities.len(), 3);
        for entity in &saved.entities {
            if let Some(stats) = entity.components.get(&kind(STATS)) {
                assert_eq!(stats.owner(), Some(tool.definition.owner.owner_id()));
            }
        }
        let original = original();
        for entity in &original.entities {
            let saved_entity = saved
                .entities
                .iter()
                .find(|saved| saved.id == entity.id)
                .unwrap();
            assert_eq!(
                saved_entity.components[&kind(NAME)],
                entity.components[&kind(NAME)]
            );
            if entity.id == id("guide") {
                assert_eq!(
                    saved_entity, entity,
                    "the unrelated entity must survive Save exactly"
                );
            }
        }
        close(&mut editor);
        drop(editor);

        let mut reopened = project.open();
        let reopened_document = reopened
            .current_frame()
            .workspace()
            .active_document
            .unwrap();
        assert_history(&reopened, reopened_document, 0, 0);
        assert_stats(&reopened, reopened_document, "enemy", 125);
        assert_stats(&reopened, reopened_document, prepared_id.as_str(), 150);
        assert_value(
            &reopened,
            reopened_document,
            prepared_id.as_str(),
            NAME,
            "value",
            ComponentValue::String("Reinforcement".into()),
        );
        assert_eq!(project.read(), saved);
        let mut runtime = Runtime::materialize(Arc::clone(&tool.definition), &saved, None).unwrap();
        for (entity, health) in [(id("enemy"), 125), (prepared_id, 150)] {
            let target = runtime.target(&entity);
            let record = runtime.read(&target, &kind(STATS)).unwrap().unwrap();
            assert_eq!(
                record.value.field("wire_health").unwrap(),
                &ComponentValue::I64(health)
            );
            assert_eq!(
                record.value.field("wire_armor").unwrap(),
                &ComponentValue::I64(10)
            );
        }
        runtime.retire().unwrap();
        close(&mut reopened);
    }
}

#[test]
fn csharp_enemy_stats_rejects_late_invalid_write_and_retained_document_results() {
    let tool = EnemyStatsTool::new(DotnetProgram::load_default());
    let project = Project::new(&schema_bundle(&tool.definition.owner), &original(), "json");
    let mut editor = project.open();
    let document = editor.current_frame().workspace().active_document.unwrap();
    attach(&tool, &mut editor, document);
    let basis = editor.document_query_basis(document).unwrap();
    let workspace = editor.current_frame().workspace().clone();
    let disk = std::fs::read(project.scene_path()).unwrap();
    for fault in ["bad_last_value", "too_many_writes", "throw"] {
        let result = tool
            .propose(&editor, document, Command::Edit(id("enemy")), 30, fault)
            .and_then(|proposal| tool.prepare(&editor, proposal));
        assert!(result.is_err());
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_stats(&editor, document, "enemy", 100);
        assert_history(&editor, document, 1, 0);
        assert_eq!(editor.current_frame().workspace(), &workspace);
        assert_eq!(std::fs::read(project.scene_path()).unwrap(), disk);
    }
    let invalid_spawn = tool
        .propose(
            &editor,
            document,
            Command::Spawn("Rejected".into()),
            10,
            "bad_last_value",
        )
        .unwrap();
    assert!(matches!(
        tool.prepare(&editor, invalid_spawn),
        Err(ToolError::Owner(_))
    ));
    assert_eq!(editor.document_query_basis(document).unwrap(), basis);
    assert_eq!(editor.current_frame().workspace(), &workspace);
    assert_eq!(std::fs::read(project.scene_path()).unwrap(), disk);
    let older = tool
        .propose(&editor, document, Command::Edit(id("enemy")), 2, "")
        .unwrap();
    let newest = tool
        .propose(&editor, document, Command::Edit(id("enemy")), 3, "")
        .unwrap();
    assert!(matches!(
        tool.prepare(&editor, older),
        Err(ToolError::StaleRequest)
    ));
    let older_review = tool.prepare(&editor, newest).unwrap();
    let cancelled = tool
        .propose(&editor, document, Command::Edit(id("enemy")), 4, "")
        .unwrap();
    assert!(matches!(
        tool.commit(&mut editor, older_review),
        Err(ToolError::StaleRequest)
    ));
    drop(cancelled);
    let before_failed_read = tool
        .propose(&editor, document, Command::Edit(id("enemy")), 5, "")
        .unwrap();
    assert!(matches!(
        tool.propose(&editor, document, Command::Edit(id("guide")), 1, ""),
        Err(ToolError::Owner(_))
    ));
    assert!(matches!(
        tool.prepare(&editor, before_failed_read),
        Err(ToolError::StaleRequest)
    ));
    let before_failed_read = edit(&tool, &editor, document, Command::Edit(id("enemy")), 6);
    assert!(matches!(
        tool.propose(&editor, document, Command::Edit(id("guide")), 1, ""),
        Err(ToolError::Owner(_))
    ));
    assert!(matches!(
        tool.commit(&mut editor, before_failed_read),
        Err(ToolError::StaleRequest)
    ));
    assert_eq!(editor.document_query_basis(document).unwrap(), basis);
    assert_stats(&editor, document, "enemy", 100);
    assert_history(&editor, document, 1, 0);
    let stale_proposal = tool
        .propose(&editor, document, Command::Edit(id("enemy")), 40, "")
        .unwrap();
    let stale_prepared = edit(&tool, &editor, document, Command::Edit(id("enemy")), 50);
    let current = edit(&tool, &editor, document, Command::Edit(id("enemy")), 1);
    assert!(tool.commit(&mut editor, current).unwrap().report().applied);
    editor.drive_editor_frame(Duration::ZERO);
    for result in [
        tool.prepare(&editor, stale_proposal).map(|_| ()),
        tool.commit(&mut editor, stale_prepared).map(|_| ()),
    ] {
        assert!(
            matches!(result, Err(ToolError::Owner(ref error)) if matches!(error.as_ref(),
            EditorEditPreparationError::QueryBasis(EditorDocumentQueryRejection::StaleRevision)))
        );
    }
    assert_stats(&editor, document, "enemy", 101);
    assert_history(&editor, document, 2, 0);
    close(&mut editor);
}

#[test]
fn csharp_enemy_stats_rejects_another_sessions_owner_or_complete_declaration() {
    let tool = EnemyStatsTool::new(DotnetProgram::load_default());
    let catalog = tool.definition.owner.current().clone();
    let mut changed = catalog.clone();
    changed.components[0].authoring_initial_value = Some(ComponentValue::map([
        ("wire_health", ComponentValue::I64(999)),
        ("wire_armor", ComponentValue::I64(10)),
    ]));
    for (owner, catalog) in [
        ("trial.another-owner", catalog),
        ("trial.enemy-stats", changed),
    ] {
        let definition = Definition::from_catalog(owner, catalog).unwrap();
        let project = Project::new(&schema_bundle(&definition.owner), &original(), "json");
        let disk = std::fs::read(project.scene_path()).unwrap();
        let mut editor = project.open();
        let document = editor.current_frame().workspace().active_document.unwrap();
        let basis = editor.document_query_basis(document).unwrap();
        assert!(matches!(
            tool.propose(&editor, document, Command::Attach(id("enemy")), 1, ""),
            Err(ToolError::SchemaMismatch)
        ));
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_history(&editor, document, 0, 0);
        assert_eq!(std::fs::read(project.scene_path()).unwrap(), disk);
        assert_eq!(project.read(), original());
        close(&mut editor);
    }
}

#[test]
fn csharp_enemy_stats_replacement_and_disable_fence_both_review_stages() {
    let mut tool = EnemyStatsTool::new(DotnetProgram::load_default());
    let project = Project::new(&schema_bundle(&tool.definition.owner), &original(), "json");
    let mut editor = project.open();
    let document = editor.current_frame().workspace().active_document.unwrap();
    attach(&tool, &mut editor, document);
    let rejected = tool
        .program
        .as_ref()
        .unwrap()
        .candidate(&module_path("NARA_BEHAVIOR_MODULE_INVALID"))
        .unwrap();
    let (mut rejected, error) = match tool.replace(rejected) {
        Err(rejected) => rejected,
        Ok(_) => panic!("an incompatible tool Schema must preserve its predecessor"),
    };
    assert!(error.contains("same complete EnemyStats catalog"));
    let rejected_retirement = rejected.retire();
    drop(rejected);
    let predecessor = edit(&tool, &editor, document, Command::Edit(id("enemy")), 1);
    assert!(
        tool.commit(&mut editor, predecessor)
            .unwrap()
            .report()
            .applied
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_stats(&editor, document, "enemy", 101);
    let proposal = tool
        .propose(&editor, document, Command::Edit(id("enemy")), 40, "")
        .unwrap();
    let reviewed = edit(&tool, &editor, document, Command::Edit(id("enemy")), 50);
    let candidate = tool
        .program
        .as_ref()
        .unwrap()
        .candidate(&module_path("NARA_BEHAVIOR_MODULE_V2"))
        .unwrap();
    let mut previous = tool
        .replace(candidate)
        .unwrap_or_else(|(_, error)| panic!("{error}"));
    let retirement = previous.retire();
    drop(previous);
    assert!(matches!(
        tool.prepare(&editor, proposal),
        Err(ToolError::StalePublication)
    ));
    assert!(matches!(
        tool.commit(&mut editor, reviewed),
        Err(ToolError::StalePublication)
    ));
    let successor = edit(&tool, &editor, document, Command::Edit(id("enemy")), 1);
    assert!(
        tool.commit(&mut editor, successor)
            .unwrap()
            .report()
            .applied
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_stats(&editor, document, "enemy", 103);
    let proposal = tool
        .propose(&editor, document, Command::Edit(id("enemy")), 60, "")
        .unwrap();
    let reviewed = edit(&tool, &editor, document, Command::Edit(id("enemy")), 70);
    let mut disabled = tool.disable();
    let disabled_retirement = disabled.retire();
    drop(disabled);
    assert!(matches!(
        tool.prepare(&editor, proposal),
        Err(ToolError::Inactive)
    ));
    assert!(matches!(
        tool.commit(&mut editor, reviewed),
        Err(ToolError::Inactive)
    ));
    assert!(matches!(
        tool.propose(&editor, document, Command::Edit(id("enemy")), 1, ""),
        Err(ToolError::Inactive)
    ));
    assert_stats(&editor, document, "enemy", 103);
    assert_history(&editor, document, 3, 0);
    for retirement in [rejected_retirement, retirement, disabled_retirement] {
        assert!(
            (0..16).any(|_| retirement.complete()),
            "actual managed retirement remains pending"
        );
    }
    close(&mut editor);
}

fn module_path(variable: &str) -> std::path::PathBuf {
    std::env::var_os(variable)
        .expect("the runner builds every tested module variant")
        .into()
}
