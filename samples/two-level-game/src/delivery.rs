//! Finite game-owned Player selections shared by the cooker and executable entry points.

use nara::ProductRecipe;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RuntimeKind {
    #[default]
    Headless,
    Server,
    Desktop,
    AudioDesktop,
}

impl RuntimeKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        let runtime = match value {
            "headless" => Self::Headless,
            "server" => Self::Server,
            "desktop" => Self::Desktop,
            "audio-desktop" => Self::AudioDesktop,
            _ => {
                return Err("--runtime requires headless, server, desktop or audio-desktop".into());
            }
        };
        runtime.require_features()?;
        Ok(runtime)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Headless => "headless",
            Self::Server => "server",
            Self::Desktop => "desktop",
            Self::AudioDesktop => "audio-desktop",
        }
    }

    pub const fn binary(self) -> &'static str {
        match self {
            Self::Headless => "player",
            Self::Server => "server",
            Self::Desktop => "player_desktop",
            Self::AudioDesktop => "player_audio",
        }
    }

    pub const fn uses_window(self) -> bool {
        matches!(self, Self::Desktop | Self::AudioDesktop)
    }

    fn require_features(self) -> Result<(), String> {
        match self {
            Self::Headless | Self::Server => Ok(()),
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
pub enum GameFlow {
    #[default]
    Exploration,
    Round,
}

impl GameFlow {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "exploration" => Ok(Self::Exploration),
            "round" => Ok(Self::Round),
            _ => Err("--flow requires exploration or round".into()),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Exploration => "exploration",
            Self::Round => "round",
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
    flow: GameFlow,
}

impl PlayerSelection {
    pub fn new(runtime: RuntimeKind, controllers: ControllerBackend) -> Result<Self, String> {
        runtime.require_features()?;
        if controllers == ControllerBackend::Gilrs {
            if !runtime.uses_window() {
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
            flow: GameFlow::Exploration,
        })
    }

    pub fn with_flow(self, flow: GameFlow) -> Result<Self, String> {
        if flow == GameFlow::Round {
            if !self.runtime.uses_window() {
                return Err("round flow requires a desktop Player with a frame-menu driver".into());
            }
            if !cfg!(feature = "play-flow") {
                return Err("round flow requires the play-flow Cargo feature".into());
            }
        }
        Ok(Self { flow, ..self })
    }

    pub fn from_content_profile(profile: &str) -> Result<Self, String> {
        let (runtime, controllers, flow) = match profile {
            "two-level-shipping" => (
                RuntimeKind::Headless,
                ControllerBackend::Disabled,
                GameFlow::Exploration,
            ),
            "two-level-server-shipping" => (
                RuntimeKind::Server,
                ControllerBackend::Disabled,
                GameFlow::Exploration,
            ),
            "two-level-desktop-shipping" => (
                RuntimeKind::Desktop,
                ControllerBackend::Disabled,
                GameFlow::Exploration,
            ),
            "two-level-audio-desktop-shipping" => (
                RuntimeKind::AudioDesktop,
                ControllerBackend::Disabled,
                GameFlow::Exploration,
            ),
            "two-level-desktop-gamepad-shipping" => (
                RuntimeKind::Desktop,
                ControllerBackend::Gilrs,
                GameFlow::Exploration,
            ),
            "two-level-audio-desktop-gamepad-shipping" => (
                RuntimeKind::AudioDesktop,
                ControllerBackend::Gilrs,
                GameFlow::Exploration,
            ),
            "two-level-round-desktop-shipping" => (
                RuntimeKind::Desktop,
                ControllerBackend::Disabled,
                GameFlow::Round,
            ),
            "two-level-round-desktop-gamepad-shipping" => (
                RuntimeKind::Desktop,
                ControllerBackend::Gilrs,
                GameFlow::Round,
            ),
            "two-level-round-audio-desktop-shipping" => (
                RuntimeKind::AudioDesktop,
                ControllerBackend::Disabled,
                GameFlow::Round,
            ),
            "two-level-round-audio-desktop-gamepad-shipping" => (
                RuntimeKind::AudioDesktop,
                ControllerBackend::Gilrs,
                GameFlow::Round,
            ),
            _ => {
                return Err(
                    "compiled content profile is not a Two-Level Game Player selection".into(),
                );
            }
        };
        Self::new(runtime, controllers)?.with_flow(flow)
    }

    pub const fn runtime(self) -> RuntimeKind {
        self.runtime
    }

    pub const fn controllers(self) -> ControllerBackend {
        self.controllers
    }

    pub const fn flow(self) -> GameFlow {
        self.flow
    }

    pub const fn manifest_profile(self) -> Option<&'static str> {
        match (self.runtime, self.flow) {
            (RuntimeKind::Headless, _) => None,
            (RuntimeKind::Server, _) => Some("server"),
            (_, GameFlow::Exploration) => Some("desktop"),
            (_, GameFlow::Round) => Some("play-flow"),
        }
    }

    pub const fn binary(self) -> &'static str {
        self.runtime.binary()
    }

    pub const fn content_profile(self) -> &'static str {
        match (self.runtime, self.controllers, self.flow) {
            (RuntimeKind::Headless, _, _) => "two-level-shipping",
            (RuntimeKind::Server, _, _) => "two-level-server-shipping",
            (RuntimeKind::Desktop, ControllerBackend::Disabled, GameFlow::Exploration) => {
                "two-level-desktop-shipping"
            }
            (RuntimeKind::Desktop, ControllerBackend::Gilrs, GameFlow::Exploration) => {
                "two-level-desktop-gamepad-shipping"
            }
            (RuntimeKind::AudioDesktop, ControllerBackend::Disabled, GameFlow::Exploration) => {
                "two-level-audio-desktop-shipping"
            }
            (RuntimeKind::AudioDesktop, ControllerBackend::Gilrs, GameFlow::Exploration) => {
                "two-level-audio-desktop-gamepad-shipping"
            }
            (RuntimeKind::Desktop, ControllerBackend::Disabled, GameFlow::Round) => {
                "two-level-round-desktop-shipping"
            }
            (RuntimeKind::Desktop, ControllerBackend::Gilrs, GameFlow::Round) => {
                "two-level-round-desktop-gamepad-shipping"
            }
            (RuntimeKind::AudioDesktop, ControllerBackend::Disabled, GameFlow::Round) => {
                "two-level-round-audio-desktop-shipping"
            }
            (RuntimeKind::AudioDesktop, ControllerBackend::Gilrs, GameFlow::Round) => {
                "two-level-round-audio-desktop-gamepad-shipping"
            }
        }
    }

    pub fn features(self) -> Vec<String> {
        if self.runtime == RuntimeKind::Server {
            return vec!["cooked-server".into()];
        }
        let mut features = vec!["cooked-player".into()];
        if self.runtime.uses_window() {
            features.push("desktop".into());
        }
        if self.runtime == RuntimeKind::AudioDesktop {
            features.push("audio-cpal".into());
        }
        if self.controllers == ControllerBackend::Gilrs {
            features.push("gamepad".into());
        }
        if self.flow == GameFlow::Round {
            features.push("play-flow".into());
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

    /// Checks Cook's admitted Input definition using the Round's actual startup derivation.
    /// Exploration uses the original definition without a menu derivation.
    #[cfg(feature = "play-flow")]
    pub fn validate_round_input(
        self,
        input: Option<&nara::input::ActionMapDefinition>,
    ) -> Result<(), String> {
        if self.flow == GameFlow::Round {
            let definition = input.ok_or("round flow requires an authored Input map")?;
            crate::flow::validate_input(definition, self.round_controls())?;
        }
        Ok(())
    }

    #[cfg(feature = "play-flow")]
    fn round_controls(self) -> crate::flow::PlayFlowControls {
        use crate::flow::PlayFlowControls;

        match self.controllers {
            #[cfg(feature = "gamepad")]
            ControllerBackend::Gilrs => PlayFlowControls::KeyboardAndGamepad,
            _ => PlayFlowControls::Keyboard,
        }
    }

    pub fn recipe(self) -> Result<ProductRecipe, String> {
        let recipe = match self.runtime {
            RuntimeKind::Server => crate::cooked_server_recipe(),
            #[cfg(feature = "local-player")]
            RuntimeKind::Headless => crate::cooked_recipe(),
            #[cfg(not(feature = "local-player"))]
            RuntimeKind::Headless => {
                return Err("headless recipe requires the local-player Cargo feature".into());
            }
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
        let recipe = match self.controllers {
            ControllerBackend::Disabled => recipe,
            #[cfg(feature = "gamepad")]
            ControllerBackend::Gilrs => {
                crate::controller::with_gilrs_input(recipe).map_err(|error| error.to_string())?
            }
            #[cfg(not(feature = "gamepad"))]
            ControllerBackend::Gilrs => {
                return Err("Gilrs Player selection requires the gamepad Cargo feature".into());
            }
        };
        match self.flow {
            GameFlow::Exploration => Ok(recipe),
            #[cfg(feature = "play-flow")]
            GameFlow::Round => {
                use crate::flow::{with_play_flow, with_play_flow_hud};

                with_play_flow(recipe, self.round_controls())
                    .and_then(with_play_flow_hud)
                    .map_err(|error| error.to_string())
            }
            #[cfg(not(feature = "play-flow"))]
            GameFlow::Round => Err("round flow requires the play-flow Cargo feature".into()),
        }
    }
}
