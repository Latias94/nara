//! UI-agnostic editor Play Mode lifecycle models.

use std::fmt;

use nara_asset::ProjectAssetDatabase;
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_ecs::World;
use nara_reflect::ComponentRegistry;
use nara_scene::{
    PrefabSourceResolver, SceneAuthoringRevision, SceneAuthoringSession, SceneDocument,
    SceneEntityMap, SceneSpawnReport, SceneSpawner,
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

        apply_changes_error_report(
            Some(source_revision),
            current_revision,
            "tooling.apply-changes-unsupported",
            "Apply Changes is not implemented for runtime-to-scene write-back yet",
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
        diagnostics,
    }
}
