//! Compile-local access to Taffy's layout algorithms over the admitted root description.

use std::panic::{AssertUnwindSafe, catch_unwind};

use nara_text::TextEngine;
use taffy::{
    AvailableSpace, Cache, CacheTree, Display, Layout, LayoutFlexboxContainer, LayoutInput,
    LayoutOutput, LayoutPartialTree, NodeId, Point, Position, RunMode, Size, Style,
    TraversePartialTree, TraverseTree, compute_cached_layout, compute_flexbox_layout,
    compute_hidden_layout, compute_leaf_layout, compute_root_layout, util::ResolveOrZero,
};

use crate::{
    UiAvailableSpace, UiCustomLayout, UiEnvironment, UiLayoutChildren, UiLayoutConstraints,
    UiLayoutError, UiLayoutMeasurement, UiLayoutOutput, UiRect, UiRootDiagnostic, UiSize,
    UiTextOverflow, authoring::UiNodeText, layout::UiLayoutBackend,
};

use super::{
    LayoutWorkBudget, UiCompileWorkStats,
    root::{FlatNode, RootCompileError, taffy_style, text_compile_error},
    text::{TextCompileError, TextCompileSession},
};

pub(super) fn compute_layout(
    flat: &[FlatNode<'_>],
    text_engine: &mut TextEngine,
    work_stats: &mut UiCompileWorkStats,
    environment: UiEnvironment,
    text_session: &mut TextCompileSession,
    layout_work: &mut LayoutWorkBudget,
) -> Result<Vec<Layout>, RootCompileError> {
    let mut nodes = flat
        .iter()
        .enumerate()
        .map(|(index, node)| {
            work_stats.layout_nodes_created = work_stats.layout_nodes_created.saturating_add(1);
            LayoutNode {
                style: taffy_style(node.node.style, index == 0, environment),
                children: node.children.clone(),
                cache: Cache::new(),
                cache_width_probe: false,
                layout: Layout::new(),
                subtree_has_custom_layout: node.node.custom_layout.is_some(),
                first_text_baseline: None,
            }
        })
        .collect::<Vec<_>>();
    // Flattening places every descendant after its parent, so propagate this once bottom-up.
    for index in (0..nodes.len()).rev() {
        let subtree_has_custom_layout = nodes[index].subtree_has_custom_layout
            || nodes[index]
                .children
                .iter()
                .any(|child| nodes[*child].subtree_has_custom_layout);
        nodes[index].subtree_has_custom_layout = subtree_has_custom_layout;
    }
    let mut tree = LayoutTree {
        flat,
        nodes,
        text_engine,
        work_stats,
        environment,
        text_session,
        layout_work,
        width_probe: false,
        error: None,
    };
    tree.work_stats.layout_passes = tree.work_stats.layout_passes.saturating_add(1);
    compute_root_layout(
        &mut tree,
        NodeId::from(0usize),
        Size {
            width: AvailableSpace::Definite(environment.logical_size().width()),
            height: AvailableSpace::Definite(environment.logical_size().height()),
        },
    );
    if let Some(error) = tree.error {
        return Err(error);
    }
    Ok(tree.nodes.into_iter().map(|node| node.layout).collect())
}

struct LayoutNode {
    style: Style,
    children: Vec<usize>,
    cache: Cache,
    cache_width_probe: bool,
    layout: Layout,
    subtree_has_custom_layout: bool,
    // Genuine source/custom baseline, distinct from Taffy's synthesized box fallback.
    first_text_baseline: Option<f32>,
}

struct LayoutTree<'flat, 'node, 'work> {
    flat: &'flat [FlatNode<'node>],
    nodes: Vec<LayoutNode>,
    text_engine: &'work mut TextEngine,
    work_stats: &'work mut UiCompileWorkStats,
    environment: UiEnvironment,
    text_session: &'work mut TextCompileSession,
    layout_work: &'work mut LayoutWorkBudget,
    width_probe: bool,
    error: Option<RootCompileError>,
}

impl LayoutTree<'_, '_, '_> {
    fn charge(&mut self) -> Result<(), UiLayoutError> {
        // Charge every reached request, including cache hits and dispatch after an error.
        // Retain the first global error. A later reached global overflow must still supersede
        // a local rejection, including when the callback keeps requesting work after an error.
        if let Err(error) = self.layout_work.charge(self.work_stats)
            && self
                .error
                .as_ref()
                .is_none_or(|error| matches!(error, RootCompileError::Local(_)))
        {
            self.error = Some(error);
        }
        if self.error.is_some() {
            Err(UiLayoutError::Aborted)
        } else {
            Ok(())
        }
    }

    fn reject(&mut self, error: UiLayoutError) -> UiLayoutError {
        if self.error.is_some() {
            return UiLayoutError::Aborted;
        }
        let error = match error {
            UiLayoutError::Rejected { message } => UiLayoutError::Rejected {
                message: bounded_message(&message),
            },
            error => error,
        };
        self.error = Some(RootCompileError::Local(UiRootDiagnostic::LayoutRejected {
            message: bounded_message(&error.to_string()),
        }));
        error
    }

    fn leaf_layout(&mut self, index: usize, inputs: LayoutInput) -> LayoutOutput {
        // The style must remain available while the measure closure recursively borrows the tree.
        let style = self.nodes[index].style.clone();
        let custom = self.flat[index].node.custom_layout.clone();
        let mut baseline = None;
        let mut output = compute_leaf_layout(
            inputs,
            &style,
            |_, _| 0.0,
            |_, available| {
                if let Some(custom) = custom {
                    match self.custom_layout(index, custom, available) {
                        Ok(output) => {
                            baseline = output.first_baseline();
                            Size {
                                width: output.size().width(),
                                height: output.size().height(),
                            }
                        }
                        Err(_) => Size::ZERO,
                    }
                } else {
                    self.measure_text(index, available, &mut baseline)
                }
            },
        );
        if let Some(baseline) = baseline {
            let baseline = baseline + self.flat[index].node.style.padding.top();
            if baseline.is_finite() {
                output.first_baselines.y = Some(baseline);
            } else {
                self.reject(UiLayoutError::InvalidGeometry);
            }
        }
        output
    }

    fn measure_text(
        &mut self,
        index: usize,
        available: Size<AvailableSpace>,
        baseline: &mut Option<f32>,
    ) -> Size<f32> {
        self.work_stats.measurement_callbacks =
            self.work_stats.measurement_callbacks.saturating_add(1);
        let text = match &self.flat[index].node.text {
            Some(UiNodeText::Plain(text)) => text,
            Some(UiNodeText::Prepared(text)) => {
                *baseline = text.snapshot.first_baseline();
                return Size {
                    width: text.snapshot.width(),
                    height: text.snapshot.height(),
                };
            }
            None => return Size::ZERO,
        };
        // Available width is already content space. Keep the old positive-definite wrap rule.
        let width = match available.width {
            AvailableSpace::Definite(width) if width.is_finite() && width > 0.0 => Some(width),
            _ => None,
        };
        match text
            .request(width, self.environment.scale_factor())
            .map(|request| {
                if available.width == AvailableSpace::MinContent
                    && text.overflow == UiTextOverflow::Wrap
                {
                    request.with_min_content()
                } else {
                    request
                }
            })
            .map_err(TextCompileError::Layout)
            .and_then(|request| {
                if self.width_probe && text.overflow == UiTextOverflow::Wrap {
                    self.text_session
                        .measure_for_width(self.text_engine, request)
                        .map(|measured| {
                            (
                                measured.width(),
                                measured.height(),
                                measured.first_baseline(),
                            )
                        })
                } else {
                    self.text_session
                        .layout(self.text_engine, request)
                        .map(|layout| {
                            (
                                layout.width(),
                                layout.height(),
                                layout.lines().first().map(|line| line.baseline()),
                            )
                        })
                }
            }) {
            Ok((width, height, first_baseline)) => {
                *baseline = first_baseline;
                Size { width, height }
            }
            Err(error) => {
                self.error.get_or_insert_with(|| text_compile_error(error));
                Size::ZERO
            }
        }
    }

    fn custom_layout(
        &mut self,
        index: usize,
        custom: UiCustomLayout,
        available: Size<AvailableSpace>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        let constraints = UiLayoutConstraints {
            width: public_space(available.width),
            height: public_space(available.height),
        };
        constraints.validate().map_err(|error| self.reject(error))?;
        self.charge()?;
        self.work_stats.custom_layout_callbacks =
            self.work_stats.custom_layout_callbacks.saturating_add(1);
        let mut backend = CustomChildren {
            completed: vec![false; self.nodes[index].children.len()],
            tree: self,
            parent: index,
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            custom.run(constraints, &mut UiLayoutChildren::new(&mut backend))
        }));
        let output = match result {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => return Err(backend.tree.reject(error)),
            Err(payload) => {
                let message = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("custom layout callback panicked with a non-string payload");
                return Err(backend.tree.reject(UiLayoutError::Rejected {
                    message: format!(
                        "custom layout callback panicked: {}",
                        bounded_message(message)
                    ),
                }));
            }
        };
        backend.finish()?;
        Ok(output)
    }

    fn independent_layout(
        &mut self,
        index: usize,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        location: Point<f32>,
    ) -> Result<LayoutOutput, UiLayoutError> {
        // Independent requests may change percentage bases or replace a prior measurement's
        // soft constraints with an exact allocation. Do not reuse that root's final layout.
        self.nodes[index].cache.clear();
        let output = self.compute_child_layout(
            NodeId::from(index),
            LayoutInput {
                run_mode: RunMode::PerformLayout,
                known_dimensions,
                parent_size: available_space.into_options(),
                available_space,
                ..LayoutInput::HIDDEN
            },
        );
        if self.error.is_some() {
            return Err(UiLayoutError::Aborted);
        }
        let style = &self.nodes[index].style;
        let basis = available_space.width.into_option();
        self.nodes[index].layout = Layout {
            location,
            size: output.size,
            content_size: output.content_size,
            padding: style.padding.resolve_or_zero(basis, |_, _| 0.0),
            border: style.border.resolve_or_zero(basis, |_, _| 0.0),
            margin: style.margin.resolve_or_zero(basis, |_, _| 0.0),
            ..Layout::new()
        };
        Ok(output)
    }
}

struct CustomChildren<'tree, 'flat, 'node, 'work> {
    tree: &'tree mut LayoutTree<'flat, 'node, 'work>,
    parent: usize,
    completed: Vec<bool>,
}

impl CustomChildren<'_, '_, '_, '_> {
    fn child(&mut self, child: usize) -> Result<usize, UiLayoutError> {
        self.tree.nodes[self.parent]
            .children
            .get(child)
            .copied()
            .ok_or_else(|| {
                self.tree.reject(UiLayoutError::InvalidChild {
                    child,
                    children: self.completed.len(),
                })
            })
    }

    fn unplaced_child(&mut self, child: usize) -> Result<usize, UiLayoutError> {
        let index = self.child(child)?;
        if self.completed[child] {
            return Err(self
                .tree
                .reject(UiLayoutError::DuplicatePlacement { child }));
        }
        Ok(index)
    }

    fn place_subtree(&mut self, child: usize, rect: UiRect) -> Result<(), UiLayoutError> {
        let padding = self.tree.flat[self.parent].node.style.padding;
        let location = Point {
            x: padding.left() + rect.origin().x(),
            y: padding.top() + rect.origin().y(),
        };
        if ![
            rect.max().x(),
            rect.max().y(),
            location.x,
            location.y,
            location.x + rect.size().width(),
            location.y + rect.size().height(),
        ]
        .into_iter()
        .all(f32::is_finite)
        {
            return Err(self.tree.reject(UiLayoutError::InvalidGeometry));
        }
        let size = Size {
            width: rect.size().width(),
            height: rect.size().height(),
        };
        let index = self.tree.nodes[self.parent].children[child];
        let child_padding = self.tree.flat[index].node.style.padding;
        // Taffy's Flex main axis can keep an exact known outer size smaller than its insets.
        // Reject that impossible allocation consistently with leaf and cross-axis behavior.
        if size.width < child_padding.left() + child_padding.right()
            || size.height < child_padding.top() + child_padding.bottom()
        {
            return Err(self
                .tree
                .reject(UiLayoutError::PlacementSizeMismatch { child }));
        }
        let output = self.tree.independent_layout(
            index,
            size.map(Some),
            size.map(AvailableSpace::Definite),
            location,
        )?;
        if output.size != size {
            return Err(self
                .tree
                .reject(UiLayoutError::PlacementSizeMismatch { child }));
        }
        Ok(())
    }

    fn hide_subtree(&mut self, child: usize) -> Result<(), UiLayoutError> {
        let index = self.tree.nodes[self.parent].children[child];
        self.tree
            .compute_child_layout(NodeId::from(index), LayoutInput::HIDDEN);
        if self.tree.error.is_some() {
            Err(UiLayoutError::Aborted)
        } else {
            Ok(())
        }
    }

    fn finish(&mut self) -> Result<(), UiLayoutError> {
        if self.tree.error.is_some() {
            return Err(UiLayoutError::Aborted);
        }
        for child in 0..self.completed.len() {
            if !self.completed[child] {
                return Err(self.tree.reject(UiLayoutError::MissingPlacement { child }));
            }
        }
        Ok(())
    }
}

impl UiLayoutBackend for CustomChildren<'_, '_, '_, '_> {
    fn len(&self) -> usize {
        self.completed.len()
    }

    fn min_content_width(&mut self, child: usize) -> Result<f32, UiLayoutError> {
        self.tree.work_stats.custom_child_measurements = self
            .tree
            .work_stats
            .custom_child_measurements
            .saturating_add(1);
        self.tree.charge()?;
        let index = self.unplaced_child(child)?;
        let previous_probe = self.tree.width_probe;
        self.tree.width_probe = true;
        let result = self.tree.independent_layout(
            index,
            Size::NONE,
            Size {
                width: AvailableSpace::MinContent,
                height: AvailableSpace::MaxContent,
            },
            Point::ZERO,
        );
        self.tree.width_probe = previous_probe;
        let output = result?;
        UiSize::new(output.size.width, output.size.height)
            .map(|size| size.width())
            .ok_or_else(|| self.tree.reject(UiLayoutError::InvalidGeometry))
    }

    fn measure(
        &mut self,
        child: usize,
        constraints: UiLayoutConstraints,
    ) -> Result<UiLayoutMeasurement, UiLayoutError> {
        self.tree.work_stats.custom_child_measurements = self
            .tree
            .work_stats
            .custom_child_measurements
            .saturating_add(1);
        self.tree.charge()?;
        let index = self.unplaced_child(child)?;
        constraints
            .validate()
            .map_err(|error| self.tree.reject(error))?;
        let output = self.tree.independent_layout(
            index,
            Size::NONE,
            Size {
                width: private_space(constraints.width),
                height: private_space(constraints.height),
            },
            Point::ZERO,
        )?;
        let size = UiSize::new(output.size.width, output.size.height)
            .ok_or_else(|| self.tree.reject(UiLayoutError::InvalidGeometry))?;
        UiLayoutMeasurement::new(size, self.tree.nodes[index].first_text_baseline)
            .map_err(|error| self.tree.reject(error))
    }

    fn place(&mut self, child: usize, rect: UiRect) -> Result<(), UiLayoutError> {
        self.tree.work_stats.custom_child_placements = self
            .tree
            .work_stats
            .custom_child_placements
            .saturating_add(1);
        self.tree.charge()?;
        self.unplaced_child(child)?;
        self.place_subtree(child, rect)?;
        self.completed[child] = true;
        Ok(())
    }

    fn hide(&mut self, child: usize) -> Result<(), UiLayoutError> {
        self.tree.work_stats.custom_child_placements = self
            .tree
            .work_stats
            .custom_child_placements
            .saturating_add(1);
        self.tree.charge()?;
        self.unplaced_child(child)?;
        self.hide_subtree(child)?;
        self.completed[child] = true;
        Ok(())
    }
}

struct ChildIter<'a>(std::slice::Iter<'a, usize>);

impl Iterator for ChildIter<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().copied().map(NodeId::from)
    }
}

impl TraversePartialTree for LayoutTree<'_, '_, '_> {
    type ChildIter<'a>
        = ChildIter<'a>
    where
        Self: 'a;

    fn child_ids(&self, node_id: NodeId) -> Self::ChildIter<'_> {
        ChildIter(self.nodes[usize::from(node_id)].children.iter())
    }

    fn child_count(&self, node_id: NodeId) -> usize {
        self.nodes[usize::from(node_id)].children.len()
    }

    fn get_child_id(&self, node_id: NodeId, index: usize) -> NodeId {
        NodeId::from(self.nodes[usize::from(node_id)].children[index])
    }
}

impl TraverseTree for LayoutTree<'_, '_, '_> {}

impl LayoutPartialTree for LayoutTree<'_, '_, '_> {
    type CustomIdent = String;
    type CoreContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_core_container_style(&self, node_id: NodeId) -> Self::CoreContainerStyle<'_> {
        &self.nodes[usize::from(node_id)].style
    }

    fn set_unrounded_layout(&mut self, node_id: NodeId, layout: &Layout) {
        self.nodes[usize::from(node_id)].layout = *layout;
    }

    fn compute_child_layout(&mut self, node_id: NodeId, inputs: LayoutInput) -> LayoutOutput {
        if self.charge().is_err() {
            return LayoutOutput::HIDDEN;
        }
        let node = &mut self.nodes[usize::from(node_id)];
        if node.cache_width_probe != self.width_probe {
            node.cache.clear();
            node.cache_width_probe = self.width_probe;
        }
        if inputs.run_mode == RunMode::PerformHiddenLayout {
            self.nodes[usize::from(node_id)].first_text_baseline = None;
            return compute_hidden_layout(self, node_id);
        }
        let output = compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            let index = usize::from(node_id);
            if tree.nodes[index].style.display == Display::None {
                compute_hidden_layout(tree, node_id)
            } else if tree.flat[index].node.custom_layout.is_some()
                || tree.nodes[index].children.is_empty()
            {
                tree.leaf_layout(index, inputs)
            } else {
                compute_flexbox_layout(tree, node_id, inputs)
            }
        });
        if inputs.run_mode == RunMode::PerformLayout {
            let index = usize::from(node_id);
            let baseline = if self.nodes[index].style.display == Display::None {
                None
            } else if self.flat[index].node.custom_layout.is_some()
                || (self.nodes[index].children.is_empty() && self.flat[index].node.text.is_some())
            {
                output.first_baselines.y
            } else {
                // Public text-baseline queries use the first visible in-flow descendant in
                // authored order. Preserve Taffy's box fallback privately for its algorithms.
                self.nodes[index].children.iter().find_map(|child| {
                    let node = &self.nodes[*child];
                    if node.style.display == Display::None
                        || node.style.position == Position::Absolute
                    {
                        return None;
                    }
                    node.first_text_baseline
                        .map(|baseline| baseline + node.layout.location.y)
                })
            };
            if baseline.is_some_and(|value| !value.is_finite()) {
                self.reject(UiLayoutError::InvalidGeometry);
            }
            self.nodes[index].first_text_baseline = baseline;
        }
        output
    }
}

impl CacheTree for LayoutTree<'_, '_, '_> {
    fn cache_get(&self, node_id: NodeId, inputs: &LayoutInput) -> Option<LayoutOutput> {
        let index = usize::from(node_id);
        // Exploratory layout can mutate nested custom placements. Ancestor flex caches must
        // not bypass re-establishing them during the final pass, even at the same outer size.
        if inputs.run_mode == RunMode::PerformLayout && self.nodes[index].subtree_has_custom_layout
        {
            return None;
        }
        self.nodes[index].cache.get(inputs)
    }

    fn cache_store(&mut self, node_id: NodeId, inputs: &LayoutInput, output: LayoutOutput) {
        if self.error.is_none() {
            self.nodes[usize::from(node_id)].cache.store(inputs, output);
        }
    }

    fn cache_clear(&mut self, node_id: NodeId) {
        self.nodes[usize::from(node_id)].cache.clear();
    }
}

impl LayoutFlexboxContainer for LayoutTree<'_, '_, '_> {
    type FlexboxContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type FlexboxItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_flexbox_container_style(&self, node_id: NodeId) -> Self::FlexboxContainerStyle<'_> {
        &self.nodes[usize::from(node_id)].style
    }

    fn get_flexbox_child_style(&self, node_id: NodeId) -> Self::FlexboxItemStyle<'_> {
        &self.nodes[usize::from(node_id)].style
    }
}

fn public_space(space: AvailableSpace) -> UiAvailableSpace {
    match space {
        AvailableSpace::Definite(value) => UiAvailableSpace::Definite(if value.is_finite() {
            value.max(0.0)
        } else {
            value
        }),
        AvailableSpace::MinContent => UiAvailableSpace::MinContent,
        AvailableSpace::MaxContent => UiAvailableSpace::MaxContent,
    }
}

fn private_space(space: UiAvailableSpace) -> AvailableSpace {
    match space {
        UiAvailableSpace::Definite(value) => AvailableSpace::Definite(value),
        UiAvailableSpace::MinContent => AvailableSpace::MinContent,
        UiAvailableSpace::MaxContent => AvailableSpace::MaxContent,
    }
}

fn bounded_message(message: &str) -> String {
    let mut end = message.len().min(1024);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_owned()
}
