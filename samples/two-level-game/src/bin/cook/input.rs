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
        })
    }
}
