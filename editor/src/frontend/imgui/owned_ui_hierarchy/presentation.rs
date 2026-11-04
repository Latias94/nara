use std::{
    borrow::Cow,
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
};

use nara_ui_kernel::{
    UiActionId, UiAuthoringSnapshotBuilder, UiBrush, UiColorRole, UiLayer, UiLocalKey, UiNode,
    UiOverflow, UiRoot, UiRootStatus, UiStyle, UiText, UiTextOverflow, UiVisual,
};
use nara_ui_widgets::UiButtonVisualState;

use super::*;

const MAX_PRESENTATION_BYTES: usize = 128;
const ROW_PRESENTATION_BYTES: usize = 1536;

enum FormRow {
    Catalog(UiCatalogNodeKind, UiCatalogNodeKind),
    ChoosePlacement,
    ChooseImage,
    Field(Field),
    Message(&'static str, String),
}

impl OwnedUiHierarchy {
    pub(in crate::frontend::imgui) fn compile(
        &mut self,
        model: &EditorFrameModel,
        ui: &EditorUiAuthoringFrame,
        environment: UiEnvironment,
        theme: UiTheme,
        read_only: bool,
    ) -> Result<Arc<UiFrame>, OwnedUiHierarchyError> {
        let width = environment.logical_size().width() - 2.0 * INSET;
        let height = environment.logical_size().height();
        if width <= 16.0 || height <= 2.0 * INSET {
            return Err(OwnedUiHierarchyError::InvalidViewport);
        }
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
        let top = if height <= 160.0 { 2.0 } else { INSET };
        let action_height = 28.0;
        let half = (width - 6.0) / 2.0;
        let mut nodes = Vec::new();
        let mut buttons = HashMap::new();
        let mut inputs = Vec::new();
        let scroll_limit;

        if let Some(chooser) = self.chooser {
            self.blur_fields()?;
            nodes.push(self.button(
                Control::CloseChoices,
                "Close choices",
                !busy,
                [INSET, top, width, action_height],
                false,
                &mut buttons,
            )?);
            let y = top + action_height + 4.0;
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
            scroll_limit = (count as f32 * ROW_HEIGHT - viewport_height).max(0.0);
            self.choice_scroll = self.choice_scroll.clamp(0.0, scroll_limit);
            let range = visible_range(count, self.choice_scroll, viewport_height, ROW_HEIGHT);
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
                        index as f32 * ROW_HEIGHT - self.choice_scroll,
                        width,
                        ROW_HEIGHT,
                    ],
                    false,
                    &mut buttons,
                )?);
            }
            if count == 0 {
                children.push(label(
                    "empty-choices",
                    "No current choices are available",
                    0.0,
                    0.0,
                    width,
                    ROW_HEIGHT,
                )?);
            }
            nodes.push(viewport(y, width, viewport_height, children));
        } else if self.form.is_some() {
            // Apply/Cancel stay at the top even when the form body must scroll at 80 logical px.
            let rows = self.form_rows()?;
            let y = top + action_height + 4.0;
            let viewport_height = (height - y - top).max(0.0);
            scroll_limit = (rows.len() as f32 * ROW_HEIGHT - viewport_height).max(0.0);
            self.form_scroll = self.form_scroll.clamp(0.0, scroll_limit);
            let range = visible_range(rows.len(), self.form_scroll, viewport_height, ROW_HEIGHT);
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
                [INSET, top, half, action_height],
                false,
                &mut buttons,
            )?);
            nodes.push(self.button(
                Control::Cancel,
                "Cancel structure change",
                !busy,
                [INSET + half + 6.0, top, half, action_height],
                false,
                &mut buttons,
            )?);
            let text_limit = row_text_limit(range.len(), 2);
            let mut children = Vec::new();
            for index in range {
                let row_y = index as f32 * ROW_HEIGHT - self.form_scroll;
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
                                [column * (half + 6.0), row_y, half, ROW_HEIGHT - 2.0],
                                kind == item,
                                &mut buttons,
                            )?);
                        }
                    }
                    FormRow::ChoosePlacement => children.push(self.button(
                        Control::ChoosePlacement,
                        "Choose placement",
                        editing,
                        [0.0, row_y, width, ROW_HEIGHT - 2.0],
                        false,
                        &mut buttons,
                    )?),
                    FormRow::ChooseImage => children.push(self.button(
                        Control::ChooseImage,
                        "Choose image",
                        editing,
                        [0.0, row_y, width, ROW_HEIGHT - 2.0],
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
                            UiStyle::absolute(0.0, row_y, width, ROW_HEIGHT - 2.0),
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
                        identity,
                        &presentation_text_with_limit(message, text_limit),
                        0.0,
                        row_y,
                        width,
                        ROW_HEIGHT,
                    )?),
                }
            }
            nodes.push(viewport(y, width, viewport_height, children));
        } else {
            let third = (width - 12.0) / 3.0;
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
                    editing && structural,
                ),
            ] {
                nodes.push(self.button(
                    control,
                    name,
                    enabled,
                    [INSET + column * (third + 6.0), top, third, action_height],
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
                None
            };
            let mut y = top + action_height + 4.0;
            // Keep the exact refusal visible before optional diagnostics in compact tree mode.
            if let Some(reason) = reason {
                nodes.push(label("structural-reason", &reason, INSET, y, width, 20.0)?);
                y += 24.0;
            }
            if let Some(error) = &self.error
                && height > y + 60.0
            {
                nodes.push(label("error", error, INSET, y, width, 22.0)?);
                y += 26.0;
            }
            let viewport_height = (height - y - top).max(0.0);
            let count = projection.map_or(0, |projection| projection.hierarchy().len());
            scroll_limit = (count as f32 * SOURCE_ROW_HEIGHT - viewport_height).max(0.0);
            self.source_scroll = self.source_scroll.clamp(0.0, scroll_limit);
            let range = visible_range(
                count,
                self.source_scroll,
                viewport_height,
                SOURCE_ROW_HEIGHT,
            );
            let text_limit = row_text_limit(range.len(), 2);
            let mut children = Vec::new();
            if let Some(projection) = projection {
                for index in range {
                    let node = &projection.hierarchy()[index];
                    let indent = (f32::from(node.depth()) * 14.0).min((width - 24.0).max(0.0));
                    children.push(self.button(
                        Control::Select(node.entity().clone()),
                        &presentation_text_with_limit(
                            &format!("Select UI {}", node.entity().as_str()),
                            text_limit,
                        ),
                        navigation,
                        [
                            indent,
                            index as f32 * SOURCE_ROW_HEIGHT - self.source_scroll,
                            width - indent,
                            26.0,
                        ],
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
                        indent,
                        index as f32 * SOURCE_ROW_HEIGHT - self.source_scroll + 28.0,
                        width - indent,
                        20.0,
                    )?);
                }
            } else {
                children.push(label(
                    "no-projection",
                    "The current UI source projection is unavailable",
                    0.0,
                    0.0,
                    width,
                    ROW_HEIGHT,
                )?);
            }
            nodes.push(viewport(y, width, viewport_height, children));
        }

        let root = UiRoot::new(
            self.root.clone(),
            UiLayer::Base,
            UiStyle::fill().with_overflow(UiOverflow::Clip),
        )
        .with_visual(UiVisual::role(UiColorRole::Panel))
        .with_children(nodes);
        let snapshot = UiAuthoringSnapshotBuilder::new(self.next_snapshot)
            .with_root(root)
            .build()?;
        if self.frames.last().is_some_and(|previous| {
            previous.frame.environment() != environment
                || previous.theme != theme
                || previous.scroll_limit != scroll_limit
        }) {
            self.invalidate_actions();
        }
        let form = self.form.as_ref().map(|form| form.identity);
        if let Some(previous) = self.frames.last()
            && previous.snapshot.roots() == snapshot.roots()
            && previous.frame.environment() == environment
            && previous.theme == theme
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
                    authoring::placement_intent_label(Some(placement)),
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
                    authoring::placement_intent_label(Some(placement)),
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
        let color = match control.state.visual_state(&button) {
            UiButtonVisualState::Disabled => UiColorRole::Disabled,
            UiButtonVisualState::Pressed | UiButtonVisualState::Focused => UiColorRole::Focus,
            UiButtonVisualState::Hovered => UiColorRole::SecondaryText,
            UiButtonVisualState::Idle if selected => UiColorRole::Focus,
            UiButtonVisualState::Idle => UiColorRole::Panel,
        };
        let node = button.authored_node(
            &self.root,
            UiStyle::absolute(rect[0], rect[1], rect[2], rect[3]).with_overflow(UiOverflow::Clip),
            UiVisual::role(color),
            UiBrush::Role(UiColorRole::PrimaryText),
            TEXT_SIZE,
        )?;
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

fn viewport(y: f32, width: f32, height: f32, children: Vec<UiNode>) -> UiNode {
    UiNode::new(UiStyle::absolute(INSET, y, width, height).with_overflow(UiOverflow::Clip))
        .with_children(children)
}

fn label(
    identity: &str,
    text: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Result<UiNode, OwnedUiHierarchyError> {
    Ok(
        UiNode::new(UiStyle::absolute(x, y, width, height).with_overflow(UiOverflow::Clip))
            .with_key(UiLocalKey::runtime("nara.ui-hierarchy.label", identity)?)
            .with_text(
                UiText::new(presentation_text(text), TEXT_SIZE)?
                    .with_overflow(UiTextOverflow::Clip),
            ),
    )
}
