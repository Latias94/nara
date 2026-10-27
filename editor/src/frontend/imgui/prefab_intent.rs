//! Selection-independent review of stored intent through the existing source transaction owner.

use std::fmt::{self, Write};

use dear_imgui_rs::Ui;
use nara::{
    scene::{
        PrefabComponentIntent, PrefabDeclarationLimits, PrefabDeclarationSource,
        PrefabExpansionFailure, PrefabInstance, PrefabInstanceObservation, PrefabInstancePath,
        PrefabInstancePathSegment, PrefabOverlay, PrefabOverlaySlot, PrefabRevert,
        ScenePatchOperation,
    },
    tooling::{
        EditorDocumentQueryBasis, EditorEntityPage, EditorEntityQuery, EditorEntityQueryLimits,
        EditorEntityQueryPosition, PreparedEditorDocumentEdit,
    },
};

use super::{ImguiEditorPanelContext, PanelActionDispatch, PanelSubmissionId};

const PAGE_ROWS: usize = 64;
const DETAIL_BYTES: usize = 16 * 1024;
const REVIEW_BYTES: usize = 128 * 1024;

#[derive(Default)]
pub(super) struct ImguiPrefabIntent {
    basis: Option<EditorDocumentQueryBasis>,
    page: Option<EditorEntityPage>,
    anchor_index: usize,
    observation: Option<PrefabInstanceObservation>,
    instance_index: usize,
    rows: Vec<(PrefabOverlaySlot, String)>,
    selected: usize,
    detail: String,
    review: Option<RevertReview>,
    pending: Option<PanelSubmissionId>,
    result: Option<(PanelSubmissionId, PanelActionDispatch)>,
    result_text: Option<String>,
    notice: Option<String>,
}

struct RevertReview {
    prepared: PreparedEditorDocumentEdit,
    anchor: String,
    expected: String,
    value: String,
}

impl RevertReview {
    fn new(prepared: PreparedEditorDocumentEdit) -> Result<Self, &'static str> {
        if !prepared.is_prefab_revert() {
            return Err("Review requires a source-owned Prefab Revert candidate.");
        }
        let [
            ScenePatchOperation::SetPrefabInstance {
                entity,
                expected,
                value,
            },
        ] = prepared.patch().operations.as_slice()
        else {
            return Err("Revert did not prepare one exact instance replacement.");
        };
        let expected = bounded_debug(expected, REVIEW_BYTES)
            .ok_or("Before exceeds the review display limit; use a source editing tool.")?;
        let value = bounded_debug(value, REVIEW_BYTES)
            .ok_or("After exceeds the review display limit; use a source editing tool.")?;
        Ok(Self {
            anchor: entity.as_str().to_owned(),
            expected,
            value,
            prepared,
        })
    }
}

pub(super) fn show(ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
    // The retained state lives with the panel incarnation; temporarily moving it allows the
    // existing context to queue the exact move-only candidate without borrowing its widget twice.
    let mut state = std::mem::take(&mut context.widgets.prefab_intent);
    state.show(ui, context);
    context.widgets.prefab_intent = state;
}

impl ImguiPrefabIntent {
    pub(super) fn cancel_review(&mut self) {
        self.review = None;
    }

    pub(super) fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        if self.pending != Some(id) {
            return;
        }
        self.pending = None;
        self.result_text = Some(
            bounded_debug(&result, DETAIL_BYTES)
                .unwrap_or_else(|| "The exact dispatch result exceeds the display limit.".into()),
        );
        self.result = Some((id, result));
    }

    fn show(&mut self, ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        let Some(document) = context.model().document().map(|document| document.document) else {
            self.cancel_review();
            ui.text_disabled("Open a Scene or Prefab document to review its stored intent.");
            return;
        };
        let current = match context.document_query_basis(document) {
            Ok(basis) => basis,
            Err(error) => {
                self.cancel_review();
                ui.text_wrapped(format!("Prefab Intent unavailable: {error}"));
                self.show_result(ui);
                return;
            }
        };
        let editing =
            !context.input.read_only && super::authoring::editing_enabled(context.input.frame);
        if self.basis.is_none() {
            self.refresh(context, current.clone());
        }
        let current_basis = self.reconcile_review(&current, editing);
        ui.text_wrapped(
            "Stored Prefab intent belongs to this document. Entity selection is not required.",
        );
        if let Some(document) = context.model().document() {
            ui.text_wrapped(format!("Source owner: {}", document.title));
            if let Some(readiness) = document.authoring_readiness {
                ui.text_disabled(format!("Source document classification: {readiness:?}"));
            }
        }
        ui.text_disabled(if current.schema.is_some() {
            "Source Schema admitted; composition is observed separately."
        } else {
            "Source Schema is unavailable."
        });
        if !current_basis {
            ui.text_wrapped("Historical observation: document or Schema changed. Refresh before preparing Revert.");
        }
        if !editing {
            ui.text_disabled("Document editing is unavailable. Stored intent remains readable.");
        }
        if button(ui, "Refresh declarations", self.pending.is_none()) {
            self.refresh(context, current);
        }
        self.show_result(ui);
        if let Some(notice) = &self.notice {
            ui.text_wrapped(notice);
        }
        if self.review.is_some() {
            self.show_review(ui, context, editing && current_basis);
            return;
        }
        let locked = !current_basis || self.pending.is_some() || self.review.is_some();
        let browser_height = (ui.content_region_avail()[1] - 130.0).max(80.0);
        ui.child_window("prefab-intent-browser")
            .size([0.0, browser_height])
            .build(ui, || {
                self.show_entities(ui, context, locked);
                self.show_instances(ui, locked);
                self.show_intents(ui, locked);
                self.show_composition(ui, context, current_basis);
            });

        let overlay = self
            .selected_instance()
            .and_then(|instance| instance.overrides.as_ref());
        let can_prepare = editing && current_basis && self.pending.is_none() && overlay.is_some();
        if button(
            ui,
            "Review selected Revert",
            can_prepare && !self.rows.is_empty(),
        ) && let Some((slot, _)) = self.rows.get(self.selected)
        {
            self.prepare(context, PrefabRevert::Intent(*slot));
        }
        if button(ui, "Review entire overlay Revert", can_prepare) {
            self.prepare(context, PrefabRevert::Overlay);
        }
        ui.text_wrapped("Revert removes stored intent. Entire overlay Revert keeps the source link. No implicit reference or hierarchy cleanup is performed.");
    }

    fn refresh(
        &mut self,
        context: &ImguiEditorPanelContext<'_, '_>,
        basis: EditorDocumentQueryBasis,
    ) {
        self.cancel_review();
        self.notice = None;
        self.basis = Some(basis.clone());
        self.load_page(context, EditorEntityQueryPosition::Start(basis));
    }

    fn reconcile_review(&mut self, current: &EditorDocumentQueryBasis, editing: bool) -> bool {
        let current_basis = self.basis.as_ref() == Some(current);
        if !current_basis && self.review.take().is_some() {
            self.notice =
                Some("Document or Schema changed. Refresh and review Revert again.".into());
        } else if !editing && self.review.take().is_some() {
            self.notice = Some("Review canceled because document editing is unavailable.".into());
        }
        current_basis
    }

    fn load_page(
        &mut self,
        context: &ImguiEditorPanelContext<'_, '_>,
        position: EditorEntityQueryPosition,
    ) {
        self.page = None;
        self.observation = None;
        self.rows.clear();
        self.detail.clear();
        match context.query_document_entities(EditorEntityQuery {
            position,
            limits: EditorEntityQueryLimits::new(PAGE_ROWS, PAGE_ROWS)
                .expect("fixed positive page limits are within the public ceiling"),
        }) {
            Ok(page) => {
                self.page = Some(page);
                self.select_anchor(context, 0);
            }
            Err(error) => self.notice = Some(format!("Source entity page rejected: {error}")),
        }
    }

    fn select_anchor(&mut self, context: &ImguiEditorPanelContext<'_, '_>, index: usize) {
        self.anchor_index = index;
        self.observation = None;
        self.instance_index = 0;
        self.rows.clear();
        self.detail.clear();
        let Some(basis) = &self.basis else { return };
        let Some(entity) = self.page.as_ref().and_then(|page| page.entities.get(index)) else {
            return;
        };
        match context.observe_prefab_instance(
            basis,
            &PrefabInstancePath::new(entity.clone()),
            PrefabDeclarationLimits::default(),
        ) {
            Ok(observation) => {
                self.observation = Some(observation);
                self.rebuild_intents();
            }
            Err(error) => {
                self.notice = Some(format!("Stored instance observation rejected: {error}"))
            }
        }
    }

    fn show_entities(&mut self, ui: &Ui, context: &ImguiEditorPanelContext<'_, '_>, locked: bool) {
        let Some(page) = &self.page else { return };
        if page.entities.is_empty() {
            ui.text_disabled("No direct source entities on this page.");
        }
        let title = page
            .entities
            .get(self.anchor_index)
            .map_or("Choose a direct source entity", |id| id.as_str());
        let mut selected = None;
        ui.with_disabled_if(locked, || {
            if let Some(_combo) = ui.begin_combo("Source anchor", title) {
                for (index, entity) in page.entities.iter().enumerate() {
                    let identity = index.to_string();
                    let _id = ui.push_id(identity.as_str());
                    if ui
                        .selectable_config(entity.as_str())
                        .selected(index == self.anchor_index)
                        .build()
                    {
                        selected = Some(index);
                    }
                }
            }
        });
        #[cfg(test)]
        super::ui_interaction_tests::record_control(ui, "Prefab Intent source anchor");
        let next = page.next.clone();
        let first = button(ui, "First entity page", !locked);
        ui.same_line();
        let advance = button(ui, "Next entity page", !locked && next.is_some());
        if first {
            if let Some(basis) = self.basis.clone() {
                self.load_page(context, EditorEntityQueryPosition::Start(basis));
            }
        } else if advance {
            if let Some(next) = next {
                self.load_page(context, EditorEntityQueryPosition::Continue(next));
            }
        } else if let Some(index) = selected {
            self.select_anchor(context, index);
        }
    }

    fn selected_path(&self) -> Option<&PrefabInstancePath> {
        let observation = self.observation.as_ref()?;
        if self.instance_index == 0 {
            Some(&observation.path)
        } else {
            observation.nested_instances.get(self.instance_index - 1)
        }
    }

    fn selected_instance(&self) -> Option<&PrefabInstance> {
        let observation = self.observation.as_ref()?;
        let path = self.selected_path()?;
        let relative = path.steps.strip_prefix(observation.path.steps.as_slice())?;
        let mut instance = observation.instance.as_ref();
        for step in relative {
            let overlay = instance?.overrides.as_ref()?;
            instance = match step {
                PrefabInstancePathSegment::Addition { index } => {
                    overlay.additions.get(*index)?.prefab.as_ref()
                }
                PrefabInstancePathSegment::InstanceClaim { index } => {
                    overlay.instances.get(*index)?.value.as_ref()
                }
            };
        }
        instance
    }

    fn show_instances(&mut self, ui: &Ui, locked: bool) {
        let Some(observation) = &self.observation else {
            return;
        };
        let title = self.selected_path().map(path_label).unwrap_or_default();
        let mut selected = None;
        ui.with_disabled_if(locked, || {
            if let Some(_combo) = ui.begin_combo("Stored instance", title) {
                for (index, path) in std::iter::once(&observation.path)
                    .chain(&observation.nested_instances)
                    .enumerate()
                {
                    let identity = index.to_string();
                    let _id = ui.push_id(identity.as_str());
                    if ui
                        .selectable_config(path_label(path))
                        .selected(index == self.instance_index)
                        .build()
                    {
                        selected = Some(index);
                    }
                }
            }
        });
        if let Some(index) = selected {
            self.instance_index = index;
            self.rebuild_intents();
        }
        if let Some(instance) = self.selected_instance() {
            ui.text_wrapped(format!("Source link: {:?}", instance.source));
            if let Some(overlay) = &instance.overrides {
                ui.text_wrapped(format!("Stored base pin: {:?}", overlay.base));
            } else {
                ui.text_disabled("No stored overlay. This declaration inherits from its source.");
            }
        } else if self.instance_index == 0 {
            ui.text_disabled("No stored Prefab instance on this direct source entity.");
        } else {
            ui.text_wrapped("Explicit detach (None). Removing the parent instance claim restores inheritance; it does not convert the instance to local entities.");
        }
    }

    fn rebuild_intents(&mut self) {
        self.rows = self
            .selected_instance()
            .and_then(|instance| instance.overrides.as_ref())
            .map(intent_rows)
            .unwrap_or_default();
        self.selected = 0;
        self.rebuild_detail();
    }

    fn rebuild_detail(&mut self) {
        self.detail = self
            .selected_instance()
            .and_then(|instance| instance.overrides.as_ref())
            .zip(self.rows.get(self.selected))
            .and_then(|(overlay, (slot, _))| intent_detail(overlay, *slot))
            .unwrap_or_default();
    }

    fn show_intents(&mut self, ui: &Ui, locked: bool) {
        let mut selected = None;
        let label = self
            .rows
            .get(self.selected)
            .map_or("No stored intents", |(_, label)| label.as_str());
        ui.with_disabled_if(locked, || {
            if let Some(_combo) = ui.begin_combo("Stored intent", label) {
                for (index, (_, label)) in self.rows.iter().enumerate() {
                    let identity = index.to_string();
                    let _id = ui.push_id(identity.as_str());
                    if ui
                        .selectable_config(label)
                        .selected(index == self.selected)
                        .build()
                    {
                        selected = Some(index);
                    }
                }
            }
        });
        if let Some(index) = selected {
            self.selected = index;
            self.rebuild_detail();
        }
        if !self.detail.is_empty() {
            ui.text("Declared target / stored value");
            ui.child_window("prefab-intent-detail")
                .size([0.0, 150.0])
                .build(ui, || {
                    ui.text_wrapped(&self.detail);
                });
        }
        if self
            .rows
            .get(self.selected)
            .is_some_and(|(slot, _)| matches!(slot, PrefabOverlaySlot::InstanceClaim { .. }))
        {
            ui.text_wrapped(
                "None is an explicit detach. Reverting this instance claim restores inheritance.",
            );
        }
    }

    fn show_composition(&self, ui: &Ui, context: &ImguiEditorPanelContext<'_, '_>, current: bool) {
        if !current {
            ui.text_disabled("Resolution not checked for this historical observation.");
            return;
        }
        let Some(basis) = &self.basis else { return };
        match context.prefab_composition_failure(basis) {
            Ok(failure) => {
                let (status, foreign) = composition_status(
                    failure,
                    self.selected_path(),
                    self.rows.get(self.selected).map(|(slot, _)| *slot),
                );
                ui.text_wrapped(status);
                if let Some(failure) = failure {
                    ui.text_wrapped(format!(
                        "First reported composition failure: {:?}",
                        failure.reason()
                    ));
                    if let Some(location) = failure.location() {
                        ui.text_wrapped(format!(
                            "Declaring source: {:?}; {}",
                            location.source(),
                            path_label(location.path())
                        ));
                    }
                }
                if foreign {
                    ui.text_wrapped("Open the declaring Prefab source and review it under its own document basis. This document's Revert controls only edit its own stored declarations.");
                }
            }
            Err(error) => ui.text_wrapped(format!("Composition observation unavailable: {error}")),
        }
    }

    fn prepare(&mut self, context: &ImguiEditorPanelContext<'_, '_>, action: PrefabRevert) {
        let Some((basis, path)) = self.basis.as_ref().zip(self.selected_path()) else {
            return;
        };
        match context.prepare_prefab_revert(basis, path, action, PrefabDeclarationLimits::default())
        {
            Ok(prepared) => match RevertReview::new(prepared) {
                Ok(review) => {
                    self.review = Some(review);
                    self.notice = None;
                }
                Err(message) => self.notice = Some(message.into()),
            },
            Err(error) => self.notice = Some(format!("Revert preparation rejected: {error}")),
        }
    }

    fn show_review(
        &mut self,
        ui: &Ui,
        context: &mut ImguiEditorPanelContext<'_, '_>,
        enabled: bool,
    ) {
        let Some(review) = &self.review else { return };
        ui.separator();
        ui.text_wrapped(format!(
            "Prepared exact replacement of source anchor: {}",
            review.anchor
        ));
        ui.text_wrapped(
            "Commit submits this reviewed candidate as one Undo step. Cancel publishes nothing.",
        );
        let preview_height = ((ui.content_region_avail()[1] - 100.0) * 0.5).clamp(36.0, 130.0);
        ui.text("Before / expected");
        ui.child_window("prefab-revert-before")
            .size([0.0, preview_height])
            .build(ui, || {
                ui.text_wrapped(&review.expected);
            });
        ui.text("After / value");
        ui.child_window("prefab-revert-after")
            .size([0.0, preview_height])
            .build(ui, || {
                ui.text_wrapped(&review.value);
            });
        let commit = button(
            ui,
            "Commit reviewed Revert",
            enabled && self.pending.is_none(),
        );
        ui.same_line();
        let cancel = button(ui, "Cancel Revert review", true);
        if cancel {
            self.cancel_review();
        } else if commit {
            self.commit(context);
        }
    }

    fn commit(&mut self, context: &mut ImguiEditorPanelContext<'_, '_>) {
        let Some(review) = self.review.take() else {
            return;
        };
        let RevertReview {
            prepared,
            anchor,
            expected,
            value,
        } = review;
        match context.commit_prefab_revert(prepared) {
            Ok(id) => {
                self.pending = Some(id);
                self.result = None;
                self.result_text = None;
            }
            Err((error, prepared)) => {
                self.review = Some(RevertReview {
                    prepared,
                    anchor,
                    expected,
                    value,
                });
                self.notice = Some(format!(
                    "Revert was not queued: {error}. Reviewed candidate retained."
                ));
            }
        }
    }

    fn show_result(&self, ui: &Ui) {
        if self.pending.is_some() {
            ui.text_disabled("Revert queued; waiting for its exact dispatch result.");
        }
        if let Some(result) = &self.result_text {
            ui.text("Revert dispatch result");
            ui.text_wrapped(result);
        }
    }
}

fn composition_status(
    failure: Option<&PrefabExpansionFailure>,
    path: Option<&PrefabInstancePath>,
    slot: Option<PrefabOverlaySlot>,
) -> (&'static str, bool) {
    if let Some(location) = failure.and_then(PrefabExpansionFailure::location) {
        if matches!(location.source(), PrefabDeclarationSource::Prefab(_)) {
            return (
                "Resolution not checked for this stored intent. The first reported failure belongs to another Prefab source.",
                true,
            );
        }
        if Some(location.path()) == path {
            if failure.and_then(PrefabExpansionFailure::slot) == slot && slot.is_some() {
                return (
                    "This stored intent is the first reported composition failure.",
                    false,
                );
            }
            if failure.and_then(PrefabExpansionFailure::slot).is_none() {
                return (
                    "This instance declaration owns the first reported failure. Resolution of individual intents is not checked.",
                    false,
                );
            }
        }
    }
    (
        "Resolution not checked. No failure has been established for this stored intent.",
        false,
    )
}

fn intent_rows(overlay: &PrefabOverlay) -> Vec<(PrefabOverlaySlot, String)> {
    let mut rows = Vec::new();
    rows.extend(overlay.additions.iter().enumerate().map(|(index, value)| {
        (
            PrefabOverlaySlot::Addition { index },
            format!("Addition [{index}]: {}", value.id),
        )
    }));
    rows.extend(overlay.instances.iter().enumerate().map(|(index, value)| {
        (
            PrefabOverlaySlot::InstanceClaim { index },
            format!(
                "Instance claim [{index}]: {}",
                if value.value.is_none() {
                    "explicit detach"
                } else {
                    "stored link"
                }
            ),
        )
    }));
    for (index, component) in overlay.components.iter().enumerate() {
        rows.push((
            PrefabOverlaySlot::Component { index },
            format!("Component intent [{index}]"),
        ));
        if let PrefabComponentIntent::Properties { claims, .. } = component {
            rows.extend(claims.iter().enumerate().map(|(claim_index, _)| {
                (
                    PrefabOverlaySlot::Property {
                        component_index: index,
                        claim_index,
                    },
                    format!("Property [{index}/{claim_index}]"),
                )
            }));
        }
    }
    rows.extend(overlay.placements.iter().enumerate().map(|(index, _)| {
        (
            PrefabOverlaySlot::Placement { index },
            format!("Placement [{index}]"),
        )
    }));
    rows.extend(overlay.suppressions.iter().enumerate().map(|(index, _)| {
        (
            PrefabOverlaySlot::Suppression { index },
            format!("Suppression [{index}]"),
        )
    }));
    rows
}

fn intent_detail(overlay: &PrefabOverlay, slot: PrefabOverlaySlot) -> Option<String> {
    let output = match slot {
        PrefabOverlaySlot::Addition { index } => {
            bounded_debug(overlay.additions.get(index)?, DETAIL_BYTES)
        }
        PrefabOverlaySlot::InstanceClaim { index } => {
            bounded_debug(overlay.instances.get(index)?, DETAIL_BYTES)
        }
        PrefabOverlaySlot::Component { index } => {
            bounded_debug(overlay.components.get(index)?, DETAIL_BYTES)
        }
        PrefabOverlaySlot::Property {
            component_index,
            claim_index,
        } => {
            let PrefabComponentIntent::Properties {
                target,
                component,
                version,
                claims,
            } = overlay.components.get(component_index)?
            else {
                return None;
            };
            bounded_debug(
                &(target, component, version, claims.get(claim_index)?),
                DETAIL_BYTES,
            )
        }
        PrefabOverlaySlot::Placement { index } => {
            bounded_debug(overlay.placements.get(index)?, DETAIL_BYTES)
        }
        PrefabOverlaySlot::Suppression { index } => {
            bounded_debug(overlay.suppressions.get(index)?, DETAIL_BYTES)
        }
    };
    Some(output.unwrap_or_else(|| "Stored value exceeds the detail display limit. Preparation still requires a complete Before/After review within its display limit.".into()))
}

fn path_label(path: &PrefabInstancePath) -> String {
    let mut label = path.anchor.as_str().to_owned();
    for step in &path.steps {
        match step {
            PrefabInstancePathSegment::Addition { index } => {
                let _ = write!(label, " / addition[{index}]");
            }
            PrefabInstancePathSegment::InstanceClaim { index } => {
                let _ = write!(label, " / instance-claim[{index}]");
            }
        }
    }
    label
}

fn bounded_debug(value: &impl fmt::Debug, limit: usize) -> Option<String> {
    struct BoundedText {
        text: String,
        limit: usize,
    }
    impl Write for BoundedText {
        fn write_str(&mut self, value: &str) -> fmt::Result {
            if value.len() > self.limit.saturating_sub(self.text.len()) {
                return Err(fmt::Error);
            }
            self.text.push_str(value);
            Ok(())
        }
    }
    let mut output = BoundedText {
        text: String::new(),
        limit,
    };
    write!(output, "{value:#?}").ok()?;
    Some(output.text)
}

fn button(ui: &Ui, label: &str, enabled: bool) -> bool {
    let mut clicked = false;
    ui.with_disabled_if(!enabled, || clicked = ui.button(label));
    #[cfg(test)]
    super::ui_interaction_tests::record_control(ui, format!("Prefab Intent: {label}"));
    clicked
}

#[cfg(test)]
mod tests;
