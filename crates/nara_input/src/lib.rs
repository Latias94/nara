//! Input state primitives.

use std::{collections::HashSet, hash::Hash};

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_ecs::{ResMut, Resource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Escape,
    Space,
    Enter,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Character(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u16),
}

#[derive(Debug, Clone, Resource)]
pub struct ButtonInput<T> {
    pressed: HashSet<T>,
    just_pressed: HashSet<T>,
    just_released: HashSet<T>,
}

impl<T> Default for ButtonInput<T> {
    fn default() -> Self {
        Self {
            pressed: HashSet::new(),
            just_pressed: HashSet::new(),
            just_released: HashSet::new(),
        }
    }
}

impl<T> ButtonInput<T>
where
    T: Copy + Eq + Hash,
{
    pub fn press(&mut self, button: T) {
        if self.pressed.insert(button) {
            self.just_pressed.insert(button);
        }
        self.just_released.remove(&button);
    }

    pub fn release(&mut self, button: T) {
        if self.pressed.remove(&button) {
            self.just_released.insert(button);
        }
        self.just_pressed.remove(&button);
    }

    #[must_use]
    pub fn pressed(&self, button: T) -> bool {
        self.pressed.contains(&button)
    }

    #[must_use]
    pub fn just_pressed(&self, button: T) -> bool {
        self.just_pressed.contains(&button)
    }

    #[must_use]
    pub fn just_released(&self, button: T) -> bool {
        self.just_released.contains(&button)
    }

    pub fn clear_transitions(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }
}

/// Compatibility alias for the current keyboard state resource.
pub type InputState = ButtonInput<KeyCode>;

#[derive(Debug, Default, Clone, Copy)]
pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .add_systems(CoreStage::Last, clear_input_transitions);
        Ok(())
    }
}

fn clear_input_transitions(
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
) {
    keyboard.clear_transitions();
    mouse.clear_transitions();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_button_transitions() {
        let mut input = ButtonInput::default();

        input.press(KeyCode::Space);
        assert!(input.pressed(KeyCode::Space));
        assert!(input.just_pressed(KeyCode::Space));

        input.clear_transitions();
        assert!(!input.just_pressed(KeyCode::Space));

        input.release(KeyCode::Space);
        assert!(input.just_released(KeyCode::Space));
        assert!(!input.pressed(KeyCode::Space));
    }
}
