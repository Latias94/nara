use std::{collections::BTreeSet, sync::Arc};

use nara_text::{TextEngine, TextLayoutError, TextRect};
use taffy::{
    AlignItems, Dimension, Display, FlexDirection, JustifyContent, LengthPercentage,
    LengthPercentageAuto, Overflow, Point as TaffyPoint, Position, Rect as TaffyRect,
    Size as TaffySize, Style as TaffyStyle,
};

use crate::{
    UiAlign, UiDimension, UiEnvironment, UiFlexDirection, UiFocusFact, UiHitRegion, UiJustify,
    UiKey, UiLimits, UiNode, UiOverflow, UiPaintPrimitive, UiPoint, UiPositionMode, UiRect,
    UiResolvedBorder, UiRoot, UiRootDiagnostic, UiRootKey, UiSemanticNode, UiSemanticRole, UiStyle,
    UiTextOverflow, UiTheme,
};

use super::{
    UiCompileWorkStats,
    cache::{RootLayoutFact, RootVisual},
    order::{RootOrder, RootOrdered},
    text::{TextCompileError, TextCompileSession},
};
use crate::authoring::UiNodeText;
use crate::mesh::MeshCounts;

#[derive(Clone, Debug)]
pub(super) struct CompiledRoot {
    pub(super) visual: Arc<RootVisual>,
    pub(super) hits: Vec<RootOrdered<UiHitRegion>>,
    pub(super) focus: Vec<RootOrdered<UiFocusFact>>,
    pub(super) semantics: Vec<RootOrdered<UiSemanticNode>>,
}

#[derive(Debug)]
pub(super) enum RootCompileError {
    Local(UiRootDiagnostic),
    TextWorkLimitExceeded { actual: usize, maximum: usize },
    ShapedGlyphLimitExceeded { actual: usize, maximum: usize },
    LogicalPaintLimitExceeded { actual: usize, maximum: usize },
    LayoutWorkLimitExceeded { actual: usize, maximum: usize },
    MeshCountOverflow,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RootBudget {
    used_shaped_glyphs: usize,
    max_shaped_glyphs: usize,
    used_paint: usize,
    max_paint: usize,
}

impl RootBudget {
    pub(super) const fn new(
        used_shaped_glyphs: usize,
        max_shaped_glyphs: usize,
        used_paint: usize,
        max_paint: usize,
    ) -> Self {
        Self {
            used_shaped_glyphs,
            max_shaped_glyphs,
            used_paint,
            max_paint,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn compile_root(
    text_engine: &mut TextEngine,
    work_stats: &mut UiCompileWorkStats,
    root: &UiRoot,
    environment: UiEnvironment,
    theme: UiTheme,
    limits: UiLimits,
    text_session: &mut TextCompileSession,
    layout_work: &mut super::LayoutWorkBudget,
    budget: RootBudget,
) -> Result<CompiledRoot, RootCompileError> {
    let flat = flatten_root(root, limits).map_err(RootCompileError::Local)?;
    // Admit complete prepared sources even when fixed sizes avoid measurement or the final
    // geometry is fully clipped. Immutable source reuse pays once across the whole candidate.
    for node in &flat {
        if let Some(UiNodeText::Prepared(text)) = &node.node.text {
            text_session
                .admit_prepared(text_engine, &text.snapshot, environment.scale_factor())
                .map_err(text_compile_error)?;
        }
    }
    let layouts = super::layout::compute_layout(
        &flat,
        text_engine,
        work_stats,
        environment,
        text_session,
        layout_work,
    )?;
    let scale_factor = environment.scale_factor();

    let mut layout_entries = Vec::with_capacity(flat.len());
    let mut paint_entries = Vec::new();
    let mut hit_entries = Vec::new();
    let mut focus_entries = Vec::new();
    let mut semantic_entries = Vec::new();
    let mut shaped_glyphs = 0usize;
    let mut mesh_counts = MeshCounts::default();
    let mut origins = vec![UiPoint::new(0.0, 0.0); flat.len()];
    let mut clips = vec![ClipState::Unclipped; flat.len()];
    let mut visible = vec![false; flat.len()];

    for (index, node) in flat.iter().enumerate() {
        let layout = &layouts[index];
        let parent_origin = node
            .parent
            .map_or(UiPoint::new(0.0, 0.0), |parent| origins[parent]);
        let origin = UiPoint::new(
            parent_origin.x() + layout.location.x,
            parent_origin.y() + layout.location.y,
        );
        let rect = UiRect::from_origin_size(
            origin.x(),
            origin.y(),
            layout.size.width,
            layout.size.height,
        )
        .ok_or_else(|| {
            RootCompileError::Local(UiRootDiagnostic::LayoutRejected {
                message: "Taffy produced non-finite or negative geometry".to_owned(),
            })
        })?;
        origins[index] = origin;
        let inherited_clip = node
            .parent
            .map_or(ClipState::Unclipped, |parent| clips[parent]);
        let parent_visible = node.parent.is_none_or(|parent| visible[parent]);
        let intersects_clip = match inherited_clip {
            ClipState::Unclipped => true,
            ClipState::Rect(clip) => rect.intersect(clip).is_some(),
            ClipState::Empty => false,
        };
        let node_visible =
            parent_visible && node.node.style.visible && !rect.size().is_empty() && intersects_clip;
        visible[index] = node_visible;
        clips[index] = if node.node.style.overflow == UiOverflow::Clip {
            inherited_clip.intersect(rect)
        } else {
            inherited_clip
        };

        let order = root_order(node, 0);
        layout_entries.push(RootOrdered::new(
            order.clone(),
            RootLayoutFact {
                key: node.key.clone(),
                route_parent: node.keyed_parent.clone(),
                root_key: root.key.clone(),
                structural_index: node.structural_index,
                local_z: node.node.style.z_index,
                rect,
                physical_rect: environment.snap(rect),
                clip_rect: inherited_clip.rect(),
                visible: node_visible,
            },
        ));

        let content_rect = (node.node.text.is_some() || node.node.mesh.is_some())
            .then(|| {
                UiRect::from_origin_size(
                    parent_origin.x() + layout.content_box_x(),
                    parent_origin.y() + layout.content_box_y(),
                    layout.content_box_width().max(0.0),
                    layout.content_box_height().max(0.0),
                )
                .ok_or_else(|| {
                    RootCompileError::Local(UiRootDiagnostic::LayoutRejected {
                        message: "Taffy produced invalid content geometry".to_owned(),
                    })
                })
            })
            .transpose()?;
        let text_geometry = node.node.text.as_ref().zip(content_rect).map(
            |(text, content_rect)| {
                let text_clip = if matches!(text, UiNodeText::Prepared(_))
                    || matches!(text, UiNodeText::Plain(text) if text.overflow == UiTextOverflow::Clip)
                {
                    inherited_clip.intersect(content_rect)
                } else {
                    inherited_clip
                };
                (content_rect, text_clip)
            },
        );
        // A hidden or empty draw still admits every raster under the receiving policy. Use its
        // resolved origin so subpixel extents agree with an otherwise identical visible draw.
        let prepared_layout =
            if let (Some(UiNodeText::Prepared(text)), Some((content_rect, text_clip))) =
                (&node.node.text, text_geometry)
            {
                let surface = UiRect::from_origin_size(
                    0.0,
                    0.0,
                    environment.logical_size().width(),
                    environment.logical_size().height(),
                )
                .expect("an environment has validated logical bounds");
                let clip = (node_visible && !matches!(text_clip, ClipState::Empty))
                    .then(|| text_clip.rect().unwrap_or(content_rect).intersect(surface))
                    .flatten();
                let clip = clip.map_or(TextRect::default(), |clip| TextRect {
                    x0: clip.origin().x(),
                    y0: clip.origin().y(),
                    x1: clip.max().x(),
                    y1: clip.max().y(),
                });
                let physical_origin = environment.content_transform().physical_content_origin();
                Some(
                    text_engine
                        .prepare_window_at(
                            &text.snapshot,
                            clip,
                            scale_factor,
                            [content_rect.origin().x(), content_rect.origin().y()],
                            [physical_origin.x(), physical_origin.y()],
                        )
                        .map_err(|error| text_compile_error(TextCompileError::Window(error)))?,
                )
            } else {
                None
            };
        if !node_visible {
            continue;
        }
        if let Some(visual) = &node.node.visual {
            let brush = visual
                .background
                .as_ref()
                .map(|background| background.resolve(theme));
            let border = visual.border.as_ref().map(|border| {
                let crate::UiResolvedBrush::Solid(color) = border.brush().resolve(theme) else {
                    unreachable!("image brushes are not admitted for borders")
                };
                UiResolvedBorder::new(border.width(), color)
            });
            if brush.is_some() || border.is_some() {
                paint_entries.push(RootOrdered::new(
                    order.clone(),
                    UiPaintPrimitive::Rect {
                        key: node.key.clone(),
                        rect,
                        physical_rect: environment.snap(rect),
                        clip_rect: inherited_clip.rect(),
                        brush,
                        border,
                        radii: visual.radii,
                    },
                ));
                ensure_paint_capacity(budget.used_paint, paint_entries.len(), budget.max_paint)?;
            }
        }
        if let (Some(mesh), Some(content_rect)) = (&node.node.mesh, content_rect) {
            let mesh_clip = inherited_clip.intersect(content_rect);
            if !content_rect.size().is_empty() && !matches!(mesh_clip, ClipState::Empty) {
                mesh_counts = mesh_counts
                    .checked_add(MeshCounts::of(mesh))
                    .ok_or(RootCompileError::MeshCountOverflow)?;
                paint_entries.push(RootOrdered::new(
                    root_order(node, 1),
                    UiPaintPrimitive::Mesh {
                        key: node.key.clone(),
                        rect: content_rect,
                        physical_rect: environment.snap(content_rect),
                        clip_rect: mesh_clip.rect(),
                        mesh: mesh.clone(),
                    },
                ));
                ensure_paint_capacity(budget.used_paint, paint_entries.len(), budget.max_paint)?;
            }
        }
        if let (Some(text), Some((content_rect, text_clip))) = (&node.node.text, text_geometry) {
            if !content_rect.size().is_empty() && !matches!(text_clip, ClipState::Empty) {
                let text_layout = match text {
                    UiNodeText::Plain(text) => {
                        let wrap_width = (text.overflow == UiTextOverflow::Wrap)
                            .then_some(content_rect.size().width())
                            .filter(|width| *width > 0.0);
                        let request = text
                            .request(wrap_width, scale_factor)
                            .map_err(|error| text_compile_error(TextCompileError::Layout(error)))?;
                        text_session
                            .layout(text_engine, request)
                            .map_err(text_compile_error)?
                    }
                    UiNodeText::Prepared(_) => {
                        prepared_layout.expect("prepared text was admitted before visibility")
                    }
                };
                let candidate_glyphs = budget
                    .used_shaped_glyphs
                    .saturating_add(shaped_glyphs)
                    .saturating_add(text_layout.glyphs().len());
                if candidate_glyphs > budget.max_shaped_glyphs {
                    return Err(RootCompileError::ShapedGlyphLimitExceeded {
                        actual: candidate_glyphs,
                        maximum: budget.max_shaped_glyphs,
                    });
                }
                shaped_glyphs = shaped_glyphs.saturating_add(text_layout.glyphs().len());
                paint_entries.push(RootOrdered::new(
                    root_order(node, 2),
                    UiPaintPrimitive::Text {
                        key: node.key.clone(),
                        rect: content_rect,
                        physical_rect: environment.snap(content_rect),
                        clip_rect: text_clip.rect(),
                        color: match text.brush().resolve(theme) {
                            crate::UiResolvedBrush::Solid(color) => color,
                            crate::UiResolvedBrush::Image { .. } => {
                                return Err(RootCompileError::Local(
                                    UiRootDiagnostic::InvalidStyle {
                                        message: "text brushes cannot reference images".to_owned(),
                                    },
                                ));
                            }
                        },
                        layout: text_layout,
                    },
                ));
                ensure_paint_capacity(budget.used_paint, paint_entries.len(), budget.max_paint)?;
            }
        }
        if node.node.interaction.hit_testable {
            hit_entries.push(RootOrdered::new(
                root_order(node, u8::MAX),
                UiHitRegion {
                    key: node
                        .key
                        .clone()
                        .expect("required interaction key is validated"),
                    rect,
                    clip_rect: inherited_clip.rect(),
                    enabled: node.node.interaction.enabled,
                },
            ));
        }
        if node.node.interaction.focusable {
            focus_entries.push(RootOrdered::new(
                root_order(node, u8::MAX),
                UiFocusFact {
                    key: node.key.clone().expect("required focus key is validated"),
                    rect,
                    enabled: node.node.interaction.enabled,
                },
            ));
        }
        if let Some(semantics) = &node.node.semantics {
            semantic_entries.push(RootOrdered::new(
                root_order(node, u8::MAX),
                UiSemanticNode {
                    key: node
                        .key
                        .clone()
                        .expect("required semantic key is validated"),
                    parent: node.semantic_parent.clone(),
                    role: semantics.role,
                    name: semantics.name.clone(),
                    value: semantics.value.clone(),
                    enabled: semantic_enabled(node.node),
                    selected: semantics.selected,
                    expanded: semantics.expanded,
                    actions: semantics.actions.clone(),
                    rect,
                    clip_rect: inherited_clip.rect(),
                },
            ));
        } else if let (Some(text), Some(key)) = (&node.node.text, &node.key) {
            semantic_entries.push(RootOrdered::new(
                root_order(node, u8::MAX),
                UiSemanticNode {
                    key: key.clone(),
                    parent: node.semantic_parent.clone(),
                    role: UiSemanticRole::Text,
                    name: text.semantic_text().to_owned(),
                    value: None,
                    enabled: true,
                    selected: None,
                    expanded: None,
                    actions: Vec::new(),
                    rect,
                    clip_rect: inherited_clip.rect(),
                },
            ));
        }
    }

    Ok(CompiledRoot {
        visual: Arc::new(RootVisual {
            layouts: layout_entries,
            paint: paint_entries,
            shaped_glyphs,
            mesh_counts,
        }),
        hits: hit_entries,
        focus: focus_entries,
        semantics: semantic_entries,
    })
}

fn ensure_paint_capacity(
    used_paint: usize,
    root_paint: usize,
    maximum: usize,
) -> Result<(), RootCompileError> {
    let actual = used_paint.saturating_add(root_paint);
    if actual > maximum {
        return Err(RootCompileError::LogicalPaintLimitExceeded { actual, maximum });
    }
    Ok(())
}

fn text_error(error: TextLayoutError) -> RootCompileError {
    match error {
        TextLayoutError::GlyphLimitExceeded { actual, maximum } => {
            RootCompileError::ShapedGlyphLimitExceeded { actual, maximum }
        }
        error => RootCompileError::Local(UiRootDiagnostic::TextRejected {
            message: error.to_string(),
        }),
    }
}

pub(super) fn text_compile_error(error: TextCompileError) -> RootCompileError {
    match error {
        TextCompileError::SourceBytesLimitExceeded { actual, maximum } => {
            RootCompileError::TextWorkLimitExceeded { actual, maximum }
        }
        TextCompileError::ShapedGlyphLimitExceeded { actual, maximum } => {
            RootCompileError::ShapedGlyphLimitExceeded { actual, maximum }
        }
        TextCompileError::Layout(error) => text_error(error),
        TextCompileError::Window(error) => {
            RootCompileError::Local(UiRootDiagnostic::TextRejected {
                message: error.to_string(),
            })
        }
    }
}

fn flatten_root<'a>(
    root: &'a UiRoot,
    limits: UiLimits,
) -> Result<Vec<FlatNode<'a>>, UiRootDiagnostic> {
    let root_key =
        UiKey::root(root.key.clone()).map_err(|error| UiRootDiagnostic::KeyRejected {
            message: error.to_string(),
        })?;
    let mut nodes = Vec::new();
    let mut keys = BTreeSet::new();
    visit_node(
        &root.node,
        &root.key,
        Some(root_key),
        None,
        1,
        Vec::new(),
        None,
        None,
        &mut nodes,
        &mut keys,
        limits,
    )?;
    Ok(nodes)
}

#[allow(clippy::too_many_arguments)]
fn visit_node<'a>(
    node: &'a UiNode,
    root_key: &UiRootKey,
    forced_key: Option<UiKey>,
    parent: Option<usize>,
    depth: usize,
    stacking_ancestors: Vec<(i32, usize)>,
    semantic_parent: Option<UiKey>,
    keyed_parent: Option<UiKey>,
    nodes: &mut Vec<FlatNode<'a>>,
    keys: &mut BTreeSet<UiKey>,
    limits: UiLimits,
) -> Result<usize, UiRootDiagnostic> {
    if depth > limits.max_depth {
        return Err(UiRootDiagnostic::DepthLimitExceeded {
            maximum: limits.max_depth,
        });
    }
    node.style
        .validate()
        .map_err(|error| UiRootDiagnostic::InvalidStyle {
            message: error.to_string(),
        })?;
    if let Some(visual) = &node.visual {
        visual
            .validate()
            .map_err(|error| UiRootDiagnostic::InvalidStyle {
                message: error.to_string(),
            })?;
    }
    if node.custom_layout.is_some() && node.text.is_some() {
        return Err(UiRootDiagnostic::LayoutRejected {
            message: "a custom layout container must put intrinsic text in a child node".into(),
        });
    }
    if let Some(text) = &node.text {
        text.validate()
            .map_err(|error| UiRootDiagnostic::InvalidStyle {
                message: error.to_string(),
            })?;
    }
    let key = match forced_key {
        Some(key) => Some(key),
        None => node
            .key
            .as_ref()
            .map(|local| UiKey::scoped(root_key.clone(), local.clone()))
            .transpose()
            .map_err(|error| UiRootDiagnostic::KeyRejected {
                message: error.to_string(),
            })?,
    };
    if key
        .as_ref()
        .is_some_and(|key| key.encoded_len() > limits.max_key_bytes)
    {
        return Err(UiRootDiagnostic::KeyRejected {
            message: format!("key exceeds {} encoded bytes", limits.max_key_bytes),
        });
    }
    let requires_key = node.interaction.hit_testable
        || node.interaction.focusable
        || node.semantics.is_some()
        || node.text.is_some();
    if requires_key && key.is_none() {
        return Err(UiRootDiagnostic::MissingRequiredKey {
            structural_index: nodes.len(),
        });
    }
    if let Some(key) = &key
        && !keys.insert(key.clone())
    {
        return Err(UiRootDiagnostic::DuplicateNodeKey { key: key.clone() });
    }

    let index = nodes.len();
    let establishes_semantics = node.semantics.is_some() || (node.text.is_some() && key.is_some());
    let child_semantic_parent = if establishes_semantics {
        key.clone()
    } else {
        semantic_parent.clone()
    };
    let child_keyed_parent = key.clone().or_else(|| keyed_parent.clone());
    let mut child_stacking = stacking_ancestors.clone();
    if node.style.stacking_context {
        child_stacking.push((node.style.z_index, index));
    }
    nodes.push(FlatNode {
        node,
        parent,
        children: Vec::new(),
        key,
        structural_index: index,
        stacking_ancestors,
        semantic_parent,
        keyed_parent,
    });
    for child in &node.children {
        let child_index = visit_node(
            child,
            root_key,
            None,
            Some(index),
            depth + 1,
            child_stacking.clone(),
            child_semantic_parent.clone(),
            child_keyed_parent.clone(),
            nodes,
            keys,
            limits,
        )?;
        nodes[index].children.push(child_index);
    }
    Ok(index)
}

fn root_order(node: &FlatNode<'_>, fragment_index: u8) -> RootOrder {
    let mut stacking_path = node.stacking_ancestors.clone();
    stacking_path.push((node.node.style.z_index, node.structural_index));
    RootOrder::new(stacking_path, fragment_index)
}

pub(super) fn taffy_style(style: UiStyle, is_root: bool, environment: UiEnvironment) -> TaffyStyle {
    let width = if is_root && style.width == UiDimension::Auto {
        Dimension::length(environment.logical_size().width())
    } else {
        dimension(style.width)
    };
    let height = if is_root && style.height == UiDimension::Auto {
        Dimension::length(environment.logical_size().height())
    } else {
        dimension(style.height)
    };
    let overflow = match style.overflow {
        UiOverflow::Visible => Overflow::Visible,
        UiOverflow::Clip => Overflow::Clip,
    };
    TaffyStyle {
        display: if style.visible {
            Display::Flex
        } else {
            Display::None
        },
        position: match style.position_mode {
            UiPositionMode::Flow => Position::Relative,
            UiPositionMode::Absolute => Position::Absolute,
        },
        inset: TaffyRect {
            left: inset(style.left),
            right: LengthPercentageAuto::auto(),
            top: inset(style.top),
            bottom: LengthPercentageAuto::auto(),
        },
        size: TaffySize { width, height },
        padding: TaffyRect {
            left: LengthPercentage::length(style.padding.left()),
            right: LengthPercentage::length(style.padding.right()),
            top: LengthPercentage::length(style.padding.top()),
            bottom: LengthPercentage::length(style.padding.bottom()),
        },
        gap: TaffySize {
            width: LengthPercentage::length(style.gap),
            height: LengthPercentage::length(style.gap),
        },
        align_items: Some(match style.align_items {
            UiAlign::Start => AlignItems::Start,
            UiAlign::Center => AlignItems::Center,
            UiAlign::End => AlignItems::End,
            UiAlign::Stretch => AlignItems::Stretch,
        }),
        justify_content: Some(match style.justify_content {
            UiJustify::Start => JustifyContent::Start,
            UiJustify::Center => JustifyContent::Center,
            UiJustify::End => JustifyContent::End,
            UiJustify::SpaceBetween => JustifyContent::SpaceBetween,
        }),
        flex_direction: match style.flex_direction {
            UiFlexDirection::Row => FlexDirection::Row,
            UiFlexDirection::Column => FlexDirection::Column,
        },
        flex_grow: style.flex_grow,
        flex_shrink: style.flex_shrink,
        overflow: TaffyPoint {
            x: overflow,
            y: overflow,
        },
        ..Default::default()
    }
}

fn dimension(value: UiDimension) -> Dimension {
    match value {
        UiDimension::Auto => Dimension::auto(),
        UiDimension::Px(value) => Dimension::length(value),
        UiDimension::Percent(value) => Dimension::percent(value),
    }
}

fn inset(value: UiDimension) -> LengthPercentageAuto {
    match value {
        UiDimension::Auto => LengthPercentageAuto::auto(),
        UiDimension::Px(value) => LengthPercentageAuto::length(value),
        UiDimension::Percent(value) => LengthPercentageAuto::percent(value),
    }
}

fn semantic_enabled(node: &UiNode) -> bool {
    if node.interaction.hit_testable || node.interaction.focusable {
        node.interaction.enabled
    } else {
        true
    }
}

#[derive(Clone, Debug)]
pub(super) struct FlatNode<'a> {
    pub(super) node: &'a UiNode,
    parent: Option<usize>,
    pub(super) children: Vec<usize>,
    key: Option<UiKey>,
    structural_index: usize,
    stacking_ancestors: Vec<(i32, usize)>,
    semantic_parent: Option<UiKey>,
    keyed_parent: Option<UiKey>,
}

#[derive(Clone, Copy, Debug)]
enum ClipState {
    Unclipped,
    Rect(UiRect),
    Empty,
}

impl ClipState {
    fn intersect(self, rect: UiRect) -> Self {
        match self {
            Self::Unclipped => Self::Rect(rect),
            Self::Rect(clip) => clip.intersect(rect).map_or(Self::Empty, Self::Rect),
            Self::Empty => Self::Empty,
        }
    }

    const fn rect(self) -> Option<UiRect> {
        match self {
            Self::Rect(rect) => Some(rect),
            Self::Unclipped | Self::Empty => None,
        }
    }
}
