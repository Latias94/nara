use taffy::{
    AvailableSpace, Cache, CacheTree, Display, Layout, LayoutFlexboxContainer, LayoutInput,
    LayoutOutput, LayoutPartialTree, NodeId, RunMode, Size, Style, TraversePartialTree,
    compute_cached_layout, compute_flexbox_layout, compute_hidden_layout, compute_leaf_layout,
    compute_root_layout,
};

use crate::kernel::{UiEnvironment, UiNode, UiTextOverflow, tree::UiNodeText};
use crate::text::TextEngine;

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
    let mut tree = LayoutTree {
        nodes: flat
            .iter()
            .enumerate()
            .map(|(index, node)| LayoutNode {
                node: node.node,
                children: &node.children,
                style: taffy_style(node.node.style, index == 0, environment),
                cache: Cache::new(),
                layout: Layout::new(),
            })
            .collect(),
        text_engine,
        work_stats,
        scale: environment.scale_factor(),
        text_session,
        layout_work,
        error: None,
    };
    tree.work_stats.layout_nodes_created = tree
        .work_stats
        .layout_nodes_created
        .saturating_add(flat.len() as u64);
    tree.work_stats.layout_passes = tree.work_stats.layout_passes.saturating_add(1);
    compute_root_layout(
        &mut tree,
        NodeId::from(0usize),
        Size {
            width: AvailableSpace::Definite(environment.logical_size().width()),
            height: AvailableSpace::Definite(environment.logical_size().height()),
        },
    );
    match tree.error {
        Some(error) => Err(error),
        None => Ok(tree.nodes.into_iter().map(|node| node.layout).collect()),
    }
}

struct LayoutNode<'a> {
    node: &'a UiNode,
    children: &'a [usize],
    style: Style,
    cache: Cache,
    layout: Layout,
}

struct LayoutTree<'tree, 'work> {
    nodes: Vec<LayoutNode<'tree>>,
    text_engine: &'work mut TextEngine,
    work_stats: &'work mut UiCompileWorkStats,
    scale: f32,
    text_session: &'work mut TextCompileSession,
    layout_work: &'work mut LayoutWorkBudget,
    error: Option<RootCompileError>,
}

impl LayoutTree<'_, '_> {
    fn leaf_layout(&mut self, index: usize, inputs: LayoutInput) -> LayoutOutput {
        let style = self.nodes[index].style.clone();
        let mut baseline = None;
        let mut output = compute_leaf_layout(
            inputs,
            &style,
            |_, _| 0.0,
            |_, available| self.measure_text(index, available.width, &mut baseline),
        );
        if let Some(baseline) = baseline {
            output.first_baselines.y = Some(baseline + self.nodes[index].node.style.padding.top());
        }
        output
    }

    fn measure_text(
        &mut self,
        index: usize,
        available: AvailableSpace,
        baseline: &mut Option<f32>,
    ) -> Size<f32> {
        self.work_stats.measurement_callbacks =
            self.work_stats.measurement_callbacks.saturating_add(1);
        let text = match &self.nodes[index].node.text {
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
        let width = match available {
            AvailableSpace::Definite(width) if width.is_finite() && width > 0.0 => Some(width),
            _ => None,
        };
        let layout = text
            .request(width, self.scale)
            .map(|request| {
                if available == AvailableSpace::MinContent && text.overflow == UiTextOverflow::Wrap
                {
                    request.with_min_content()
                } else {
                    request
                }
            })
            .map_err(TextCompileError::Layout)
            .and_then(|request| self.text_session.layout(self.text_engine, request));
        match layout {
            Ok(layout) => {
                *baseline = layout.lines().first().map(|line| line.baseline());
                Size {
                    width: layout.width(),
                    height: layout.height(),
                }
            }
            Err(error) => {
                self.error = Some(text_compile_error(error));
                Size::ZERO
            }
        }
    }
}

struct ChildIter<'a>(std::slice::Iter<'a, usize>);

impl Iterator for ChildIter<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().copied().map(NodeId::from)
    }
}

impl TraversePartialTree for LayoutTree<'_, '_> {
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

impl LayoutPartialTree for LayoutTree<'_, '_> {
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
        if let Err(error) = self.layout_work.charge(self.work_stats) {
            self.error = Some(error);
        }
        if self.error.is_some() {
            return LayoutOutput::HIDDEN;
        }
        if inputs.run_mode == RunMode::PerformHiddenLayout {
            return compute_hidden_layout(self, node_id);
        }
        compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            let index = usize::from(node_id);
            let node = &tree.nodes[index];
            if node.style.display == Display::None {
                compute_hidden_layout(tree, node_id)
            } else if node.children.is_empty() {
                tree.leaf_layout(index, inputs)
            } else {
                compute_flexbox_layout(tree, node_id, inputs)
            }
        })
    }
}

impl CacheTree for LayoutTree<'_, '_> {
    fn cache_get(&self, node_id: NodeId, inputs: &LayoutInput) -> Option<LayoutOutput> {
        self.nodes[usize::from(node_id)].cache.get(inputs)
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

impl LayoutFlexboxContainer for LayoutTree<'_, '_> {
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
