use std::collections::HashMap;

use nara_core::Vec2;
use nara_ecs::{Entity, Query, Res, ResMut, Resource};
use nara_render::{ExtractedViews, RenderTarget, ViewportRect};
use nara_scene::Parent;

use crate::{UiNode, UiRoot, style::resolve_ui_position, style::resolve_ui_size};

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UiRect {
    pub min: Vec2,
    pub size: Vec2,
}

impl UiRect {
    #[must_use]
    pub const fn new(min: Vec2, size: Vec2) -> Self {
        Self { min, size }
    }

    #[must_use]
    pub const fn from_origin_size(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            min: Vec2::new(x, y),
            size: Vec2::new(width, height),
        }
    }

    #[must_use]
    pub fn max(self) -> Vec2 {
        self.min + self.size
    }

    #[must_use]
    pub fn is_non_empty(self) -> bool {
        self.min.is_finite() && self.size.is_finite() && self.size.x > 0.0 && self.size.y > 0.0
    }

    #[must_use]
    pub fn contains(self, point: Vec2) -> bool {
        let max = self.max();
        point.x >= self.min.x && point.y >= self.min.y && point.x < max.x && point.y < max.y
    }

    #[must_use]
    pub fn intersect(self, other: Self) -> Option<Self> {
        let min = self.min.max(other.min);
        let max = self.max().min(other.max());
        let size = max - min;
        (size.x > 0.0 && size.y > 0.0).then_some(Self::new(min, size))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComputedUiLayout {
    pub entity: Entity,
    pub root: Entity,
    pub view_index: usize,
    pub target: RenderTarget,
    pub order: i32,
    pub z_index: i32,
    pub rect: UiRect,
    pub visible: bool,
    pub clip_rect: Option<UiRect>,
    pub clips_children: bool,
}

impl ComputedUiLayout {
    #[must_use]
    pub fn hit_testable(self) -> bool {
        self.visible && self.rect.is_non_empty()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Resource)]
pub struct ComputedUiLayouts {
    layouts: Vec<ComputedUiLayout>,
}

impl ComputedUiLayouts {
    pub fn clear(&mut self) {
        self.layouts.clear();
    }

    pub fn replace(&mut self, mut layouts: Vec<ComputedUiLayout>) {
        layouts.sort_by_key(|layout| {
            (
                layout.order,
                layout.view_index,
                layout.z_index,
                layout.entity.to_bits(),
            )
        });
        self.layouts = layouts;
    }

    #[must_use]
    pub fn get(&self, entity: Entity) -> Option<&ComputedUiLayout> {
        self.layouts.iter().find(|layout| layout.entity == entity)
    }

    #[must_use]
    pub fn as_slice(&self) -> &[ComputedUiLayout] {
        &self.layouts
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.layouts.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.layouts.is_empty()
    }
}

pub fn compute_ui_layouts(
    mut computed: ResMut<ComputedUiLayouts>,
    views: Res<ExtractedViews>,
    roots: Query<(Entity, &UiRoot, Option<&UiNode>)>,
    nodes: Query<(Entity, &UiNode, Option<&Parent>)>,
) {
    let mut layouts = Vec::new();
    let mut by_entity = HashMap::<Entity, ComputedUiLayout>::new();
    let mut root_entries = roots
        .iter()
        .filter_map(|(entity, root, node)| {
            view_for_target(&views, root.target).map(|(view_index, viewport)| RootEntry {
                entity,
                root: *root,
                node: node.copied(),
                view_index,
                viewport,
            })
        })
        .collect::<Vec<_>>();
    root_entries.sort_by_key(|entry| (entry.root.order, entry.view_index, entry.entity.to_bits()));

    for entry in root_entries {
        let viewport_rect = rect_from_viewport(entry.viewport);
        let Some(rect) = entry.node.map_or(Some(viewport_rect), |node| {
            resolve_node_rect(node, viewport_rect)
        }) else {
            continue;
        };
        let node_visible = entry.node.is_none_or(|node| node.visible);
        let layout = ComputedUiLayout {
            entity: entry.entity,
            root: entry.entity,
            view_index: entry.view_index,
            target: entry.root.target,
            order: entry.root.order,
            z_index: entry.node.map_or(0, |node| node.z_index),
            rect,
            visible: node_visible && rect.is_non_empty(),
            clip_rect: None,
            clips_children: entry.node.is_some_and(|node| node.clip),
        };
        by_entity.insert(entry.entity, layout);
        layouts.push(layout);
    }

    let mut pending = nodes
        .iter()
        .filter_map(|(entity, node, parent)| {
            let parent = parent?;
            Some(NodeEntry {
                entity,
                node: *node,
                parent: parent.0,
            })
        })
        .collect::<Vec<_>>();
    pending.sort_by_key(|entry| entry.entity.to_bits());

    let mut progressed = true;
    while progressed && !pending.is_empty() {
        progressed = false;
        let mut next_pending = Vec::new();

        for entry in pending {
            let Some(parent) = by_entity.get(&entry.parent).copied() else {
                next_pending.push(entry);
                continue;
            };
            let Some(rect) = resolve_node_rect(entry.node, parent.rect) else {
                continue;
            };
            let clip_rect = inherited_clip(parent);
            let visible = parent.visible && entry.node.visible && rect.is_non_empty();
            let layout = ComputedUiLayout {
                entity: entry.entity,
                root: parent.root,
                view_index: parent.view_index,
                target: parent.target,
                order: parent.order,
                z_index: entry.node.z_index,
                rect,
                visible,
                clip_rect,
                clips_children: entry.node.clip,
            };
            by_entity.insert(entry.entity, layout);
            layouts.push(layout);
            progressed = true;
        }

        pending = next_pending;
    }

    computed.replace(layouts);
}

#[must_use]
pub fn resolve_node_rect(node: UiNode, parent: UiRect) -> Option<UiRect> {
    let left = resolve_ui_position(node.style.left, parent.size.x)?;
    let top = resolve_ui_position(node.style.top, parent.size.y)?;
    let width = resolve_ui_size(node.style.width, parent.size.x)?;
    let height = resolve_ui_size(node.style.height, parent.size.y)?;
    Some(UiRect::new(
        parent.min + Vec2::new(left, top),
        Vec2::new(width, height),
    ))
}

#[must_use]
pub fn rect_from_viewport(viewport: ViewportRect) -> UiRect {
    UiRect::from_origin_size(
        viewport.physical_x as f32,
        viewport.physical_y as f32,
        viewport.physical_width as f32,
        viewport.physical_height as f32,
    )
}

fn inherited_clip(parent: ComputedUiLayout) -> Option<UiRect> {
    if parent.clips_children {
        match parent.clip_rect {
            Some(clip) => clip.intersect(parent.rect),
            None => Some(parent.rect),
        }
    } else {
        parent.clip_rect
    }
}

fn view_for_target(views: &ExtractedViews, target: RenderTarget) -> Option<(usize, ViewportRect)> {
    views
        .as_slice()
        .iter()
        .enumerate()
        .filter(|(_, view)| view.target == target)
        .min_by_key(|(index, view)| (view.order, *index))
        .map(|(index, view)| (index, view.viewport))
}

#[derive(Debug, Clone, Copy)]
struct RootEntry {
    entity: Entity,
    root: UiRoot,
    node: Option<UiNode>,
    view_index: usize,
    viewport: ViewportRect,
}

#[derive(Debug, Clone, Copy)]
struct NodeEntry {
    entity: Entity,
    node: UiNode,
    parent: Entity,
}
