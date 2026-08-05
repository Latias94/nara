use std::{collections::BTreeSet, sync::Arc};

use nara_text::{FontFace, TextEngine, TextEngineCreateError, TextLimits};
use taffy::{
    AlignItems, AvailableSpace, Dimension, Display, FlexDirection, JustifyContent,
    LengthPercentage, LengthPercentageAuto, Overflow, Point as TaffyPoint, Position,
    Rect as TaffyRect, Size as TaffySize, Style as TaffyStyle, TaffyTree,
};
use thiserror::Error;

use crate::{
    UiAlign, UiAuthoringSnapshot, UiCompileError::FrameRevisionExhausted, UiDiagnostic,
    UiDimension, UiEnvironment, UiFlexDirection, UiFocusFact, UiFrame, UiFrameRevision,
    UiHitRegion, UiJustify, UiKey, UiLayer, UiLayoutFact, UiLimits, UiLimitsError, UiNode,
    UiOverflow, UiPaintPrimitive, UiPoint, UiPositionMode, UiRect, UiResolvedBorder, UiRoot,
    UiRootDiagnostic, UiRootKey, UiRootOutcome, UiRootStatus, UiSemanticNode, UiSemanticRole,
    UiStyle, UiTextOverflow, UiTheme, authoring::duplicate_root_keys,
};

/// Stateful owner of text measurement, bounded last-good roots, and frame revisions.
pub struct UiCompiler {
    text_engine: TextEngine,
    limits: UiLimits,
    next_frame_revision: u64,
    cache_clock: u64,
    cache: Vec<CachedRoot>,
}

impl UiCompiler {
    pub fn new(font_face: FontFace, limits: UiLimits) -> Result<Self, UiCompileError> {
        Self::with_text_limits(font_face, limits, TextLimits::default())
    }

    pub fn with_text_limits(
        font_face: FontFace,
        limits: UiLimits,
        text_limits: TextLimits,
    ) -> Result<Self, UiCompileError> {
        limits.validate()?;
        Ok(Self {
            text_engine: TextEngine::new(font_face, text_limits)?,
            limits,
            next_frame_revision: 1,
            cache_clock: 0,
            cache: Vec::new(),
        })
    }

    /// Compiles all roots independently and publishes one immutable cross-projection frame.
    pub fn compile(
        &mut self,
        snapshot: &UiAuthoringSnapshot,
        environment: UiEnvironment,
        theme: UiTheme,
    ) -> Result<Arc<UiFrame>, UiCompileError> {
        if environment.theme_revision() != theme.revision() {
            return Err(UiCompileError::ThemeRevisionMismatch {
                environment: environment.theme_revision().get(),
                theme: theme.revision().get(),
            });
        }
        if snapshot.roots.len() > self.limits.max_roots {
            return Err(UiCompileError::SnapshotRootLimitExceeded {
                actual: snapshot.roots.len(),
                maximum: self.limits.max_roots,
            });
        }
        let revision = UiFrameRevision::new(self.next_frame_revision);
        self.next_frame_revision = self
            .next_frame_revision
            .checked_add(1)
            .ok_or(FrameRevisionExhausted)?;
        self.cache_clock = self
            .cache_clock
            .checked_add(1)
            .ok_or(UiCompileError::CacheRevisionExhausted)?;

        let duplicate_roots = duplicate_root_keys(&snapshot.roots);
        let authored_keys = snapshot
            .roots
            .iter()
            .map(|root| root.key.clone())
            .collect::<BTreeSet<_>>();
        self.cache
            .retain(|entry| authored_keys.contains(&entry.key));

        let mut outcomes = Vec::with_capacity(snapshot.roots.len());
        let mut included_fallbacks = BTreeSet::new();
        let mut compiled_roots = Vec::with_capacity(snapshot.roots.len());
        let mut remaining_paint = self.limits.max_paint_primitives;

        for (root_sequence, root) in snapshot.roots.iter().enumerate() {
            let forced_error = duplicate_roots
                .contains(&root.key)
                .then_some(UiRootDiagnostic::DuplicateRootKey);
            let candidate = match forced_error {
                Some(error) => Err(error),
                None => self.compile_root(root, root_sequence, environment, theme, remaining_paint),
            };

            match candidate {
                Ok(compiled) => {
                    remaining_paint = remaining_paint.saturating_sub(compiled.paint.len());
                    let compiled = Arc::new(compiled);
                    self.store_last_good(root.key.clone(), environment, Arc::clone(&compiled));
                    outcomes.push(UiRootOutcome {
                        key: root.key.clone(),
                        status: UiRootStatus::Published,
                        diagnostic: None,
                    });
                    compiled_roots.push((compiled, true));
                }
                Err(diagnostic) => {
                    let fallback = if included_fallbacks.insert(root.key.clone()) {
                        self.compatible_last_good(&root.key, environment)
                            .filter(|root| root.paint.len() <= remaining_paint)
                    } else {
                        None
                    };
                    let status = if let Some(fallback) = fallback {
                        remaining_paint = remaining_paint.saturating_sub(fallback.paint.len());
                        compiled_roots.push((fallback, false));
                        UiRootStatus::RetainedVisual
                    } else {
                        UiRootStatus::Absent
                    };
                    outcomes.push(UiRootOutcome {
                        key: root.key.clone(),
                        status,
                        diagnostic: Some(diagnostic),
                    });
                }
            }
        }

        let mut layouts = Vec::new();
        let mut paint = Vec::new();
        let mut hits = Vec::new();
        let mut focus = Vec::new();
        let mut semantics = Vec::new();
        for (root, interactive) in compiled_roots {
            layouts.extend(root.layouts.iter().cloned());
            paint.extend(root.paint.iter().cloned());
            if interactive {
                hits.extend(root.hits.iter().cloned());
                focus.extend(root.focus.iter().cloned());
                semantics.extend(root.semantics.iter().cloned());
            }
        }
        layouts.sort_by(|left, right| left.order.cmp(&right.order));
        paint.sort_by(|left, right| left.order.cmp(&right.order));
        hits.sort_by(|left, right| right.order.cmp(&left.order));
        focus.sort_by(|left, right| left.order.cmp(&right.order));
        semantics.sort_by(|left, right| left.order.cmp(&right.order));

        let diagnostics = bounded_diagnostics(&outcomes, self.limits.max_diagnostics);
        let frame = UiFrame {
            revision,
            source_revision: snapshot.source_revision,
            environment,
            root_outcomes: outcomes,
            layouts: layouts.into_iter().map(|entry| entry.value).collect(),
            paint: paint.into_iter().map(|entry| entry.value).collect(),
            hits: hits.into_iter().map(|entry| entry.value).collect(),
            focus: focus.into_iter().map(|entry| entry.value).collect(),
            semantics: semantics.into_iter().map(|entry| entry.value).collect(),
            diagnostics,
        };
        Ok(Arc::new(frame))
    }

    fn compile_root(
        &mut self,
        root: &UiRoot,
        root_sequence: usize,
        environment: UiEnvironment,
        theme: UiTheme,
        remaining_paint: usize,
    ) -> Result<CompiledRoot, UiRootDiagnostic> {
        let flat = flatten_root(root, self.limits)?;
        let mut tree = TaffyTree::<usize>::with_capacity(flat.len());
        tree.disable_rounding();
        let mut node_ids = vec![None; flat.len()];
        for index in (0..flat.len()).rev() {
            let children = flat[index]
                .children
                .iter()
                .map(|child| node_ids[*child].expect("child node is created before its parent"))
                .collect::<Vec<_>>();
            let style = taffy_style(flat[index].node.style, index == 0, environment);
            let id = if children.is_empty() && flat[index].node.text.is_some() {
                tree.new_leaf_with_context(style, index)
            } else if children.is_empty() {
                tree.new_leaf(style)
            } else {
                tree.new_with_children(style, &children)
            }
            .map_err(|error| UiRootDiagnostic::LayoutRejected {
                message: error.to_string(),
            })?;
            node_ids[index] = Some(id);
        }

        let root_id = node_ids[0].expect("a flattened root always has one Taffy node");
        let mut measurement_error = None;
        let scale_factor = environment.scale_factor();
        tree.compute_layout_with_measure(
            root_id,
            TaffySize {
                width: AvailableSpace::Definite(environment.logical_size().width()),
                height: AvailableSpace::Definite(environment.logical_size().height()),
            },
            |known, available, _node_id, context, _style| {
                let Some(index) = context.map(|index| *index) else {
                    return TaffySize::ZERO;
                };
                let Some(text) = flat[index].node.text.as_ref() else {
                    return TaffySize::ZERO;
                };
                let wrap_width = known.width.or_else(|| definite(available.width));
                match text
                    .request(wrap_width, scale_factor)
                    .and_then(|request| self.text_engine.layout(&request))
                {
                    Ok(layout) => TaffySize {
                        width: known.width.unwrap_or_else(|| layout.width()),
                        height: known.height.unwrap_or_else(|| layout.height()),
                    },
                    Err(error) => {
                        measurement_error.get_or_insert_with(|| error.to_string());
                        TaffySize::ZERO
                    }
                }
            },
        )
        .map_err(|error| UiRootDiagnostic::LayoutRejected {
            message: error.to_string(),
        })?;
        if let Some(message) = measurement_error {
            return Err(UiRootDiagnostic::TextRejected { message });
        }

        let mut layout_entries = Vec::with_capacity(flat.len());
        let mut paint_entries = Vec::new();
        let mut hit_entries = Vec::new();
        let mut focus_entries = Vec::new();
        let mut semantic_entries = Vec::new();
        let mut origins = vec![UiPoint::new(0.0, 0.0); flat.len()];
        let mut clips = vec![ClipState::Unclipped; flat.len()];
        let mut visible = vec![false; flat.len()];

        for (index, node) in flat.iter().enumerate() {
            let layout = tree
                .layout(node_ids[index].expect("Taffy node exists for each flat node"))
                .map_err(|error| UiRootDiagnostic::LayoutRejected {
                    message: error.to_string(),
                })?;
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
            .ok_or_else(|| UiRootDiagnostic::LayoutRejected {
                message: "Taffy produced non-finite or negative geometry".to_owned(),
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
            let node_visible = parent_visible
                && node.node.style.visible
                && !rect.size().is_empty()
                && intersects_clip;
            visible[index] = node_visible;
            clips[index] = if node.node.style.overflow == UiOverflow::Clip {
                inherited_clip.intersect(rect)
            } else {
                inherited_clip
            };

            let order = PaintOrder::for_node(root.layer, root_sequence, node, 0);
            let layout_fact = UiLayoutFact {
                key: node.key.clone(),
                root_key: root.key.clone(),
                layer: root.layer,
                structural_index: node.structural_index,
                local_z: node.node.style.z_index,
                rect,
                physical_rect: environment.snap(rect),
                clip_rect: inherited_clip.rect(),
                visible: node_visible,
            };
            layout_entries.push(Ordered::new(order.clone(), layout_fact));

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
                    paint_entries.push(Ordered::new(
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
                }
            }
            if let Some(text) = &node.node.text {
                let wrap_width = (text.overflow == UiTextOverflow::Wrap)
                    .then_some(rect.size().width())
                    .filter(|width| *width > 0.0);
                let request = text.request(wrap_width, scale_factor).map_err(|error| {
                    UiRootDiagnostic::TextRejected {
                        message: error.to_string(),
                    }
                })?;
                let text_layout = self.text_engine.layout(&request).map_err(|error| {
                    UiRootDiagnostic::TextRejected {
                        message: error.to_string(),
                    }
                })?;
                let text_clip = if text.overflow == UiTextOverflow::Clip {
                    inherited_clip.intersect(rect).rect()
                } else {
                    inherited_clip.rect()
                };
                paint_entries.push(Ordered::new(
                    PaintOrder::for_node(root.layer, root_sequence, node, 1),
                    UiPaintPrimitive::Text {
                        key: node.key.clone(),
                        rect,
                        physical_rect: environment.snap(rect),
                        clip_rect: text_clip,
                        color: match text.brush.resolve(theme) {
                            crate::UiResolvedBrush::Solid(color) => color,
                            crate::UiResolvedBrush::Image { .. } => {
                                return Err(UiRootDiagnostic::InvalidStyle {
                                    message: "text brushes cannot reference images".to_owned(),
                                });
                            }
                        },
                        layout: Arc::new(text_layout),
                    },
                ));
            }
            if node.node.interaction.hit_testable {
                hit_entries.push(Ordered::new(
                    PaintOrder::for_node(root.layer, root_sequence, node, u8::MAX),
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
                focus_entries.push(Ordered::new(
                    PaintOrder::for_node(root.layer, root_sequence, node, u8::MAX),
                    UiFocusFact {
                        key: node.key.clone().expect("required focus key is validated"),
                        rect,
                        enabled: node.node.interaction.enabled,
                    },
                ));
            }
            if let Some(semantics) = &node.node.semantics {
                semantic_entries.push(Ordered::new(
                    PaintOrder::for_node(root.layer, root_sequence, node, u8::MAX),
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
                        actions: semantics.actions.clone(),
                        rect,
                    },
                ));
            } else if let (Some(text), Some(key)) = (&node.node.text, &node.key) {
                semantic_entries.push(Ordered::new(
                    PaintOrder::for_node(root.layer, root_sequence, node, u8::MAX),
                    UiSemanticNode {
                        key: key.clone(),
                        parent: node.semantic_parent.clone(),
                        role: UiSemanticRole::Text,
                        name: text
                            .semantic_text
                            .clone()
                            .unwrap_or_else(|| text.content.clone()),
                        value: None,
                        enabled: true,
                        actions: Vec::new(),
                        rect,
                    },
                ));
            }
        }

        if paint_entries.len() > remaining_paint {
            return Err(UiRootDiagnostic::PaintLimitExceeded {
                maximum: remaining_paint,
            });
        }
        Ok(CompiledRoot {
            layouts: layout_entries,
            paint: paint_entries,
            hits: hit_entries,
            focus: focus_entries,
            semantics: semantic_entries,
        })
    }

    fn store_last_good(
        &mut self,
        key: UiRootKey,
        environment: UiEnvironment,
        root: Arc<CompiledRoot>,
    ) {
        if let Some(entry) = self.cache.iter_mut().find(|entry| entry.key == key) {
            entry.environment = environment;
            entry.root = root;
            entry.last_used = self.cache_clock;
            return;
        }
        self.cache.push(CachedRoot {
            key,
            environment,
            root,
            last_used: self.cache_clock,
        });
        if self.cache.len() > self.limits.max_cache_roots
            && let Some((oldest, _)) = self
                .cache
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| (entry.last_used, entry.key.clone()))
        {
            self.cache.remove(oldest);
        }
    }

    fn compatible_last_good(
        &mut self,
        key: &UiRootKey,
        environment: UiEnvironment,
    ) -> Option<Arc<CompiledRoot>> {
        let entry = self
            .cache
            .iter_mut()
            .find(|entry| &entry.key == key && entry.environment == environment)?;
        entry.last_used = self.cache_clock;
        Some(Arc::clone(&entry.root))
    }
}

fn flatten_root<'a>(
    root: &'a UiRoot,
    limits: UiLimits,
) -> Result<Vec<FlatNode<'a>>, UiRootDiagnostic> {
    let root_key =
        UiKey::root(root.key.clone()).map_err(|error| UiRootDiagnostic::InvalidStyle {
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
    nodes: &mut Vec<FlatNode<'a>>,
    keys: &mut BTreeSet<UiKey>,
    limits: UiLimits,
) -> Result<usize, UiRootDiagnostic> {
    if nodes.len() == limits.max_nodes {
        return Err(UiRootDiagnostic::NodeLimitExceeded {
            maximum: limits.max_nodes,
        });
    }
    if depth > limits.max_depth {
        return Err(UiRootDiagnostic::LayoutRejected {
            message: format!("root depth exceeds {}", limits.max_depth),
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
            .map_err(|error| UiRootDiagnostic::InvalidStyle {
                message: error.to_string(),
            })?,
    };
    if key
        .as_ref()
        .is_some_and(|key| key.encoded_len() > limits.max_key_bytes)
    {
        return Err(UiRootDiagnostic::InvalidStyle {
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
            nodes,
            keys,
            limits,
        )?;
        nodes[index].children.push(child_index);
    }
    Ok(index)
}

fn taffy_style(style: UiStyle, is_root: bool, environment: UiEnvironment) -> TaffyStyle {
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

fn definite(value: AvailableSpace) -> Option<f32> {
    match value {
        AvailableSpace::Definite(value) if value.is_finite() && value > 0.0 => Some(value),
        _ => None,
    }
}

fn semantic_enabled(node: &UiNode) -> bool {
    if node.interaction.hit_testable || node.interaction.focusable {
        node.interaction.enabled
    } else {
        true
    }
}

fn bounded_diagnostics(outcomes: &[UiRootOutcome], maximum: usize) -> Vec<UiDiagnostic> {
    let mut diagnostics = outcomes
        .iter()
        .filter_map(|outcome| {
            outcome.diagnostic.clone().map(|detail| UiDiagnostic::Root {
                key: outcome.key.clone(),
                detail,
            })
        })
        .collect::<Vec<_>>();
    if diagnostics.len() > maximum {
        let omitted = diagnostics.len() - maximum + 1;
        diagnostics.truncate(maximum - 1);
        diagnostics.push(UiDiagnostic::Truncated { omitted });
    }
    diagnostics
}

#[derive(Clone, Debug)]
struct CachedRoot {
    key: UiRootKey,
    environment: UiEnvironment,
    root: Arc<CompiledRoot>,
    last_used: u64,
}

#[derive(Clone, Debug)]
struct CompiledRoot {
    layouts: Vec<Ordered<UiLayoutFact>>,
    paint: Vec<Ordered<UiPaintPrimitive>>,
    hits: Vec<Ordered<UiHitRegion>>,
    focus: Vec<Ordered<UiFocusFact>>,
    semantics: Vec<Ordered<UiSemanticNode>>,
}

#[derive(Clone, Debug)]
struct FlatNode<'a> {
    node: &'a UiNode,
    parent: Option<usize>,
    children: Vec<usize>,
    key: Option<UiKey>,
    structural_index: usize,
    stacking_ancestors: Vec<(i32, usize)>,
    semantic_parent: Option<UiKey>,
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PaintOrder {
    layer: UiLayer,
    root_sequence: usize,
    stacking_path: Vec<(i32, usize)>,
    fragment_index: u8,
}

impl PaintOrder {
    fn for_node(
        layer: UiLayer,
        root_sequence: usize,
        node: &FlatNode<'_>,
        fragment_index: u8,
    ) -> Self {
        let mut stacking_path = node.stacking_ancestors.clone();
        stacking_path.push((node.node.style.z_index, node.structural_index));
        Self {
            layer,
            root_sequence,
            stacking_path,
            fragment_index,
        }
    }
}

#[derive(Clone, Debug)]
struct Ordered<T> {
    order: PaintOrder,
    value: T,
}

impl<T> Ordered<T> {
    const fn new(order: PaintOrder, value: T) -> Self {
        Self { order, value }
    }
}

#[derive(Debug, Error)]
pub enum UiCompileError {
    #[error(transparent)]
    Limits(#[from] UiLimitsError),
    #[error(transparent)]
    TextEngine(#[from] TextEngineCreateError),
    #[error("UI environment theme revision {environment} does not match theme revision {theme}")]
    ThemeRevisionMismatch { environment: u64, theme: u64 },
    #[error("UI snapshot has {actual} roots; compiler maximum is {maximum}")]
    SnapshotRootLimitExceeded { actual: usize, maximum: usize },
    #[error("UI frame revision is exhausted")]
    FrameRevisionExhausted,
    #[error("UI cache revision is exhausted")]
    CacheRevisionExhausted,
}
