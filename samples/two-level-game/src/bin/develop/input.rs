use std::{
    fs,
    io::{BufRead, Read},
    path::{Path, PathBuf},
};

use nara::{
    core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape},
    fs::RelativePath,
    project_tooling::cargo_build::{CargoBuildLimits, CargoBuildRequest, CargoBuildTools},
    project_workflow::cargo::CargoBuildSelection,
};
use serde::Deserialize;
use two_level_game::{
    development::protocol::{DevelopmentPlacement, MAX_STEP_COMMANDS, SemanticCommand},
    resume::{ResumeState, encode_resume},
};

pub(super) const MAX_LINE_BYTES: usize = 256 * 1024;
pub(super) const MAX_PROBES: usize = 64;
const MAX_SOURCE_ENTRIES: usize = 1024;
const MAX_SOURCE_DEPTH: usize = 16;

pub(super) struct Arguments {
    pub project_root: PathBuf,
    pub work_parent: PathBuf,
    pub target_dir: PathBuf,
    pub target: String,
    pub profile: String,
    pub placement: DevelopmentPlacement,
}

impl Arguments {
    pub fn parse() -> Result<Self, String> {
        let mut project_root = None;
        let mut work_parent = None;
        let mut target_dir = None;
        let mut target = None;
        let mut profile = None;
        let mut placement = None;
        let mut arguments = std::env::args_os().skip(1);
        while let Some(flag) = arguments.next() {
            let value = arguments
                .next()
                .ok_or("every develop option requires a value")?;
            match flag.to_str() {
                Some("--project-root") if project_root.is_none() => {
                    project_root = Some(PathBuf::from(value));
                }
                Some("--work-parent") if work_parent.is_none() => {
                    work_parent = Some(PathBuf::from(value));
                }
                Some("--target-dir") if target_dir.is_none() => {
                    target_dir = Some(PathBuf::from(value));
                }
                Some("--target") if target.is_none() => {
                    target = Some(value.into_string().map_err(|_| "target must be UTF-8")?);
                }
                Some("--profile") if profile.is_none() => {
                    profile = Some(value.into_string().map_err(|_| "profile must be UTF-8")?);
                }
                Some("--placement") if placement.is_none() => {
                    placement = Some(match value.to_str() {
                        Some("headless") => DevelopmentPlacement::SourceHeadless,
                        Some("desktop") => DevelopmentPlacement::SourceDesktop,
                        _ => return Err("placement must be headless or desktop".into()),
                    });
                }
                _ => return Err("unknown or repeated develop option".into()),
            }
        }
        let project_root = project_root.ok_or("--project-root ABS is required")?;
        let work_parent = work_parent.ok_or("--work-parent ABS is required")?;
        let target_dir = target_dir.ok_or("--target-dir ABS is required")?;
        if !project_root.is_absolute() || !work_parent.is_absolute() || !target_dir.is_absolute() {
            return Err("project root, work parent and target directory must all be explicit absolute paths".into());
        }
        require_directory(&project_root)?;
        require_directory(&work_parent)?;
        let project_root = fs::canonicalize(project_root)
            .map_err(|error| format!("resolve project root: {error}"))?;
        let work_parent = fs::canonicalize(work_parent)
            .map_err(|error| format!("resolve work parent: {error}"))?;
        if work_parent.starts_with(&project_root) {
            return Err("development storage must be outside the source project".into());
        }
        let arguments = Self {
            project_root,
            work_parent,
            target_dir,
            target: target.ok_or("--target TRIPLE is required")?,
            profile: profile.unwrap_or_else(|| "dev".into()),
            placement: placement.unwrap_or_default(),
        };
        arguments.selection()?;
        Ok(arguments)
    }

    fn selection(&self) -> Result<CargoBuildSelection, String> {
        CargoBuildSelection::new(
            "two-level-game",
            "continuity",
            &self.profile,
            &self.target,
            match self.placement {
                DevelopmentPlacement::SourceHeadless => Vec::new(),
                DevelopmentPlacement::SourceDesktop => vec!["desktop".to_owned()],
            },
            false,
        )
        .map_err(|error| error.to_string())
    }

    pub fn build_request(&self, tools: &CargoBuildTools) -> Result<CargoBuildRequest, String> {
        let mut inputs = Vec::new();
        for name in ["Cargo.toml", "Cargo.lock", "nara.toml"] {
            let path = self.project_root.join(name);
            let metadata =
                fs::symlink_metadata(&path).map_err(|error| format!("inspect {name}: {error}"))?;
            if linked(&metadata) || !metadata.is_file() {
                return Err(format!("{name} must be a regular non-symlink file"));
            }
            inputs.push(
                RelativePath::new(name)
                    .map_err(|error| format!("invalid inventory path: {error}"))?,
            );
        }
        let mut entries = inputs.len() + 1;
        inventory(&self.project_root, "src", &mut entries, &mut inputs)?;
        CargoBuildRequest::new(
            &self.project_root,
            RelativePath::new("Cargo.toml")
                .map_err(|error| format!("invalid manifest path: {error}"))?,
            self.selection()?,
            tools.clone(),
            inputs,
            CargoBuildLimits::default(),
        )
        .and_then(|request| request.with_target_directory(&self.target_dir))
        .map_err(|error| error.to_string())
    }
}

fn inventory(
    root: &Path,
    relative: &str,
    count: &mut usize,
    inputs: &mut Vec<RelativePath>,
) -> Result<(), String> {
    let admitted =
        RelativePath::new(relative).map_err(|error| format!("invalid source path: {error}"))?;
    if admitted.len() > MAX_SOURCE_DEPTH {
        return Err("source inventory exceeds its 16-component depth limit".into());
    }
    require_directory(&root.join(relative))?;
    let mut entries = Vec::new();
    let directory = fs::read_dir(root.join(relative))
        .map_err(|error| format!("enumerate source directory: {error}"))?;
    for entry in directory {
        if *count >= MAX_SOURCE_ENTRIES {
            return Err("source inventory exceeds its 1024-entry limit".into());
        }
        *count += 1;
        let entry = entry.map_err(|error| format!("read source entry: {error}"))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "source filenames must have portable UTF-8 spellings")?;
        let relative = format!("{relative}/{name}");
        let admitted = RelativePath::new(&relative)
            .map_err(|error| format!("invalid source entry: {error}"))?;
        if admitted.len() > MAX_SOURCE_DEPTH {
            return Err("source inventory exceeds its 16-component depth limit".into());
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| format!("inspect source entry: {error}"))?;
        if linked(&metadata) || (!metadata.is_file() && !metadata.is_dir()) {
            return Err(
                "source inventory accepts only regular files and directories, without symlinks"
                    .into(),
            );
        }
        entries.push((name, relative, admitted, metadata.is_dir()));
    }
    entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    for (_, relative, admitted, directory) in entries {
        if directory {
            inventory(root, &relative, count, inputs)?;
        } else if Path::new(&relative)
            .extension()
            .is_some_and(|extension| extension == "rs")
        {
            inputs.push(admitted);
        }
    }
    Ok(())
}

fn require_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("inspect directory {}: {error}", path.display()))?;
    if linked(&metadata) || !metadata.is_dir() {
        return Err(format!(
            "directory must exist without a symlink: {}",
            path.display(),
        ));
    }
    Ok(())
}

fn linked(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Command {
    Observe {},
    ExecutionStatus {},
    Pause {},
    Resume {},
    Step {
        commands: Vec<SemanticCommand>,
    },
    Prepare {
        basis: ResumeState,
        probes: Vec<WireProbe>,
    },
    Adopt {},
    Cancel {},
    RetryCleanup {},
    IsolateInput {},
    ConfirmPresentation {},
    ActivateInput {},
    Close {},
}

impl Command {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Observe {} => "observe",
            Self::ExecutionStatus {} => "execution_status",
            Self::Pause {} => "pause",
            Self::Resume {} => "resume",
            Self::Step { .. } => "step",
            Self::Prepare { .. } => "prepare",
            Self::Adopt {} => "adopt",
            Self::Cancel {} => "cancel",
            Self::RetryCleanup {} => "retry_cleanup",
            Self::IsolateInput {} => "isolate_input",
            Self::ConfirmPresentation {} => "confirm_presentation",
            Self::ActivateInput {} => "activate_input",
            Self::Close {} => "close",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireProbe {
    pub commands: Vec<SemanticCommand>,
    pub expected: ResumeState,
}

pub(super) fn read_command(reader: &mut impl BufRead) -> Result<Option<Command>, String> {
    let mut bytes = Vec::new();
    let read = reader
        .take((MAX_LINE_BYTES + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(|error| format!("read command: {error}"))?;
    if read == 0 {
        return Ok(None);
    }
    if bytes.len() > MAX_LINE_BYTES {
        return Err("command line exceeds its 256 KiB byte limit".into());
    }
    let limits = SerdeShapeLimits::new(
        DepthLimit::new(16).expect("nonzero CLI depth limit"),
        ItemLimit::new(8192).expect("nonzero CLI node limit"),
        ItemLimit::new(8192).expect("nonzero CLI container limit"),
        ByteLimit::new(MAX_LINE_BYTES).expect("nonzero CLI string limit"),
        ByteLimit::new(MAX_LINE_BYTES).expect("nonzero CLI total string limit"),
    );
    let mut deserializer = serde_json::Deserializer::from_slice(&bytes);
    preflight_serde_shape(&mut deserializer, limits)
        .map_err(|error| format!("command JSON refused: {error}"))?;
    deserializer
        .end()
        .map_err(|error| format!("trailing command data: {error}"))?;
    let command: Command =
        serde_json::from_slice(&bytes).map_err(|error| format!("invalid command: {error}"))?;
    match &command {
        Command::Step { commands } => validate_commands(commands)?,
        Command::Prepare { basis, probes } => {
            if probes.is_empty() || probes.len() > MAX_PROBES {
                return Err("prepare requires 1..=64 complete-state probe steps".into());
            }
            encode_resume(basis).map_err(|error| format!("invalid prepare basis: {error}"))?;
            for probe in probes {
                validate_commands(&probe.commands)?;
                encode_resume(&probe.expected)
                    .map_err(|error| format!("invalid expected probe state: {error}"))?;
            }
        }
        _ => {}
    }
    Ok(Some(command))
}

fn validate_commands(commands: &[SemanticCommand]) -> Result<(), String> {
    if commands.len() > MAX_STEP_COMMANDS {
        return Err(format!(
            "step accepts at most {MAX_STEP_COMMANDS} semantic commands",
        ));
    }
    if commands.iter().any(
        |command| matches!(command, SemanticCommand::Move { direction, .. } if *direction >= 4),
    ) {
        return Err("movement direction must be less than four".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use nara::fs::RelativePath;

    use super::{inventory, read_command};

    #[test]
    fn source_inventory_admits_the_actual_nested_continuity_product() {
        let mut inputs = Vec::new();
        let mut count = 0;
        inventory(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            "src",
            &mut count,
            &mut inputs,
        )
        .unwrap();
        assert!(inputs.contains(&RelativePath::new("src/bin/continuity.rs").unwrap()));
        assert!(inputs.contains(&RelativePath::new("src/development/protocol.rs").unwrap()));
    }

    #[test]
    fn no_argument_commands_refuse_unknown_selection_fields() {
        for operation in ["observe", "adopt", "cancel", "retry_cleanup", "close"] {
            let valid = format!(r#"{{"operation":"{operation}"}}"#);
            assert_eq!(
                read_command(&mut valid.as_bytes())
                    .unwrap()
                    .unwrap()
                    .operation(),
                operation,
            );
            for selection in [r#""generation":999999"#, r#""candidate":"foreign""#] {
                let invalid = format!(r#"{{"operation":"{operation}",{selection}}}"#);
                assert!(read_command(&mut invalid.as_bytes()).is_err(), "{invalid}");
            }
        }
    }

    #[test]
    fn execution_commands_refuse_caller_supplied_execution_facts() {
        for operation in ["execution_status", "pause", "resume"] {
            let valid = format!(r#"{{"operation":"{operation}"}}"#);
            assert_eq!(
                read_command(&mut valid.as_bytes())
                    .unwrap()
                    .unwrap()
                    .operation(),
                operation,
            );
            for fact in [
                r#""state":"paused""#,
                r#""input_activation":"active""#,
                r#""observation":{}"#,
                r#""generation":999999"#,
            ] {
                let invalid = format!(r#"{{"operation":"{operation}",{fact}}}"#);
                assert!(read_command(&mut invalid.as_bytes()).is_err(), "{invalid}");
            }
        }
    }
}
