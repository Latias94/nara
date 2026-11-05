use nara_ui_kernel::{UiAccessibilityInput, UiSeatId, UiSemanticFact};

use crate::platform::drawn::operations::Observation;

use super::*;

impl Workflow<'_> {
    fn observation(&mut self, kind: &str) -> TestResult<Observation> {
        self.native
            .observe()?
            .into_iter()
            .find(|observation| observation.image.endpoint().token().kind() == kind)
            .ok_or_else(|| format!("The {kind} panel has no accepted public observation"))
    }

    fn observed_control(
        observation: &Observation,
        role: UiSemanticRole,
        name: &str,
    ) -> TestResult<UiSemanticFact> {
        let mut matches = observation
            .semantics
            .nodes()
            .iter()
            .filter(|node| node.role() == role && node.name() == name);
        let node = matches
            .next()
            .cloned()
            .ok_or_else(|| format!("No observed {role:?} {name:?}"))?;
        if matches.next().is_some() {
            return Err(format!("Ambiguous observed {role:?} {name:?}"));
        }
        let rect = node.physical_rect();
        if observation
            .image
            .window_bounds(nara::render::DisplayRect {
                min: [rect.min_x() as f32, rect.min_y() as f32],
                max: [rect.max_x() as f32, rect.max_y() as f32],
            })
            .is_none()
        {
            return Err("The observed semantic control has no visible native mapping".into());
        }
        Ok(node)
    }

    fn observed_activate(
        &mut self,
        kind: &str,
        name: &str,
    ) -> TestResult<Vec<crate::PanelSubmissionId>> {
        self.paint()?;
        let observation = self.observation(kind)?;
        let node = Self::observed_control(&observation, UiSemanticRole::Button, name)?;
        if !node.state().enabled() || node.state().inert() || node.actions().len() != 1 {
            return Err(format!(
                "The observed {kind}/{name} has no single admitted behavior action"
            ));
        }
        let request = UiAccessibilityInput::semantic_action_from_route(
            observation.semantics.route(),
            UiSeatId::PRIMARY,
            node.key().clone(),
            node.actions()[0].clone(),
        );
        let panel = observation.image.endpoint().instance();
        let ids = self.native.operate(&observation, &request)?;
        if ids.iter().any(|(instance, _)| *instance != panel) {
            return Err("An observed control submitted work for another original panel".into());
        }
        self.paint()?;
        Ok(ids.into_iter().map(|(_, id)| id).collect())
    }

    fn observed_key(&mut self, observation: &Observation, key: Key) -> TestResult {
        for state in [ElementState::Pressed, ElementState::Released] {
            if !self.native.observed_input(
                observation,
                EditorPanelPlatformEvent::logical_key(key.clone(), state, false, None),
            )? {
                return Err("The observed keyboard input was not consumed".into());
            }
        }
        Ok(())
    }

    pub(super) fn observed_inspector_workflow(&mut self) -> TestResult {
        let original = self.project.read(fixture::SCENE)?;
        let initial = self.state()?;
        let selected = self.selected()?;
        if !self
            .observed_activate(INSPECTOR, "Add Component")?
            .is_empty()
            || self
                .observed_activate(INSPECTOR, "Add nara.scene.Name")?
                .len()
                != 1
        {
            return Err("Observed component creation lost its original submission identity".into());
        }
        let before = self.expect_one_mutation(&initial)?;
        let observation = self.observation(INSPECTOR)?;
        if observation.image.endpoint().token().document() != Some(before.document) {
            return Err("The observed Inspector lost its exact document scope".into());
        }
        let node = observation
            .semantics
            .nodes()
            .iter()
            .find(|node| node.role() == UiSemanticRole::TextInput && node.state().enabled())
            .ok_or("No observed editable Inspector field")?
            .clone();
        let field = node.name().to_owned();
        let original_value = node
            .value()
            .ok_or("The observed field has no original semantic value")?
            .to_owned();
        let focus = UiAccessibilityInput::focus_target_from_route(
            observation.semantics.route(),
            UiSeatId::PRIMARY,
            node.key().clone(),
        );
        if !self.native.operate(&observation, &focus)?.is_empty() {
            return Err("Observed focus submitted a source mutation".into());
        }
        if !self
            .native
            .observed_input(
                &observation,
                EditorPanelPlatformEvent::Text("late focus".into()),
            )
            .is_err_and(|error| error.contains("text-focus lifetime"))
        {
            return Err(
                "The observation made before focus could type into its successor scope".into(),
            );
        }
        let focused = self.observation(INSPECTOR)?;
        if !focused.image.owns_keyboard()
            || focused.image.text_scope().is_none()
            || !focused
                .semantics
                .nodes()
                .iter()
                .any(|current| current.key() == node.key() && current.state().focused())
        {
            return Err("Semantic focus failed to acquire the original native text scope".into());
        }
        self.native.observed_input(
            &focused,
            EditorPanelPlatformEvent::ModifiersChanged(ModifiersState::CONTROL),
        )?;
        self.observed_key(&focused, Key::Character("a".into()))?;
        self.native.observed_input(
            &focused,
            EditorPanelPlatformEvent::ModifiersChanged(ModifiersState::empty()),
        )?;
        self.native.observed_input(
            &focused,
            EditorPanelPlatformEvent::Text("Observed 属性 café".into()),
        )?;
        self.paint()?;
        if self.panel_field_value(INSPECTOR, &field)? != "Observed 属性 café"
            || self.state()? != before
        {
            return Err("Observed input bypassed the draft or changed source before Apply".into());
        }
        if self.native.operate(&observation, &focus).is_ok() {
            return Err("An old observed image retargeted the newly presented field".into());
        }
        let enabled = self.observation(INSPECTOR)?;
        let apply = Self::observed_control(&enabled, UiSemanticRole::Button, "Apply")?;
        let apply_action = apply
            .actions()
            .first()
            .cloned()
            .ok_or("Enabled Apply did not advertise its original action")?;
        if self.observed_activate(INSPECTOR, "Apply")?.len() != 1 {
            return Err(
                "Observed Apply did not expose exactly one original source submission".into(),
            );
        }
        let applied = self.expect_one_mutation(&before)?;
        let disabled = self.observation(INSPECTOR)?;
        let button = Self::observed_control(&disabled, UiSemanticRole::Button, "Apply")?;
        if button.state().enabled() || !button.actions().is_empty() {
            return Err("Disabled Apply advertised executable semantic behavior".into());
        }
        let request = UiAccessibilityInput::semantic_action_from_route(
            disabled.semantics.route(),
            UiSeatId::PRIMARY,
            button.key().clone(),
            apply_action,
        );
        if self.native.operate(&disabled, &request).is_ok() || self.state()? != applied {
            return Err("A disabled observed action changed source or reported success".into());
        }
        if self.observed_activate(SHELL, "Undo")?.len() != 1 {
            return Err("Observed Undo lost its original action ID".into());
        }
        self.expect_rows(&before, before.undo, 1)?;
        if self.panel_field_value(INSPECTOR, &field)? != original_value {
            return Err("Observed Undo failed to restore the original field value".into());
        }
        self.observed_activate(SHELL, "Redo")?;
        self.expect_rows(&applied, applied.undo, 0)?;
        if self.panel_field_value(INSPECTOR, &field)? != "Observed 属性 café" {
            return Err("Observed Redo failed to restore the applied field value".into());
        }
        let save_basis = self.state()?;
        self.observed_activate(SHELL, "Save")?;
        self.wait("finish the observed Save attempt", |workflow| {
            workflow.frame().is_ok_and(|frame| {
                matches!(frame.persistence(), EditorPersistenceView::Terminal { .. })
            })
        })?;
        let save = self.frame()?;
        let attempt = save
            .persistence()
            .attempt()
            .ok_or("Observed Save has no exact attempt")?;
        if !matches!(save.persistence().result(), Some(EditorPersistenceResult::Saved { document, revision, .. }) if document == save_basis.document && revision == save_basis.revision)
        {
            return Err("Observed Save completed for another source/revision".into());
        }
        let saved = self.project.read(fixture::SCENE)?;
        if saved == original || self.state()?.dirty {
            return Err("Observed Save did not persist the visible edit".into());
        }
        self.observed_activate(SHELL, "Acknowledge persistence result")?;
        self.observed_activate(SHELL, "Reopen")?;
        self.wait("finish the observed Reopen attempt", |workflow| {
            workflow.frame().is_ok_and(|frame| {
                matches!(frame.persistence(), EditorPersistenceView::Terminal { .. })
            })
        })?;
        let reopened = self.frame()?;
        if reopened
            .persistence()
            .attempt()
            .is_none_or(|next| next == attempt)
            || !matches!(reopened.persistence().result(), Some(EditorPersistenceResult::Opened { document, .. }) if document == applied.document)
            || self.project.read(fixture::SCENE)? != saved
        {
            return Err("Observed Reopen lost its distinct result or exact persisted value".into());
        }
        self.expect_rows(&applied, 0, 0)?;
        let reopened_source = self.state()?;
        if reopened_source.dirty
            || reopened_source.revision.source_id() == save_basis.revision.source_id()
        {
            return Err("Observed Reopen did not replace the source incarnation cleanly".into());
        }
        self.observed_activate(SHELL, "Acknowledge persistence result")?;
        self.observed_activate(HIERARCHY, &format!("Select {selected}"))?;
        if self.panel_field_value(INSPECTOR, &field)? != "Observed 属性 café"
            || self.state()? != reopened_source
        {
            return Err("The reopened observed field lost its exact persisted value".into());
        }
        self.observed_activate(INSPECTOR, "Remove nara.scene.Name")?;
        let restored = self.state()?;
        self.save_reopen(fixture::SCENE, &restored)?;
        if self.project.read(fixture::SCENE)? != original {
            return Err(
                "The observed-control trajectory failed to restore the original complete source"
                    .into(),
            );
        }
        self.observed_activate(HIERARCHY, &format!("Select {selected}"))?;
        Ok(())
    }
}
