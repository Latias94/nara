use std::{collections::BTreeMap, sync::Arc, time::Duration};

use nara::tooling::{EditorActionBatch, EditorFrameModel};

use crate::{
    EditorApplication, EditorApplicationUpdate,
    app::EditorApplicationDispatch,
    workspace::{
        EditorPanelDescriptor, EditorPanelInstance, EditorPanelInstanceId, EditorPanelModel,
        EditorPanelRegistrationError,
    },
};

use super::{
    binding::PanelBinding,
    context::EditorPanelContext,
    submission::{
        PanelActionDispatch, PanelActionNotDispatched, PanelSubmissionId, PreparedSubmission,
        SubmissionAdmission, SubmissionOwner, resolve_result,
    },
};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// State and exact action observations for a source-built semantic panel.
///
/// Drawing belongs to the caller of [`EditorPanelHost::prepare`]. Panel state survives compatible
/// relocation and graphics reconstruction. Hiding or closing the instance, changing its target or
/// resolved document, restoring/resetting the layout, or replacing the project retires the state.
pub trait EditorPanel {
    /// Receives one exact dispatch result after application drive, while the original instance lives.
    fn action_finished(&mut self, _id: PanelSubmissionId, _dispatch: PanelActionDispatch) {}
}

/// Owned work prepared by exactly one live panel Host candidate.
///
/// Dropping the value performs no application work. Preparing a successor supersedes it; a foreign
/// Host or a later candidate cannot consume or settle its original observations.
#[must_use]
pub struct EditorPanelSubmission(PreparedSubmission<()>);

struct PanelState<P> {
    panel: P,
    binding: PanelBinding,
    lifetime: Arc<()>,
}

/// Toolkit-independent custody for typed panel state and ordered action observations.
///
/// The Host retains the Application's admitted frame and delegates every mutation to the original
/// action dispatcher. It owns no window, GPU, input route, document, Session, or drawing interface.
pub struct EditorPanelHost<P: EditorPanel> {
    factories: BTreeMap<String, Box<dyn Fn() -> P>>,
    instances: BTreeMap<EditorPanelInstanceId, PanelState<P>>,
    submissions: SubmissionOwner<()>,
    frame: Option<Arc<EditorFrameModel>>,
    workspace_generation: Option<u64>,
    layout_lifecycle: Option<Arc<()>>,
}

impl<P: EditorPanel> Default for EditorPanelHost<P> {
    fn default() -> Self {
        Self {
            factories: BTreeMap::new(),
            instances: BTreeMap::new(),
            submissions: SubmissionOwner::default(),
            frame: None,
            workspace_generation: None,
            layout_lifecycle: None,
        }
    }
}

impl<P: EditorPanel> EditorPanelHost<P> {
    pub(crate) fn identity(&self) -> &Arc<()> {
        &self.submissions.identity
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers the descriptor and factory together, preserving both on admission failure.
    pub fn register_panel(
        &mut self,
        application: &mut EditorApplication,
        descriptor: EditorPanelDescriptor,
        factory: impl Fn() -> P + 'static,
    ) -> Result<(), EditorPanelRegistrationError> {
        let kind = descriptor.kind().to_owned();
        if self.factories.contains_key(&kind) {
            return Err(EditorPanelRegistrationError::Duplicate(kind));
        }
        application.panel_workspace_mut().register(descriptor)?;
        self.factories.insert(kind, Box::new(factory));
        Ok(())
    }

    pub(crate) fn install_registered_panel(
        &mut self,
        application: &EditorApplication,
        kind: &str,
        factory: impl Fn() -> P + 'static,
    ) -> Result<(), EditorPanelRegistrationError> {
        if self.factories.contains_key(kind) {
            return Err(EditorPanelRegistrationError::Duplicate(kind.to_owned()));
        }
        if application
            .panel_workspace()
            .registry()
            .descriptor(kind)
            .is_none()
        {
            return Err(EditorPanelRegistrationError::Missing(kind.to_owned()));
        }
        self.factories.insert(kind.to_owned(), Box::new(factory));
        Ok(())
    }

    /// Reconciles live instances and prepares their actions against one retained admitted frame.
    ///
    /// The callback chooses the panel's presentation or semantic operation. It receives each
    /// visible registered instance in workspace order. Preparing again settles the preceding
    /// candidate as superseded only for its exact original instances that still live.
    pub fn prepare(
        &mut self,
        application: &EditorApplication,
        mut callback: impl FnMut(&mut P, &mut EditorPanelContext<'_, '_>),
    ) -> Option<EditorPanelSubmission> {
        let visits = application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .enumerate()
            .map(|(index, instance)| (index, Some(instance)));
        self.prepare_instances(application, visits, |_, panel, context| {
            callback(panel, context)
        })
    }

    /// Uses the same candidate for chronologically ordered native cancellation/input deliveries.
    /// Each callback index refers to the original visit, even when another visit was retired.
    pub(crate) fn prepare_ordered(
        &mut self,
        application: &EditorApplication,
        visits: &[EditorPanelInstanceId],
        mut callback: impl FnMut(usize, &mut P, &mut EditorPanelContext<'_, '_>),
    ) -> Option<EditorPanelSubmission> {
        let visits = visits
            .iter()
            .enumerate()
            .map(|(index, id)| (index, application.panel_workspace().layout().instance(*id)));
        self.prepare_instances(application, visits, &mut callback)
    }

    fn prepare_instances<'app>(
        &mut self,
        application: &'app EditorApplication,
        visits: impl IntoIterator<Item = (usize, Option<&'app EditorPanelInstance>)>,
        mut callback: impl FnMut(usize, &mut P, &mut EditorPanelContext<'_, '_>),
    ) -> Option<EditorPanelSubmission> {
        self.reconcile(application);
        self.settle_pending(None, Some(PanelActionNotDispatched::Superseded));
        let frame = Arc::clone(self.frame.as_ref()?);
        let mut actions = Vec::new();
        let mut submissions = Vec::new();
        let mut panels = Vec::new();
        for (index, instance) in visits {
            let Some(instance) = instance else {
                continue;
            };
            if !instance.is_visible() {
                continue;
            }
            let Some(factory) = self.factories.get(instance.kind()) else {
                continue;
            };
            let state = self
                .instances
                .entry(instance.id())
                .or_insert_with(|| PanelState {
                    panel: factory(),
                    binding: PanelBinding::for_instance(&frame, instance),
                    lifetime: Arc::new(()),
                });
            let origin = state.binding.origin(instance.id());
            panels.push(origin.clone());
            let mut context = EditorPanelContext {
                application,
                model: EditorPanelModel::new(&frame, instance),
                origin,
                state_lifetime: Arc::downgrade(&state.lifetime),
                host: Arc::clone(&self.submissions.identity),
                actions: &mut actions,
                submissions: &mut submissions,
                next_submission: &mut self.submissions.next_submission,
            };
            callback(index, &mut state.panel, &mut context);
        }
        let action_origins = submissions
            .iter()
            .map(|submission| (submission.slot, submission.origin.clone()))
            .collect();
        let serial = self.submissions.stage(
            self.workspace_generation,
            submissions,
            panels,
            action_origins,
        );
        self.submissions
            .seal(serial, EditorActionBatch::new(frame.revision(), actions))
            .map(EditorPanelSubmission)
    }

    /// Dispatches the current owned candidate, drives once, and settles original live callbacks.
    ///
    /// Foreign and superseded candidates perform no application work and cannot settle the current
    /// candidate. Actions belonging to retired instances are filtered before dispatch without
    /// changing the surviving actions' original result slots.
    pub fn submit_and_drive(
        &mut self,
        application: &mut EditorApplication,
        submission: EditorPanelSubmission,
        real_delta: Duration,
    ) -> EditorApplicationUpdate {
        if !self.submissions.owns(&submission.0) {
            return EditorApplicationUpdate::idle();
        }
        self.reconcile(application);
        let instances = &self.instances;
        match self
            .submissions
            .dispatch(application, submission.0, |origin| {
                instances.get(&origin.instance).is_some_and(|state| {
                    Arc::ptr_eq(&origin.incarnation, &state.binding.incarnation)
                })
            }) {
            SubmissionAdmission::Foreign => EditorApplicationUpdate::idle(),
            SubmissionAdmission::ProjectChanged => {
                self.settle_pending(None, Some(PanelActionNotDispatched::ProjectChanged));
                EditorApplicationUpdate::idle()
            }
            SubmissionAdmission::Dispatched(dispatch) => {
                let update =
                    application.drive_editor_frame(real_delta, dispatch.actions_observed(), true);
                self.reconcile(application);
                self.settle_pending(Some(&dispatch), None);
                update
            }
        }
    }

    fn reconcile(&mut self, application: &EditorApplication) {
        let lifecycle = application.panel_workspace().layout().lifecycle();
        if self
            .layout_lifecycle
            .as_ref()
            .is_none_or(|previous| !Arc::ptr_eq(previous, lifecycle))
        {
            self.instances.clear();
            self.layout_lifecycle = Some(Arc::clone(lifecycle));
        }
        let generation = application.workspace_generation();
        if self.workspace_generation != generation {
            self.instances.clear();
            self.workspace_generation = generation;
        }
        self.frame = application.current_frame();
        if let Some(frame) = &self.frame {
            self.instances.retain(|id, state| {
                application
                    .panel_workspace()
                    .layout()
                    .instance(*id)
                    .is_some_and(|instance| state.binding.matches(frame, instance))
            });
        } else {
            self.instances.clear();
        }
    }

    fn settle_pending(
        &mut self,
        dispatch: Option<&EditorApplicationDispatch>,
        reason: Option<PanelActionNotDispatched>,
    ) {
        let Some(pending) = self.submissions.pending.take() else {
            return;
        };
        let reason = reason.or(pending.failure);
        for submission in &pending.submissions {
            let Some(state) = self
                .instances
                .get_mut(&submission.origin.instance)
                .filter(|state| {
                    Arc::ptr_eq(&submission.origin.incarnation, &state.binding.incarnation)
                })
            else {
                continue;
            };
            state.panel.action_finished(
                submission.id,
                resolve_result(submission.slot, dispatch, reason),
            );
        }
    }
}
