use nara_core::Vec2;
use nara_ecs::{Entity, Query, Res, ResMut, Resource};
use nara_input::{ButtonInput, MouseButton, PointerState};
use nara_render::RenderTarget;

use crate::{ComputedUiLayout, ComputedUiLayouts, UiNode};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
pub struct UiInteractionState {
    pointer_route: UiPointerRoute,
    hovered: Option<UiInteractionTarget>,
    pressed: Option<UiInteractionTarget>,
    focused: Option<UiInteractionTarget>,
}

impl UiInteractionState {
    #[must_use]
    pub fn hovered(self) -> Option<Entity> {
        self.hovered.map(|target| target.entity)
    }

    #[must_use]
    pub fn pressed(self) -> Option<Entity> {
        self.pressed.map(|target| target.entity)
    }

    #[must_use]
    pub fn focused(self) -> Option<Entity> {
        self.focused.map(|target| target.entity)
    }

    #[must_use]
    pub const fn pointer_route(self) -> UiPointerRoute {
        self.pointer_route
    }

    #[must_use]
    pub const fn hovered_target(self) -> Option<UiInteractionTarget> {
        self.hovered
    }

    #[must_use]
    pub const fn pressed_target(self) -> Option<UiInteractionTarget> {
        self.pressed
    }

    #[must_use]
    pub const fn focused_target(self) -> Option<UiInteractionTarget> {
        self.focused
    }

    pub fn set_pointer_route(&mut self, route: UiPointerRoute) {
        self.pointer_route = route;
    }

    pub fn clear_pointer_route(&mut self) {
        self.pointer_route = UiPointerRoute::default();
    }

    pub fn clear_hovered(&mut self) {
        self.hovered = None;
    }

    pub fn clear_pressed(&mut self) {
        self.pressed = None;
    }

    pub fn clear_focused(&mut self) {
        self.focused = None;
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UiPointerRoute {
    pub target: Option<RenderTarget>,
    pub view_index: Option<usize>,
}

impl UiPointerRoute {
    #[must_use]
    pub const fn any() -> Self {
        Self {
            target: None,
            view_index: None,
        }
    }

    #[must_use]
    pub const fn for_target(target: RenderTarget) -> Self {
        Self {
            target: Some(target),
            view_index: None,
        }
    }

    #[must_use]
    pub const fn for_view(view_index: usize) -> Self {
        Self {
            target: None,
            view_index: Some(view_index),
        }
    }

    #[must_use]
    pub const fn for_target_view(target: RenderTarget, view_index: usize) -> Self {
        Self {
            target: Some(target),
            view_index: Some(view_index),
        }
    }

    #[must_use]
    pub fn matches(self, layout: ComputedUiLayout) -> bool {
        self.target.is_none_or(|target| target == layout.target)
            && self
                .view_index
                .is_none_or(|view_index| view_index == layout.view_index)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiInteractionTarget {
    pub entity: Entity,
    pub root: Entity,
    pub view_index: usize,
    pub target: RenderTarget,
}

impl UiInteractionTarget {
    #[must_use]
    pub const fn from_layout(layout: ComputedUiLayout) -> Self {
        Self {
            entity: layout.entity,
            root: layout.root,
            view_index: layout.view_index,
            target: layout.target,
        }
    }
}

pub fn update_ui_interaction(
    mut state: ResMut<UiInteractionState>,
    pointer: Res<PointerState>,
    mouse: Res<ButtonInput<MouseButton>>,
    layouts: Res<ComputedUiLayouts>,
    nodes: Query<&UiNode>,
) {
    let hovered = pointer
        .position()
        .and_then(|position| top_hit_target(position, state.pointer_route, &layouts, &nodes));

    state.hovered = hovered;

    if mouse.just_pressed(MouseButton::Left) {
        state.pressed = hovered;
        state.focused =
            hovered.filter(|target| nodes.get(target.entity).is_ok_and(|node| node.focusable));
    }

    if mouse.just_released(MouseButton::Left) {
        state.pressed = None;
    }

    if state
        .pressed
        .is_some_and(|target| !is_still_hit_eligible(target.entity, &layouts, &nodes))
    {
        state.pressed = None;
    }
    if state
        .focused
        .is_some_and(|target| !is_focus_eligible(target.entity, &layouts, &nodes))
    {
        state.focused = None;
    }
}

#[must_use]
pub fn top_hit(
    position: Vec2,
    layouts: &ComputedUiLayouts,
    nodes: &Query<&UiNode>,
) -> Option<Entity> {
    top_hit_target(position, UiPointerRoute::any(), layouts, nodes).map(|target| target.entity)
}

#[must_use]
pub fn top_hit_target(
    position: Vec2,
    route: UiPointerRoute,
    layouts: &ComputedUiLayouts,
    nodes: &Query<&UiNode>,
) -> Option<UiInteractionTarget> {
    layouts
        .as_slice()
        .iter()
        .filter(|layout| route.matches(**layout) && hit_test_layout(**layout, position, nodes))
        .max_by_key(|layout| {
            (
                layout.order,
                layout.view_index,
                layout.z_index,
                layout.entity.to_bits(),
            )
        })
        .map(|layout| UiInteractionTarget::from_layout(*layout))
}

fn hit_test_layout(layout: ComputedUiLayout, position: Vec2, nodes: &Query<&UiNode>) -> bool {
    layout.hit_testable()
        && layout.rect.contains(position)
        && layout
            .clip_rect
            .is_none_or(|clip_rect| clip_rect.contains(position))
        && nodes.get(layout.entity).is_ok_and(|node| node.visible)
}

fn is_still_hit_eligible(
    entity: Entity,
    layouts: &ComputedUiLayouts,
    nodes: &Query<&UiNode>,
) -> bool {
    let Some(layout) = layouts.get(entity) else {
        return false;
    };
    layout.hit_testable() && nodes.get(entity).is_ok_and(|node| node.visible)
}

fn is_focus_eligible(entity: Entity, layouts: &ComputedUiLayouts, nodes: &Query<&UiNode>) -> bool {
    let Some(layout) = layouts.get(entity) else {
        return false;
    };
    layout.hit_testable()
        && nodes
            .get(entity)
            .is_ok_and(|node| node.visible && node.focusable)
}
