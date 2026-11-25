use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};

use nara::project::delivery::{DELIVERY_ARGUMENT, DELIVERY_PROTOCOL_VERSION, DeliveryOperation};

pub(super) use two_level_game::delivery::{
    ControllerBackend, GameFlow, PlayerSelection, RuntimeKind,
};

pub(super) struct Options {
    pub operation: Option<DeliveryOperation>,
    pub project: PathBuf,
    pub output: PathBuf,
    pub name: String,
    pub target_directory: Option<PathBuf>,
    pub target: String,
    pub profile: String,
    pub tools: Option<(PathBuf, PathBuf)>,
    pub selection: PlayerSelection,
}

impl Options {
    pub fn read() -> Result<Self, String> {
        Self::from_args(std::env::args_os().skip(1))
    }

    pub fn from_args(arguments: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut arguments = arguments.into_iter().peekable();
        let operation = if arguments.next_if(|arg| arg == DELIVERY_ARGUMENT).is_some() {
            if arguments.next().as_deref() != Some(std::ffi::OsStr::new(DELIVERY_PROTOCOL_VERSION))
            {
                return Err("unsupported Nara delivery protocol version".into());
            }
            Some(
                match arguments
                    .next()
                    .as_deref()
                    .and_then(std::ffi::OsStr::to_str)
                {
                    Some("cook") => DeliveryOperation::Cook,
                    Some("export") => DeliveryOperation::Export,
                    _ => return Err("expected the cook or export delivery operation".into()),
                },
            )
        } else {
            None
        };
        let mut extras = false;
        let mut values = BTreeMap::new();
        while let Some(flag) = arguments.next() {
            let flag = flag.into_string().map_err(|_| "flags require UTF-8")?;
            if operation.is_some() && !extras && flag == "--" {
                extras = true;
                continue;
            }
            let common = matches!(
                flag.as_str(),
                "--project-root" | "--output-parent" | "--name" | "--target"
            );
            let recipe_option = matches!(flag.as_str(), "--controllers" | "--flow");
            let build_option = matches!(
                flag.as_str(),
                "--target-dir" | "--profile" | "--cargo" | "--rustc"
            );
            let allowed = match (operation, extras) {
                (None, _) => common || recipe_option || build_option || flag == "--runtime",
                (Some(_), false) => common || flag == "--recipe",
                (Some(operation), true) => {
                    recipe_option || (operation == DeliveryOperation::Export && build_option)
                }
            };
            if !allowed {
                return Err(format!("unknown argument: {flag}"));
            }
            let value = arguments
                .next()
                .ok_or_else(|| format!("missing value for {flag}"))?;
            if values.insert(flag, value).is_some() {
                return Err("duplicate argument".into());
            }
        }
        let project = PathBuf::from(take(&mut values, "--project-root")?);
        let output = PathBuf::from(take(&mut values, "--output-parent")?);
        let target_directory = values.remove("--target-dir").map(PathBuf::from);
        if operation.is_none() && target_directory.is_none() {
            return Err("missing --target-dir".into());
        }
        let tools = match (values.remove("--cargo"), values.remove("--rustc")) {
            (Some(cargo), Some(rustc)) => Some((PathBuf::from(cargo), PathBuf::from(rustc))),
            (None, None) if operation.is_some() => None,
            (None, _) => return Err("missing --cargo".into()),
            (_, None) => return Err("missing --rustc".into()),
        };
        let name = take(&mut values, "--name")?
            .into_string()
            .map_err(|_| "name requires UTF-8")?;
        let target = take(&mut values, "--target")?
            .into_string()
            .map_err(|_| "target requires UTF-8")?;
        let profile = values.remove("--profile").map_or_else(
            || Ok("release".to_owned()),
            |value| value.into_string().map_err(|_| "profile requires UTF-8"),
        )?;
        let runtime = match operation {
            Some(_) => Some(take(&mut values, "--recipe")?),
            None => values.remove("--runtime"),
        }
        .map_or(Ok(RuntimeKind::Headless), |value| {
            let value = value.into_string().map_err(|_| "recipe requires UTF-8")?;
            RuntimeKind::parse(&value)
        })?;
        let controllers =
            values
                .remove("--controllers")
                .map_or(Ok(ControllerBackend::Disabled), |value| {
                    let value = value
                        .into_string()
                        .map_err(|_| "controllers requires UTF-8")?;
                    ControllerBackend::parse(&value)
                })?;
        let flow = values
            .remove("--flow")
            .map_or(Ok(GameFlow::Exploration), |value| {
                let value = value.into_string().map_err(|_| "flow requires UTF-8")?;
                GameFlow::parse(&value)
            })?;
        let selection = PlayerSelection::new(runtime, controllers)?.with_flow(flow)?;
        selection.validate_target(&target)?;
        for path in [&project, &output]
            .into_iter()
            .chain(target_directory.iter())
            .chain(tools.iter().flat_map(|(cargo, rustc)| [cargo, rustc]))
        {
            if !path.is_absolute() {
                return Err(format!(
                    "an explicit absolute path is required: {}",
                    path.display()
                ));
            }
        }
        Ok(Self {
            operation,
            project,
            output,
            name,
            target_directory,
            target,
            profile,
            tools,
            selection,
        })
    }
}

fn take(values: &mut BTreeMap<String, OsString>, flag: &str) -> Result<OsString, String> {
    values.remove(flag).ok_or_else(|| format!("missing {flag}"))
}
