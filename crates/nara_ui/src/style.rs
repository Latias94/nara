#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum UiVal {
    Px(f32),
    Percent(f32),
    Auto,
}

impl UiVal {
    #[must_use]
    pub const fn px(value: f32) -> Self {
        Self::Px(value)
    }

    #[must_use]
    pub const fn percent(value: f32) -> Self {
        Self::Percent(value)
    }

    #[must_use]
    pub const fn auto() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UiStyle {
    pub left: UiVal,
    pub top: UiVal,
    pub width: UiVal,
    pub height: UiVal,
}

impl UiStyle {
    #[must_use]
    pub const fn absolute(left: f32, top: f32, width: f32, height: f32) -> Self {
        Self {
            left: UiVal::Px(left),
            top: UiVal::Px(top),
            width: UiVal::Px(width),
            height: UiVal::Px(height),
        }
    }

    #[must_use]
    pub const fn fill() -> Self {
        Self {
            left: UiVal::Px(0.0),
            top: UiVal::Px(0.0),
            width: UiVal::Auto,
            height: UiVal::Auto,
        }
    }

    #[must_use]
    pub const fn with_left(mut self, left: UiVal) -> Self {
        self.left = left;
        self
    }

    #[must_use]
    pub const fn with_top(mut self, top: UiVal) -> Self {
        self.top = top;
        self
    }

    #[must_use]
    pub const fn with_width(mut self, width: UiVal) -> Self {
        self.width = width;
        self
    }

    #[must_use]
    pub const fn with_height(mut self, height: UiVal) -> Self {
        self.height = height;
        self
    }

    #[must_use]
    pub const fn with_position(mut self, left: UiVal, top: UiVal) -> Self {
        self.left = left;
        self.top = top;
        self
    }

    #[must_use]
    pub const fn with_size(mut self, width: UiVal, height: UiVal) -> Self {
        self.width = width;
        self.height = height;
        self
    }
}

impl Default for UiStyle {
    fn default() -> Self {
        Self::fill()
    }
}

#[must_use]
pub fn resolve_ui_position(value: UiVal, parent_size: f32) -> Option<f32> {
    match value {
        UiVal::Px(value) => finite(value),
        UiVal::Percent(value) => finite(parent_size * value),
        UiVal::Auto => Some(0.0),
    }
}

#[must_use]
pub fn resolve_ui_size(value: UiVal, parent_size: f32) -> Option<f32> {
    match value {
        UiVal::Px(value) => finite(value),
        UiVal::Percent(value) => finite(parent_size * value),
        UiVal::Auto => finite(parent_size),
    }
}

fn finite(value: f32) -> Option<f32> {
    value.is_finite().then_some(value)
}
