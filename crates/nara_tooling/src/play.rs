//! UI-agnostic editor Play Mode lifecycle models.

use std::collections::BTreeSet;

use nara_asset::{AssetRefExportPolicy, ProjectAssetDatabase};
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_ecs::{Entity, World};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentEncodeContext, ComponentMigrationError,
    ComponentRegistry, ComponentTypeId, PersistentApplyRejection,
};
use nara_scene::{
    SceneAuthoringRevision, SceneAuthoringSession, SceneComponentRecord, SceneEntityId,
    SceneEntityRecord, SceneEntitySource, ScenePatchDocument, ScenePatchOperation,
    ScenePatchReport,
};

use crate::diagnostic;
use crate::inspector::{
    SceneInspectorCommand, SceneInspectorCommandReport, SceneInspectorModel, SceneInspectorState,
};
use crate::snapshot::WorldIdentitySnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPlayCommand {
    Play,
    Cancel,
    Pause,
    Resume,
    StepFixedTick,
    Stop,
    Restart,
    RetryRetirement,
    RetryClose,
    AcknowledgeResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPlayOperation {
    Play,
    Cancel,
    Pause,
    Resume,
    StepFixedTick,
    Stop,
    Restart,
    RetryRetirement,
    RetryClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPlayState {
    Empty,
    PreparingPlay,
    Starting,
    RetiringPlay,
    Running,
    Paused,
    Stepping,
    Stopping,
    Faulted,
    RetirementIncomplete,
    CloseIncomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPlayRejection {
    Busy,
    NoActiveDocument,
    InvalidState,
    ResultPending,
    StaleDocument,
    RuntimeOutOfDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPlayFailure {
    Preparation,
    Start,
    Runtime,
    Retirement,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPlayOperationResult {
    Pending {
        operation: EditorPlayOperation,
    },
    Applied {
        operation: EditorPlayOperation,
        generation: Option<u64>,
    },
    Rejected {
        operation: EditorPlayOperation,
        reason: EditorPlayRejection,
    },
    Failed {
        operation: EditorPlayOperation,
        failure: EditorPlayFailure,
    },
    Cancelled {
        operation: EditorPlayOperation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPlayRequestResult {
    Accepted,
    Rejected(EditorPlayRejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorPlayView {
    state: EditorPlayState,
    generation: Option<u64>,
    source_revision: Option<SceneAuthoringRevision>,
    current_revision: Option<SceneAuthoringRevision>,
    out_of_date: bool,
    result: Option<EditorPlayOperationResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorRuntimeEditRequest {
    pub generation: u64,
    pub document_revision: SceneAuthoringRevision,
    pub entity: SceneEntityId,
    pub component: ComponentTypeId,
    pub component_version: nara_reflect::ComponentSchemaVersion,
    pub field: nara_reflect::ComponentFieldId,
    pub value: nara_reflect::ComponentValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorRuntimeEditRejection {
    Busy,
    InvalidRuntimeState,
    StaleGeneration,
    StaleDocument,
    MissingEntity,
    AmbiguousEntity,
    UnknownComponent,
    MissingComponent,
    UnknownField,
    NotEditable,
    SchemaVersionMismatch,
    InvalidValue,
    ApplyRejected,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditorRuntimeEditResult {
    Pending(EditorRuntimeEditRequest),
    Applied(EditorRuntimeEditRequest),
    Rejected {
        request: EditorRuntimeEditRequest,
        reason: EditorRuntimeEditRejection,
    },
    Cancelled(EditorRuntimeEditRequest),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorApplyChangesRejection {
    Busy,
    InvalidRuntimeState,
    StaleGeneration,
    StaleDocument,
    RuntimeOutOfDate,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditorApplyChangesResult {
    Pending {
        generation: u64,
        document_revision: SceneAuthoringRevision,
        request: SceneApplyChangesRequest,
    },
    Applied(SceneApplyChangesReport),
    Rejected {
        request: SceneApplyChangesRequest,
        reason: EditorApplyChangesRejection,
        report: Option<SceneApplyChangesReport>,
    },
    Cancelled(SceneApplyChangesRequest),
}

impl EditorPlayView {
    #[must_use]
    pub const fn new(
        state: EditorPlayState,
        generation: Option<u64>,
        source_revision: Option<SceneAuthoringRevision>,
        current_revision: Option<SceneAuthoringRevision>,
        out_of_date: bool,
        result: Option<EditorPlayOperationResult>,
    ) -> Self {
        Self {
            state,
            generation,
            source_revision,
            current_revision,
            out_of_date,
            result,
        }
    }

    #[must_use]
    pub const fn state(self) -> EditorPlayState {
        self.state
    }

    #[must_use]
    pub const fn generation(self) -> Option<u64> {
        self.generation
    }

    #[must_use]
    pub const fn source_revision(self) -> Option<SceneAuthoringRevision> {
        self.source_revision
    }

    #[must_use]
    pub const fn current_revision(self) -> Option<SceneAuthoringRevision> {
        self.current_revision
    }

    #[must_use]
    pub const fn is_out_of_date(self) -> bool {
        self.out_of_date
    }

    #[must_use]
    pub const fn result(self) -> Option<EditorPlayOperationResult> {
        self.result
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneApplyChangesReport {
    pub applied: bool,
    pub supported: bool,
    pub source_revision: Option<SceneAuthoringRevision>,
    pub current_revision: SceneAuthoringRevision,
    pub patch: Option<ScenePatchDocument>,
    pub patch_report: Option<ScenePatchReport>,
    pub components: Vec<SceneApplyChangesComponentReport>,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneApplyChangesRequest {
    pub entity: SceneEntityId,
    pub components: Vec<ComponentTypeId>,
    pub asset_ref_export_policy: AssetRefExportPolicy,
}

impl SceneApplyChangesRequest {
    #[must_use]
    pub fn new(
        entity: SceneEntityId,
        components: impl IntoIterator<Item = ComponentTypeId>,
    ) -> Self {
        Self {
            entity,
            components: components.into_iter().collect(),
            asset_ref_export_policy: AssetRefExportPolicy::default(),
        }
    }

    #[must_use]
    pub const fn with_asset_ref_export_policy(mut self, policy: AssetRefExportPolicy) -> Self {
        self.asset_ref_export_policy = policy;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneApplyChangesComponentStatus {
    Pending,
    Applied,
    NoOp,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneApplyChangesComponentReport {
    pub entity: SceneEntityId,
    pub component: ComponentTypeId,
    pub status: SceneApplyChangesComponentStatus,
    pub operation_count: usize,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneEditorModel {
    pub inspector: SceneInspectorModel,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Default)]
pub struct SceneEditorState {
    inspector: SceneInspectorState,
}

impl SceneEditorState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn inspector(&self) -> &SceneInspectorState {
        &self.inspector
    }

    #[must_use]
    pub fn inspector_mut(&mut self) -> &mut SceneInspectorState {
        &mut self.inspector
    }

    #[must_use]
    pub fn model(
        &mut self,
        session: &SceneAuthoringSession,
        registry: &ComponentRegistry,
        edit_world_snapshot: Option<&WorldIdentitySnapshot>,
    ) -> SceneEditorModel {
        let inspector = self.inspector.model(session, registry, edit_world_snapshot);

        SceneEditorModel {
            inspector,
            diagnostics: DiagnosticReport::default(),
        }
    }

    #[must_use]
    pub fn model_with_selection(
        &mut self,
        session: &SceneAuthoringSession,
        registry: &ComponentRegistry,
        selected_entity: Option<&SceneEntityId>,
        edit_world_snapshot: Option<&WorldIdentitySnapshot>,
    ) -> SceneEditorModel {
        let inspector = self.inspector.model_with_selection(
            session,
            registry,
            selected_entity,
            edit_world_snapshot,
        );
        SceneEditorModel {
            inspector,
            diagnostics: DiagnosticReport::default(),
        }
    }

    pub fn apply_inspector_command(
        &mut self,
        session: &mut SceneAuthoringSession,
        registry: &ComponentRegistry,
        command: SceneInspectorCommand,
    ) -> SceneInspectorCommandReport {
        self.inspector.apply_command(session, registry, command)
    }

    pub fn apply_inspector_command_with_asset_database(
        &mut self,
        session: &mut SceneAuthoringSession,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
        command: SceneInspectorCommand,
    ) -> SceneInspectorCommandReport {
        self.inspector
            .apply_command_with_asset_database(session, registry, database, command)
    }
}

/// Exports one bounded Apply Changes request from a Host-owned runtime safe point.
///
/// The runtime `World` is borrowed only for this call and is never retained by tooling state.
#[doc(hidden)]
pub fn __export_apply_changes_from_world(
    world: &World,
    session: &SceneAuthoringSession,
    registry: &ComponentRegistry,
    source_revision: SceneAuthoringRevision,
    request: SceneApplyChangesRequest,
) -> SceneApplyChangesReport {
    let current_revision = session.revision();
    if source_revision != current_revision {
        return apply_changes_error_report(
            Some(source_revision),
            current_revision,
            "tooling.apply-changes-revision-mismatch",
            "Apply Changes is unsupported because the authoring document changed after Play Mode started",
        );
    }
    let Some(document_entity) = session
        .document()
        .entities
        .iter()
        .find(|entity| entity.id == request.entity)
    else {
        let runtime_contains_entity = world.iter_entities().any(|entity| {
            entity
                .get::<SceneEntitySource>()
                .is_some_and(|source| source.entity_id == request.entity)
        });
        let code = if runtime_contains_entity {
            "tooling.apply-changes-prefab-expanded-entity"
        } else {
            "tooling.apply-changes-missing-scene-entity"
        };
        return apply_changes_entity_error_report(
            Some(source_revision),
            current_revision,
            code,
            "Apply Changes targets an entity that cannot be written back to the authoring document",
            &request.entity,
        );
    };
    if document_entity.prefab.is_some() {
        return apply_changes_entity_error_report(
            Some(source_revision),
            current_revision,
            "tooling.apply-changes-prefab-anchor-entity",
            "Apply Changes does not write directly to prefab instance anchors yet",
            &request.entity,
        );
    }

    let mut runtime_entity = None;
    let mut matches = 0_usize;
    for entity in world.iter_entities() {
        if entity
            .get::<SceneEntitySource>()
            .is_some_and(|source| source.entity_id == request.entity)
        {
            matches = matches.saturating_add(1);
            runtime_entity = Some(entity.id());
        }
    }
    let Some(runtime_entity) = runtime_entity else {
        return apply_changes_entity_error_report(
            Some(source_revision),
            current_revision,
            "tooling.apply-changes-missing-runtime-entity",
            "Apply Changes could not resolve the selected runtime entity",
            &request.entity,
        );
    };
    if matches != 1 {
        return apply_changes_entity_error_report(
            Some(source_revision),
            current_revision,
            "tooling.apply-changes-ambiguous-runtime-entity",
            "Apply Changes resolved more than one runtime entity",
            &request.entity,
        );
    }
    if request.components.is_empty() {
        return SceneApplyChangesReport {
            applied: false,
            supported: true,
            source_revision: Some(source_revision),
            current_revision,
            patch: None,
            patch_report: None,
            components: Vec::new(),
            diagnostics: DiagnosticReport::default(),
        };
    }

    let context =
        ComponentEncodeContext::new().with_asset_ref_export_policy(request.asset_ref_export_policy);
    let mut seen_components = BTreeSet::new();
    let mut operations = Vec::new();
    let mut component_reports = Vec::new();
    let mut diagnostics = DiagnosticReport::default();
    for component in &request.components {
        if !seen_components.insert(component.clone()) {
            let component_report = rejected_component_report(
                &request.entity,
                component,
                component_error(
                    "tooling.apply-changes-duplicate-component",
                    "Apply Changes request contains the same component more than once",
                    &request.entity,
                    component,
                ),
            );
            let _ = diagnostics.extend(component_report.diagnostics.clone());
            component_reports.push(component_report);
            continue;
        }
        let component_change = export_component_change(
            world,
            runtime_entity,
            document_entity,
            registry,
            component,
            &context,
        );
        let _ = diagnostics.extend(component_change.report.diagnostics.clone());
        if let Some(operation) = component_change.operation {
            operations.push(operation);
        }
        component_reports.push(component_change.report);
    }
    if component_reports
        .iter()
        .any(|report| report.status == SceneApplyChangesComponentStatus::Rejected)
    {
        return SceneApplyChangesReport {
            applied: false,
            supported: false,
            source_revision: Some(source_revision),
            current_revision,
            patch: None,
            patch_report: None,
            components: component_reports,
            diagnostics,
        };
    }
    SceneApplyChangesReport {
        applied: false,
        supported: true,
        source_revision: Some(source_revision),
        current_revision,
        patch: (!operations.is_empty()).then(|| ScenePatchDocument::new(operations)),
        patch_report: None,
        components: component_reports,
        diagnostics,
    }
}

fn apply_changes_error_report(
    source_revision: Option<SceneAuthoringRevision>,
    current_revision: SceneAuthoringRevision,
    code: &'static str,
    summary: &'static str,
) -> SceneApplyChangesReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(apply_changes_diagnostic(
        code,
        summary,
        source_revision,
        current_revision,
    ));
    SceneApplyChangesReport {
        applied: false,
        supported: false,
        source_revision,
        current_revision,
        patch: None,
        patch_report: None,
        components: Vec::new(),
        diagnostics,
    }
}

fn apply_changes_entity_error_report(
    source_revision: Option<SceneAuthoringRevision>,
    current_revision: SceneAuthoringRevision,
    code: &'static str,
    summary: &'static str,
    entity: &SceneEntityId,
) -> SceneApplyChangesReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic::with_entity(
        apply_changes_diagnostic(code, summary, source_revision, current_revision),
        entity,
    ));
    SceneApplyChangesReport {
        applied: false,
        supported: false,
        source_revision,
        current_revision,
        patch: None,
        patch_report: None,
        components: Vec::new(),
        diagnostics,
    }
}

struct ExportedComponentChange {
    report: SceneApplyChangesComponentReport,
    operation: Option<ScenePatchOperation>,
}

fn export_component_change(
    world: &World,
    play_entity: Entity,
    document_entity: &SceneEntityRecord,
    registry: &ComponentRegistry,
    component: &ComponentTypeId,
    context: &ComponentEncodeContext<'_>,
) -> ExportedComponentChange {
    let Some(schema) = registry.schema(component) else {
        return ExportedComponentChange {
            report: rejected_component_report(
                &document_entity.id,
                component,
                component_error(
                    "tooling.apply-changes-unknown-component",
                    "Apply Changes targets a component that is not registered",
                    &document_entity.id,
                    component,
                ),
            ),
            operation: None,
        };
    };

    if !schema.has_capability(ComponentCapability::Scene)
        || !schema.has_capability(ComponentCapability::Edit)
    {
        return ExportedComponentChange {
            report: rejected_component_report(
                &document_entity.id,
                component,
                component_error(
                    "tooling.apply-changes-component-not-editable",
                    "Apply Changes targets a component without scene/edit capabilities",
                    &document_entity.id,
                    component,
                ),
            ),
            operation: None,
        };
    }

    let Some(encoded) =
        registry.encode_component_with_context(component, world, play_entity, context)
    else {
        return ExportedComponentChange {
            report: rejected_component_report(
                &document_entity.id,
                component,
                component_error(
                    "tooling.apply-changes-missing-component-codec",
                    "Apply Changes targets a component without an encode codec",
                    &document_entity.id,
                    component,
                ),
            ),
            operation: None,
        };
    };

    let runtime_value = match encoded {
        Ok(value) => value,
        Err(error) => {
            return ExportedComponentChange {
                report: rejected_component_report(
                    &document_entity.id,
                    component,
                    with_codec_error(
                        component_error(
                            "tooling.apply-changes-component-encode-failed",
                            "Apply Changes could not encode the runtime component",
                            &document_entity.id,
                            component,
                        ),
                        &error,
                    ),
                ),
                operation: None,
            };
        }
    };

    let document_component = match document_entity.components.get(component) {
        Some(component_record) => match registry.migrate_component_value(
            component,
            component_record.version,
            &component_record.value,
        ) {
            Ok(migrated) => Some(SceneComponentRecord::new(migrated.version, migrated.value)),
            Err(error) => {
                return ExportedComponentChange {
                    report: rejected_component_report(
                        &document_entity.id,
                        component,
                        with_migration_error(
                            component_error(
                                "tooling.apply-changes-document-component-migration-failed",
                                "Apply Changes could not canonicalize the document component",
                                &document_entity.id,
                                component,
                            ),
                            &error,
                        ),
                    ),
                    operation: None,
                };
            }
        },
        None => None,
    };
    let operation = match (document_component.as_ref(), runtime_value) {
        (None, None) => None,
        (Some(_), None) => Some(ScenePatchOperation::RemoveComponent {
            entity: document_entity.id.clone(),
            component: component.clone(),
        }),
        (None, Some(value)) => Some(ScenePatchOperation::AddComponent {
            entity: document_entity.id.clone(),
            component: component.clone(),
            value: SceneComponentRecord::new(schema.version, value),
        }),
        (Some(current), Some(value)) => {
            let replacement = SceneComponentRecord::new(schema.version, value);
            if current == &replacement {
                None
            } else {
                Some(ScenePatchOperation::ReplaceComponent {
                    entity: document_entity.id.clone(),
                    component: component.clone(),
                    value: replacement,
                })
            }
        }
    };

    let operation_count = usize::from(operation.is_some());
    ExportedComponentChange {
        report: SceneApplyChangesComponentReport {
            entity: document_entity.id.clone(),
            component: component.clone(),
            status: if operation.is_some() {
                SceneApplyChangesComponentStatus::Pending
            } else {
                SceneApplyChangesComponentStatus::NoOp
            },
            operation_count,
            diagnostics: DiagnosticReport::default(),
        },
        operation,
    }
}

fn rejected_component_report(
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    entry: Diagnostic,
) -> SceneApplyChangesComponentReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic::with_public_u64(entry, "operation_count", 0));
    SceneApplyChangesComponentReport {
        entity: entity.clone(),
        component: component.clone(),
        status: SceneApplyChangesComponentStatus::Rejected,
        operation_count: 0,
        diagnostics,
    }
}

fn component_error(
    code: &'static str,
    summary: &'static str,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
) -> Diagnostic {
    diagnostic::with_component(
        diagnostic::with_entity(diagnostic::error(code, summary), entity),
        component,
    )
}

fn with_codec_error(entry: Diagnostic, error: &ComponentCodecError) -> Diagnostic {
    match error {
        ComponentCodecError::MissingField { field } => diagnostic::with_public_identifier(
            diagnostic::with_public_identifier(entry, "codec_reason", "missing_field"),
            "field",
            field,
        ),
        ComponentCodecError::InvalidField { field, .. } => diagnostic::with_secret(
            diagnostic::with_public_identifier(
                diagnostic::with_public_identifier(entry, "codec_reason", "invalid_field"),
                "field",
                field,
            ),
            "expected",
        ),
        ComponentCodecError::InvalidAssetRef { field, .. } => diagnostic::with_secret(
            diagnostic::with_sensitive(
                diagnostic::with_public_identifier(
                    diagnostic::with_public_identifier(entry, "codec_reason", "invalid_asset_ref"),
                    "field",
                    field,
                ),
                "asset_ref",
            ),
            "message",
        ),
        ComponentCodecError::EntityMissing => {
            diagnostic::with_public_identifier(entry, "codec_reason", "entity_missing")
        }
        ComponentCodecError::WrongWorld => {
            diagnostic::with_public_identifier(entry, "codec_reason", "wrong_world")
        }
        ComponentCodecError::AssetServerChanged => {
            diagnostic::with_public_identifier(entry, "codec_reason", "asset_server_changed")
        }
        ComponentCodecError::PreparedComponentTypeMismatch { .. } => diagnostic::with_secret(
            diagnostic::with_public_identifier(
                entry,
                "codec_reason",
                "prepared_component_type_mismatch",
            ),
            "codec_detail",
        ),
        ComponentCodecError::PersistentApplyReceiptMissing => diagnostic::with_public_identifier(
            entry,
            "codec_reason",
            "persistent_apply_receipt_missing",
        ),
        ComponentCodecError::PersistentApplyTargetNotEmpty => diagnostic::with_public_identifier(
            entry,
            "codec_reason",
            "persistent_apply_target_not_empty",
        ),
        ComponentCodecError::PersistentApplyBindingConflict { component_id } => {
            diagnostic::with_public_identifier(
                diagnostic::with_public_identifier(
                    entry,
                    "codec_reason",
                    "persistent_apply_binding_conflict",
                ),
                "persistent_component",
                component_id.as_str(),
            )
        }
        ComponentCodecError::PersistentApplySupportRejected { reason } => {
            with_persistent_apply_rejection(
                diagnostic::with_public_identifier(
                    entry,
                    "codec_reason",
                    "persistent_apply_support_rejected",
                ),
                reason,
            )
        }
        ComponentCodecError::PersistentApplyRejected {
            component_id,
            reason,
        } => {
            let entry = diagnostic::with_public_identifier(
                diagnostic::with_public_identifier(
                    entry,
                    "codec_reason",
                    "persistent_apply_rejected",
                ),
                "persistent_component",
                component_id.as_str(),
            );
            with_persistent_apply_rejection(entry, reason)
        }
        ComponentCodecError::Message(_) => diagnostic::with_secret(
            diagnostic::with_public_identifier(entry, "codec_reason", "message"),
            "message",
        ),
    }
}

fn with_persistent_apply_rejection(
    entry: Diagnostic,
    reason: &PersistentApplyRejection,
) -> Diagnostic {
    match reason {
        PersistentApplyRejection::ComponentMetadataMissing => diagnostic::with_public_identifier(
            entry,
            "persistent_apply_reason",
            "component_metadata_missing",
        ),
        PersistentApplyRejection::RequiredComponents => diagnostic::with_public_identifier(
            entry,
            "persistent_apply_reason",
            "required_components",
        ),
        PersistentApplyRejection::LifecycleHook { event } => diagnostic::with_public_identifier(
            diagnostic::with_public_identifier(entry, "persistent_apply_reason", "lifecycle_hook"),
            "lifecycle_event",
            event.as_str(),
        ),
        PersistentApplyRejection::Observer { event, scope } => diagnostic::with_public_identifier(
            diagnostic::with_public_identifier(
                diagnostic::with_public_identifier(
                    entry,
                    "persistent_apply_reason",
                    "lifecycle_observer",
                ),
                "lifecycle_event",
                event.as_str(),
            ),
            "observer_scope",
            scope.as_str(),
        ),
    }
}

fn with_migration_error(entry: Diagnostic, error: &ComponentMigrationError) -> Diagnostic {
    match error {
        ComponentMigrationError::UnknownComponentId { component_id } => {
            migration_error_base(entry, "unknown_component", component_id)
        }
        ComponentMigrationError::UnsupportedVersion {
            component_id,
            from_version,
            target_version,
        } => diagnostic::with_public_u64(
            diagnostic::with_public_u64(
                migration_error_base(entry, "unsupported_version", component_id),
                "from_version",
                u64::from(from_version.0),
            ),
            "target_version",
            u64::from(target_version.0),
        ),
        ComponentMigrationError::MissingMigration {
            component_id,
            from_version,
            target_version,
        } => diagnostic::with_public_u64(
            diagnostic::with_public_u64(
                migration_error_base(entry, "missing_migration", component_id),
                "from_version",
                u64::from(from_version.0),
            ),
            "target_version",
            u64::from(target_version.0),
        ),
        ComponentMigrationError::MigrationFailed {
            component_id,
            from_version,
            to_version,
            error,
        } => with_codec_error(
            diagnostic::with_public_u64(
                diagnostic::with_public_u64(
                    migration_error_base(entry, "migration_failed", component_id),
                    "from_version",
                    u64::from(from_version.0),
                ),
                "to_version",
                u64::from(to_version.0),
            ),
            error,
        ),
    }
}

fn migration_error_base(
    entry: Diagnostic,
    reason: &'static str,
    component: &ComponentTypeId,
) -> Diagnostic {
    diagnostic::with_public_identifier(
        diagnostic::with_public_identifier(entry, "migration_reason", reason),
        "migration_component",
        component.as_str(),
    )
}

fn apply_changes_diagnostic(
    code: &'static str,
    summary: &'static str,
    source_revision: Option<SceneAuthoringRevision>,
    current_revision: SceneAuthoringRevision,
) -> Diagnostic {
    let mut entry = diagnostic::with_public_u64(
        diagnostic::error(code, summary),
        "current_revision",
        current_revision.generation(),
    );
    if let Some(source_revision) = source_revision {
        entry = diagnostic::with_public_u64(entry, "source_revision", source_revision.generation());
    }
    entry
}
