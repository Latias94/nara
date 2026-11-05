use super::*;

#[derive(Debug)]
pub(super) struct ObservedUiOperationRefused(pub(super) nara_ui_kernel::UiNonActivation);

impl std::fmt::Display for ObservedUiOperationRefused {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "observed UI operation was refused: {:?}", self.0)
    }
}

impl std::error::Error for ObservedUiOperationRefused {}

impl Probe {
    pub(super) fn observe_panel(&mut self) -> Result<EditorPanelImageObservation<UiFrame>> {
        let mut observations = self.native.observe(&self.application)?;
        assert_eq!(observations.len(), 1);
        let observation = observations.pop().unwrap();
        assert_eq!(observation.endpoint(), observation.image().endpoint());
        assert_eq!(observation.endpoint().token().instance(), self.instance);
        assert_eq!(
            observation.endpoint().token().kind(),
            "consumer.native-caption"
        );
        assert_eq!(observation.endpoint().token().document(), None);
        assert!(Arc::ptr_eq(
            observation.snapshot(),
            observation.image().snapshot()
        ));
        assert!(Arc::ptr_eq(
            observation.snapshot(),
            self.presenter.journal().latest(self.surface)?.frame()
        ));
        let size = self.window.inner_size();
        assert_eq!(
            observation.geometry().physical_size,
            [size.width, size.height]
        );
        assert_eq!(
            observation.geometry().scale_factor,
            self.window.scale_factor()
        );
        assert_eq!(
            observation.display_frame_index(),
            self.gpu.as_ref().unwrap().frame
        );
        let extent = observation.image().mapping().target();
        assert!(
            observation
                .window_bounds(DisplayRect {
                    min: [0.0, 0.0],
                    max: [extent.width as f32, extent.height as f32],
                })
                .is_some()
        );
        Ok(observation)
    }

    pub(super) fn observed_event(&mut self, event: EditorPanelPlatformEvent) -> Result<bool> {
        let observation = self.observe_panel()?;
        assert!(observation.owns_keyboard());
        assert_eq!(
            observation.text_scope(),
            self.text_scope.as_ref().map(|(_, scope)| scope)
        );
        self.dispatch_input(Some(&observation), Some(event), None)
    }

    pub(super) fn invoke_control(&mut self, name: &str, activate: bool) -> Result<()> {
        let observation = self.observe_panel()?;
        let semantics = self.interaction.semantic_snapshot_with_action_authority(
            self.presenter.journal(),
            self.route.unwrap(),
            UiSeatId::PRIMARY,
            |key, action| {
                observation
                    .snapshot()
                    .semantics()
                    .iter()
                    .any(|node| node.key() == key && node.actions().contains(action))
            },
        )?;
        let mut matches = semantics.nodes().iter().filter(|node| node.name() == name);
        let node = matches
            .next()
            .ok_or_else(|| io::Error::other(format!("missing observed control: {name}")))?;
        assert!(matches.next().is_none());
        let rect = node.physical_rect();
        assert!(
            observation
                .window_bounds(DisplayRect {
                    min: [rect.min_x() as f32, rect.min_y() as f32],
                    max: [rect.max_x() as f32, rect.max_y() as f32],
                })
                .is_some()
        );
        let request = if activate {
            assert_eq!(node.actions().len(), 1);
            UiAccessibilityInput::semantic_action_from_route(
                semantics.route(),
                semantics.seat(),
                node.key().clone(),
                node.actions()[0].clone(),
            )
        } else {
            UiAccessibilityInput::focus_target_from_route(
                semantics.route(),
                semantics.seat(),
                node.key().clone(),
            )
        };
        assert!(self.dispatch_input(Some(&observation), None, Some(request))?);
        Ok(())
    }

    pub(super) fn invoke_inert_control(&mut self, name: &str) -> Result<()> {
        let observation = self.observe_panel()?;
        let semantics = self.interaction.semantic_snapshot_with_action_authority(
            self.presenter.journal(),
            self.route.unwrap(),
            UiSeatId::PRIMARY,
            |key, action| {
                observation
                    .snapshot()
                    .semantics()
                    .iter()
                    .any(|node| node.key() == key && node.actions().contains(action))
            },
        )?;
        let node = semantics
            .nodes()
            .iter()
            .find(|node| node.name() == name)
            .ok_or_else(|| io::Error::other(format!("missing inert control: {name}")))?;
        assert!(node.state().inert());
        assert!(node.actions().is_empty());
        let authored = observation
            .snapshot()
            .semantics()
            .iter()
            .find(|authored| authored.key() == node.key())
            .unwrap();
        assert_eq!(authored.actions().len(), 1);
        let request = UiAccessibilityInput::semantic_action_from_route(
            semantics.route(),
            semantics.seat(),
            authored.key().clone(),
            authored.actions()[0].clone(),
        );
        assert!(self.dispatch_input(Some(&observation), None, Some(request))?);
        Ok(())
    }

    pub(super) fn refuse_observed_scope(
        &mut self,
        observation: &EditorPanelImageObservation<UiFrame>,
    ) -> Result<()> {
        let token = observation.endpoint().token().clone();
        let state = self
            .panels
            .inspect(&self.application, &token, |panel, _| {
                (
                    panel.draft().to_owned(),
                    panel.display_text().to_owned(),
                    panel.input_token(),
                    panel.reviews(),
                    panel.popup_open(),
                )
            })
            .unwrap();
        let scope = self.text_scope.clone();
        let finished = self.evidence.finished.borrow().len();
        let cancellations = self.native_evidence.cancellations.len();
        let keys = self.native_evidence.keys;
        for event in [
            EditorPanelPlatformEvent::logical_key(
                Key::Character("stale".into()),
                ElementState::Pressed,
                false,
                Some("stale".into()),
            ),
            EditorPanelPlatformEvent::Text("stale observed scope".into()),
            EditorPanelPlatformEvent::Ime(Ime::Commit("stale observed scope".into())),
        ] {
            assert!(matches!(
                self.native.dispatch_observed(
                    &self.application,
                    &mut self.panels,
                    observation,
                    event,
                    |_, _, _, _| panic!("changed observed scope reached the provider"),
                ),
                Err(EditorPanelImageError::ChangedInputScope)
            ));
        }
        assert_eq!(
            self.panels.inspect(&self.application, &token, |panel, _| {
                (
                    panel.draft().to_owned(),
                    panel.display_text().to_owned(),
                    panel.input_token(),
                    panel.reviews(),
                    panel.popup_open(),
                )
            }),
            Some(state)
        );
        assert_eq!(self.text_scope, scope);
        assert_eq!(self.evidence.finished.borrow().len(), finished);
        assert_eq!(self.native_evidence.cancellations.len(), cancellations);
        assert_eq!(self.native_evidence.keys, keys);
        assert!(self.action.is_none() && self.action_id.is_none());
        Ok(())
    }

    pub(super) fn check_observation_replacement(&mut self) -> Result<()> {
        if let Some(previous) = &self.retained_observation {
            assert!(matches!(
                self.native.invoke_observed(
                    &self.application,
                    &mut self.panels,
                    previous,
                    |_, _, _, _| panic!("replaced observation reached the provider"),
                ),
                Err(EditorPanelImageError::StaleObservation)
            ));
            assert!(matches!(
                self.native.dispatch_observed(
                    &self.application,
                    &mut self.panels,
                    previous,
                    EditorPanelPlatformEvent::Text("replaced observation".into()),
                    |_, _, _, _| panic!("replaced observed input reached the provider"),
                ),
                Err(EditorPanelImageError::StaleObservation)
            ));
        }
        if self.stage != 4 {
            self.retained_observation = Some(self.observe_panel()?);
        } else {
            assert!(self.native.observe(&self.application)?.is_empty());
        }
        Ok(())
    }

    pub(super) fn check_foreign_observation(&mut self) -> Result<()> {
        let observation = self.observe_panel()?;
        let mut foreign = EditorPanelImageHost::new(
            Arc::clone(&self.window),
            WindowId::PRIMARY,
            self.clipboard.clone(),
        );
        foreign.attach(
            &self.application,
            observation.endpoint().token().clone(),
            [
                EditorPanelCapability::Keyboard,
                EditorPanelCapability::ObservedOperations,
            ],
        )?;
        assert!(foreign.observe(&self.application)?.is_empty());
        assert!(matches!(
            foreign.invoke_observed(
                &self.application,
                &mut self.panels,
                &observation,
                |_, _, _, _| panic!("foreign observation reached the provider"),
            ),
            Err(EditorPanelImageError::ForeignObservation)
        ));
        assert!(matches!(
            foreign.dispatch_observed(
                &self.application,
                &mut self.panels,
                &observation,
                EditorPanelPlatformEvent::Text("foreign observation".into()),
                |_, _, _, _| panic!("foreign observed input reached the provider"),
            ),
            Err(EditorPanelImageError::ForeignObservation)
        ));
        foreign.close();
        Ok(())
    }

    pub(super) fn inspect_pending_apply(
        &mut self,
        submission: &EditorPanelSubmission,
        id: PanelSubmissionId,
    ) -> Result<()> {
        let before = current(&self.application);
        let observation = self.observe_panel()?;
        assert_eq!(
            submission.submissions().collect::<Vec<_>>(),
            [(self.instance, id)]
        );
        assert_eq!(
            self.panels.inspect(
                &self.application,
                observation.endpoint().token(),
                |panel, model| {
                    assert_eq!(model.instance().id(), self.instance);
                    assert!(model.document().is_none());
                    (panel.draft().to_owned(), panel.popup_open())
                }
            ),
            Some(("native draft".into(), false))
        );
        assert_eq!(self.native.observe(&self.application)?.len(), 1);
        assert!(Arc::ptr_eq(&before, &current(&self.application)));
        assert!(self.evidence.finished.borrow().is_empty());
        assert_eq!(
            submission.submissions().collect::<Vec<_>>(),
            [(self.instance, id)]
        );
        Ok(())
    }
}
