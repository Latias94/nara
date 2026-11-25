use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use nara::{
    fs::RelativePath,
    project_tooling::{
        cargo_build::{CargoBuildLimits, CargoBuildRequest, CargoBuildTools},
        process::SupervisedProcessCommand,
        project_cook::project_rust_input_inventory,
    },
    project_workflow::cargo::CargoBuildSelection,
};

use crate::authored_project::{AuthoredFixture, create_authored_project};

use super::super::{
    command,
    process::{Custody, Output, install, remove_owned_tree},
    workspace::read_toml,
};

pub(super) mod source;

const PROJECT_SLUG: &str = "nara";
const COOKED_DELIVERY_NAME: &str = "push-box";
const BUILD_TIMEOUT: Duration = Duration::from_secs(900);
const RUN_TIMEOUT: Duration = Duration::from_secs(60);

pub(super) struct Project {
    pub(super) root: PathBuf,
    cargo_package_name: String,
    custody: Custody,
    tools: CargoBuildTools,
    host: String,
    target: PathBuf,
    binaries: PathBuf,
    cwd: PathBuf,
}

impl Project {
    pub(super) fn create() -> Self {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
            .canonicalize()
            .unwrap();
        assert_eq!(target, checkout.join("target").canonicalize().unwrap());
        assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
        assert_eq!(std::env::var("CARGO_NET_OFFLINE").as_deref(), Ok("true"));
        assert!(
            std::env::var("RUSTUP_TOOLCHAIN")
                .is_ok_and(|toolchain| toolchain.starts_with("1.95.0"))
        );

        let mut custody = Custody::create(checkout.parent().unwrap());
        let parent = custody.directory("independent author");
        let tools = CargoBuildTools::discover_in(&checkout).unwrap();
        let root =
            create_authored_project(&checkout, &parent, PROJECT_SLUG, AuthoredFixture::Actions);
        assert!(root.starts_with(&custody.root) && !root.starts_with(&checkout));
        let cargo = read_toml(&root.join("Cargo.toml"));
        let cargo_package_name = cargo["package"]["name"].as_str().unwrap().to_owned();
        let compiler = custody.tool(
            command(tools.rustc_path(), &root, &tools, &target).argument("-vV"),
            RUN_TIMEOUT,
        );
        compiler.assert_success();
        assert!(
            compiler
                .stdout
                .lines()
                .any(|line| line == "release: 1.95.0")
        );
        let host = compiler
            .stdout
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .unwrap()
            .to_owned();
        let binaries = custody.directory("captured tools");
        let cwd = custody.directory("unrelated empty cwd");
        Self {
            root,
            cargo_package_name,
            custody,
            tools,
            host,
            target,
            binaries,
            cwd,
        }
    }

    pub(super) fn build(&mut self, bin: &str, installed: &str, features: &[&str]) -> PathBuf {
        let request = CargoBuildRequest::new(
            &self.root,
            RelativePath::new("Cargo.toml").unwrap(),
            CargoBuildSelection::new(
                self.cargo_package_name.as_str(),
                bin,
                "dev",
                self.host.as_str(),
                features.iter().copied(),
                false,
            )
            .unwrap(),
            self.tools.clone(),
            project_rust_input_inventory(&self.root).unwrap(),
            CargoBuildLimits::default(),
        )
        .unwrap()
        .with_target_directory(&self.target)
        .unwrap();
        install(self.custody.build(&request), &self.binaries, installed)
    }

    pub(super) fn run(
        &mut self,
        executable: &Path,
        configure: impl FnOnce(SupervisedProcessCommand) -> SupervisedProcessCommand,
    ) -> Output {
        let invocation = configure(SupervisedProcessCommand::new(executable, &self.cwd));
        let output = self.custody.tool(invocation, RUN_TIMEOUT);
        output.assert_success();
        assert!(fs::read_dir(&self.cwd).unwrap().next().is_none());
        output
    }

    pub(super) fn cook_and_relocate(&mut self) -> PathBuf {
        let cooker = self.build("cook", "cook", &[]);
        let output = self.custody.directory("cook output");
        let cooked = self.custody.tool(
            command(&cooker, &self.cwd, &self.tools, &self.target).arguments([
                OsStr::new("--project-root"),
                self.root.as_os_str(),
                OsStr::new("--output-parent"),
                output.as_os_str(),
                OsStr::new("--name"),
                OsStr::new(COOKED_DELIVERY_NAME),
                OsStr::new("--target-dir"),
                self.target.as_os_str(),
                OsStr::new("--target"),
                OsStr::new(&self.host),
                OsStr::new("--cargo"),
                self.tools.cargo_path().as_os_str(),
                OsStr::new("--rustc"),
                self.tools.rustc_path().as_os_str(),
                OsStr::new("--profile"),
                OsStr::new("dev"),
            ]),
            BUILD_TIMEOUT,
        );
        cooked.assert_success();
        assert_eq!(
            cooked
                .stdout
                .lines()
                .filter(|line| line.starts_with("STARTER_PACKAGE_PUBLISHED "))
                .count(),
            1,
            "{}",
            cooked.stdout
        );

        let relocated = self.custody.root.join("relocated player");
        fs::rename(output.join(COOKED_DELIVERY_NAME), &relocated).unwrap();
        let executable_name = format!("player{}", std::env::consts::EXE_SUFFIX);
        let mut installed = fs::read_dir(&relocated)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        installed.sort_unstable();
        assert_eq!(
            installed,
            ["content.pack", "package.json", &executable_name]
        );
        assert!(output.join("nara-cook-cache").is_dir());
        for owned in [&self.root, &output, &self.binaries] {
            remove_owned_tree(&self.custody.root, owned);
            assert!(!owned.exists());
        }
        relocated.join(executable_name)
    }

    pub(super) fn run_exact_player(&mut self, executable: &Path) -> Output {
        let output = self.custody.player(executable, &self.cwd);
        output.assert_success();
        assert!(fs::read_dir(&self.cwd).unwrap().next().is_none());
        output
    }
}
