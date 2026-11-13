use std::{
    borrow::Cow,
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
};

use nara_ui_kernel::{
    UiActionId, UiAuthoringSnapshotBuilder, UiCaptureContinuity, UiInteraction, UiLayer,
    UiLocalKey, UiNode, UiOverflow, UiRoot, UiRootStatus, UiStyle, UiTextOverflow, UiVisual,
};
use nara_ui_widgets::{UiAppearanceError, UiAppearancePalette, UiButtonVariant, UiControlSize};

use super::*;

const MAX_PRESENTATION_BYTES: usize = 128;
const ROW_PRESENTATION_BYTES: usize = 1536;

#[derive(Clone, Copy)]
pub(super) struct Metrics {
    appearance: UiAppearance,
    body: f32,
    caption: f32,
    title: f32,
    action: f32,
    field: f32,
    source_row: f32,
}

impl Metrics {
    fn new(appearance: UiAppearance, engine: &mut TextEngine) -> Result<Self, UiAppearanceError> {
        let mut measure = |role| {
            let layout = engine.layout(&nara_text::TextLayoutRequest::from_typography(
                "Ag",
                appearance.typography(role)?,
            ))?;
            Ok::<_, UiAppearanceError>(line_box(&layout))
        };
        let body = measure(UiTextRole::Body)?;
        let caption = measure(UiTextRole::Caption)?;
        let title = measure(UiTextRole::Title)?;
        let action = appearance.control_metrics(UiControlSize::Small)?;
        let field = appearance.control_metrics(UiControlSize::Regular)?;
        let action = action
            .height()
            .max(body + action.padding().top() + action.padding().bottom());
        let field = field
            .height()
            .max(body + field.padding().top() + field.padding().bottom());
        Ok(Self {
            appearance,
            body,
            caption,
            title,
            action,
            field,
            source_row: action + caption + 2.0 * appearance.gap(),
        })
    }
}

enum FormRow {
    Catalog(UiCatalogNodeKind, UiCatalogNodeKind),
    ChoosePlacement,
    ChooseImage,
    Field(Field),
    Message(&'static str, String),
}

impl OwnedUiHierarchy {
    pub(in crate::frontend::controls) fn compile(
        &mut self,
        model: &EditorFrameModel,
        ui: &EditorUiAuthoringFrame,
        environment: UiEnvironment,
        theme: UiTheme,
        read_only: bool,
    ) -> Result<Arc<UiFrame>, OwnedUiHierarchyError> {
        if !self.explicit_appearance
            && self
                .frames
                .last()
                .is_none_or(|previous| previous.theme != theme)
        {
            let appearance = reference_appearance(theme)?;
            if self.appearance != appearance {
                self.appearance = appearance;
                self.invalidate_actions();
            }
        }
        let appearance = self.appearance;
        let inset = appearance.panel_inset();
        let gap = appearance.gap();
        let width = environment.logical_size().width() - 2.0 * inset;
        let height = environment.logical_size().height();
        if width <= 1.0 || height <= 0.0 {
            return Err(OwnedUiHierarchyError::InvalidViewport);
        }
        let metrics = match self.metrics {
            Some(metrics)
                if metrics.appearance.density() == appearance.density()
                    && metrics.appearance.text_scale() == appearance.text_scale() =>
            {
                metrics
            }
            _ => Metrics::new(appearance, &mut self.engine)?,
        };
        self.metrics = Some(metrics);
        let geometry_changed = self.frames.last().is_some_and(|previous| {
            previous.appearance.density() != appearance.density()
                || previous.appearance.text_scale() != appearance.text_scale()
        });
        let basis = UiHierarchyBasis::capture(model, ui, read_only);
        if self.basis != basis {
            let had_form = self.form.is_some();
            self.cancel_form()?;
            if had_form {
                self.error = Some("The original UI source, selection, Schema, images or permissions changed. The structure form was cancelled; open a new form.".to_owned());
            }
            if self.basis.as_ref().map(UiHierarchyBasis::scope)
                != basis.as_ref().map(UiHierarchyBasis::scope)
            {
                self.source_scroll = 0.0;
            }
            self.controls.clear();
            self.identities.clear();
            self.basis = basis.clone();
        }
        let busy = self.busy();
        let editing = basis
            .as_ref()
            .is_some_and(|basis| basis.editing && basis.structure)
            && !busy;
        let navigation = basis.as_ref().is_some_and(|basis| basis.navigation) && !busy;
        let projection = basis
            .as_ref()
            .and_then(|basis| basis.projection.as_ref().ok());
        let selected = basis.as_ref().and_then(|basis| basis.selected.as_ref());
        if let Some(form) = &mut self.form {
            for field in &mut form.fields {
                field.input.set_enabled(editing, &mut self.engine)?;
                field
                    .input
                    .reconfigure(&mut self.engine, appearance.typography(UiTextRole::Body)?)?;
                let field_appearance = super::super::appearance::field_appearance(
                    appearance,
                    &field.input,
                    width,
                    self.input_error.is_some(),
                )?;
                field.input.set_appearance(field_appearance);
            }
        }
        let top = if height < 4.0 * metrics.action + 2.0 * inset {
            2.0
        } else {
            inset
        };
        let action_height = metrics.action;
        let row_height = action_height + gap;
        let half = ((width - gap) / 2.0).max(1.0);
        let mut nodes = Vec::new();
        let mut buttons = HashMap::new();
        let mut inputs = Vec::new();
        let scroll_limit;
        let mut toolbar_y = top;
        if self.chooser.is_none()
            && height
                >= 2.0 * top + metrics.title + action_height + 2.0 * gap + 4.0 * metrics.source_row
        {
            let heading = match self.form.as_ref().map(|form| &form.dialog) {
                Some(StructureDialog::Create { .. }) => "Create UI node",
                Some(StructureDialog::Move { .. }) => "Move UI subtree",
                Some(StructureDialog::Delete { .. }) => "Delete UI subtree",
                None => "UI Hierarchy",
            };
            nodes.push(label(
                appearance,
                UiTextRole::Title,
                "heading",
                heading,
                [inset, toolbar_y, width, metrics.title],
            )?);
            toolbar_y += metrics.title + gap;
        }

        if let Some(chooser) = self.chooser {
            self.blur_fields()?;
            let side_by_side = height < 2.0 * top + 2.0 * action_height + gap;
            let close_width = if side_by_side { half } else { width };
            nodes.push(self.button(
                Control::CloseChoices,
                "Close choices",
                !busy,
                [inset, top, close_width, action_height],
                false,
                &mut buttons,
            )?);
            let viewport_x = if side_by_side {
                inset + half + gap
            } else {
                inset
            };
            let viewport_width = if side_by_side { half } else { width };
            let y = if side_by_side {
                top
            } else {
                top + action_height + gap
            };
            let viewport_height = (height - y - top).max(0.0);
            let choices = match chooser {
                Chooser::Placement => {
                    let form = self
                        .form
                        .as_ref()
                        .ok_or(OwnedUiHierarchyError::ChangedProjection)?;
                    ui.projection()?
                        .structure_placement_candidates(form.moving())?
                }
                Chooser::Image => Vec::new(),
            };
            let images = ui.image_choices().ok();
            let count = match chooser {
                Chooser::Placement => choices.len(),
                Chooser::Image => images.map_or(0, |images| images.choices().len()),
            };
            scroll_limit = (count as f32 * row_height - viewport_height).max(0.0);
            self.choice_scroll = self.choice_scroll.clamp(0.0, scroll_limit);
            let range = visible_range(count, self.choice_scroll, viewport_height, row_height);
            let text_limit = row_text_limit(range.len(), 1);
            let mut children = Vec::new();
            for index in range {
                let (control, name) = match chooser {
                    Chooser::Placement => {
                        let key = PlacementKey::of(&choices[index])?;
                        let name = placement_label(&key);
                        (Control::Placement(key), name)
                    }
                    Chooser::Image => {
                        let choice = &images
                            .ok_or(OwnedUiHierarchyError::ChangedProjection)?
                            .choices()[index];
                        (
                            Control::Image {
                                source: choice.source_identity().to_owned(),
                                path: choice.project_path().to_owned(),
                            },
                            format!("Image: {}", choice.project_path()),
                        )
                    }
                };
                children.push(self.button(
                    control,
                    &presentation_text_with_limit(&name, text_limit),
                    editing,
                    [
                        0.0,
                        index as f32 * row_height - self.choice_scroll,
                        viewport_width,
                        action_height,
                    ],
                    false,
                    &mut buttons,
                )?);
            }
            if count == 0 {
                children.push(label(
                    appearance,
                    UiTextRole::Body,
                    "empty-choices",
                    "No current choices are available",
                    [0.0, 0.0, viewport_width, metrics.body],
                )?);
            }
            nodes.push(viewport(
                viewport_x,
                y,
                viewport_width,
                viewport_height,
                children,
            ));
        } else if self.form.is_some() {
            // Apply/Cancel stay at the top even when the form body must scroll at 80 logical px.
            let rows = self.form_rows()?;
            let y = toolbar_y + action_height + gap;
            let viewport_height = (height - y - top).max(0.0);
            let mut row_bounds = Vec::with_capacity(rows.len());
            let mut content_height = 0.0;
            for row in &rows {
                let height = match row {
                    FormRow::Field(kind) => {
                        let input = self.field(*kind)?;
                        let padding = input
                            .appearance()
                            .expect("configured field appearance")
                            .padding;
                        metrics.field.max(
                            line_box(input.editor().layout()) + padding.top() + padding.bottom(),
                        )
                    }
                    FormRow::Message(..) => metrics.caption,
                    _ => action_height,
                };
                row_bounds.push((content_height, height));
                content_height += height + gap;
            }
            scroll_limit = (content_height - viewport_height).max(0.0);
            if geometry_changed {
                for (row, &(row_y, row_height)) in rows.iter().zip(&row_bounds) {
                    if let FormRow::Field(kind) = row
                        && self.field(*kind)?.input_token().is_some()
                    {
                        if row_y < self.form_scroll {
                            self.form_scroll = row_y;
                        } else if row_y + row_height > self.form_scroll + viewport_height {
                            self.form_scroll = row_y + row_height - viewport_height;
                        }
                    }
                }
            }
            self.form_scroll = self.form_scroll.clamp(0.0, scroll_limit);
            let start = row_bounds
                .iter()
                .position(|(y, height)| y + height > self.form_scroll)
                .unwrap_or(rows.len());
            let end = row_bounds
                .iter()
                .rposition(|(y, _)| *y < self.form_scroll + viewport_height)
                .map_or(start, |index| index + 1);
            let range = start..end.max(start);
            let shown_fields = rows[range.clone()]
                .iter()
                .filter_map(|row| match row {
                    FormRow::Field(kind) => Some(*kind),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if let Some(form) = &mut self.form {
                for field in &mut form.fields {
                    if !shown_fields.contains(&field.kind) {
                        field.input.blur(&mut self.engine)?;
                    }
                    field.input.set_enabled(editing, &mut self.engine)?;
                }
            }
            let prepared = self
                .form
                .as_ref()
                .ok_or(OwnedUiHierarchyError::ChangedProjection)?
                .prepare(ui);
            nodes.push(self.button(
                Control::Apply,
                "Apply structure change",
                editing && prepared.is_ok(),
                [inset, toolbar_y, half, action_height],
                false,
                &mut buttons,
            )?);
            nodes.push(self.button(
                Control::Cancel,
                "Cancel structure change",
                !busy,
                [inset + half + gap, toolbar_y, half, action_height],
                false,
                &mut buttons,
            )?);
            let text_limit = row_text_limit(range.len(), 2);
            let mut children = Vec::new();
            for index in range {
                let (row_y, row_height) = row_bounds[index];
                let row_y = row_y - self.form_scroll;
                match &rows[index] {
                    FormRow::Catalog(first, second) => {
                        let kind = match &self
                            .form
                            .as_ref()
                            .ok_or(OwnedUiHierarchyError::ChangedProjection)?
                            .dialog
                        {
                            StructureDialog::Create { kind, .. } => *kind,
                            _ => return Err(OwnedUiHierarchyError::ChangedProjection),
                        };
                        for (column, item) in [(0.0, *first), (1.0, *second)] {
                            children.push(self.button(
                                Control::Kind(item),
                                &format!("Kind: {}", item.label()),
                                editing,
                                [column * (half + gap), row_y, half, row_height],
                                kind == item,
                                &mut buttons,
                            )?);
                        }
                    }
                    FormRow::ChoosePlacement => children.push(self.button(
                        Control::ChoosePlacement,
                        "Choose placement",
                        editing,
                        [0.0, row_y, width, row_height],
                        false,
                        &mut buttons,
                    )?),
                    FormRow::ChooseImage => children.push(self.button(
                        Control::ChooseImage,
                        "Choose image",
                        editing,
                        [0.0, row_y, width, row_height],
                        false,
                        &mut buttons,
                    )?),
                    FormRow::Field(kind) => {
                        let form_id = self
                            .form
                            .as_ref()
                            .ok_or(OwnedUiHierarchyError::ChangedProjection)?
                            .identity;
                        let input = self.field(*kind)?;
                        children.push(input.authored_node(
                            &self.root,
                            UiStyle::absolute(0.0, row_y, width, row_height),
                            width,
                        )?);
                        inputs.push(DisplayedInput {
                            key: input.key().clone(),
                            form: form_id,
                            kind: *kind,
                            width,
                            display_text: input.editor().display_text().to_owned(),
                        });
                    }
                    FormRow::Message(identity, message) => children.push(label(
                        appearance,
                        UiTextRole::Caption,
                        identity,
                        &presentation_text_with_limit(message, text_limit),
                        [0.0, row_y, width, row_height],
                    )?),
                }
            }
            nodes.push(viewport(inset, y, width, viewport_height, children));
        } else {
            let third = ((width - 2.0 * gap) / 3.0).max(1.0);
            let selected_node = projection.and_then(|projection| {
                projection
                    .hierarchy()
                    .iter()
                    .find(|node| Some(node.entity()) == selected)
            });
            let structural = selected_node.is_some_and(|node| {
                node.structural_capability() == UiAuthoringStructuralCapability::Editable
            });
            let create = projection.is_some_and(|projection| {
                projection
                    .structure_placement_candidates(None)
                    .is_ok_and(|choices| !choices.is_empty())
            });
            for (column, control, name, enabled) in [
                (0.0, Control::Create, "Add UI node", editing && create),
                (1.0, Control::Move, "Move subtree", editing && structural),
                (
                    2.0,
                    Control::Delete,
                    "Delete subtree",
                    editing
                        && structural
                        && basis
                            .as_ref()
                            .is_some_and(|basis| basis.deletion_unavailable_reason.is_none()),
                ),
            ] {
                nodes.push(self.button(
                    control,
                    name,
                    enabled,
                    [
                        inset + column * (third + gap),
                        toolbar_y,
                        third,
                        action_height,
                    ],
                    false,
                    &mut buttons,
                )?);
            }
            let reason = if let Some(node) = selected_node
                && let UiAuthoringStructuralCapability::ReadOnly(reason) =
                    node.structural_capability()
            {
                Some(format!("Structure is read-only: {reason:?}"))
            } else if basis.as_ref().is_some_and(|basis| !basis.structure) {
                Some("Structure changes are unavailable in preservation-safe authoring".to_owned())
            } else {
                basis
                    .as_ref()
                    .and_then(|basis| basis.deletion_unavailable_reason)
                    .map(|reason| format!("Deletion unavailable: {reason}"))
            };
            let y = toolbar_y + action_height + gap;
            let mut messages = Vec::new();
            if let Some(reason) = reason {
                messages.push(("structural-reason", reason));
            }
            if busy {
                messages.push((
                    "pending",
                    "Awaiting the exact UI Hierarchy action result".to_owned(),
                ));
            }
            if let Some(error) = &self.error {
                messages.push(("error", error.clone()));
            }
            let viewport_height = (height - y - top).max(0.0);
            let prefix_height = messages.len() as f32 * (metrics.caption + gap);
            let count = projection.map_or(0, |projection| projection.hierarchy().len());
            scroll_limit =
                (prefix_height + count as f32 * metrics.source_row - viewport_height).max(0.0);
            self.source_scroll = self.source_scroll.clamp(0.0, scroll_limit);
            let range = visible_range(
                count,
                (self.source_scroll - prefix_height).max(0.0),
                (viewport_height - (prefix_height - self.source_scroll).max(0.0)).max(0.0),
                metrics.source_row,
            );
            let text_limit = row_text_limit(range.len(), 2);
            let mut children = Vec::new();
            if let Some(projection) = projection {
                for index in range {
                    let node = &projection.hierarchy()[index];
                    let indent = (f32::from(node.depth()) * metrics.body * 0.7).min(
                        (width
                            - 2.0
                                * appearance
                                    .control_metrics(UiControlSize::Small)?
                                    .padding()
                                    .left()
                            - 1.0)
                            .max(0.0),
                    );
                    let row_y =
                        prefix_height + index as f32 * metrics.source_row - self.source_scroll;
                    children.push(self.button(
                        Control::Select(node.entity().clone()),
                        &presentation_text_with_limit(
                            &format!("Select UI {}", node.entity().as_str()),
                            text_limit,
                        ),
                        navigation,
                        [indent, row_y, width - indent, action_height],
                        basis.as_ref().is_some_and(|basis| {
                            basis
                                .selection
                                .entities()
                                .any(|entity| entity == node.entity())
                        }),
                        &mut buttons,
                    )?);
                    let identity = self.identities[&Control::Select(node.entity().clone())];
                    children.push(label(
                        appearance,
                        UiTextRole::Caption,
                        &format!("source.{identity}.metadata"),
                        &presentation_text_with_limit(
                            &format!(
                                "{}: {} / {}",
                                node.kind().label(),
                                node.display_label(),
                                node.display_path()
                            ),
                            text_limit,
                        ),
                        [
                            indent,
                            row_y + action_height + gap,
                            width - indent,
                            metrics.caption,
                        ],
                    )?);
                }
            } else {
                children.push(label(
                    appearance,
                    UiTextRole::Body,
                    "no-projection",
                    "The current UI source projection is unavailable",
                    [0.0, prefix_height - self.source_scroll, width, metrics.body],
                )?);
            }
            for (index, (identity, message)) in messages.iter().enumerate() {
                let row_y = index as f32 * (metrics.caption + gap) - self.source_scroll;
                if row_y + metrics.caption > 0.0 && row_y < viewport_height {
                    children.push(label(
                        appearance,
                        UiTextRole::Caption,
                        identity,
                        message,
                        [0.0, row_y, width, metrics.caption],
                    )?);
                }
            }
            nodes.push(viewport(inset, y, width, viewport_height, children));
        }

        let root = UiRoot::new(
            self.root.clone(),
            UiLayer::Base,
            UiStyle::fill().with_overflow(UiOverflow::Clip),
        )
        .with_visual(UiVisual::solid(appearance.palette().panel))
        .with_children(nodes);
        let snapshot = UiAuthoringSnapshotBuilder::new(self.next_snapshot)
            .with_root(root)
            .build()?;
        if self.frames.last().is_some_and(|previous| {
            previous.frame.environment() != environment || previous.theme != theme
        }) {
            self.invalidate_actions();
        } else if self
            .frames
            .last()
            .is_some_and(|previous| previous.scroll_limit != scroll_limit)
        {
            self.viewport_observation = Arc::new(());
        }
        let form = self.form.as_ref().map(|form| form.identity);
        if let Some(previous) = self.frames.last()
            && previous.snapshot.roots() == snapshot.roots()
            && previous.frame.environment() == environment
            && previous.theme == theme
            && previous.appearance == appearance
            && previous.basis == basis
            && previous.form == form
            && Arc::ptr_eq(&previous.observation, &self.observation)
            && previous.scroll_limit == scroll_limit
        {
            return Ok(Arc::clone(&previous.frame));
        }
        let next_snapshot = self
            .next_snapshot
            .checked_add(1)
            .ok_or(OwnedUiHierarchyError::IdentityExhausted)?;
        let frame = self.compiler.compile(&snapshot, environment, theme)?;
        self.next_snapshot = next_snapshot;
        if !frame.root_outcomes().iter().any(|outcome| {
            outcome.key() == &self.root && outcome.status() == UiRootStatus::Published
        }) {
            self.cancel_input()?;
            self.controls.clear();
            return Err(OwnedUiHierarchyError::Unpublished);
        }
        let visible = frame
            .layouts()
            .iter()
            .filter(|layout| layout.visible())
            .filter_map(|layout| layout.key())
            .collect::<HashSet<_>>();
        let projected = buttons
            .values()
            .map(|button| &button.control)
            .collect::<HashSet<_>>();
        self.identities
            .retain(|control, _| projected.contains(control));
        buttons.retain(|key, _| visible.contains(key));
        inputs.retain(|input| visible.contains(&input.key));
        self.controls
            .retain(|_, control| buttons.contains_key(&control.key));
        if let Some(form) = &mut self.form {
            for field in &mut form.fields {
                if !inputs.iter().any(|input| input.key == *field.input.key()) {
                    field.input.blur(&mut self.engine)?;
                }
            }
        }
        self.frames.push(CompiledHierarchy {
            frame: Arc::clone(&frame),
            snapshot,
            theme,
            appearance,
            basis,
            form,
            observation: Arc::clone(&self.observation),
            viewport_observation: Arc::clone(&self.viewport_observation),
            buttons,
            inputs,
            scroll_limit,
        });
        if self.frames.len() > RETAINED_FRAMES {
            self.frames.remove(0);
        }
        Ok(frame)
    }

    fn form_rows(&self) -> Result<Vec<FormRow>, OwnedUiHierarchyError> {
        let form = self
            .form
            .as_ref()
            .ok_or(OwnedUiHierarchyError::ChangedProjection)?;
        let mut rows = Vec::new();
        match &form.dialog {
            StructureDialog::Create {
                kind,
                placement,
                image,
            } => {
                rows.push(FormRow::Catalog(
                    UiCatalogNodeKind::Container,
                    UiCatalogNodeKind::Text,
                ));
                rows.push(FormRow::Catalog(
                    UiCatalogNodeKind::Image,
                    UiCatalogNodeKind::Button,
                ));
                rows.push(FormRow::ChoosePlacement);
                rows.push(FormRow::Message(
                    "placement",
                    placement_intent_label(Some(placement)),
                ));
                match kind {
                    UiCatalogNodeKind::Container => {}
                    UiCatalogNodeKind::Text => rows.push(FormRow::Field(Field::Text)),
                    UiCatalogNodeKind::Image => {
                        rows.push(FormRow::ChooseImage);
                        rows.push(FormRow::Message(
                            "image",
                            image.as_ref().map_or_else(
                                || "Select an image before applying".to_owned(),
                                |image| format!("Image: {}", image.project_path()),
                            ),
                        ));
                    }
                    UiCatalogNodeKind::Button => {
                        rows.push(FormRow::Field(Field::Label));
                        rows.push(FormRow::Field(Field::Action));
                    }
                }
            }
            StructureDialog::Move { target, placement } => {
                rows.push(FormRow::Message(
                    "target",
                    format!("Move {}", target.as_str()),
                ));
                rows.push(FormRow::ChoosePlacement);
                rows.push(FormRow::Message(
                    "placement",
                    placement_intent_label(Some(placement)),
                ));
            }
            StructureDialog::Delete { target } => {
                rows.push(FormRow::Message(
                    "target",
                    format!("Delete {} and its source-local subtree?", target.as_str()),
                ));
                rows.push(FormRow::Message(
                    "undo",
                    "Undo remains available after deletion".to_owned(),
                ));
            }
        }
        if self.busy() {
            rows.push(FormRow::Message(
                "pending",
                "Awaiting the exact structure action result".to_owned(),
            ));
        }
        if let Some(error) = &self.input_error {
            rows.push(FormRow::Message("input-error", error.clone()));
        }
        if let Some(error) = &self.error {
            rows.push(FormRow::Message("error", error.clone()));
        }
        // Owner validation feedback is presentation only; Apply prepares again on activation.
        if let Err(error) = form.intent().and_then(|intent| {
            form.basis
                .projection
                .as_ref()
                .map_err(|error| OwnedUiHierarchyError::Projection(*error))?
                .prepare_structure(&intent, form.basis.images.as_ref().ok())?;
            Ok(())
        }) {
            rows.push(FormRow::Message("validation", error.to_string()));
        }
        Ok(rows)
    }

    fn button(
        &mut self,
        action: Control,
        name: &str,
        enabled: bool,
        rect: [f32; 4],
        selected: bool,
        displayed: &mut HashMap<UiKey, DisplayedButton>,
    ) -> Result<UiNode, OwnedUiHierarchyError> {
        if !self.controls.contains_key(&action) {
            let identity = if let Some(identity) = self.identities.get(&action) {
                *identity
            } else {
                let identity = self.next_control;
                self.next_control = identity
                    .checked_add(1)
                    .ok_or(OwnedUiHierarchyError::IdentityExhausted)?;
                self.identities.insert(action.clone(), identity);
                identity
            };
            let key = UiKey::scoped(
                self.root.clone(),
                UiLocalKey::runtime("nara.ui-hierarchy.action", identity.to_string())?,
            )?;
            self.controls.insert(
                action.clone(),
                ButtonControl {
                    key,
                    state: UiButtonState::default(),
                },
            );
        }
        let control = &self.controls[&action];
        let button = UiButton::new(
            control.key.clone(),
            presentation_text(name),
            UiActionId::new("ui-hierarchy.activate")?,
        )?
        .with_enabled(enabled);
        let variant = match action {
            Control::Apply
                if self
                    .form
                    .as_ref()
                    .is_some_and(|form| matches!(form.dialog, StructureDialog::Delete { .. })) =>
            {
                UiButtonVariant::Destructive
            }
            Control::Apply => UiButtonVariant::Primary,
            Control::Create | Control::Move => UiButtonVariant::Secondary,
            _ => UiButtonVariant::Quiet,
        };
        let mut recipe = self.appearance.button(
            &button,
            control.state,
            variant,
            UiControlSize::Small,
            selected,
        )?;
        if action == Control::Delete && enabled {
            recipe = recipe.with_text_brush(nara_ui_kernel::UiBrush::Solid(
                self.appearance.palette().danger_text,
            ));
        }
        let node = recipe
            .authored_node(
                &button,
                &self.root,
                UiStyle::absolute(rect[0], rect[1], rect[2], rect[3])
                    .with_overflow(UiOverflow::Clip),
            )?
            .with_interaction(
                UiInteraction::focus_target()
                    .with_enabled(enabled)
                    .with_capture_continuity(UiCaptureContinuity::TargetGeometry),
            );
        displayed.insert(
            control.key.clone(),
            DisplayedButton {
                control: action,
                button,
            },
        );
        Ok(node)
    }
}

fn placement_label(key: &PlacementKey) -> String {
    let slot = match &key.slot {
        PlacementSlot::First => "first".to_owned(),
        PlacementSlot::Last => "last".to_owned(),
        PlacementSlot::Before(entity) => format!("before {}", entity.as_str()),
        PlacementSlot::After(entity) => format!("after {}", entity.as_str()),
    };
    format!("Placement: {} / {slot}", key.parent.as_str())
}

fn presentation_text(text: &str) -> Cow<'_, str> {
    presentation_text_with_limit(text, MAX_PRESENTATION_BYTES)
}

fn row_text_limit(rows: usize, texts_per_row: usize) -> usize {
    (ROW_PRESENTATION_BYTES / rows.saturating_mul(texts_per_row).max(1))
        .clamp(1, MAX_PRESENTATION_BYTES)
}

fn presentation_text_with_limit(text: &str, maximum: usize) -> Cow<'_, str> {
    if text.len() <= maximum {
        return Cow::Borrowed(text);
    }
    let mut digest = DefaultHasher::new();
    text.hash(&mut digest);
    let digest = digest.finish();
    let marker = if maximum >= 64 {
        format!(" … [{} bytes, {digest:016x}] … ", text.len())
    } else if maximum <= 2 {
        "~".repeat(maximum)
    } else {
        let hex = format!("{digest:016x}");
        format!("~{}~", &hex[..(maximum - 2).min(hex.len())])
    };
    let retained = (maximum - marker.len()) / 2;
    let mut prefix_end = retained;
    while !text.is_char_boundary(prefix_end) {
        prefix_end -= 1;
    }
    let mut suffix_start = text.len() - retained;
    while !text.is_char_boundary(suffix_start) {
        suffix_start += 1;
    }
    Cow::Owned(format!(
        "{}{}{}",
        &text[..prefix_end],
        marker,
        &text[suffix_start..]
    ))
}

fn visible_range(
    total: usize,
    scroll: f32,
    height: f32,
    row_height: f32,
) -> std::ops::Range<usize> {
    if height <= 0.0 {
        return 0..0;
    }
    let start = ((scroll / row_height).floor() as usize).min(total);
    let end = (((scroll + height) / row_height).ceil() as usize).min(total);
    start..end
}

fn viewport(x: f32, y: f32, width: f32, height: f32, children: Vec<UiNode>) -> UiNode {
    UiNode::new(UiStyle::absolute(x, y, width, height).with_overflow(UiOverflow::Clip))
        .with_children(children)
}

fn label(
    appearance: UiAppearance,
    role: UiTextRole,
    identity: &str,
    text: &str,
    rect: [f32; 4],
) -> Result<UiNode, OwnedUiHierarchyError> {
    Ok(UiNode::new(
        UiStyle::absolute(rect[0], rect[1], rect[2], rect[3]).with_overflow(UiOverflow::Clip),
    )
    .with_key(UiLocalKey::runtime("nara.ui-hierarchy.label", identity)?)
    .with_text(
        appearance
            .text(presentation_text(text), role, role == UiTextRole::Caption)?
            .with_overflow(UiTextOverflow::Clip),
    ))
}

fn line_box(layout: &nara_text::TextLayout) -> f32 {
    let (min, max) = layout
        .lines()
        .iter()
        .fold((0.0_f32, layout.height()), |(min, max), line| {
            (min.min(line.block_min()), max.max(line.block_max()))
        });
    max - min
}

fn reference_appearance(theme: UiTheme) -> Result<UiAppearance, UiAppearanceError> {
    let colors = theme.palette();
    UiAppearance::default().with_palette(UiAppearancePalette {
        window: colors.surface,
        panel: colors.panel,
        raised: colors.panel,
        input: colors.surface,
        border: colors.secondary_text,
        text: colors.primary_text,
        muted_text: colors.secondary_text,
        disabled_text: colors.disabled,
        accent: colors.accent,
        on_accent: colors.primary_text,
        selection: colors.accent,
        focus: colors.focus,
        danger: colors.danger,
        danger_text: colors.danger,
        positive: colors.positive,
        hover: colors.secondary_text,
        pressed: colors.focus,
    })
}
