//! Finite game-owned Player selections shared by the cooker and executable entry points.

use nara::ProductRecipe;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RuntimeKind {
    #[default]
    Headless,
    Desktop,
    AudioDesktop,
}

impl RuntimeKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        let runtime = match value {
            "headless" => Self::Headless,
            "desktop" => Self::Desktop,
            "audio-desktop" => Self::AudioDesktop,
            _ => return Err("--runtime requires headless, desktop or audio-desktop".into()),
        };
        runtime.require_features()?;
        Ok(runtime)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Headless => "headless",
            Self::Desktop => "desktop",
            Self::AudioDesktop => "audio-desktop",
        }
    }

    pub const fn binary(self) -> &'static str {
        match self {
            Self::Headless => "player",
            Self::Desktop => "player_desktop",
            Self::AudioDesktop => "player_audio",
        }
    }

    pub const fn manifest_profile(self) -> Option<&'static str> {
        match self {
            Self::Headless => None,
            Self::Desktop | Self::AudioDesktop => Some("desktop"),
        }
    }

    fn require_features(self) -> Result<(), String> {
        match self {
            Self::Headless => Ok(()),
            Self::Desktop if cfg!(feature = "desktop") => Ok(()),
            Self::Desktop => Err("desktop selection requires the desktop Cargo feature".into()),
            Self::AudioDesktop if cfg!(all(windows, feature = "desktop", feature = "audio-cpal")) => Ok(()),
            Self::AudioDesktop => Err(
                "audio-desktop selection requires Windows and the desktop and audio-cpal Cargo features".into(),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControllerBackend {
    #[default]
    Disabled,
    Gilrs,
}

impl ControllerBackend {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "none" => Ok(Self::Disabled),
            "gilrs" => Ok(Self::Gilrs),
            _ => Err("--controllers requires none or gilrs".into()),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Disabled => "none",
            Self::Gilrs => "gilrs",
        }
    }
}

/// One admitted build/content/recipe selection. Construction acquires no native devices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerSelection {
    runtime: RuntimeKind,
    controllers: ControllerBackend,
}

impl PlayerSelection {
    pub fn new(runtime: RuntimeKind, controllers: ControllerBackend) -> Result<Self, String> {
        runtime.require_features()?;
        if controllers == ControllerBackend::Gilrs {
            if runtime == RuntimeKind::Headless {
                return Err("native controllers require a desktop Player".into());
            }
            if !cfg!(all(windows, feature = "gamepad")) {
                return Err(
                    "Gilrs Player selection requires Windows and the gamepad Cargo feature".into(),
                );
            }
        }
        Ok(Self {
            runtime,
            controllers,
        })
    }

    pub fn from_content_profile(profile: &str) -> Result<Self, String> {
        let (runtime, controllers) = match profile {
            "two-level-shipping" => (RuntimeKind::Headless, ControllerBackend::Disabled),
            "two-level-desktop-shipping" => (RuntimeKind::Desktop, ControllerBackend::Disabled),
            "two-level-audio-desktop-shipping" => {
                (RuntimeKind::AudioDesktop, ControllerBackend::Disabled)
            }
            "two-level-desktop-gamepad-shipping" => {
                (RuntimeKind::Desktop, ControllerBackend::Gilrs)
            }
            "two-level-audio-desktop-gamepad-shipping" => {
                (RuntimeKind::AudioDesktop, ControllerBackend::Gilrs)
            }
            _ => {
                return Err(
                    "compiled content profile is not a Two-Level Game Player selection".into(),
                );
            }
        };
        Self::new(runtime, controllers)
    }

    pub const fn runtime(self) -> RuntimeKind {
        self.runtime
    }

    pub const fn controllers(self) -> ControllerBackend {
        self.controllers
    }

    pub const fn binary(self) -> &'static str {
        self.runtime.binary()
    }

    pub const fn content_profile(self) -> &'static str {
        match (self.runtime, self.controllers) {
            (RuntimeKind::Headless, _) => "two-level-shipping",
            (RuntimeKind::Desktop, ControllerBackend::Disabled) => "two-level-desktop-shipping",
            (RuntimeKind::Desktop, ControllerBackend::Gilrs) => {
                "two-level-desktop-gamepad-shipping"
            }
            (RuntimeKind::AudioDesktop, ControllerBackend::Disabled) => {
                "two-level-audio-desktop-shipping"
            }
            (RuntimeKind::AudioDesktop, ControllerBackend::Gilrs) => {
                "two-level-audio-desktop-gamepad-shipping"
            }
        }
    }

    pub fn features(self) -> Vec<String> {
        let mut features = vec!["cooked-player".into()];
        if self.runtime != RuntimeKind::Headless {
            features.push("desktop".into());
        }
        if self.runtime == RuntimeKind::AudioDesktop {
            features.push("audio-cpal".into());
        }
        if self.controllers == ControllerBackend::Gilrs {
            features.push("gamepad".into());
        }
        features
    }

    pub fn validate_target(self, triple: &str) -> Result<(), String> {
        if (self.runtime == RuntimeKind::AudioDesktop
            || self.controllers == ControllerBackend::Gilrs)
            && !triple.contains("-windows-")
        {
            return Err(
                "selected native audio/controllers require an explicit Windows target".into(),
            );
        }
        Ok(())
    }

    pub fn recipe(self) -> Result<ProductRecipe, String> {
        let recipe = match self.runtime {
            RuntimeKind::Headless => crate::cooked_recipe(),
            #[cfg(feature = "desktop")]
            RuntimeKind::Desktop => crate::cooked_desktop_recipe(),
            #[cfg(not(feature = "desktop"))]
            RuntimeKind::Desktop => {
                return Err("desktop selection requires the desktop Cargo feature".into());
            }
            #[cfg(all(windows, feature = "desktop", feature = "audio-cpal"))]
            RuntimeKind::AudioDesktop => crate::audio::cooked_recipe(),
            #[cfg(not(all(windows, feature = "desktop", feature = "audio-cpal")))]
            RuntimeKind::AudioDesktop => {
                return Err(
                    "audio-desktop selection requires Windows and the audio-cpal Cargo feature"
                        .into(),
                );
            }
        };
        match self.controllers {
            ControllerBackend::Disabled => Ok(recipe),
            #[cfg(feature = "gamepad")]
            ControllerBackend::Gilrs => {
                crate::controller::with_gilrs_input(recipe).map_err(|error| error.to_string())
            }
            #[cfg(not(feature = "gamepad"))]
            ControllerBackend::Gilrs => {
                Err("Gilrs Player selection requires the gamepad Cargo feature".into())
            }
        }
    }
}
