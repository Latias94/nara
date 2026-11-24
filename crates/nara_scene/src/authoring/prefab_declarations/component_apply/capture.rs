use std::{mem::size_of, sync::Arc};

use crate::{
    AuthoringSchemaBasis, PrefabAuthoringSession, PrefabDocument, SceneAuthoringRevision,
    SceneAuthoringSession, SceneDocument, ScenePatchReport, SourceDocumentKey, SourceDocumentKind,
    authoring::{
        AuthoringHistory, AuthoringSessionAuthority, history_error_report,
        source_checkpoint::{BorrowedSource, FrozenSource, SourceCheckpointError, admit_incumbent},
    },
    authoring_decode::{AuthoringDocumentState, AuthoringSourceParts},
    prefab::SourceDocumentInput,
};

use super::{PreparedPrefabComponentApply, work_limit_report};

#[derive(Clone, Copy)]
pub enum PrefabComponentApplySource<'a> {
    Scene(&'a SceneAuthoringSession),
    Prefab(&'a PrefabAuthoringSession),
}

#[derive(Debug)]
pub(super) enum CapturedContaining {
    Scene(FrozenSource<SceneDocument>),
    Prefab(FrozenSource<PrefabDocument>),
}

/// The exact published sources and history observations used by detached preparation.
#[derive(Debug)]
#[must_use]
pub struct CapturedPrefabComponentApplySources {
    pub(super) containing: CapturedContaining,
    pub(super) destination: FrozenSource<PrefabDocument>,
    pub(super) identity: Arc<()>,
    containing_key: SourceDocumentKey,
    destination_key: SourceDocumentKey,
}

impl CapturedPrefabComponentApplySources {
    pub fn containing_key(&self) -> &SourceDocumentKey {
        &self.containing_key
    }

    pub fn destination_key(&self) -> &SourceDocumentKey {
        &self.destination_key
    }

    pub fn containing_revision(&self) -> SceneAuthoringRevision {
        match &self.containing {
            CapturedContaining::Scene(source) => source.snapshot.revision,
            CapturedContaining::Prefab(source) => source.snapshot.revision,
        }
    }

    pub fn destination_revision(&self) -> SceneAuthoringRevision {
        self.destination.snapshot.revision
    }

    pub fn schema_basis(&self) -> AuthoringSchemaBasis {
        self.destination.state.schema_basis()
    }

    pub fn shared_containing_scene(&self) -> Option<Arc<SceneDocument>> {
        if let CapturedContaining::Scene(source) = &self.containing {
            Some(Arc::clone(&source.document))
        } else {
            None
        }
    }

    pub fn shared_containing_prefab(&self) -> Option<Arc<PrefabDocument>> {
        if let CapturedContaining::Prefab(source) = &self.containing {
            Some(Arc::clone(&source.document))
        } else {
            None
        }
    }

    pub fn shared_destination(&self) -> Arc<PrefabDocument> {
        Arc::clone(&self.destination.document)
    }

    pub fn owns_prepared(&self, prepared: &PreparedPrefabComponentApply) -> bool {
        prepared.matches_capture(&self.identity)
    }

    pub(super) fn containing_document(&self) -> SourceDocumentInput<'_> {
        match &self.containing {
            CapturedContaining::Scene(source) => SourceDocumentInput::Scene(&source.document),
            CapturedContaining::Prefab(source) => SourceDocumentInput::Prefab(&source.document),
        }
    }
}

/// Captures the actual source allocations only after admitting their retained work.
pub fn capture_prefab_component_apply_sources(
    containing: PrefabComponentApplySource<'_>,
    containing_key: &SourceDocumentKey,
    destination: &PrefabAuthoringSession,
    destination_key: &SourceDocumentKey,
    mut admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
) -> Result<CapturedPrefabComponentApplySources, ScenePatchReport> {
    let containing_kind = match containing {
        PrefabComponentApplySource::Scene(session) => {
            if session.live_instance().is_some() {
                return Err(history_error_report(
                    "scene.authoring-prefab-component-apply-live-source",
                    "Component Apply requires a detached Scene",
                ));
            }
            SourceDocumentKind::Scene
        }
        PrefabComponentApplySource::Prefab(_) => SourceDocumentKind::Prefab,
    };
    if containing_key.kind() != containing_kind
        || destination_key.kind() != SourceDocumentKind::Prefab
        || containing_key == destination_key
    {
        return Err(invalid_sources());
    }
    let destination_source = BorrowedSource::from_session(
        &destination.document,
        &destination.authority,
        &destination.history,
    )
    .map_err(checkpoint_error)?;
    let capture_bytes = size_of::<CapturedPrefabComponentApplySources>()
        .checked_add(2 * size_of::<usize>())
        .and_then(|bytes| bytes.checked_add(containing_key.checked_retained_bytes()?))
        .and_then(|bytes| bytes.checked_add(destination_key.checked_retained_bytes()?));
    let mut admit_pair = |parts, containing_state: &AuthoringDocumentState, history| {
        if containing_state.source() == destination_source.state.source() {
            return Err(invalid_sources());
        }
        if containing_state.schema_basis() != destination_source.state.schema_basis() {
            return Err(stale_source());
        }
        admit(capture_bytes)?;
        capture_bytes.ok_or_else(work_limit_report)?;
        admit_source(parts, containing_state, history, &mut admit)?;
        admit_source(
            destination_source.document.as_ref().into(),
            destination_source.state,
            &destination.history,
            &mut admit,
        )
    };
    let containing = match containing {
        PrefabComponentApplySource::Scene(session) => CapturedContaining::Scene(freeze_containing(
            &session.document,
            &session.authority,
            &session.history,
            &mut admit_pair,
        )?),
        PrefabComponentApplySource::Prefab(session) => {
            CapturedContaining::Prefab(freeze_containing(
                &session.document,
                &session.authority,
                &session.history,
                &mut admit_pair,
            )?)
        }
    };
    Ok(CapturedPrefabComponentApplySources {
        containing,
        destination: FrozenSource::capture(destination_source, || Ok::<(), ScenePatchReport>(()))?,
        identity: Arc::new(()),
        containing_key: containing_key.clone(),
        destination_key: destination_key.clone(),
    })
}

fn freeze_containing<'a, D>(
    document: &'a Arc<D>,
    authority: &'a AuthoringSessionAuthority,
    history: &'a AuthoringHistory,
    admit: &mut impl FnMut(
        AuthoringSourceParts<'a>,
        &'a AuthoringDocumentState,
        &'a AuthoringHistory,
    ) -> Result<(), ScenePatchReport>,
) -> Result<FrozenSource<D>, ScenePatchReport>
where
    AuthoringSourceParts<'a>: From<&'a D>,
{
    let source =
        BorrowedSource::from_session(document, authority, history).map_err(checkpoint_error)?;
    let state = source.state;
    FrozenSource::capture(source, || admit(document.as_ref().into(), state, history))
}

fn admit_source(
    parts: AuthoringSourceParts<'_>,
    state: &AuthoringDocumentState,
    history: &AuthoringHistory,
    admit: &mut impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
) -> Result<(), ScenePatchReport> {
    admit_incumbent(
        parts.placements,
        parts.entities,
        parts.inputs,
        state,
        history,
        0,
        |_, _, bytes| admit(bytes),
    )?;
    Ok(())
}

fn invalid_sources() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-prefab-component-apply-invalid-sources",
        "Component Apply requires distinct sources of the declared document kinds",
    )
}

pub(super) fn stale_source() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-prefab-component-apply-stale-source",
        "Component Apply requires the exact captured source, classification and history",
    )
}

pub(super) fn checkpoint_error(error: SourceCheckpointError) -> ScenePatchReport {
    match error {
        SourceCheckpointError::Stale => stale_source(),
        SourceCheckpointError::AuthorityUnavailable => history_error_report(
            "scene.authoring-prefab-component-apply-authority",
            "Component Apply requires published authoring authority for both sources",
        ),
    }
}
