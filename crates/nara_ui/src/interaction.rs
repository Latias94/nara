use nara_core::Vec2;
use nara_ecs::{Entity, Query, Res, ResMut, Resource};
use nara_input::{ButtonInput, MouseButton, PointerState};

use crate::{ComputedUiLayout, ComputedUiLayouts, UiNode};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
pub struct UiInteractionState {
    hovered: Option<Entity>,
    pressed: Option<Entity>,
    focused: Option<Entity>,
}

impl UiInteractionState {
    #[must_use]
    pub const fn hovered(self) -> Option<Entity> {
        self.hovered
    }

    #[must_use]
    pub const fn pressed(self) -> Option<Entity> {
        self.pressed
    }

    #[must_use]
    pub const fn focused(self) -> Option<Entity> {
        self.focused
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

pub fn update_ui_interaction(
    mut state: ResMut<UiInteractionState>,
    pointer: Res<PointerState>,
    mouse: Res<ButtonInput<MouseButton>>,
    layouts: Res<ComputedUiLayouts>,
    nodes: Query<&UiNode>,
) {
    let hovered = pointer
        .position()
        .and_then(|position| top_hit(position, &layouts, &nodes));

    state.hovered = hovered;

    if mouse.just_pressed(MouseButton::Left) {
        state.pressed = hovered;
        state.focused =
            hovered.filter(|entity| nodes.get(*entity).is_ok_and(|node| node.focusable));
    }

    if mouse.just_released(MouseButton::Left) {
        state.pressed = None;
    }

    if state
        .pressed
        .is_some_and(|entity| !is_still_hit_eligible(entity, &layouts, &nodes))
    {
        state.pressed = None;
    }
    if state
        .focused
        .is_some_and(|entity| !is_focus_eligible(entity, &layouts, &nodes))
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
    layouts
        .as_slice()
        .iter()
        .filter(|layout| hit_test_layout(**layout, position, nodes))
        .max_by_key(|layout| {
            (
                layout.order,
                layout.view_index,
                layout.z_index,
                layout.entity.to_bits(),
            )
        })
        .map(|layout| layout.entity)
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
