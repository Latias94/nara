use nara::reflect::ComponentValue;

use super::*;

impl Workflow<'_> {
    pub(super) fn local_field_enter_keeps_focus(&mut self, name: &str, text: &str) -> TestResult {
        let before = self.state()?;
        self.replace_text(name, text)?;
        self.key(Key::Named(NamedKey::Enter))?;
        self.focus_field(name)?;
        self.key(Key::Named(NamedKey::End))?;
        self.event(EditorPanelPlatformEvent::Ime(Ime::Commit("x".into())))?;
        self.paint()?;
        if self.field_value(name)? != format!("{text}x") || self.state()? != before {
            return Err(
                "Enter or same-field refocus revoked local text input or mutated source".into(),
            );
        }
        Ok(())
    }

    pub(super) fn hierarchy_filter_completion(&mut self) -> TestResult {
        let before = self.state()?;
        let source = self.project.read(fixture::SCENE)?;
        self.local_field_enter_keeps_focus("Filter entities", "original")?;
        self.key(Key::Named(NamedKey::Escape))?;
        if !self.field_value("Filter entities")?.is_empty() {
            return Err("filter Escape did not discard the local search".into());
        }
        self.focus_field("Filter entities")?;
        self.event(EditorPanelPlatformEvent::Ime(Ime::Commit(
            "original".into(),
        )))?;
        self.paint()?;
        if self.field_value("Filter entities")? != "original"
            || self.state()? != before
            || self.project.read(fixture::SCENE)? != source
        {
            return Err(
                "filter Escape left semantic focus without a new usable text lifetime".into(),
            );
        }
        self.click(HIERARCHY, "Clear filter")?;
        Ok(())
    }

    pub(super) fn inspector_edit_and_restore(&mut self) -> TestResult {
        let original_source = self.project.read(fixture::SCENE)?;
        let before_component = self.state()?;
        self.click(INSPECTOR, "Add Component")?;
        self.click(INSPECTOR, "Add nara.scene.Name")?;
        self.expect_one_mutation(&before_component)?;
        let before = self.state()?;
        let fields = self.panel_frame(INSPECTOR)?;
        let editable = fields
            .semantics()
            .iter()
            .find(|node| node.role() == UiSemanticRole::TextInput && node.enabled())
            .ok_or("the selected named entity has no original editable Inspector field")?;
        let label = editable.name().to_owned();
        let original = editable
            .value()
            .ok_or("the editable field has no exact semantic value")?
            .to_owned();
        self.replace_panel_text(INSPECTOR, &label, "Discarded Inspector draft")?;
        self.key(Key::Named(NamedKey::Escape))?;
        if self.panel_field_value(INSPECTOR, &label)? != original
            || self.state()? != before
            || self.native.recovery.is_some()
        {
            return Err(
                "Inspector Escape failed to discard the draft and retire its input normally".into(),
            );
        }
        self.replace_panel_text(INSPECTOR, &label, "Styled 属性 café")?;
        self.capture_workspace("normal-inspector")?;
        if self.state()? != before {
            return Err(
                "editing the retained Inspector draft mutated its source before Apply".into(),
            );
        }
        self.event(EditorPanelPlatformEvent::Ime(Ime::Preedit(
            "未提交".into(),
            Some((0, 9)),
        )))?;
        self.paint()?;
        for (density, scale) in [
            (nara_ui_widgets::UiDensity::Compact, 1.0),
            (nara_ui_widgets::UiDensity::Compact, 2.0),
            (nara_ui_widgets::UiDensity::Normal, 2.0),
            (nara_ui_widgets::UiDensity::Normal, 1.0),
        ] {
            self.native.appearance = nara_ui_widgets::UiAppearance::default()
                .with_density(density)
                .with_text_scale(scale)
                .map_err(|error| error.to_string())?;
            for _ in 0..3 {
                self.paint()?;
            }
            if scale == 2.0 && density == nara_ui_widgets::UiDensity::Compact {
                self.capture_workspace("compact-large-text-preedit")?;
            }
            if self.panel_field_value(INSPECTOR, &label)? != "Styled 属性 café"
                || self.state()? != before
            {
                return Err("live appearance adoption lost the original Inspector draft or changed its source".into());
            }
        }
        if self.panel_field_value(INSPECTOR, &label)? != "Styled 属性 café" {
            return Err("Inspector preedit replaced the complete committed semantic value".into());
        }
        self.key(Key::Named(NamedKey::Escape))?;
        if self.panel_field_value(INSPECTOR, &label)? != "Styled 属性 café"
            || self.state()? != before
        {
            return Err(
                "Inspector composition Escape discarded the draft or changed source".into(),
            );
        }
        self.key(Key::Named(NamedKey::Enter))?;
        let applied = self.expect_one_mutation(&before)?;
        let selected = self
            .frame()?
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.editor.inspector.selected_entity_view.as_ref())
            .cloned()
            .ok_or("the actual Inspector lost its selected source entity")?;
        if !selected
            .components
            .iter()
            .flat_map(|component| &component.fields)
            .any(|field| {
                matches!(
                    &field.state,
                    nara::tooling::SceneInspectorFieldState::Present
                ) && field.value.as_ref().is_some_and(|value| {
                    value == &ComponentValue::String("Styled 属性 café".into())
                })
            })
        {
            return Err(
                "the actual Inspector Apply did not publish its exact Unicode source value".into(),
            );
        }
        self.click(SHELL, "Undo")?;
        self.expect_rows(&before, before.undo, before.redo + 1)?;
        if self.panel_field_value(INSPECTOR, &label)? != original
            || self.project.read(fixture::SCENE)? != original_source
        {
            return Err("Inspector Undo did not restore its original source value and disk".into());
        }
        self.click(SHELL, "Redo")?;
        self.expect_rows(&applied, applied.undo, 0)?;
        if self.panel_field_value(INSPECTOR, &label)? != "Styled 属性 café" {
            return Err("Inspector Redo lost the exact applied Unicode source value".into());
        }
        self.click(SHELL, "Undo")?;
        self.click(SHELL, "Undo")?;
        self.expect_rows(&before_component, before_component.undo, 2)?;
        self.action_menu_dismissal()?;
        Ok(())
    }

    fn action_menu_dismissal(&mut self) -> TestResult {
        let before = self.state()?;
        let trigger = self.point(SHELL, UiSemanticRole::Button, "Document actions")?;
        self.pointer(trigger)?;
        self.mouse(ElementState::Pressed)?;
        self.paint()?;
        self.mouse(ElementState::Released)?;
        self.native.drain_staged_cancellation_for_test = true;
        self.paint()?;
        for _ in 0..3 {
            self.paint()?;
            if !self.panel_frame(SHELL)?.blocks_unhandled_input() {
                return Err("old native output retirement closed the successor action menu".into());
            }
        }
        let shell = self.panel_frame(SHELL)?;
        if !shell.blocks_unhandled_input()
            || !shell
                .semantics()
                .iter()
                .any(|node| node.name() == "Close actions menu")
        {
            return Err("the anchored actions menu did not reach actual modal presentation".into());
        }
        self.capture_workspace("anchored-actions-menu")?;
        self.key(Key::Named(NamedKey::Escape))?;
        if self.panel_frame(SHELL)?.blocks_unhandled_input() || self.state()? != before {
            return Err(
                "menu Escape changed source or failed to dismiss the presented modal scope".into(),
            );
        }
        self.key(Key::Named(NamedKey::Enter))?;
        if !self.panel_frame(SHELL)?.blocks_unhandled_input() {
            return Err(
                "closing the menu did not restore keyboard focus to its current trigger".into(),
            );
        }
        self.click(SHELL, "Close actions menu")?;
        if self.panel_frame(SHELL)?.blocks_unhandled_input() || self.state()? != before {
            return Err(
                "the real menu dismissal button changed source or retained modal input".into(),
            );
        }
        for point in [
            self.control_point(SHELL, UiSemanticRole::Button, "Reopen", false)?,
            [
                4.0 * self.native.window.scale_factor(),
                4.0 * self.native.window.scale_factor(),
            ],
            self.point(HIERARCHY, UiSemanticRole::TextInput, "Filter entities")?,
        ] {
            self.click(SHELL, "Document actions")?;
            self.pointer(point)?;
            self.mouse(ElementState::Pressed)?;
            self.paint()?;
            self.mouse(ElementState::Released)?;
            self.paint()?;
            if self.panel_frame(SHELL)?.blocks_unhandled_input() || self.state()? != before {
                return Err("clicking outside the actual menu failed to dismiss it or activated the blocked underlay".into());
            }
            self.key(Key::Named(NamedKey::Enter))?;
            if !self.panel_frame(SHELL)?.blocks_unhandled_input() {
                return Err(
                    "outside dismissal did not preserve the successor trigger focus".into(),
                );
            }
            self.click(SHELL, "Close actions menu")?;
        }
        Ok(())
    }
}
