use std::{env, ffi::OsStr, thread, time::Duration};

use nara::project_workflow::cargo::{CargoBuildSelection, CargoMetadataFacts};
use serde_json::{Value, json};

use super::*;

const FIXTURE_HOST: &str = "x86_64-pc-windows-msvc";
const FIXTURE_PACKAGE: &str = "desktop-build-consumer";
const FIXTURE_PACKAGE_ID: &str = "path+file:///desktop-build-consumer#0.1.0";

#[test]
fn rustc_identity_requires_one_bounded_host_target() {
    let identity = parse_rustc_identity(
        b"rustc 1.91.0\nbinary: rustc\nhost: x86_64-pc-windows-msvc\nrelease: 1.91.0\n",
    )
    .expect("one rustc host target is admitted");
    assert_eq!(identity.host_target, "x86_64-pc-windows-msvc");

    assert!(parse_rustc_identity(b"rustc 1.91.0\n").is_none());
    assert!(
        parse_rustc_identity(b"rustc 1.91.0\nhost: x86_64-pc-windows-msvc\nhost: other-target\n")
            .is_none()
    );
}

#[test]
fn bounded_stdout_rejects_growth_before_appending_it() {
    let mut output = BoundedStdout::new(4);
    output.push(b"1234").unwrap();
    assert!(matches!(
        output.push(b"5"),
        Err(BoundedStdoutError::LimitExceeded)
    ));
    assert_eq!(output.take(), b"1234");
}

#[test]
fn tool_admission_rejects_relative_process_authority() {
    assert!(matches!(
        DesktopBuildTools::new(PathBuf::from("cargo"), PathBuf::from("rustc")),
        Err(DesktopBuildToolError::OverrideNotAbsolute {
            tool: DesktopBuildTool::Cargo
        })
    ));
}

#[cfg(windows)]
#[test]
fn tool_admission_preserves_behavioral_symlink_proxy_name() {
    let path = env::var_os("PATH").expect("the Rust test environment provides PATH");
    let proxy = env::split_paths(&path)
        .map(|directory| directory.join("rustc.exe"))
        .find(|candidate| {
            fs::symlink_metadata(candidate).is_ok_and(|metadata| metadata.file_type().is_symlink())
        });
    let Some(proxy) = proxy else {
        return;
    };
    let expected = fs::canonicalize(proxy.parent().expect("the rustc proxy has a parent"))
        .unwrap()
        .join(proxy.file_name().expect("the rustc proxy has a file name"));

    let admitted = DesktopBuildTools::new(proxy.clone(), proxy)
        .unwrap()
        .rustc()
        .to_path_buf();

    assert_eq!(admitted, expected);
}

#[test]
fn revision_mismatch_names_the_first_changed_authority_member() {
    let admitted = DesktopBuildInputRevision::new(
        ContentDigest::of_bytes(b"manifest-a"),
        ContentDigest::of_bytes(b"lock-a"),
    );
    assert_eq!(
        revision_mismatch(
            admitted,
            DesktopBuildInputRevision::new(
                ContentDigest::of_bytes(b"manifest-b"),
                ContentDigest::of_bytes(b"lock-b"),
            ),
        ),
        Some(DesktopBuildInputMember::Manifest)
    );
    assert_eq!(
        revision_mismatch(
            admitted,
            DesktopBuildInputRevision::new(admitted.manifest(), ContentDigest::of_bytes(b"lock-b"),),
        ),
        Some(DesktopBuildInputMember::Lockfile)
    );
}

#[test]
fn desktop_preset_uses_matching_metadata_and_build_selection_with_explicit_tools() {
    let request = fixture_request();
    let metadata = serde_json::to_vec(&fixture_metadata(&request)).unwrap();
    let invocation =
        desktop_build_invocation(&metadata, &request.manifest_path, FIXTURE_HOST).unwrap();

    assert_eq!(invocation.package().name(), FIXTURE_PACKAGE);
    assert_eq!(invocation.target_name(), "desktop");
    assert_eq!(invocation.profile(), "dev");
    assert_eq!(invocation.target_triple(), FIXTURE_HOST);
    assert!(!invocation.default_features());
    assert_eq!(invocation.features(), &["desktop"]);
    assert_eq!(
        invocation.metadata_features(),
        &["desktop", "renderer", "workspace-only"]
    );

    let metadata = metadata_command(&request, FIXTURE_HOST);
    let build = cargo_build_command(&request, &invocation, FIXTURE_HOST).unwrap();
    assert!(
        !metadata
            .arguments_ref()
            .iter()
            .any(|argument| argument == "--no-deps")
    );
    assert_eq!(
        argument_value(&metadata, "--filter-platform"),
        Some(OsStr::new(FIXTURE_HOST))
    );
    assert_eq!(
        argument_value(&build, "--target"),
        Some(OsStr::new(FIXTURE_HOST))
    );
    assert_eq!(
        argument_value(&build, "--package"),
        Some(OsStr::new(FIXTURE_PACKAGE))
    );
    assert_eq!(argument_value(&build, "--bin"), Some(OsStr::new("desktop")));
    assert_eq!(argument_value(&build, "--profile"), Some(OsStr::new("dev")));
    for command in [&metadata, &build] {
        assert_eq!(command.executable(), request.tools.cargo.as_path());
        assert_eq!(
            argument_value(command, "--manifest-path"),
            Some(request.manifest_path.as_os_str())
        );
        assert_eq!(
            argument_value(command, "--features"),
            Some(OsStr::new("desktop"))
        );
        assert!(
            command
                .arguments_ref()
                .iter()
                .any(|argument| argument == "--locked")
        );
        assert!(
            command
                .arguments_ref()
                .iter()
                .any(|argument| argument == "--no-default-features")
        );
        assert!(
            command.environment_ref().iter().any(|(name, value)| {
                name == "RUSTC" && value == request.tools.rustc.as_os_str()
            })
        );
    }
    assert!(build.environment_ref().iter().any(|(name, value)| {
        name == "CARGO_TARGET_DIR" && value == invocation.target_directory().as_os_str()
    }));
}

#[test]
fn desktop_candidate_retains_compiler_unit_features_separately_from_metadata() {
    let request = fixture_request();
    let metadata = fixture_metadata(&request);
    let invocation = desktop_build_invocation(
        &serde_json::to_vec(&metadata).unwrap(),
        &request.manifest_path,
        FIXTURE_HOST,
    )
    .unwrap();
    let executable = invocation
        .target_directory()
        .join(FIXTURE_HOST)
        .join("debug/desktop.exe");
    let artifact = json!({
        "reason": "compiler-artifact",
        "package_id": FIXTURE_PACKAGE_ID,
        "manifest_path": request.manifest_path,
        "target": metadata["packages"][0]["targets"][0],
        "profile": {"test": false},
        "features": ["desktop", "renderer"],
        "executable": executable,
        "fresh": true,
    });
    let stream = format!(
        "{artifact}\n{}\n",
        json!({"reason": "build-finished", "success": true}),
    );
    let mut decoder = CargoBuildDecoder::new(invocation.clone());
    decoder.push(stream.as_bytes()).unwrap();
    let build = decoder.finish().unwrap();

    let candidate = VerifiedDesktopBuildCandidate {
        source_revision: request.expected_revision,
        manifest_path: request.manifest_path,
        lockfile_path: request.lockfile_path,
        toolchain: DesktopBuildToolchainFacts {
            cargo_version_digest: ContentDigest::of_bytes(b"cargo version"),
            rustc_verbose_version_digest: ContentDigest::of_bytes(b"rustc -vV"),
        },
        metadata_digest: ContentDigest::of_bytes(&serde_json::to_vec(&metadata).unwrap()),
        invocation_digest: digest_build_invocation(&invocation),
        invocation,
        build,
    };

    assert_eq!(candidate.artifact().features(), &["desktop", "renderer"]);
    assert_ne!(
        candidate.artifact().features(),
        candidate.invocation().metadata_features()
    );
    assert_eq!(candidate.artifact().target_triple(), FIXTURE_HOST);
    assert_eq!(candidate.artifact_path(), executable.as_path());
}

#[test]
fn desktop_preset_requires_the_selected_packages_resolve_node() {
    let request = fixture_request();
    let mut metadata = fixture_metadata(&request);
    metadata["resolve"]["nodes"] = json!([]);

    assert!(matches!(
        desktop_build_invocation(
            &serde_json::to_vec(&metadata).unwrap(),
            &request.manifest_path,
            FIXTURE_HOST,
        ),
        Err(DesktopBuildFailure::Metadata(
            CargoMetadataFactsError::MissingSelectedResolveNode
        )),
    ));
}

#[test]
fn supervised_desktop_rejects_a_foreign_invocation_before_spawning_cargo() {
    let request = fixture_request();
    let metadata = serde_json::to_vec(&fixture_metadata(&request)).unwrap();
    let selection = CargoBuildSelection::new(
        FIXTURE_PACKAGE,
        "desktop",
        "dev",
        "wasm32-wasip1",
        ["desktop"],
        false,
    )
    .unwrap();
    let facts = CargoMetadataFacts::parse(&metadata, &request.manifest_path, &selection).unwrap();
    let invocation = facts.build_invocation(selection).unwrap();

    assert!(matches!(
        cargo_build_command(&request, &invocation, FIXTURE_HOST),
        Err(DesktopBuildFailure::NonNativeTarget),
    ));
}

#[test]
fn invocation_digest_binds_requested_selection_and_metadata_feature_observation() {
    let request = fixture_request();
    let metadata = fixture_metadata(&request);
    let invocation = |metadata: &Value, selection: CargoBuildSelection| {
        CargoMetadataFacts::parse(
            &serde_json::to_vec(metadata).unwrap(),
            &request.manifest_path,
            &selection,
        )
        .unwrap()
        .build_invocation(selection)
        .unwrap()
    };
    let select = |profile: &str, target: &str, features: &[&str], defaults| {
        CargoBuildSelection::new(
            FIXTURE_PACKAGE,
            "desktop",
            profile,
            target,
            features.iter().copied(),
            defaults,
        )
        .unwrap()
    };
    let baseline = invocation(&metadata, select("dev", FIXTURE_HOST, &["desktop"], false));
    let digest = digest_build_invocation(&baseline);
    for selection in [
        select("dev", FIXTURE_HOST, &["desktop", "renderer"], false),
        select("dev", FIXTURE_HOST, &["desktop"], true),
        select("release", FIXTURE_HOST, &["desktop"], false),
        select("dev", "wasm32-wasip1", &["desktop"], false),
    ] {
        let changed = invocation(&metadata, selection);
        assert_eq!(changed.metadata_features(), baseline.metadata_features());
        assert_ne!(digest_build_invocation(&changed), digest);
    }
    let mut changed = metadata;
    changed["resolve"]["nodes"][0]["features"] =
        json!(["desktop", "renderer", "shared", "workspace-only"]);
    let changed = invocation(&changed, select("dev", FIXTURE_HOST, &["desktop"], false));
    assert_eq!(changed.features(), baseline.features());
    assert_ne!(digest_build_invocation(&changed), digest);
}

fn fixture_request() -> DesktopBuildRequest {
    let executable = env::current_exe().unwrap();
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    DesktopBuildRequest {
        manifest_path: project_root.join("Cargo.toml"),
        lockfile_path: project_root.join("Cargo.lock"),
        project_root,
        expected_revision: DesktopBuildInputRevision::new(
            ContentDigest::of_bytes(b"manifest"),
            ContentDigest::of_bytes(b"lockfile"),
        ),
        tools: DesktopBuildTools {
            cargo: executable.clone(),
            rustc: executable,
        },
    }
}

fn fixture_metadata(request: &DesktopBuildRequest) -> Value {
    json!({
        "version": 1,
        "workspace_root": request.project_root,
        "target_directory": request.project_root.join("target"),
        "workspace_members": [FIXTURE_PACKAGE_ID],
        "packages": [{
            "id": FIXTURE_PACKAGE_ID,
            "name": FIXTURE_PACKAGE,
            "version": "0.1.0",
            "source": null,
            "manifest_path": request.manifest_path,
            "features": {
                "desktop": ["renderer"],
                "renderer": [],
                "shared": [],
                "workspace-only": [],
            },
            "targets": [{
                "name": "desktop",
                "kind": ["bin"],
                "crate_types": ["bin"],
                "src_path": request.project_root.join("src/bin/desktop.rs"),
                "required-features": ["desktop"],
            }],
        }],
        "resolve": {
            "root": FIXTURE_PACKAGE_ID,
            "nodes": [{
                "id": FIXTURE_PACKAGE_ID,
                "dependencies": [],
                "deps": [],
                "features": ["desktop", "renderer", "workspace-only"],
            }],
        },
    })
}

fn argument_value<'a>(command: &'a SupervisedProcessCommand, flag: &str) -> Option<&'a OsStr> {
    command
        .arguments_ref()
        .windows(2)
        .find(|arguments| arguments[0] == flag)
        .map(|arguments| arguments[1].as_os_str())
}

#[test]
fn cancellation_becomes_terminal_only_after_supervised_cleanup() {
    let executable = env::current_exe().unwrap();
    let project_root = executable.parent().unwrap().to_path_buf();
    let request = DesktopBuildRequest {
        manifest_path: project_root.join("Cargo.toml"),
        lockfile_path: project_root.join("Cargo.lock"),
        project_root,
        expected_revision: DesktopBuildInputRevision::new(
            ContentDigest::of_bytes(b"manifest"),
            ContentDigest::of_bytes(b"lockfile"),
        ),
        tools: DesktopBuildTools {
            cargo: executable.clone(),
            rustc: executable,
        },
    };
    let process = SupervisedProcessOwner::spawn(
        &crate::supervised_play::tests::helper_command("wait"),
        default_log_limits(),
    )
    .unwrap();
    let mut attempt = DesktopBuildAttempt::from_spawned_process(request, process);
    let start = Instant::now();

    attempt.request_cancel(start).unwrap();
    assert!(attempt.terminal().is_none());

    let deadline = start + Duration::from_secs(10);
    while attempt.terminal().is_none() {
        attempt.advance(Instant::now()).unwrap();
        assert!(Instant::now() < deadline, "build cleanup timed out");
        thread::sleep(Duration::from_millis(5));
    }

    assert!(matches!(
        attempt.terminal(),
        Some(DesktopBuildTerminal::Cancelled)
    ));
    assert!(attempt.process.is_none());
}
