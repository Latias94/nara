use std::sync::Arc;

use nara::render::{DisplayMapping2d, DisplayRect, DisplayScale, Extent2d};
use nara_text::FontFace;
use nara_ui_kernel::{
    UiActionId, UiAuthoringError, UiAuthoringSnapshotBuilder, UiBrush, UiColorRole, UiCompileError,
    UiCompiler, UiDimension, UiDispatchBatch, UiEdges, UiEnvironment, UiFlexDirection, UiFrame,
    UiInteraction, UiKey, UiKeyError, UiLayer, UiLocalKey, UiNode, UiOverflow, UiPoint, UiRect,
    UiRoot, UiRootKey, UiRootStatus, UiRoutePhase, UiRoutedEventKind, UiSemanticRole, UiSemantics,
    UiStyle, UiStyleError, UiText, UiTheme, UiVisual,
};
use nara_ui_widgets::{
    UiButton, UiButtonState, UiButtonVisualState, UiIntentBatch, UiIntentBatchError, UiWidgetError,
};
use thiserror::Error;

const TEXT_SIZE: f32 = 16.0;
const RETAINED_FRAMES: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SceneViewImage {
    pub(crate) extent: Extent2d,
    pub(crate) occurrence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SceneViewInput {
    pub image: Option<SceneViewImage>,
    pub presented: bool,
    pub editing: bool,
    pub unsaved_dependencies: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SceneViewPlacement {
    pub(crate) image: Option<SceneViewImage>,
    pub(crate) mapping: Option<DisplayMapping2d>,
    pub(crate) clip: DisplayRect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SceneViewPick {
    pub point: [f32; 2],
    pub image: SceneViewImage,
    pub mapping: DisplayMapping2d,
}

struct CompiledSceneView {
    frame: Arc<UiFrame>,
    input: SceneViewInput,
    uv: DisplayRect,
    placement: SceneViewPlacement,
    reset_visual: UiButtonVisualState,
    pointer: Option<UiPoint>,
}

pub(super) struct OwnedSceneView {
    root: UiRootKey,
    compiler: UiCompiler,
    reset: UiButton,
    reset_state: UiButtonState,
    viewport: UiKey,
    uv: DisplayRect,
    next_snapshot: u64,
    frames: Vec<CompiledSceneView>,
}

impl OwnedSceneView {
    pub(super) fn new(root: UiRootKey, fonts: Vec<FontFace>) -> Result<Self, OwnedSceneViewError> {
        let reset = UiButton::new(
            UiKey::scoped(root.clone(), local_key("reset")?)?,
            "Reset View",
            UiActionId::new("nara.scene-view.reset")?,
        )?;
        let viewport = UiKey::scoped(root.clone(), local_key("viewport")?)?;
        Ok(Self {
            root,
            compiler: UiCompiler::with_fonts(fonts, Default::default())?,
            reset,
            reset_state: UiButtonState::default(),
            viewport,
            uv: DisplayRect::UNIT,
            next_snapshot: 1,
            frames: Vec::new(),
        })
    }

    pub(super) fn compiled_frame(&self) -> Option<&Arc<UiFrame>> {
        self.frames.last().map(|compiled| &compiled.frame)
    }

    pub(super) fn compile(
        &mut self,
        environment: UiEnvironment,
        input: SceneViewInput,
    ) -> Result<Arc<UiFrame>, OwnedSceneViewError> {
        let reset_visual = self.reset_state.visual_state(&self.reset);
        if let Some(previous) = self.frames.last()
            && previous.frame.environment() == environment
            && previous.input == input
            && previous.uv == self.uv
            && previous.reset_visual == reset_visual
        {
            return Ok(Arc::clone(&previous.frame));
        }
        let reset_color = match reset_visual {
            UiButtonVisualState::Disabled => UiColorRole::Disabled,
            UiButtonVisualState::Pressed | UiButtonVisualState::Focused => UiColorRole::Focus,
            UiButtonVisualState::Hovered => UiColorRole::SecondaryText,
            UiButtonVisualState::Idle => UiColorRole::Panel,
        };
        let header_height = 44.0_f32.min((environment.logical_size().height() - 1.0).max(0.0));
        let mut header = UiNode::new(
            UiStyle::flex()
                .with_height(UiDimension::Px(header_height))
                .with_flex_direction(UiFlexDirection::Row)
                .with_flex_shrink(0.0)
                .with_gap(12.0)
                .with_padding(UiEdges::symmetric(8.0, 8.0_f32.min(header_height * 0.5))?)
                .with_overflow(UiOverflow::Clip),
        )
        .with_visual(UiVisual::role(UiColorRole::Surface))
        .with_child(
            self.reset.authored_node(
                &self.root,
                UiStyle::flex()
                    .with_flex_shrink(0.0)
                    .with_padding(UiEdges::symmetric(10.0, 6.0)?),
                UiVisual::role(reset_color),
                UiBrush::Role(UiColorRole::PrimaryText),
                TEXT_SIZE,
            )?,
        )
        .with_child(label(
            "status",
            if input.presented {
                "Presented"
            } else {
                "Preparing Scene View"
            },
        )?);
        if input.unsaved_dependencies != 0 {
            header = header.with_child(label(
                "unsaved-dependencies",
                format!("Unsaved dependencies: {}", input.unsaved_dependencies),
            )?);
        }
        let mut viewport = UiNode::new(
            UiStyle::flex()
                .with_width(UiDimension::Percent(1.0))
                .with_height(UiDimension::Px(0.0))
                .with_flex_grow(1.0)
                .with_overflow(UiOverflow::Clip),
        )
        .with_key(local_key("viewport")?)
        .with_interaction(UiInteraction::hit_target().with_enabled(input.image.is_some()))
        .with_semantics(
            UiSemantics::new(UiSemanticRole::Button, "Scene viewport")?
                .with_action(UiActionId::new("nara.scene-view.pick")?)?,
        );
        if input.image.is_none() {
            viewport = viewport.with_child(label("waiting", "Waiting for a Nara Scene image")?);
        }
        let root = UiRoot::new(
            self.root.clone(),
            UiLayer::Base,
            UiStyle::fill().with_overflow(UiOverflow::Clip),
        )
        .with_visual(UiVisual::role(UiColorRole::Panel))
        .with_children([header, viewport]);
        let snapshot = UiAuthoringSnapshotBuilder::new(self.next_snapshot)
            .with_root(root)
            .build()?;
        let next_snapshot = self
            .next_snapshot
            .checked_add(1)
            .ok_or(OwnedSceneViewError::IdentityExhausted)?;
        let frame = self
            .compiler
            .compile(&snapshot, environment, UiTheme::default())?;
        if !frame.root_outcomes().iter().any(|outcome| {
            outcome.key() == &self.root && outcome.status() == UiRootStatus::Published
        }) {
            return Err(OwnedSceneViewError::UnavailableViewport);
        }
        let placement = frame_placement(&frame, &self.viewport, input.image, self.uv)?;
        self.next_snapshot = next_snapshot;
        if self.frames.len() == RETAINED_FRAMES {
            self.frames.remove(0);
        }
        self.frames.push(CompiledSceneView {
            frame: Arc::clone(&frame),
            input,
            uv: self.uv,
            placement,
            reset_visual,
            pointer: None,
        });
        Ok(frame)
    }

    pub(super) fn placement(
        &self,
        frame: &Arc<UiFrame>,
    ) -> Result<SceneViewPlacement, OwnedSceneViewError> {
        Ok(self.frames[self.retained_index(frame)?].placement)
    }

    pub(super) fn set_pointer(
        &mut self,
        frame: &Arc<UiFrame>,
        point: UiPoint,
    ) -> Result<(), OwnedSceneViewError> {
        let index = self.retained_index(frame)?;
        if !point.x().is_finite() || !point.y().is_finite() {
            return Err(OwnedSceneViewError::InvalidPointer);
        }
        self.frames[index].pointer = Some(point);
        Ok(())
    }

    pub(super) fn clear_pointer(
        &mut self,
        frame: &Arc<UiFrame>,
    ) -> Result<(), OwnedSceneViewError> {
        let index = self.retained_index(frame)?;
        self.frames[index].pointer = None;
        Ok(())
    }

    pub(super) fn dispatch(
        &mut self,
        frame: &Arc<UiFrame>,
        dispatch: &UiDispatchBatch,
    ) -> Result<Option<SceneViewPick>, OwnedSceneViewError> {
        let index = self.retained_index(frame)?;
        if dispatch.frame_revision() != frame.revision() {
            return Err(OwnedSceneViewError::ForeignDispatch);
        }
        let mut reset_state = self.reset_state;
        let mut intents = UiIntentBatch::default();
        reset_state.reduce_batch(&self.reset, dispatch, &mut intents)?;
        if dispatch.is_reconciliation() {
            self.reset_state = reset_state;
            self.frames[index].pointer = None;
            return Ok(None);
        }
        self.reset_state = reset_state;
        if !intents.intents().is_empty() {
            self.uv = DisplayRect::UNIT;
        }
        let mut pick = None;
        let compiled = &mut self.frames[index];
        for event in dispatch.events() {
            if event.phase() != UiRoutePhase::Target || event.target() != &self.viewport {
                continue;
            }
            match event.kind() {
                UiRoutedEventKind::Cancelled => compiled.pointer = None,
                UiRoutedEventKind::Pressed
                    if compiled.input.presented && compiled.input.editing =>
                {
                    if let Some(point) = compiled.pointer
                        && let Some(image) = compiled.input.image
                        && let Some(mapping) = mapping_at(compiled.placement, point)
                    {
                        pick = Some(SceneViewPick {
                            point: [point.x(), point.y()],
                            image,
                            mapping,
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(pick)
    }

    pub(super) fn pan(
        &mut self,
        frame: &Arc<UiFrame>,
        delta: [f32; 2],
    ) -> Result<bool, OwnedSceneViewError> {
        let compiled = &self.frames[self.retained_index(frame)?];
        if delta.iter().any(|value| !value.is_finite()) || delta == [0.0, 0.0] {
            return Ok(false);
        }
        let Some(mapping) = compiled
            .pointer
            .and_then(|point| mapping_at(compiled.placement, point))
        else {
            return Ok(false);
        };
        let content = mapping.content_rect();
        let uv = self.uv;
        let span: [f32; 2] = std::array::from_fn(|axis| uv.max[axis] - uv.min[axis]);
        let min = std::array::from_fn(|axis| {
            uv.min[axis] - delta[axis] / (content.max[axis] - content.min[axis]) * span[axis]
        });
        Ok(self.set_uv(clamp_uv(min, span)))
    }

    pub(super) fn zoom(
        &mut self,
        frame: &Arc<UiFrame>,
        point: UiPoint,
        wheel: f32,
    ) -> Result<bool, OwnedSceneViewError> {
        let compiled = &self.frames[self.retained_index(frame)?];
        if !wheel.is_finite() || !point.x().is_finite() || !point.y().is_finite() || wheel == 0.0 {
            return Ok(false);
        }
        let Some(mapping) = mapping_at(compiled.placement, point) else {
            return Ok(false);
        };
        let content = mapping.content_rect();
        let uv = self.uv;
        let point = [point.x(), point.y()];
        let anchor: [f32; 2] = std::array::from_fn(|axis| {
            ((point[axis] - content.min[axis]) / (content.max[axis] - content.min[axis]))
                .clamp(0.0, 1.0)
        });
        let span: [f32; 2] = std::array::from_fn(|axis| uv.max[axis] - uv.min[axis]);
        let next = span.map(|span| (span * (-wheel * 0.1).exp()).clamp(0.125, 1.0));
        let min =
            std::array::from_fn(|axis| uv.min[axis] + anchor[axis] * (span[axis] - next[axis]));
        Ok(self.set_uv(clamp_uv(min, next)))
    }

    pub(super) fn cancel_input(&mut self) -> Result<(), OwnedSceneViewError> {
        self.reset_state.cancel_pointer_interaction();
        for compiled in &mut self.frames {
            compiled.pointer = None;
        }
        Ok(())
    }

    pub(super) fn invalidated_controls(
        &self,
        previous: &Arc<UiFrame>,
        candidate: &Arc<UiFrame>,
    ) -> Result<Vec<UiKey>, OwnedSceneViewError> {
        let previous = &self.frames[self.retained_index(previous)?];
        let candidate = &self.frames[self.retained_index(candidate)?];
        if previous.placement != candidate.placement
            || previous.uv != candidate.uv
            || previous.input.editing != candidate.input.editing
            || previous.input.presented != candidate.input.presented
        {
            Ok(vec![self.viewport.clone()])
        } else {
            Ok(Vec::new())
        }
    }

    fn retained_index(&self, frame: &Arc<UiFrame>) -> Result<usize, OwnedSceneViewError> {
        self.frames
            .iter()
            .position(|compiled| Arc::ptr_eq(&compiled.frame, frame))
            .ok_or(OwnedSceneViewError::RetiredFrame)
    }

    fn set_uv(&mut self, uv: DisplayRect) -> bool {
        let changed = self.uv != uv;
        self.uv = uv;
        changed
    }
}

fn frame_placement(
    frame: &UiFrame,
    viewport: &UiKey,
    image: Option<SceneViewImage>,
    uv: DisplayRect,
) -> Result<SceneViewPlacement, OwnedSceneViewError> {
    let layout = frame
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(viewport))
        .ok_or(OwnedSceneViewError::UnavailableViewport)?;
    let panel = layout.rect();
    let clip = if layout.visible() {
        layout
            .clip_rect()
            .map_or(Some(panel), |clip| panel.intersect(clip))
    } else {
        None
    };
    let mapping = image.filter(|_| clip.is_some()).and_then(|image| {
        DisplayMapping2d::fit(
            image.extent,
            display_rect(panel),
            frame.environment().scale_factor(),
            DisplayScale::Fit,
        )?
        .with_uv(uv)
    });
    let clip = clip.map_or(
        DisplayRect {
            min: [panel.origin().x(), panel.origin().y()],
            max: [panel.origin().x(), panel.origin().y()],
        },
        display_rect,
    );
    Ok(SceneViewPlacement {
        image,
        mapping,
        clip,
    })
}

fn display_rect(rect: UiRect) -> DisplayRect {
    DisplayRect {
        min: [rect.origin().x(), rect.origin().y()],
        max: [rect.max().x(), rect.max().y()],
    }
}

fn mapping_at(placement: SceneViewPlacement, point: UiPoint) -> Option<DisplayMapping2d> {
    let point = [point.x(), point.y()];
    placement.mapping.filter(|mapping| {
        [placement.clip, mapping.content_rect()]
            .into_iter()
            .all(|rect| {
                rect.is_valid()
                    && rect.contains(point)
                    && point[0] < rect.max[0]
                    && point[1] < rect.max[1]
            })
    })
}

fn local_key(name: &str) -> Result<UiLocalKey, UiKeyError> {
    UiLocalKey::runtime("nara.scene-view", name)
}

fn label(name: &str, text: impl Into<String>) -> Result<UiNode, OwnedSceneViewError> {
    Ok(UiNode::new(UiStyle::flex())
        .with_key(local_key(name)?)
        .with_text(UiText::new(text, TEXT_SIZE)?))
}

fn clamp_uv(min: [f32; 2], span: [f32; 2]) -> DisplayRect {
    let min = std::array::from_fn(|axis| min[axis].clamp(0.0, 1.0 - span[axis]));
    DisplayRect {
        min,
        max: [min[0] + span[0], min[1] + span[1]],
    }
}

#[derive(Debug, Error)]
pub(super) enum OwnedSceneViewError {
    #[error(transparent)]
    Compile(#[from] UiCompileError),
    #[error(transparent)]
    Authoring(#[from] UiAuthoringError),
    #[error(transparent)]
    Key(#[from] UiKeyError),
    #[error(transparent)]
    Style(#[from] UiStyleError),
    #[error(transparent)]
    Button(#[from] UiWidgetError),
    #[error(transparent)]
    Intents(#[from] UiIntentBatchError),
    #[error("Scene View snapshot identity is exhausted")]
    IdentityExhausted,
    #[error("the exact compiled Scene View frame is no longer retained")]
    RetiredFrame,
    #[error("the dispatch does not belong to this exact Scene View frame revision")]
    ForeignDispatch,
    #[error("the compiled Scene View viewport is unavailable")]
    UnavailableViewport,
    #[error("the Scene View pointer must be finite")]
    InvalidPointer,
}

#[cfg(test)]
mod tests;
