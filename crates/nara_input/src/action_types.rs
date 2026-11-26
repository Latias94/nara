use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use crate::{ActionValueKind, AxisValue, KeyCode, MAX_ACTION_IDENTIFIER_BYTES, MouseButton};

macro_rules! action_name {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Box<str>);

        impl $name {
            pub fn new(id: impl Into<String>) -> Result<Self, ActionIdError> {
                let id = id.into();
                if id.is_empty() {
                    return Err(ActionIdError::Empty);
                }
                if id.chars().any(char::is_control) {
                    return Err(ActionIdError::ContainsControl);
                }
                if id.len() > MAX_ACTION_IDENTIFIER_BYTES {
                    return Err(ActionIdError::TooLong {
                        bytes: id.len(),
                        limit: MAX_ACTION_IDENTIFIER_BYTES,
                    });
                }
                Ok(Self(id.into_boxed_str()))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        #[cfg(feature = "serde")]
        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let id = <String as serde::Deserialize>::deserialize(deserializer)?;
                Self::new(id).map_err(serde::de::Error::custom)
            }
        }
    };
}

action_name!(ActionId);
action_name!(ActionContext);

impl Default for ActionContext {
    fn default() -> Self {
        Self::gameplay()
    }
}

impl ActionContext {
    pub(crate) const GAMEPLAY_NAME: &str = "gameplay";

    #[must_use]
    pub fn gameplay() -> Self {
        Self(Self::GAMEPLAY_NAME.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionIdError {
    Empty,
    ContainsControl,
    TooLong { bytes: usize, limit: usize },
}

impl Display for ActionIdError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("action id cannot be empty"),
            Self::ContainsControl => {
                formatter.write_str("action id cannot contain control characters")
            }
            Self::TooLong { bytes, limit } => write!(
                formatter,
                "action identifier has {bytes} bytes, limit {limit}"
            ),
        }
    }
}

impl Error for ActionIdError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum InputBinding {
    Key(KeyCode),
    Mouse(MouseButton),
    GamepadButton(u16),
    GamepadAxis(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionPhase {
    Started,
    Changed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ActionValue {
    Button(bool),
    Axis(AxisValue),
}

impl Default for ActionValue {
    fn default() -> Self {
        Self::released()
    }
}

impl ActionValue {
    #[must_use]
    pub const fn pressed() -> Self {
        Self::Button(true)
    }

    #[must_use]
    pub const fn released() -> Self {
        Self::Button(false)
    }

    #[must_use]
    pub const fn kind(self) -> ActionValueKind {
        match self {
            Self::Button(_) => ActionValueKind::Button,
            Self::Axis(_) => ActionValueKind::Axis,
        }
    }

    #[must_use]
    pub fn is_active(self) -> bool {
        match self {
            Self::Button(pressed) => pressed,
            Self::Axis(value) => value != AxisValue::ZERO,
        }
    }
}
