use nara::{
    render::{DisplayMapping2d, DisplayScale, Extent2d},
    tooling::{
        EditorCloseDecision, EditorDocumentOpenResult, EditorImageImportAttemptView,
        EditorImageImportSettings, EditorNavigationAction, EditorProjectBrowserLocalAction,
    },
};
use nara_render_wgpu::WgpuWindowFrameOutcome;

use crate::{
    native::{EditorPanelImage, EditorPanelImageFrame, EditorPanelImageStatus},
    workspace::EditorPanelInstanceId,
};

use super::*;

impl Workflow<'_> {
    pub(super) fn prepare_image_fixture(&mut self) -> TestResult {
        if !self.native.application.apply_project_browser_local_action(
            EditorProjectBrowserLocalAction::Select {
                relative_path: Some(Arc::from("assets/textures/player.png")),
            },
        ) {
            return Err("the original Browser could not select the fixture image".into());
        }
        self.wait(
            "prepare the fixture's real Image import detail",
            |workflow| {
                workflow
                    .frame()
                    .is_ok_and(|frame| frame.image_import().is_some())
            },
        )?;
        let frame = self.frame()?;
        let detail = frame
            .image_import()
            .ok_or("the fixture image detail retired")?;
        if !detail.can_submit() {
            return Err(format!(
                "the fixture image has no admitted import: {detail:?}"
            ));
        }
        self.setup_action(EditorAction::ImageImport(
            detail.submit_action(EditorImageImportSettings::default()),
        ))?;
        self.wait(
            "publish the fixture's actual imported Image choice",
            |workflow| {
                workflow.frame().is_ok_and(|frame| {
                    !frame.image_import_work_pending()
                        && frame.image_import().is_some_and(|detail| {
                            matches!(
                                detail.attempt(),
                                EditorImageImportAttemptView::Succeeded { .. }
                            )
                        })
                })
            },
        )?;
        Ok(())
    }

    pub(super) fn frame(&self) -> TestResult<Arc<EditorFrameModel>> {
        self.native.application.current_frame().ok_or_else(|| {
            format!(
                "the original Application has no frame: {:?}",
                self.native.application.state(),
            )
        })
    }

    pub(super) fn state(&self) -> TestResult<SourceState> {
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("no active source model")?;
        let tab = frame
            .workspace()
            .document(scene.document)
            .ok_or("no active source tab")?;
        if frame.workspace().active_document != Some(scene.document) {
            return Err("the active panel and document source disagree".into());
        }
        Ok(SourceState {
            document: scene.document,
            kind: scene.kind,
            revision: scene.revision,
            rows: scene
                .editor
                .inspector
                .entities
                .iter()
                .map(|row| (row.id.clone(), row.placement.clone()))
                .collect(),
            undo: tab.history.undo_depth,
            redo: tab.history.redo_depth,
            dirty: tab.dirty,
        })
    }

    pub(super) fn selected(&self) -> TestResult<SceneEntityId> {
        self.frame()?
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.editor.inspector.selected_entity.clone())
            .ok_or_else(|| "the source panel has no selected entity".into())
    }

    pub(super) fn paint(&mut self) -> TestResult {
        let deadline = (Instant::now() + OPERATION_TIMEOUT).min(self.deadline);
        loop {
            if Instant::now() >= deadline {
                return Err(format!(
                    "the original native presentation timed out: {:?}",
                    self.native.application.state()
                ));
            }
            self.native.application.advance(Duration::ZERO);
            self.native.poll_graphics_retry()?;
            if self.native.recovery.is_some() {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            if self.native.paint()? {
                self.presentations += 1;
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    pub(super) fn wait(&mut self, operation: &str, ready: impl Fn(&Self) -> bool) -> TestResult {
        let deadline = (Instant::now() + OPERATION_TIMEOUT).min(self.deadline);
        loop {
            self.paint()?;
            if ready(self) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "native acceptance timed out waiting to {operation}"
                ));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    pub(super) fn event(&mut self, event: EditorPanelPlatformEvent) -> TestResult<bool> {
        if Instant::now() >= self.deadline {
            return Err("the complete native acceptance exceeded its finite deadline".into());
        }
        self.native.dispatch(Some(event))
    }

    pub(super) fn panel_frame(&self, kind: &str) -> TestResult<Arc<UiFrame>> {
        let id = self.panel_id(kind)?;
        let frame = self
            .native
            .presented
            .get(&id)
            .ok_or_else(|| format!("the {kind} panel has no actual presented frame"))?;
        let graphics = self
            .native
            .graphics
            .as_ref()
            .ok_or("the original GPU Host is closed")?;
        let accepted = graphics
            .journal(id)
            .ok_or("the original UI presenter is absent")?
            .latest(frame.environment().surface_id())
            .map_err(|error| error.to_string())?;
        if !Arc::ptr_eq(accepted.frame(), frame)
            || accepted.receipt().frame_revision() != frame.revision()
            || accepted.receipt().presentation_epoch() == 0
        {
            return Err(format!(
                "the {kind} panel has no matching original presentation receipt"
            ));
        }
        Ok(Arc::clone(frame))
    }

    pub(super) fn panel_id(&self, kind: &str) -> TestResult<EditorPanelInstanceId> {
        self.native
            .application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .find(|instance| instance.kind() == kind)
            .map(|instance| instance.id())
            .ok_or_else(|| format!("the actual workspace has no {kind} panel"))
    }

    pub(super) fn point(
        &self,
        kind: &str,
        role: UiSemanticRole,
        name: &str,
    ) -> TestResult<[f64; 2]> {
        self.control_point(kind, role, name, true)
    }

    pub(super) fn control_point(
        &self,
        kind: &str,
        role: UiSemanticRole,
        name: &str,
        require_hit: bool,
    ) -> TestResult<[f64; 2]> {
        let id = self.panel_id(kind)?;
        let frame = self.panel_frame(kind)?;
        let mut nodes = frame
            .semantics()
            .iter()
            .filter(|node| node.role() == role && node.name() == name);
        let node = nodes
            .next()
            .ok_or_else(|| format!("{kind} has no presented {role:?} {name:?}"))?;
        if nodes.next().is_some() {
            return Err(format!("{kind} has ambiguous {role:?} {name:?}"));
        }
        let rect = match node.clip_rect() {
            Some(clip) => node.rect().intersect(clip),
            None => Some(node.rect()),
        }
        .ok_or_else(|| format!("the presented {name:?} control is fully clipped"))?;
        let point = UiPoint::new(
            rect.origin().x() + rect.size().width() * 0.5,
            rect.origin().y() + rect.size().height() * 0.5,
        );
        if require_hit
            && frame
                .hit_test(point)
                .is_none_or(|hit| hit.key() != node.key())
        {
            return Err(format!(
                "the actual {name:?} control does not own its pointer point"
            ));
        }
        let surface = self
            .native
            .surfaces
            .get(&id)
            .ok_or("the original native bounds are absent")?;
        let transform = frame.environment().content_transform();
        let target = transform
            .authored_to_physical(point)
            .ok_or("the control has no target-pixel point")?;
        let physical = transform.physical_surface_size();
        let extent = Extent2d::new(
            physical.width().ceil() as u32,
            physical.height().ceil() as u32,
        )
        .ok_or("the original image has no physical extent")?;
        let mapping =
            DisplayMapping2d::fit(extent, surface.bounds, surface.scale, DisplayScale::Fit)
                .ok_or("the original image bounds have no display mapping")?;
        let logical = mapping
            .uv_to_panel([
                target.x() / extent.width as f32,
                target.y() / extent.height as f32,
            ])
            .ok_or("the control falls outside its original image mapping")?;
        if !surface.bounds.contains(logical) {
            return Err("the control escaped its original native panel bounds".into());
        }
        let inverse = mapping
            .panel_to_target(logical)
            .ok_or("the control has no inverse mapping")?;
        if (inverse[0] - target.x()).abs() > 0.01 || (inverse[1] - target.y()).abs() > 0.01 {
            return Err(
                "the control point did not round-trip through the original image mapping".into(),
            );
        }
        Ok(logical.map(|value| f64::from(value) * self.native.window.scale_factor()))
    }

    pub(super) fn enabled(&self, kind: &str, name: &str) -> TestResult<bool> {
        let frame = self.panel_frame(kind)?;
        frame
            .semantics()
            .iter()
            .find(|node| node.role() == UiSemanticRole::Button && node.name() == name)
            .map(|node| node.enabled())
            .ok_or_else(|| {
                format!(
                    "{kind} has no button {name:?}; presented controls: {:?}",
                    frame
                        .semantics()
                        .iter()
                        .map(|node| node.name())
                        .collect::<Vec<_>>()
                )
            })
    }

    pub(super) fn pointer(&mut self, position: [f64; 2]) -> TestResult {
        if !self.event(EditorPanelPlatformEvent::PointerMoved {
            physical_position: position,
        })? {
            return Err(
                "the actual NativeImageHost did not consume the mapped pointer motion".into(),
            );
        }
        Ok(())
    }

    pub(super) fn mouse(&mut self, state: ElementState) -> TestResult {
        self.event(EditorPanelPlatformEvent::PointerButton {
            button: MouseButton::Left,
            state,
        })?;
        Ok(())
    }

    pub(super) fn click(&mut self, kind: &str, name: &str) -> TestResult {
        self.paint()?;
        if !self.enabled(kind, name)? {
            return Err(format!("the actual {kind} button {name:?} is disabled"));
        }
        let position = self.point(kind, UiSemanticRole::Button, name)?;
        self.pointer(position)?;
        self.mouse(ElementState::Pressed)?;
        self.paint()?;
        self.mouse(ElementState::Released)?;
        self.paint()
    }

    pub(super) fn select(&mut self, entity: &SceneEntityId, ui: bool) -> TestResult {
        self.click(
            HIERARCHY,
            &format!("Select {}{entity}", if ui { "UI " } else { "" }),
        )?;
        if self.selected()? != *entity {
            return Err(format!("the actual source row did not select {entity}"));
        }
        Ok(())
    }

    pub(super) fn key(&mut self, key: Key) -> TestResult {
        for state in [ElementState::Pressed, ElementState::Released] {
            self.event(EditorPanelPlatformEvent::logical_key(
                key.clone(),
                state,
                false,
                None,
            ))?;
        }
        self.paint()
    }

    pub(super) fn field_value(&self, name: &str) -> TestResult<String> {
        self.panel_field_value(HIERARCHY, name)
    }

    pub(super) fn panel_field_value(&self, kind: &str, name: &str) -> TestResult<String> {
        self.panel_frame(kind)?
            .semantics()
            .iter()
            .find(|node| node.role() == UiSemanticRole::TextInput && node.name() == name)
            .and_then(|node| node.value())
            .map(str::to_owned)
            .ok_or_else(|| format!("the actual source panel has no text field {name:?}"))
    }

    pub(super) fn focus_field(&mut self, name: &str) -> TestResult {
        self.focus_panel_field(HIERARCHY, name)
    }

    pub(super) fn focus_panel_field(&mut self, kind: &str, name: &str) -> TestResult {
        self.paint()?;
        let position = self.point(kind, UiSemanticRole::TextInput, name)?;
        self.pointer(position)?;
        self.mouse(ElementState::Pressed)?;
        self.paint()?;
        self.mouse(ElementState::Released)?;
        self.paint()?;
        self.event(EditorPanelPlatformEvent::Ime(Ime::Disabled))?;
        self.event(EditorPanelPlatformEvent::Ime(Ime::Enabled))?;
        Ok(())
    }

    pub(super) fn replace_text(&mut self, name: &str, text: &str) -> TestResult {
        self.replace_panel_text(HIERARCHY, name, text)
    }

    pub(super) fn replace_panel_text(&mut self, kind: &str, name: &str, text: &str) -> TestResult {
        self.focus_panel_field(kind, name)?;
        self.event(EditorPanelPlatformEvent::ModifiersChanged(
            ModifiersState::CONTROL,
        ))?;
        self.key(Key::Character("a".into()))?;
        self.event(EditorPanelPlatformEvent::ModifiersChanged(
            ModifiersState::empty(),
        ))?;
        self.event(EditorPanelPlatformEvent::Ime(Ime::Commit(text.into())))?;
        self.paint()?;
        if self.panel_field_value(kind, name)? != text {
            return Err(format!(
                "the presented {name:?} field did not accept actual keyboard/IME input"
            ));
        }
        Ok(())
    }

    pub(super) fn switch_document(&mut self, document: EditorDocumentId) -> TestResult {
        if self.state()?.document == document {
            return self.paint();
        }
        let frame = self.panel_frame(SHELL)?;
        let marker = format!(" #{} ", document.raw());
        let name = frame
            .semantics()
            .iter()
            .find(|node| node.role() == UiSemanticRole::Button && node.name().contains(&marker))
            .map(|node| node.name().to_owned())
            .ok_or("the shell has no exact document tab")?;
        self.click(SHELL, &name)?;
        if self.state()?.document != document {
            return Err("the actual shell tab did not navigate to its exact document".into());
        }
        Ok(())
    }

    pub(super) fn setup_action(&mut self, action: EditorAction) -> TestResult<EditorActionOutcome> {
        let revision = self.frame()?.revision();
        let result = self
            .native
            .application
            .submit_and_drive(EditorActionBatch::new(revision, [action]), Duration::ZERO);
        match result.actions() {
            [EditorApplicationActionDispatch::Dispatched(outcome)] => Ok(outcome.clone()),
            other => Err(format!(
                "the fixture's source-open setup did not dispatch: {other:?}"
            )),
        }
    }

    pub(super) fn open_source(&mut self, path: &str) -> TestResult<EditorDocumentId> {
        self.paint()?;
        while self.enabled(SHELL, "Previous sources")? {
            self.click(SHELL, "Previous sources")?;
        }
        let label = format!("Open {path}");
        loop {
            if self
                .panel_frame(SHELL)?
                .semantics()
                .iter()
                .any(|node| node.role() == UiSemanticRole::Button && node.name() == label)
            {
                self.click(SHELL, &label)?;
                break;
            }
            if !self.enabled(SHELL, "Next sources")? {
                return Err(format!(
                    "The original source snapshot has no visible source {path}"
                ));
            }
            self.click(SHELL, "Next sources")?;
        }
        let opening = self.frame()?;
        let request = opening
            .document_open()
            .pending()
            .map(|(request, _, _)| request)
            .or_else(|| {
                opening
                    .document_open()
                    .result()
                    .map(|result| result.request())
            })
            .ok_or("The actual Open control did not submit a source request")?;
        self.wait(
            "open a fixture source through the original Browser owner",
            |workflow| {
                workflow
                    .native
                    .application
                    .current_frame()
                    .is_some_and(|frame| frame.document_open().result().is_some())
            },
        )?;
        let Some(EditorDocumentOpenResult::Opened {
            request: completed,
            document,
        }) = self.frame()?.document_open().result()
        else {
            return Err(format!(
                "the Browser did not open {path}: {:?}; diagnostics {:?}",
                self.frame()?.document_open(),
                self.frame()?.diagnostics()
            ));
        };
        if request != completed {
            return Err("the Browser reported another source-open request".into());
        }
        self.click(SHELL, "Acknowledge source open")?;
        self.paint()?;
        Ok(document)
    }

    pub(super) fn expect_rows(
        &self,
        expected: &SourceState,
        undo: usize,
        redo: usize,
    ) -> TestResult {
        let actual = self.state()?;
        if actual.document != expected.document
            || actual.kind != expected.kind
            || actual.rows != expected.rows
            || actual.undo != undo
            || actual.redo != redo
        {
            return Err(format!(
                "the original panel source/history differ: expected rows {expected:?} with Undo {undo}/Redo {redo}; actual {actual:?}"
            ));
        }
        Ok(())
    }

    pub(super) fn expect_one_mutation(&self, before: &SourceState) -> TestResult<SourceState> {
        let actual = self.state()?;
        if actual.document != before.document
            || actual.kind != before.kind
            || actual.revision.source_id() != before.revision.source_id()
            || actual.revision.generation() != before.revision.generation() + 1
            || actual.undo != before.undo + 1
            || actual.redo != 0
            || !actual.dirty
        {
            return Err(format!(
                "the presented action must publish exactly one source/history mutation: before {before:?}; after {actual:?}; controls {:?}; diagnostics {:?}",
                self.panel_frame(HIERARCHY)?
                    .semantics()
                    .iter()
                    .map(|node| node.name())
                    .collect::<Vec<_>>(),
                self.frame()?.diagnostics(),
            ));
        }
        Ok(actual)
    }

    pub(super) fn history_roundtrip(
        &mut self,
        before: &SourceState,
        after: &SourceState,
    ) -> TestResult {
        self.click(SHELL, "Undo")?;
        self.expect_rows(before, before.undo, 1)?;
        if self.state()?.dirty != before.dirty {
            return Err("exact Undo did not restore the original dirty checkpoint".into());
        }
        self.click(SHELL, "Redo")?;
        self.expect_rows(after, after.undo, 0)?;
        if self.state()?.dirty != after.dirty {
            return Err("exact Redo did not restore the edited dirty checkpoint".into());
        }
        Ok(())
    }

    pub(super) fn save_reopen(
        &mut self,
        path: &str,
        expected: &SourceState,
    ) -> TestResult<Vec<u8>> {
        let original = self.project.read(path)?;
        self.click(SHELL, "Save")?;
        self.wait("finish the shell's exact Save attempt", |workflow| {
            workflow.frame().is_ok_and(|frame| {
                matches!(frame.persistence(), EditorPersistenceView::Terminal { .. })
            })
        })?;
        let saved_frame = self.frame()?;
        let attempt = saved_frame
            .persistence()
            .attempt()
            .ok_or("Save has no exact attempt ID")?;
        if !matches!(saved_frame.persistence().result(),
            Some(EditorPersistenceResult::Saved { document, revision, .. })
                if document == expected.document && revision == expected.revision)
        {
            return Err(format!(
                "the actual shell Save failed: {:?}",
                saved_frame.persistence()
            ));
        }
        self.expect_rows(expected, expected.undo, expected.redo)?;
        if self.state()?.dirty {
            return Err("Save did not publish a clean original source checkpoint".into());
        }
        let saved = self.project.read(path)?;
        if saved == original {
            return Err("the actual Save did not change the edited original source file".into());
        }
        self.click(SHELL, "Acknowledge persistence result")?;
        if self.frame()?.persistence() != EditorPersistenceView::Idle {
            return Err("the shell did not acknowledge the exact Save attempt".into());
        }
        self.click(SHELL, "Reopen")?;
        self.wait("finish the shell's exact Reopen attempt", |workflow| {
            workflow.frame().is_ok_and(|frame| {
                matches!(frame.persistence(), EditorPersistenceView::Terminal { .. })
            })
        })?;
        let reopened = self.frame()?;
        if reopened
            .persistence()
            .attempt()
            .is_none_or(|next| next == attempt)
            || !matches!(reopened.persistence().result(),
                Some(EditorPersistenceResult::Opened { document, .. }) if document == expected.document)
        {
            return Err(format!(
                "Reopen did not finish as a distinct exact attempt: {:?}",
                reopened.persistence()
            ));
        }
        self.expect_rows(expected, 0, 0)?;
        if self.state()?.dirty
            || self.state()?.revision.source_id() == expected.revision.source_id()
            || self.project.read(path)? != saved
        {
            return Err("Reopen did not replace the source incarnation with the exact saved file and clean history".into());
        }
        self.click(SHELL, "Acknowledge persistence result")?;
        if self.frame()?.persistence() != EditorPersistenceView::Idle {
            return Err("the shell did not acknowledge the exact Reopen attempt".into());
        }
        Ok(saved)
    }

    fn stage_controls(&mut self, compile: bool) -> TestResult<EditorPanelImageFrame<UiFrame>> {
        let mut frames = Vec::new();
        let mut error = None;
        let ui = self.native.application.current_ui_authoring_frame();
        let presented = &self.native.presented;
        drop(
            self.native
                .panels
                .prepare(&self.native.application, |panel, context| {
                    let id = context.model().instance().id();
                    let Some(previous) = presented.get(&id) else {
                        error
                            .get_or_insert("a live panel has no actual presented image".to_owned());
                        return;
                    };
                    let frame = if compile {
                        panel.compile_panel(context, ui.as_deref(), previous.environment())
                    } else {
                        Ok(Arc::clone(previous))
                    };
                    match frame {
                        Ok(frame) => frames.push((id, context.instance_token(), frame)),
                        Err(failure) => {
                            error.get_or_insert(failure);
                        }
                    }
                }),
        );
        if let Some(error) = error {
            return Err(error);
        }
        self.native.begin_graphics_frame()?;
        let gpu = self
            .native
            .graphics
            .as_mut()
            .ok_or("the original GPU Host is closed")?;
        let mut images = Vec::new();
        for (id, token, frame) in frames {
            let bounds = self
                .native
                .surfaces
                .get(&id)
                .ok_or("the panel bounds retired")?
                .bounds;
            let (display, frame) = gpu.render(id, frame)?;
            let mapping = DisplayMapping2d::fit(
                display.output().extent(),
                bounds,
                self.native.window.scale_factor() as f32,
                DisplayScale::Fit,
            )
            .ok_or("the actual image has no display mapping")?;
            let modal = frame.blocks_unhandled_input();
            images.push(
                EditorPanelImage::new(token, display, frame, mapping, bounds)
                    .with_window_modal(modal),
            );
        }
        self.native
            .native
            .stage(&self.native.application, gpu.frame, images)
            .map_err(|error| error.to_string())
    }

    pub(super) fn verify_workspace_composites(&mut self) -> TestResult<WgpuWindowFrameOutcome> {
        if cfg!(feature = "imgui-reference") {
            return Err(
                "the independent acceptance must not compile the reference context owner".into(),
            );
        }
        let shell = self.panel_id(SHELL)?;
        let hierarchy = self.panel_id(HIERARCHY)?;
        let inspector = self.panel_id(INSPECTOR)?;
        if self.native.presented.len() != 3 || self.native.surfaces.len() != 3 {
            return Err(
                "the source workspace must own three actual simultaneous panel images".into(),
            );
        }
        self.panel_frame(SHELL)?;
        self.panel_frame(HIERARCHY)?;
        self.panel_frame(INSPECTOR)?;
        let candidate = self.stage_controls(false)?;
        if candidate.images().len() != 3
            || ![shell, hierarchy, inspector].iter().all(|id| {
                candidate
                    .images()
                    .iter()
                    .any(|image| image.token().instance() == *id)
            })
            || (0..3).any(|index| {
                ((index + 1)..3).any(|other| {
                    candidate.images()[index].display().receipt()
                        == candidate.images()[other].display().receipt()
                })
            })
        {
            return Err("the three panels do not own distinct original GPU images".into());
        }
        let gpu = self
            .native
            .graphics
            .as_mut()
            .ok_or("the original GPU Host is closed")?;
        let outcome = gpu.present(&self.native.window, &candidate)?;
        let window = outcome
            .window()
            .ok_or("the hidden native window was not actually presented")?;
        if outcome.composites().len() != 3
            || candidate.images().iter().any(|image| {
                outcome
                    .composites()
                    .iter()
                    .filter(|composite| {
                        composite.window() == window
                            && composite.image() == image.display().receipt()
                            && composite.occurrence() == image.display().occurrence()
                    })
                    .count()
                    != 1
            })
        {
            return Err(
                "the original native outcome lacks all three exact image composites".into(),
            );
        }
        gpu.complete(&outcome)?;
        let accepted = candidate
            .images()
            .iter()
            .map(|image| (image.token().instance(), Arc::clone(image.snapshot())))
            .collect();
        if self
            .native
            .native
            .presented(&self.native.application, candidate, &outcome)
            .map_err(|error| error.to_string())?
            != EditorPanelImageStatus::Presented
        {
            return Err("the original native Host did not adopt all three panel composites".into());
        }
        self.native.presented = accepted;
        self.native.dispatch(None)?;
        self.presentations += 1;
        Ok(outcome)
    }

    pub(super) fn close_with_preedit_and_pending_candidate(&mut self) -> TestResult {
        let before = self.state()?;
        let files = [
            fixture::SCENE,
            fixture::PREFAB,
            fixture::PRESERVED,
            fixture::UI,
        ]
        .map(|path| self.project.read(path))
        .into_iter()
        .collect::<TestResult<Vec<_>>>()?;
        self.focus_field("Filter entities")?;
        if !self.field_value("Filter entities")?.is_empty()
            || self.enabled(HIERARCHY, "Clear filter")?
        {
            return Err("the closing filter must begin empty and without composition".into());
        }
        let original = self.verify_workspace_composites()?;
        if !self.event(EditorPanelPlatformEvent::Ime(Ime::Preedit(
            "closing preedit".into(),
            Some((0, 15)),
        )))? {
            return Err("the original NativeImageHost did not accept closing preedit".into());
        }
        let candidate = self.stage_controls(true)?;
        let hierarchy = self.panel_id(HIERARCHY)?;
        let staged = candidate
            .images()
            .iter()
            .find(|image| image.token().instance() == hierarchy)
            .ok_or("the closing candidate lost the actual Hierarchy image")?;
        if Arc::ptr_eq(staged.snapshot(), &self.panel_frame(HIERARCHY)?)
            || !staged.snapshot().semantics().iter().any(|node| {
                node.role() == UiSemanticRole::Button
                    && node.name() == "Clear filter"
                    && node.enabled()
            })
            || staged.snapshot().semantics().iter().any(|node| {
                node.role() == UiSemanticRole::TextInput
                    && node.name() == "Filter entities"
                    && node.value() != Some("")
            })
        {
            return Err(
                "the pending original image does not retain active uncommitted preedit".into(),
            );
        }
        self.expect_rows(&before, before.undo, before.redo)?;
        self.close_graphics()?;
        if self.native.graphics.is_some()
            || !self.native.presented.is_empty()
            || !self.native.surfaces.is_empty()
        {
            return Err(
                "close did not retire the original GPU Host and native panel surfaces".into(),
            );
        }
        let mut delivered = 0;
        let late = self
            .native
            .native
            .dispatch(
                &self.native.application,
                &mut self.native.panels,
                Some(EditorPanelPlatformEvent::Ime(Ime::Commit(
                    "late closing commit".into(),
                ))),
                |_, _, _, _| delivered += 1,
            )
            .map_err(|error| error.to_string())?;
        if late.submission.is_some() {
            return Err("Closed input created a semantic submission".into());
        }
        if delivered != 0
            || self
                .native
                .native
                .presented(&self.native.application, candidate, &original)
                .is_ok()
        {
            return Err(
                "late input or an old native outcome reached the closed panel lifetime".into(),
            );
        }
        self.expect_rows(&before, before.undo, before.redo)?;
        for (path, original) in [
            fixture::SCENE,
            fixture::PREFAB,
            fixture::PRESERVED,
            fixture::UI,
        ]
        .into_iter()
        .zip(files)
        {
            if self.project.read(path)? != original {
                return Err(format!(
                    "closing active preedit changed original source {path}"
                ));
            }
        }
        Ok(())
    }

    pub(super) fn close(&mut self) -> TestResult {
        if self.closed {
            return Ok(());
        }
        let graphics = self.close_graphics();
        let mut update = self.native.application.request_exit();
        let deadline = Instant::now() + OPERATION_TIMEOUT;
        while !update.exit_ready() && Instant::now() < deadline {
            if let Some(frame) = self.native.application.current_frame() {
                if let Some(prompt) = frame.workspace_intent().decision_prompt() {
                    self.native.application.submit_and_drive(
                        EditorActionBatch::new(
                            frame.revision(),
                            [EditorAction::WorkspaceIntent(
                                prompt.resolve(EditorCloseDecision::Discard),
                            )],
                        ),
                        Duration::ZERO,
                    );
                }
            }
            update = self.native.application.advance(Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
        self.closed = update.exit_ready();
        graphics?;
        if !self.closed {
            return Err("the original Application did not finish physical exit retirement".into());
        }
        if self.native.application.current_frame().is_some() || self.native.paint()? {
            return Err(
                "a redraw after accepted Exit must remain inert without an authoring Session"
                    .into(),
            );
        }
        Ok(())
    }

    fn close_graphics(&mut self) -> TestResult {
        let deadline = Instant::now() + OPERATION_TIMEOUT;
        while !self.native.poll_close() {
            if Instant::now() >= deadline {
                return Err(
                    "the retained original graphics Host did not finish physical close".into(),
                );
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if self.native.cleanup_errors == self.expected_cleanup_errors {
            Ok(())
        } else {
            Err(format!(
                "native cleanup errors differ: expected {:?}; actual {:?}",
                self.expected_cleanup_errors, self.native.cleanup_errors
            ))
        }
    }

    pub(super) fn replace_hierarchy_incarnation(&mut self) -> TestResult {
        let before = self.state()?;
        self.focus_field("Filter entities")?;
        self.event(EditorPanelPlatformEvent::Ime(Ime::Preedit(
            "retiring preedit".into(),
            Some((0, 16)),
        )))?;
        let candidate = self.stage_controls(true)?;
        let hierarchy = self.panel_id(HIERARCHY)?;
        let old_token = candidate
            .images()
            .iter()
            .find(|image| image.token().instance() == hierarchy)
            .ok_or("the retiring candidate has no actual Hierarchy image")?
            .token()
            .clone();
        let old_surface = self
            .native
            .surfaces
            .get(&hierarchy)
            .ok_or("the retiring Hierarchy has no original Surface")?
            .surface;
        let gpu = self
            .native
            .graphics
            .as_mut()
            .ok_or("the original GPU Host is closed")?;
        let original_outcome = gpu.present(&self.native.window, &candidate)?;
        if original_outcome.window().is_none() || original_outcome.composites().len() != 3 {
            return Err(
                "the retiring panel candidate did not reach actual three-image native presentation"
                    .into(),
            );
        }
        gpu.complete(&original_outcome)?;
        if !self
            .native
            .application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(hierarchy, false)
        {
            return Err("the actual Hierarchy instance could not be hidden".into());
        }
        self.paint()?;
        if old_token.is_current(&self.native.application)
            || self.native.presented.contains_key(&hierarchy)
        {
            return Err(
                "hiding the Hierarchy retained its old semantic incarnation or native image".into(),
            );
        }
        if !self
            .native
            .application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(hierarchy, true)
        {
            return Err("the actual Hierarchy instance could not be restored".into());
        }
        self.paint()?;
        let replacement = self
            .native
            .surfaces
            .get(&hierarchy)
            .ok_or("the restored Hierarchy has no native Surface")?;
        if replacement.token == old_token
            || replacement.surface == old_surface
            || !replacement.token.is_current(&self.native.application)
        {
            return Err(
                "the restored Hierarchy reused the retired panel incarnation or Surface".into(),
            );
        }
        if self
            .native
            .native
            .presented(&self.native.application, candidate, &original_outcome)
            .is_ok()
        {
            return Err(
                "the replacement panel adopted an original outcome from its retired incarnation"
                    .into(),
            );
        }
        self.event(EditorPanelPlatformEvent::Ime(Ime::Commit(
            "late retired preedit".into(),
        )))?;
        self.key(Key::Named(NamedKey::Enter))?;
        self.paint()?;
        if self.state()? != before
            || !self.field_value("Filter entities")?.is_empty()
            || self.enabled(HIERARCHY, "Clear filter")?
        {
            return Err(
                "late native text reached the replacement panel's local state or original source"
                    .into(),
            );
        }
        self.verify_workspace_composites()?;
        Ok(())
    }

    pub(super) fn replace_source_while_pressed(
        &mut self,
        document: EditorDocumentId,
    ) -> TestResult {
        self.setup_action(EditorAction::Navigate(
            EditorNavigationAction::SetActiveDocument { document },
        ))?;
        self.paint()
    }
}

impl Drop for Workflow<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("drawn native acceptance retirement: {error}");
        }
    }
}
