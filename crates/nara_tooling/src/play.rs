//! UI-agnostic editor Play Mode lifecycle models.

use std::{collections::BTreeSet, fmt};

use nara_asset::{AssetRefExportPolicy, ProjectAssetDatabase};
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_ecs::{Entity, World};
use nara_reflect::{ComponentEncodeContext, ComponentRegistry, ComponentTypeId};
use nara_scene::{
    PrefabSourceResolver, SceneAuthoringRevision, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneEntityId, SceneEntityMap, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, ScenePatchReport, SceneSpawnReport, SceneSpawner,
};

use crate::inspector::{
    SceneInspectorCommand, SceneInspectorCommandReport, SceneInspectorModel, SceneInspectorState,
};
use crate::snapshot::WorldSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneEditorMode {
    Edit,
    Play {
        source_revision: SceneAuthoringRevision,
    },
    Paused {
        source_revision: SceneAuthoringRevision,
    },
}

impl SceneEditorMode {
    #[must_use]
    pub const fn source_revision(self) -> Option<SceneAuthoringRevision> {
        match self {
            Self::Edit => None,
            Self::Play { source_revision } | Self::Paused { source_revision } => {
                Some(source_revision)
            }
        }
    }

    #[must_use]
    pub const fn is_edit(self) -> bool {
        matches!(self, Self::Edit)
    }

    #[must_use]
    pub const fn is_play(self) -> bool {
        matches!(self, Self::Play { .. })
    }

    #[must_use]
    pub const fn is_paused(self) -> bool {
        matches!(self, Self::Paused { .. })
    }
}

pub struct ScenePlaySession {
    world: World,
    entity_map: SceneEntityMap,
    source_revision: SceneAuthoringRevision,
}

impl ScenePlaySession {
    #[must_use]
    pub fn new(
        world: World,
        entity_map: SceneEntityMap,
        source_revision: SceneAuthoringRevision,
    ) -> Self {
        Self {
            world,
            entity_map,
            source_revision,
        }
    }

    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    #[must_use]
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    #[must_use]
    pub fn entity_map(&self) -> &SceneEntityMap {
        &self.entity_map
    }

    #[must_use]
    pub fn source_revision(&self) -> SceneAuthoringRevision {
        self.source_revision
    }
}

impl fmt::Debug for ScenePlaySession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScenePlaySession")
            .field("entity_map", &self.entity_map)
            .field("source_revision", &self.source_revision)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenePlayTransitionReport {
    pub applied: bool,
    pub mode: SceneEditorMode,
    pub source_revision: Option<SceneAuthoringRevision>,
    pub entity_map: Option<SceneEntityMap>,
    pub diagnostics: DiagnosticReport,
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
    pub mode: SceneEditorMode,
    pub inspector: SceneInspectorModel,
    pub play_world_snapshot: Option<WorldSnapshot>,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Default)]
pub struct SceneEditorState {
    inspector: SceneInspectorState,
    play_session: Option<ScenePlaySession>,
    play_paused: bool,
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
    pub fn mode(&self) -> SceneEditorMode {
        let Some(play_session) = &self.play_session else {
            return SceneEditorMode::Edit;
        };

        let source_revision = play_session.source_revision();
        if self.play_paused {
            SceneEditorMode::Paused { source_revision }
        } else {
            SceneEditorMode::Play { source_revision }
        }
    }

    #[must_use]
    pub fn play_session(&self) -> Option<&ScenePlaySession> {
        self.play_session.as_ref()
    }

    #[must_use]
    pub fn play_session_mut(&mut self) -> Option<&mut ScenePlaySession> {
        self.play_session.as_mut()
    }

    #[must_use]
    pub fn play_world(&self) -> Option<&World> {
        self.play_session.as_ref().map(ScenePlaySession::world)
    }

    #[must_use]
    pub fn play_world_mut(&mut self) -> Option<&mut World> {
        self.play_session.as_mut().map(ScenePlaySession::world_mut)
    }

    #[must_use]
    pub fn play_entity_map(&self) -> Option<&SceneEntityMap> {
        self.play_session.as_ref().map(ScenePlaySession::entity_map)
    }

    pub fn start_play(
        &mut self,
        session: &SceneAuthoringSession,
        registry: &ComponentRegistry,
    ) -> ScenePlayTransitionReport {
        self.start_play_with_spawn(session, |spawner, world, document| {
            spawner.spawn(world, registry, document)
        })
    }

    pub fn start_play_with_asset_database(
        &mut self,
        session: &SceneAuthoringSession,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePlayTransitionReport {
        self.start_play_with_spawn(session, |spawner, world, document| {
            spawner.spawn_with_asset_database(world, registry, document, database)
        })
    }

    pub fn start_play_with_prefab_resolver<R: PrefabSourceResolver + ?Sized>(
        &mut self,
        session: &SceneAuthoringSession,
        registry: &ComponentRegistry,
        resolver: &R,
    ) -> ScenePlayTransitionReport {
        self.start_play_with_spawn(session, |spawner, world, document| {
            spawner.spawn_with_prefab_resolver(world, registry, document, resolver)
        })
    }

    pub fn start_play_with_prefab_resolver_and_asset_database<R: PrefabSourceResolver + ?Sized>(
        &mut self,
        session: &SceneAuthoringSession,
        registry: &ComponentRegistry,
        resolver: &R,
        database: &ProjectAssetDatabase,
    ) -> ScenePlayTransitionReport {
        self.start_play_with_spawn(session, |spawner, world, document| {
            spawner.spawn_with_prefab_resolver_and_asset_database(
                world, registry, document, resolver, database,
            )
        })
    }

    pub fn pause_play(&mut self) -> ScenePlayTransitionReport {
        let Some(play_session) = &self.play_session else {
            return transition_error_report(
                self.mode(),
                "tooling.play-pause-invalid-mode",
                "Play Mode can only be paused while currently playing",
            );
        };
        if self.play_paused {
            return transition_error_report(
                self.mode(),
                "tooling.play-pause-invalid-mode",
                "Play Mode is already paused",
            );
        }

        let source_revision = play_session.source_revision();
        let entity_map = play_session.entity_map().clone();
        self.play_paused = true;
        ScenePlayTransitionReport {
            applied: true,
            mode: self.mode(),
            source_revision: Some(source_revision),
            entity_map: Some(entity_map),
            diagnostics: DiagnosticReport::default(),
        }
    }

    pub fn resume_play(&mut self) -> ScenePlayTransitionReport {
        let Some(play_session) = &self.play_session else {
            return transition_error_report(
                self.mode(),
                "tooling.play-resume-invalid-mode",
                "Play Mode can only be resumed from Paused",
            );
        };
        if !self.play_paused {
            return transition_error_report(
                self.mode(),
                "tooling.play-resume-invalid-mode",
                "Play Mode is already running",
            );
        }

        let source_revision = play_session.source_revision();
        let entity_map = play_session.entity_map().clone();
        self.play_paused = false;
        ScenePlayTransitionReport {
            applied: true,
            mode: self.mode(),
            source_revision: Some(source_revision),
            entity_map: Some(entity_map),
            diagnostics: DiagnosticReport::default(),
        }
    }

    pub fn stop_play(&mut self) -> ScenePlayTransitionReport {
        let Some(play_session) = self.play_session.take() else {
            return transition_error_report(
                self.mode(),
                "tooling.play-stop-invalid-mode",
                "Play Mode can only be stopped while Play or Paused is active",
            );
        };

        let source_revision = play_session.source_revision();
        let entity_map = play_session.entity_map().clone();
        self.play_paused = false;
        ScenePlayTransitionReport {
            applied: true,
            mode: self.mode(),
            source_revision: Some(source_revision),
            entity_map: Some(entity_map),
            diagnostics: DiagnosticReport::default(),
        }
    }

    #[must_use]
    pub fn model(
        &mut self,
        session: &SceneAuthoringSession,
        registry: &ComponentRegistry,
        edit_world_snapshot: Option<&WorldSnapshot>,
    ) -> SceneEditorModel {
        let inspector = self.inspector.model(session, registry, edit_world_snapshot);
        let play_world_snapshot = self
            .play_session
            .as_mut()
            .map(|play_session| WorldSnapshot::capture(play_session.world_mut()));

        SceneEditorModel {
            mode: self.mode(),
            inspector,
            play_world_snapshot,
            diagnostics: DiagnosticReport::default(),
        }
    }

    pub fn apply_inspector_command(
        &mut self,
        session: &mut SceneAuthoringSession,
        registry: &ComponentRegistry,
        command: SceneInspectorCommand,
    ) -> SceneInspectorCommandReport {
        if self.mode().is_edit() || matches!(command, SceneInspectorCommand::SelectEntity { .. }) {
            return self.inspector.apply_command(session, registry, command);
        }

        persistent_inspector_command_rejected(self.inspector.selected_entity().cloned())
    }

    pub fn apply_inspector_command_with_asset_database(
        &mut self,
        session: &mut SceneAuthoringSession,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
        command: SceneInspectorCommand,
    ) -> SceneInspectorCommandReport {
        if self.mode().is_edit() || matches!(command, SceneInspectorCommand::SelectEntity { .. }) {
            return self
                .inspector
                .apply_command_with_asset_database(session, registry, database, command);
        }

        persistent_inspector_command_rejected(self.inspector.selected_entity().cloned())
    }

    #[must_use]
    pub fn apply_changes_status(&self, session: &SceneAuthoringSession) -> SceneApplyChangesReport {
        let current_revision = session.revision();
        let Some(play_session) = &self.play_session else {
            return apply_changes_error_report(
                None,
                current_revision,
                "tooling.apply-changes-not-in-play-mode",
                "Apply Changes requires an active Play Mode session",
            );
        };

        let source_revision = play_session.source_revision();
        if source_revision != current_revision {
            return apply_changes_error_report(
                Some(source_revision),
                current_revision,
                "tooling.apply-changes-revision-mismatch",
                "Apply Changes is unsupported because the authoring document changed after Play Mode started",
            );
        }

        SceneApplyChangesReport {
            applied: false,
            supported: true,
            source_revision: Some(source_revision),
            current_revision,
            patch: None,
            patch_report: None,
            components: Vec::new(),
            diagnostics: DiagnosticReport::default(),
        }
    }

    #[must_use]
    pub fn export_apply_changes(
        &self,
        session: &SceneAuthoringSession,
        registry: &ComponentRegistry,
        request: SceneApplyChangesRequest,
    ) -> SceneApplyChangesReport {
        let context = ComponentEncodeContext::new()
            .with_asset_ref_export_policy(request.asset_ref_export_policy);
        self.export_apply_changes_with_context(session, registry, request, &context)
    }

    #[must_use]
    pub fn export_apply_changes_with_asset_database(
        &self,
        session: &SceneAuthoringSession,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
        request: SceneApplyChangesRequest,
    ) -> SceneApplyChangesReport {
        let context = ComponentEncodeContext::new()
            .with_asset_ref_export_policy(request.asset_ref_export_policy)
            .with_project_asset_database(database);
        self.export_apply_changes_with_context(session, registry, request, &context)
    }

    pub fn apply_changes(
        &self,
        session: &mut SceneAuthoringSession,
        registry: &ComponentRegistry,
        request: SceneApplyChangesRequest,
    ) -> SceneApplyChangesReport {
        let report = self.export_apply_changes(session, registry, request);
        self.apply_exported_changes(
            session,
            |session, patch| session.apply_patch(patch, registry),
            report,
        )
    }

    pub fn apply_changes_with_asset_database(
        &self,
        session: &mut SceneAuthoringSession,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
        request: SceneApplyChangesRequest,
    ) -> SceneApplyChangesReport {
        let report =
            self.export_apply_changes_with_asset_database(session, registry, database, request);
        self.apply_exported_changes(
            session,
            |session, patch| session.apply_patch_with_asset_database(patch, registry, database),
            report,
        )
    }

    fn start_play_with_spawn(
        &mut self,
        session: &SceneAuthoringSession,
        mut spawn: impl FnMut(&mut SceneSpawner, &mut World, &SceneDocument) -> SceneSpawnReport,
    ) -> ScenePlayTransitionReport {
        if !self.mode().is_edit() {
            return transition_error_report(
                self.mode(),
                "tooling.play-start-invalid-mode",
                "Play Mode can only be started from Edit mode",
            );
        }

        let source_revision = session.revision();
        let mut world = World::new();
        let mut spawner = SceneSpawner::new();
        let report = spawn(&mut spawner, &mut world, session.document());

        if report.diagnostics.has_errors() {
            return ScenePlayTransitionReport {
                applied: false,
                mode: self.mode(),
                source_revision: Some(source_revision),
                entity_map: None,
                diagnostics: report.diagnostics,
            };
        }

        let entity_map = report.entity_map;
        self.play_session = Some(ScenePlaySession::new(
            world,
            entity_map.clone(),
            source_revision,
        ));
        self.play_paused = false;
        ScenePlayTransitionReport {
            applied: true,
            mode: self.mode(),
            source_revision: Some(source_revision),
            entity_map: Some(entity_map),
            diagnostics: report.diagnostics,
        }
    }

    fn export_apply_changes_with_context(
        &self,
        session: &SceneAuthoringSession,
        registry: &ComponentRegistry,
        request: SceneApplyChangesRequest,
        context: &ComponentEncodeContext<'_>,
    ) -> SceneApplyChangesReport {
        let current_revision = session.revision();
        let Some(play_session) = &self.play_session else {
            return apply_changes_error_report(
                None,
                current_revision,
                "tooling.apply-changes-not-in-play-mode",
                "Apply Changes requires an active Play Mode session",
            );
        };

        let source_revision = play_session.source_revision();
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
            let code = if play_session.entity_map().get(&request.entity).is_some() {
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

        let Some(play_entity) = play_session.entity_map().get(&request.entity) else {
            return apply_changes_entity_error_report(
                Some(source_revision),
                current_revision,
                "tooling.apply-changes-missing-runtime-entity",
                "Apply Changes targets an entity that was not spawned into the Play Mode world",
                &request.entity,
            );
        };

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

        let mut seen_components = BTreeSet::new();
        let mut operations = Vec::new();
        let mut component_reports = Vec::new();
        let mut diagnostics = DiagnosticReport::default();
        for component in &request.components {
            if !seen_components.insert(component.clone()) {
                let component_report = rejected_component_report(
                    &request.entity,
                    component,
                    Diagnostic::error(
                        "tooling.apply-changes-duplicate-component",
                        "Apply Changes request contains the same component more than once",
                    )
                    .with_entity_id(request.entity.as_str())
                    .with_component_id(component.as_str()),
                );
                diagnostics.extend(component_report.diagnostics.clone());
                component_reports.push(component_report);
                continue;
            }

            let component_change = export_component_change(
                play_session.world(),
                play_entity,
                document_entity,
                registry,
                component,
                context,
            );
            diagnostics.extend(component_change.report.diagnostics.clone());
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

        if operations.is_empty() {
            return SceneApplyChangesReport {
                applied: false,
                supported: true,
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
            patch: Some(ScenePatchDocument::new(operations)),
            patch_report: None,
            components: component_reports,
            diagnostics,
        }
    }

    fn apply_exported_changes(
        &self,
        session: &mut SceneAuthoringSession,
        mut apply_patch: impl FnMut(&mut SceneAuthoringSession, &ScenePatchDocument) -> ScenePatchReport,
        mut report: SceneApplyChangesReport,
    ) -> SceneApplyChangesReport {
        let Some(patch) = report.patch.clone() else {
            return report;
        };

        let patch_report = apply_patch(session, &patch);
        let applied = patch_report.applied;
        if applied {
            for component_report in &mut report.components {
                if component_report.status == SceneApplyChangesComponentStatus::Pending {
                    component_report.status = SceneApplyChangesComponentStatus::Applied;
                }
            }
        } else {
            for component_report in &mut report.components {
                if component_report.status == SceneApplyChangesComponentStatus::Pending {
                    component_report.status = SceneApplyChangesComponentStatus::Rejected;
                }
            }
        }
        report.applied = applied;
        report.current_revision = session.revision();
        report.diagnostics.extend(patch_report.diagnostics.clone());
        report.patch_report = Some(patch_report);
        report
    }
}

fn transition_error_report(
    mode: SceneEditorMode,
    code: impl Into<String>,
    message: impl Into<String>,
) -> ScenePlayTransitionReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(Diagnostic::error(code, message));
    ScenePlayTransitionReport {
        applied: false,
        mode,
        source_revision: mode.source_revision(),
        entity_map: None,
        diagnostics,
    }
}

fn persistent_inspector_command_rejected(
    selected_entity: Option<nara_scene::SceneEntityId>,
) -> SceneInspectorCommandReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(Diagnostic::error(
        "tooling.inspector-persistent-command-in-play-mode",
        "persistent inspector commands are only allowed in Edit mode",
    ));

    SceneInspectorCommandReport {
        applied: false,
        selected_entity,
        patch: None,
        patch_report: None,
        diagnostics,
    }
}

fn apply_changes_error_report(
    source_revision: Option<SceneAuthoringRevision>,
    current_revision: SceneAuthoringRevision,
    code: impl Into<String>,
    message: impl Into<String>,
) -> SceneApplyChangesReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(Diagnostic::error(code, message));
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
    code: impl Into<String>,
    message: impl Into<String>,
    entity: &SceneEntityId,
) -> SceneApplyChangesReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(Diagnostic::error(code, message).with_entity_id(entity.as_str()));
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
                Diagnostic::error(
                    "tooling.apply-changes-unknown-component",
                    "Apply Changes targets a component that is not registered",
                )
                .with_entity_id(document_entity.id.as_str())
                .with_component_id(component.as_str()),
            ),
            operation: None,
        };
    };

    if !schema.serializable {
        return ExportedComponentChange {
            report: rejected_component_report(
                &document_entity.id,
                component,
                Diagnostic::error(
                    "tooling.apply-changes-nonserializable-component",
                    "Apply Changes targets a component that is not serializable",
                )
                .with_entity_id(document_entity.id.as_str())
                .with_component_id(component.as_str()),
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
                Diagnostic::error(
                    "tooling.apply-changes-missing-component-codec",
                    "Apply Changes targets a component without an encode codec",
                )
                .with_entity_id(document_entity.id.as_str())
                .with_component_id(component.as_str()),
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
                    Diagnostic::error(
                        "tooling.apply-changes-component-encode-failed",
                        format!("Apply Changes could not encode the runtime component: {error}"),
                    )
                    .with_entity_id(document_entity.id.as_str())
                    .with_component_id(component.as_str()),
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
                        Diagnostic::error(
                            "tooling.apply-changes-document-component-migration-failed",
                            format!(
                                "Apply Changes could not canonicalize the document component: {error}"
                            ),
                        )
                        .with_entity_id(document_entity.id.as_str())
                        .with_component_id(component.as_str()),
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
    diagnostic: Diagnostic,
) -> SceneApplyChangesComponentReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic);
    SceneApplyChangesComponentReport {
        entity: entity.clone(),
        component: component.clone(),
        status: SceneApplyChangesComponentStatus::Rejected,
        operation_count: 0,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::AssetRef;
    use nara_ecs::Component;
    use nara_reflect::{
        ComponentCodecError, ComponentFieldPath, ComponentFieldSchema, ComponentSchemaVersion,
        ComponentValue, ComponentValueKind, PreparedComponent, Reflect, bevy_reflect,
    };
    use nara_scene::{
        InMemoryPrefabSourceResolver, Name, PrefabDocument, PrefabInstance, SceneEntityRecord,
        register_scene_components,
    };
    use nara_transform::{Transform2d, register_transform_components};

    #[derive(Clone, Debug, PartialEq, Component, Reflect)]
    struct RuntimeOnly;

    #[derive(Clone, Debug, PartialEq, Component)]
    struct BadExport;

    #[derive(Clone, Debug, PartialEq, Component)]
    struct MigratingPosition {
        x: i32,
    }

    #[test]
    fn exports_selected_runtime_component_change_as_replace_patch() {
        let registry = test_registry();
        let id = scene_id("player");
        let mut session = SceneAuthoringSession::new(scene_with_name(&id, "Hero"));
        let mut editor = SceneEditorState::new();
        let start = editor.start_play(&session, &registry);
        assert!(start.applied);

        let play_entity = editor.play_entity_map().unwrap().get(&id).unwrap();
        editor
            .play_world_mut()
            .unwrap()
            .entity_mut(play_entity)
            .insert(Name::new("Runtime Hero"));

        let export = editor.export_apply_changes(
            &session,
            &registry,
            SceneApplyChangesRequest::new(id.clone(), [name_type_id()]),
        );

        assert!(export.supported);
        assert!(!export.applied);
        assert_eq!(export.components.len(), 1);
        assert_eq!(
            export.components[0].status,
            SceneApplyChangesComponentStatus::Pending
        );
        let patch = export.patch.as_ref().unwrap();
        assert_eq!(patch.operations.len(), 1);
        assert!(matches!(
            &patch.operations[0],
            ScenePatchOperation::ReplaceComponent { entity, component, value }
                if entity == &id
                    && component == &name_type_id()
                    && value.value == ComponentValue::String("Runtime Hero".to_string())
        ));
        assert_eq!(session.history_status().undo_depth, 0);

        let apply = editor.apply_changes(
            &mut session,
            &registry,
            SceneApplyChangesRequest::new(id.clone(), [name_type_id()]),
        );

        assert!(apply.applied);
        assert!(apply.supported);
        assert_eq!(
            apply.components[0].status,
            SceneApplyChangesComponentStatus::Applied
        );
        assert_eq!(document_name_value(&session, &id), "Runtime Hero");
        assert_eq!(session.history_status().undo_depth, 1);

        let stale = editor.apply_changes(
            &mut session,
            &registry,
            SceneApplyChangesRequest::new(id.clone(), [name_type_id()]),
        );
        assert!(!stale.supported);
        assert!(!stale.applied);
        assert!(stale.diagnostics.diagnostics().iter().any(|diagnostic| {
            diagnostic.code.as_str() == "tooling.apply-changes-revision-mismatch"
        }));
        assert_eq!(session.history_status().undo_depth, 1);

        let undo = session.undo(&registry);
        assert!(undo.applied);
        assert_eq!(document_name_value(&session, &id), "Hero");
    }

    #[test]
    fn exports_changed_transform2d_as_replace_patch() {
        let registry = test_registry();
        let id = scene_id("player");
        let mut session = SceneAuthoringSession::new(scene_with_transform(&id, 0.0));
        let mut editor = SceneEditorState::new();
        assert!(editor.start_play(&session, &registry).applied);

        let play_entity = editor.play_entity_map().unwrap().get(&id).unwrap();
        editor
            .play_world_mut()
            .unwrap()
            .entity_mut(play_entity)
            .insert(Transform2d {
                rotation: 1.25,
                ..Transform2d::IDENTITY
            });

        let report = editor.apply_changes(
            &mut session,
            &registry,
            SceneApplyChangesRequest::new(id.clone(), [transform_type_id()]),
        );

        assert!(report.applied);
        assert!(report.supported);
        assert_eq!(
            report.components[0].status,
            SceneApplyChangesComponentStatus::Applied
        );
        assert_eq!(document_transform_rotation(&session, &id), 1.25);
        assert_eq!(session.history_status().undo_depth, 1);
    }

    #[test]
    fn adds_selected_runtime_component_missing_from_document() {
        let registry = test_registry();
        let id = scene_id("player");
        let mut session = SceneAuthoringSession::new(scene_empty_entity(&id));
        let mut editor = SceneEditorState::new();
        assert!(editor.start_play(&session, &registry).applied);

        let play_entity = editor.play_entity_map().unwrap().get(&id).unwrap();
        editor
            .play_world_mut()
            .unwrap()
            .entity_mut(play_entity)
            .insert(Name::new("Runtime Hero"));

        let report = editor.apply_changes(
            &mut session,
            &registry,
            SceneApplyChangesRequest::new(id.clone(), [name_type_id()]),
        );

        assert!(report.applied);
        assert!(matches!(
            &report.patch.as_ref().unwrap().operations[0],
            ScenePatchOperation::AddComponent { entity, component, value }
                if entity == &id
                    && component == &name_type_id()
                    && value.value == ComponentValue::String("Runtime Hero".to_string())
        ));
        assert_eq!(document_name_value(&session, &id), "Runtime Hero");
    }

    #[test]
    fn removes_selected_runtime_component_missing_from_play_world() {
        let registry = test_registry();
        let id = scene_id("player");
        let mut session = SceneAuthoringSession::new(scene_with_name(&id, "Hero"));
        let mut editor = SceneEditorState::new();
        assert!(editor.start_play(&session, &registry).applied);

        let play_entity = editor.play_entity_map().unwrap().get(&id).unwrap();
        editor
            .play_world_mut()
            .unwrap()
            .entity_mut(play_entity)
            .remove::<Name>();

        let report = editor.apply_changes(
            &mut session,
            &registry,
            SceneApplyChangesRequest::new(id.clone(), [name_type_id()]),
        );

        assert!(report.applied);
        assert!(matches!(
            &report.patch.as_ref().unwrap().operations[0],
            ScenePatchOperation::RemoveComponent { entity, component }
                if entity == &id && component == &name_type_id()
        ));
        assert!(!document_has_component(&session, &id, &name_type_id()));

        let undo = session.undo(&registry);
        assert!(undo.applied);
        assert_eq!(document_name_value(&session, &id), "Hero");
    }

    #[test]
    fn no_op_selected_runtime_component_change_does_not_push_undo() {
        let registry = test_registry();
        let id = scene_id("player");
        let mut session = SceneAuthoringSession::new(scene_with_name(&id, "Hero"));
        let mut editor = SceneEditorState::new();
        assert!(editor.start_play(&session, &registry).applied);

        let report = editor.apply_changes(
            &mut session,
            &registry,
            SceneApplyChangesRequest::new(id.clone(), [name_type_id()]),
        );

        assert!(report.supported);
        assert!(!report.applied);
        assert!(report.patch.is_none());
        assert_eq!(
            report.components[0].status,
            SceneApplyChangesComponentStatus::NoOp
        );
        assert_eq!(session.history_status().undo_depth, 0);
        assert_eq!(document_name_value(&session, &id), "Hero");
    }

    #[test]
    fn empty_apply_changes_request_is_supported_no_op() {
        let registry = test_registry();
        let id = scene_id("player");
        let mut session = SceneAuthoringSession::new(scene_with_name(&id, "Hero"));
        let mut editor = SceneEditorState::new();
        assert!(editor.start_play(&session, &registry).applied);

        let report = editor.apply_changes(
            &mut session,
            &registry,
            SceneApplyChangesRequest::new(id.clone(), []),
        );

        assert!(report.supported);
        assert!(!report.applied);
        assert!(report.patch.is_none());
        assert!(report.components.is_empty());
        assert!(report.diagnostics.is_empty());
        assert_eq!(session.history_status().undo_depth, 0);
        assert_eq!(document_name_value(&session, &id), "Hero");
    }

    #[test]
    fn no_op_comparison_uses_migrated_document_component_value() {
        let registry = test_registry_with_migrating_position();
        let id = scene_id("player");
        let mut session = SceneAuthoringSession::new(scene_with_legacy_migrating_position(&id, 7));
        let mut editor = SceneEditorState::new();
        assert!(editor.start_play(&session, &registry).applied);

        let report = editor.apply_changes(
            &mut session,
            &registry,
            SceneApplyChangesRequest::new(id.clone(), [migrating_position_type_id()]),
        );

        assert!(report.supported);
        assert!(!report.applied);
        assert!(report.patch.is_none());
        assert_eq!(
            report.components[0].status,
            SceneApplyChangesComponentStatus::NoOp
        );
        assert_eq!(session.history_status().undo_depth, 0);
        assert_eq!(
            document_component_version(&session, &id, &migrating_position_type_id()),
            ComponentSchemaVersion(1)
        );
    }

    #[test]
    fn rejects_apply_changes_without_active_play_session() {
        let registry = test_registry();
        let id = scene_id("player");
        let mut session = SceneAuthoringSession::new(scene_with_name(&id, "Hero"));
        let editor = SceneEditorState::new();

        let report = editor.apply_changes(
            &mut session,
            &registry,
            SceneApplyChangesRequest::new(id.clone(), [name_type_id()]),
        );

        assert!(!report.supported);
        assert!(!report.applied);
        assert!(report.patch.is_none());
        assert!(report.diagnostics.diagnostics().iter().any(|diagnostic| {
            diagnostic.code.as_str() == "tooling.apply-changes-not-in-play-mode"
        }));
    }

    #[test]
    fn rejects_apply_changes_when_authoring_revision_changed_after_play_started() {
        let registry = test_registry();
        let id = scene_id("player");
        let mut session = SceneAuthoringSession::new(scene_with_name(&id, "Hero"));
        let mut editor = SceneEditorState::new();
        assert!(editor.start_play(&session, &registry).applied);
        session.replace_document(scene_with_name(&id, "Edited"));

        let report = editor.apply_changes(
            &mut session,
            &registry,
            SceneApplyChangesRequest::new(id.clone(), [name_type_id()]),
        );

        assert!(!report.supported);
        assert!(!report.applied);
        assert!(report.patch.is_none());
        assert!(report.diagnostics.diagnostics().iter().any(|diagnostic| {
            diagnostic.code.as_str() == "tooling.apply-changes-revision-mismatch"
        }));
        assert_eq!(document_name_value(&session, &id), "Edited");
    }

    #[test]
    fn rejects_apply_changes_for_missing_scene_entity() {
        let registry = test_registry();
        let id = scene_id("player");
        let missing = scene_id("missing");
        let mut session = SceneAuthoringSession::new(scene_with_name(&id, "Hero"));
        let mut editor = SceneEditorState::new();
        assert!(editor.start_play(&session, &registry).applied);

        let report = editor.apply_changes(
            &mut session,
            &registry,
            SceneApplyChangesRequest::new(missing, [name_type_id()]),
        );

        assert!(!report.supported);
        assert!(!report.applied);
        assert!(report.diagnostics.diagnostics().iter().any(|diagnostic| {
            diagnostic.code.as_str() == "tooling.apply-changes-missing-scene-entity"
        }));
    }

    #[test]
    fn rejects_explicit_runtime_only_component_without_partial_mutation() {
        let registry = test_registry_with_runtime_only();
        let id = scene_id("player");
        let mut session = SceneAuthoringSession::new(scene_with_name(&id, "Hero"));
        let mut editor = SceneEditorState::new();
        assert!(editor.start_play(&session, &registry).applied);

        let play_entity = editor.play_entity_map().unwrap().get(&id).unwrap();
        editor
            .play_world_mut()
            .unwrap()
            .entity_mut(play_entity)
            .insert(RuntimeOnly);

        let report = editor.apply_changes(
            &mut session,
            &registry,
            SceneApplyChangesRequest::new(id.clone(), [runtime_only_type_id()]),
        );

        assert!(!report.supported);
        assert!(!report.applied);
        assert!(report.patch.is_none());
        assert_eq!(
            report.components[0].status,
            SceneApplyChangesComponentStatus::Rejected
        );
        assert!(report.diagnostics.diagnostics().iter().any(|diagnostic| {
            diagnostic.code.as_str() == "tooling.apply-changes-nonserializable-component"
        }));
        assert_eq!(document_name_value(&session, &id), "Hero");
        assert_eq!(session.history_status().undo_depth, 0);
    }

    #[test]
    fn rejects_prefab_expanded_entity_write_back() {
        let registry = test_registry();
        let source = AssetRef::path("prefabs/enemy.ron").unwrap();
        let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
            source.clone(),
            PrefabDocument::new([SceneEntityRecord::new(scene_id("visual"))
                .with_component(name_type_id(), name_record("Visual"))]),
        );
        let mut anchor = SceneEntityRecord::new(scene_id("enemy"));
        anchor.prefab = Some(PrefabInstance {
            source,
            overrides: ScenePatchDocument::default(),
        });
        let mut session = SceneAuthoringSession::new(SceneDocument::new([anchor]));
        let mut editor = SceneEditorState::new();
        assert!(
            editor
                .start_play_with_prefab_resolver(&session, &registry, &resolver)
                .applied
        );

        let report = editor.apply_changes(
            &mut session,
            &registry,
            SceneApplyChangesRequest::new(scene_id("enemy/visual"), [name_type_id()]),
        );

        assert!(!report.supported);
        assert!(!report.applied);
        assert!(report.patch.is_none());
        assert!(report.diagnostics.diagnostics().iter().any(|diagnostic| {
            diagnostic.code.as_str() == "tooling.apply-changes-prefab-expanded-entity"
        }));
    }

    #[test]
    fn failed_patch_validation_preserves_document_and_undo_history() {
        let registry = test_registry_with_bad_export();
        let id = scene_id("player");
        let mut session = SceneAuthoringSession::new(scene_empty_entity(&id));
        let mut editor = SceneEditorState::new();
        assert!(editor.start_play(&session, &registry).applied);

        let play_entity = editor.play_entity_map().unwrap().get(&id).unwrap();
        editor
            .play_world_mut()
            .unwrap()
            .entity_mut(play_entity)
            .insert(BadExport);

        let report = editor.apply_changes(
            &mut session,
            &registry,
            SceneApplyChangesRequest::new(id.clone(), [bad_export_type_id()]),
        );

        assert!(report.supported);
        assert!(!report.applied);
        assert!(report.patch.is_some());
        assert_eq!(
            report.components[0].status,
            SceneApplyChangesComponentStatus::Rejected
        );
        assert!(
            report
                .patch_report
                .as_ref()
                .unwrap()
                .diagnostics
                .has_errors()
        );
        assert!(!document_has_component(
            &session,
            &id,
            &bad_export_type_id()
        ));
        assert_eq!(session.history_status().undo_depth, 0);
    }

    fn test_registry() -> ComponentRegistry {
        let mut registry = ComponentRegistry::new();
        register_scene_components(&mut registry);
        register_transform_components(&mut registry);
        registry
    }

    fn test_registry_with_runtime_only() -> ComponentRegistry {
        let mut registry = test_registry();
        registry
            .register_component::<RuntimeOnly>(runtime_only_type_id(), ComponentSchemaVersion(1))
            .unwrap();
        registry
    }

    fn test_registry_with_bad_export() -> ComponentRegistry {
        let mut registry = test_registry();
        registry
            .register_component_codec_with_fields::<BadExport, _, _>(
                bad_export_type_id(),
                ComponentSchemaVersion(1),
                [ComponentFieldSchema::required(
                    ComponentFieldPath::from_fields(["x"]),
                    ComponentValueKind::I64,
                )],
                |value| {
                    value.field_i64("x")?;
                    Ok(PreparedComponent::insert(BadExport))
                },
                |world, entity| {
                    if world.get::<BadExport>(entity).is_some() {
                        Ok(Some(ComponentValue::Map(Default::default())))
                    } else {
                        Ok(None)
                    }
                },
            )
            .unwrap();
        registry
    }

    fn test_registry_with_migrating_position() -> ComponentRegistry {
        let mut registry = test_registry();
        registry
            .register_serializable_component_with_fields::<MigratingPosition, _, _>(
                migrating_position_type_id(),
                ComponentSchemaVersion(2),
                [ComponentFieldSchema::required(
                    ComponentFieldPath::from_fields(["x2"]),
                    ComponentValueKind::I64,
                )],
                |value| {
                    let x = value.field_i64("x2")?;
                    Ok(MigratingPosition {
                        x: i32::try_from(x)
                            .map_err(|_| ComponentCodecError::invalid_field("x2", "i32"))?,
                    })
                },
                |position| {
                    Ok(ComponentValue::map([(
                        "x2",
                        ComponentValue::I64(i64::from(position.x)),
                    )]))
                },
            )
            .unwrap()
            .register_component_migration(
                &migrating_position_type_id(),
                ComponentSchemaVersion(1),
                ComponentSchemaVersion(2),
                |value| {
                    let ComponentValue::Map(mut fields) = value else {
                        return Err(ComponentCodecError::invalid_field(
                            "MigratingPosition",
                            "map",
                        ));
                    };
                    if let Some(x) = fields.remove("x") {
                        fields.insert("x2".to_string(), x);
                    }
                    Ok(ComponentValue::Map(fields))
                },
            )
            .unwrap();
        registry
    }

    fn scene_empty_entity(id: &SceneEntityId) -> SceneDocument {
        SceneDocument::new([SceneEntityRecord::new(id.clone())])
    }

    fn scene_with_name(id: &SceneEntityId, name: &str) -> SceneDocument {
        SceneDocument::new([
            SceneEntityRecord::new(id.clone()).with_component(name_type_id(), name_record(name))
        ])
    }

    fn scene_with_transform(id: &SceneEntityId, rotation: f64) -> SceneDocument {
        SceneDocument::new([SceneEntityRecord::new(id.clone())
            .with_component(transform_type_id(), transform_record(rotation))])
    }

    fn scene_with_legacy_migrating_position(id: &SceneEntityId, x: i64) -> SceneDocument {
        SceneDocument::new([SceneEntityRecord::new(id.clone()).with_component(
            migrating_position_type_id(),
            SceneComponentRecord::new(
                ComponentSchemaVersion(1),
                ComponentValue::map([("x", ComponentValue::I64(x))]),
            ),
        )])
    }

    fn name_record(name: &str) -> SceneComponentRecord {
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            ComponentValue::String(name.to_string()),
        )
    }

    fn transform_record(rotation: f64) -> SceneComponentRecord {
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            ComponentValue::map([
                ("translation", vec2_value(0.0, 0.0)),
                ("rotation", ComponentValue::f64(rotation).unwrap()),
                ("scale", vec2_value(1.0, 1.0)),
            ]),
        )
    }

    fn vec2_value(x: f64, y: f64) -> ComponentValue {
        ComponentValue::map([
            ("x", ComponentValue::f64(x).unwrap()),
            ("y", ComponentValue::f64(y).unwrap()),
        ])
    }

    fn scene_id(id: &str) -> SceneEntityId {
        SceneEntityId::new(id).unwrap()
    }

    fn name_type_id() -> ComponentTypeId {
        ComponentTypeId::new("nara.scene.Name")
    }

    fn transform_type_id() -> ComponentTypeId {
        ComponentTypeId::new("nara.transform.Transform2d")
    }

    fn runtime_only_type_id() -> ComponentTypeId {
        ComponentTypeId::new("nara.test.RuntimeOnly")
    }

    fn bad_export_type_id() -> ComponentTypeId {
        ComponentTypeId::new("nara.test.BadExport")
    }

    fn migrating_position_type_id() -> ComponentTypeId {
        ComponentTypeId::new("nara.test.MigratingPosition")
    }

    fn document_name_value(session: &SceneAuthoringSession, id: &SceneEntityId) -> String {
        let component = session
            .document()
            .entities
            .iter()
            .find(|entity| &entity.id == id)
            .and_then(|entity| entity.components.get(&name_type_id()))
            .expect("test scene should contain Name");
        match &component.value {
            ComponentValue::String(value) => value.clone(),
            other => panic!("expected string Name value, got {other:?}"),
        }
    }

    fn document_transform_rotation(session: &SceneAuthoringSession, id: &SceneEntityId) -> f64 {
        let component = session
            .document()
            .entities
            .iter()
            .find(|entity| &entity.id == id)
            .and_then(|entity| entity.components.get(&transform_type_id()))
            .expect("test scene should contain Transform2d");
        component.value.field("rotation").unwrap().as_f64().unwrap()
    }

    fn document_has_component(
        session: &SceneAuthoringSession,
        id: &SceneEntityId,
        component: &ComponentTypeId,
    ) -> bool {
        session
            .document()
            .entities
            .iter()
            .find(|entity| &entity.id == id)
            .is_some_and(|entity| entity.components.contains_key(component))
    }

    fn document_component_version(
        session: &SceneAuthoringSession,
        id: &SceneEntityId,
        component: &ComponentTypeId,
    ) -> ComponentSchemaVersion {
        session
            .document()
            .entities
            .iter()
            .find(|entity| &entity.id == id)
            .and_then(|entity| entity.components.get(component))
            .expect("test scene should contain requested component")
            .version
    }
}
