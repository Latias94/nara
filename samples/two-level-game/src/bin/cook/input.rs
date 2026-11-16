use std::{collections::BTreeMap, path::PathBuf};

pub(super) struct Options {
    pub project: PathBuf,
    pub output: PathBuf,
    pub name: String,
    pub target_directory: PathBuf,
    pub target: String,
    pub profile: String,
    pub cargo: PathBuf,
    pub rustc: PathBuf,
    pub runtime: RuntimeKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RuntimeKind {
    #[default]
    Headless,
    Desktop,
    AudioDesktop,
}

impl RuntimeKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "headless" => Ok(Self::Headless),
            "desktop" if cfg!(feature = "desktop") => Ok(Self::Desktop),
            "desktop" => Err("desktop cooking requires the cooker's desktop Cargo feature".into()),
            "audio-desktop" if cfg!(all(windows, feature = "desktop", feature = "audio-cpal")) => {
                Ok(Self::AudioDesktop)
            }
            "audio-desktop" => Err(
                "audio-desktop cooking requires Windows and the cooker's desktop and audio-cpal Cargo features".into(),
            ),
            _ => Err("--runtime requires headless, desktop or audio-desktop".into()),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Headless => "headless",
            Self::Desktop => "desktop",
            Self::AudioDesktop => "audio-desktop",
        }
    }

    pub const fn content_profile(self) -> &'static str {
        match self {
            Self::Headless => "two-level-shipping",
            Self::Desktop => "two-level-desktop-shipping",
            Self::AudioDesktop => "two-level-audio-desktop-shipping",
        }
    }

    pub const fn binary(self) -> &'static str {
        match self {
            Self::Headless => "player",
            Self::Desktop => "player_desktop",
            Self::AudioDesktop => "player_audio",
        }
    }

    pub fn features(self) -> Vec<String> {
        match self {
            Self::Headless => vec!["cooked-player".into()],
            Self::Desktop => vec!["cooked-player".into(), "desktop".into()],
            Self::AudioDesktop => {
                vec![
                    "cooked-player".into(),
                    "desktop".into(),
                    "audio-cpal".into(),
                ]
            }
        }
    }

    pub fn recipe(self) -> Result<nara::ProductRecipe, String> {
        match self {
            Self::Headless => Ok(two_level_game::cooked_recipe()),
            #[cfg(feature = "desktop")]
            Self::Desktop => Ok(two_level_game::cooked_desktop_recipe()),
            #[cfg(not(feature = "desktop"))]
            Self::Desktop => {
                Err("desktop cooking requires the cooker's desktop Cargo feature".into())
            }
            #[cfg(all(windows, feature = "desktop", feature = "audio-cpal"))]
            Self::AudioDesktop => Ok(two_level_game::audio::cooked_recipe()),
            #[cfg(not(all(windows, feature = "desktop", feature = "audio-cpal")))]
            Self::AudioDesktop => Err(
                "audio-desktop cooking requires Windows and the cooker's desktop and audio-cpal Cargo features".into(),
            ),
        }
    }
}

impl Options {
    pub fn read() -> Result<Self, String> {
        let mut arguments = std::env::args_os().skip(1);
        let mut values = BTreeMap::new();
        while let Some(flag) = arguments.next() {
            let flag = flag.into_string().map_err(|_| "flags require UTF-8")?;
            if ![
                "--project-root",
                "--output-parent",
                "--name",
                "--target-dir",
                "--target",
                "--profile",
                "--cargo",
                "--rustc",
                "--runtime",
            ]
            .contains(&flag.as_str())
            {
                return Err(format!("unknown argument: {flag}"));
            }
            let value = arguments
                .next()
                .ok_or_else(|| format!("missing value for {flag}"))?;
            if values.insert(flag, value).is_some() {
                return Err("duplicate argument".into());
            }
        }
        let mut take = |flag: &str| values.remove(flag).ok_or_else(|| format!("missing {flag}"));
        let project = PathBuf::from(take("--project-root")?);
        let output = PathBuf::from(take("--output-parent")?);
        let target_directory = PathBuf::from(take("--target-dir")?);
        let cargo = PathBuf::from(take("--cargo")?);
        let rustc = PathBuf::from(take("--rustc")?);
        let name = take("--name")?
            .into_string()
            .map_err(|_| "name requires UTF-8")?;
        let target = take("--target")?
            .into_string()
            .map_err(|_| "target requires UTF-8")?;
        let profile = values.remove("--profile").map_or_else(
            || Ok("release".to_owned()),
            |value| value.into_string().map_err(|_| "profile requires UTF-8"),
        )?;
        let runtime = values
            .remove("--runtime")
            .map_or(Ok(RuntimeKind::Headless), |value| {
                let value = value.into_string().map_err(|_| "runtime requires UTF-8")?;
                RuntimeKind::parse(&value)
            })?;
        for path in [&project, &output, &target_directory, &cargo, &rustc] {
            if !path.is_absolute() {
                return Err(format!(
                    "an explicit absolute path is required: {}",
                    path.display()
                ));
            }
        }
        Ok(Self {
            project,
            output,
            name,
            target_directory,
            target,
            profile,
            cargo,
            rustc,
            runtime,
        })
    }
}
