//! Owner-bound, move-only preparation over the existing authoring transaction.

use super::{
    AuthoringSessionAuthority, PrefabAuthoringSession, SceneAuthoringHistoryCost,
    SceneAuthoringRevision, SceneAuthoringSession, authoring_field_edit_session_rejection,
    history_error_report,
};
use crate::{
    PrefabDocument, SceneDocument, ScenePatchDocument, ScenePatchReport,
    authoring_decode::{
        PreparedPrefabAuthoringPatch, PreparedSceneAuthoringPatch, prepare_prefab_authoring_patch,
        prepare_scene_authoring_patch,
    },
};

/// An unpublished Scene edit, valid only for its originating session and exact basis.
#[derive(Debug)]
pub struct PreparedSceneAuthoringEdit {
    revision: SceneAuthoringRevision,
    history_cost: SceneAuthoringHistoryCost,
    candidate: PreparedSceneAuthoringPatch,
}

impl PreparedSceneAuthoringEdit {
    #[must_use]
    pub const fn base_revision(&self) -> SceneAuthoringRevision {
        self.revision
    }

    #[must_use]
    pub fn document(&self) -> &SceneDocument {
        &self.candidate.document
    }

    /// Exact forward changes admitted against the previewed candidate.
    #[must_use]
    pub fn patch(&self) -> &ScenePatchDocument {
        &self.candidate.patch
    }

    /// Owner-created reverse changes; callers do not supply a second Undo implementation.
    #[must_use]
    pub fn inverse(&self) -> &ScenePatchDocument {
        self.candidate
            .report
            .inverse
            .as_ref()
            .expect("prepared history owns its inverse")
    }
}

impl SceneAuthoringSession {
    /// Prepares the same candidate later consumed by commit, without publishing any owner state.
    pub fn prepare_authoring_edit(
        &self,
        base_revision: SceneAuthoringRevision,
        patch: &ScenePatchDocument,
    ) -> Result<PreparedSceneAuthoringEdit, ScenePatchReport> {
        if base_revision != self.revision() {
            return Err(stale_basis());
        }
        let state = self
            .authority
            .published()
            .ok_or_else(authoring_field_edit_session_rejection)?;
        let mut candidate = prepare_scene_authoring_patch(&self.document, state, patch, false)?;
        let (report, history_cost) = self
            .history
            .admit_forward_report(&candidate.patch, candidate.report)?;
        candidate.report = report;
        Ok(PreparedSceneAuthoringEdit {
            revision: base_revision,
            history_cost,
            candidate,
        })
    }

    /// Consumes a prepared edit once, revalidating its basis before document/history publication.
    pub fn commit_authoring_edit(
        &mut self,
        prepared: PreparedSceneAuthoringEdit,
    ) -> ScenePatchReport {
        if self.revision() != prepared.revision
            || self.authoring_schema_basis() != Some(prepared.candidate.state.schema_basis())
        {
            return stale_basis();
        }
        let PreparedSceneAuthoringPatch {
            document,
            state,
            report,
            ..
        } = prepared.candidate;
        let report = self.history.publish_forward(
            std::sync::Arc::make_mut(&mut self.document),
            document,
            report,
            prepared.history_cost,
        );
        if report.applied {
            self.authority = AuthoringSessionAuthority::Published(state);
            self.live_dirty = true;
        }
        report
    }
}

/// An unpublished Prefab edit; it cannot be submitted to a Scene session.
#[derive(Debug)]
pub struct PreparedPrefabAuthoringEdit {
    revision: SceneAuthoringRevision,
    history_cost: SceneAuthoringHistoryCost,
    candidate: PreparedPrefabAuthoringPatch,
}

impl PreparedPrefabAuthoringEdit {
    #[must_use]
    pub const fn base_revision(&self) -> SceneAuthoringRevision {
        self.revision
    }

    #[must_use]
    pub fn document(&self) -> &PrefabDocument {
        &self.candidate.document
    }

    #[must_use]
    pub fn patch(&self) -> &ScenePatchDocument {
        &self.candidate.patch
    }

    #[must_use]
    pub fn inverse(&self) -> &ScenePatchDocument {
        self.candidate
            .report
            .inverse
            .as_ref()
            .expect("prepared history owns its inverse")
    }
}

impl PrefabAuthoringSession {
    /// Prepares an edit without changing the nominal Prefab document or its history.
    pub fn prepare_authoring_edit(
        &self,
        base_revision: SceneAuthoringRevision,
        patch: &ScenePatchDocument,
    ) -> Result<PreparedPrefabAuthoringEdit, ScenePatchReport> {
        if base_revision != self.revision() {
            return Err(stale_basis());
        }
        let state = self
            .authority
            .published()
            .ok_or_else(authoring_field_edit_session_rejection)?;
        let mut candidate = prepare_prefab_authoring_patch(&self.document, state, patch, false)?;
        let (report, history_cost) = self
            .history
            .admit_forward_report(&candidate.patch, candidate.report)?;
        candidate.report = report;
        Ok(PreparedPrefabAuthoringEdit {
            revision: base_revision,
            history_cost,
            candidate,
        })
    }

    /// Publishes the retained candidate and one history entry, or changes nothing on rejection.
    pub fn commit_authoring_edit(
        &mut self,
        prepared: PreparedPrefabAuthoringEdit,
    ) -> ScenePatchReport {
        if self.revision() != prepared.revision
            || self.authoring_schema_basis() != Some(prepared.candidate.state.schema_basis())
        {
            return stale_basis();
        }
        let PreparedPrefabAuthoringPatch {
            document,
            state,
            report,
            ..
        } = prepared.candidate;
        let report = self.history.publish_forward(
            &mut self.document,
            document,
            report,
            prepared.history_cost,
        );
        if report.applied {
            self.authority = AuthoringSessionAuthority::Published(state);
        }
        report
    }
}

pub(super) fn stale_basis() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-edit-stale-basis",
        "Prepared authoring edits require their exact originating session, revision and Schema admission",
    )
}
