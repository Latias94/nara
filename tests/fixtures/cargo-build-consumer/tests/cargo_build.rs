//! Public consumer qualification. These ignored tests run actual, serialized nested Cargo builds.

#![cfg(any(windows, target_os = "linux"))]
#![forbid(unsafe_code)]

use std::{fs, path::Path};

use engine::{
    core::ByteLimit,
    fs::{ContentDigest, FsError, RelativePath},
    project_tooling::cargo_build::{
        CapturedCargoExecutable, CargoBuildFailure, CargoBuildLimits, CargoBuildPoint,
        CargoBuildRequest, build_executable,
    },
    project_workflow::cargo::CargoBuildSelection,
    runtime_content::RuntimeContentTarget,
};

#[path = "../../runtime-runner/renamed-root/tests/support/processed_input_probe.rs"]
#[allow(dead_code)]
mod probe;
#[path = "../../runtime-runner/renamed-root/tests/support/processed_input_fixture.rs"]
mod processed_input_fixture;
#[path = "support/workspace.rs"]
mod workspace;

use workspace::{
    FOREIGN_BIN, FOREIGN_INPUTS, FOREIGN_MARKER, FOREIGN_PACKAGE, FOREIGN_TARGET, GENERATION,
    GOOD_GENERATION, HOST_BIN, HOST_INPUTS, HOST_MARKER, HOST_PACKAGE, PLAYER_BIN, PLAYER_INPUTS,
    PLAYER_PACKAGE, PROFILE, Temporary, Tools, Workspace, install_native, run_host, selection,
};

#[test]
#[ignore = "actual pinned Cargo/rustc builds; run serially with the shared repository target"]
fn custom_workspace_captures_host_tool_and_runs_native_player_after_source_removal() {
    let tools = Tools::pinned();
    let workspace = Workspace::new(&tools, true);
    let host_selection = selection(HOST_PACKAGE, HOST_BIN, &tools.host, &["trial-host"]);
    assert!(
        workspace
            .host_metadata_features(&tools, &host_selection)
            .iter()
            .any(|feature| feature == "metadata-only"),
        "the decoy must widen metadata while the selected compiler unit remains narrower",
    );
    assert_mandatory_inventory(&workspace, &tools, &host_selection);
    let tool_bytes = usize::try_from(
        fs::metadata(&tools.cargo)
            .unwrap()
            .len()
            .max(fs::metadata(&tools.rustc).unwrap().len()),
    )
    .unwrap();
    let limits = CargoBuildLimits {
        tool_bytes: ByteLimit::new(tool_bytes).unwrap(),
        executable_bytes: ByteLimit::new(1024 * 1024).unwrap(),
        ..CargoBuildLimits::default()
    };
    assert!(limits.tool_bytes.get() > limits.executable_bytes.get());
    let too_small = workspace.request_with_limits(
        &tools,
        "host-kit/Cargo.toml",
        host_selection.clone(),
        HOST_INPUTS,
        CargoBuildLimits {
            tool_bytes: ByteLimit::new(tool_bytes - 1).unwrap(),
            ..limits
        },
    );
    assert!(matches!(
        build_executable(&too_small, |_| Ok(())),
        Err(CargoBuildFailure::Filesystem(FsError::ByteLimitExceeded { limit }))
            if limit == (tool_bytes - 1) as u64
    ));
    let request = workspace.request_with_limits(
        &tools,
        "host-kit/Cargo.toml",
        host_selection.clone(),
        HOST_INPUTS,
        limits,
    );
    assert!(matches!(
        workspace
            .request(
                &tools,
                "host-kit/Cargo.toml",
                host_selection.clone(),
                HOST_INPUTS
            )
            .with_target_directory("relative-output"),
        Err(CargoBuildFailure::InvalidTargetDirectory),
    ));
    let mut points = Vec::new();
    let mut reentry_attempts = 0;
    let a = build_executable(&request, |point| {
        points.push(point);
        if point == CargoBuildPoint::AfterBuild {
            reentry_attempts += 1;
            let mut inner_build_started = false;
            let inner = build_executable(&request, |inner_point| {
                inner_build_started |= inner_point == CargoBuildPoint::BeforeBuild;
                Ok(())
            })
            .expect_err("the outer attempt must retain its cooperative target-directory lock");
            assert!(matches!(inner, CargoBuildFailure::TargetBusy), "{inner:?}");
            assert!(
                !inner_build_started,
                "reentry must reject before starting another build",
            );
        }
        Ok(())
    })
    .expect("build the explicitly selected custom Host binary despite the same-bin decoy");
    assert_eq!(reentry_attempts, 1);
    assert_eq!(
        points,
        [
            CargoBuildPoint::BeforeMetadata,
            CargoBuildPoint::BeforeBuild,
            CargoBuildPoint::AfterBuild,
            CargoBuildPoint::BeforeCapture,
            CargoBuildPoint::BeforeReturn,
        ],
    );
    assert_provenance(
        &a,
        &workspace,
        &tools,
        &host_selection,
        &["expanded", "host-entry", "trial-host"],
        HOST_INPUTS,
    );
    assert!(
        !workspace.root.join(HOST_MARKER).exists(),
        "building must never launch the tool",
    );
    assert_original_output(&workspace, &host_selection, &a);
    let original_bytes = a.bytes().to_vec();
    let original_digest = a.digest();
    let original_provenance = a.provenance().digest();
    let retained = Temporary::new();
    run_host(a.bytes(), &retained.root, "a-after-rejected-reentry");

    // Equivalent resolved features still retain the trusted Host's distinct requested features.
    let explicit = selection(
        HOST_PACKAGE,
        HOST_BIN,
        &tools.host,
        &["trial-host", "expanded"],
    );
    let equivalent = build_executable(
        &workspace.request(&tools, "host-kit/Cargo.toml", explicit.clone(), HOST_INPUTS),
        |_| Ok(()),
    )
    .unwrap();
    assert_provenance(
        &equivalent,
        &workspace,
        &tools,
        &explicit,
        &["expanded", "host-entry", "trial-host"],
        HOST_INPUTS,
    );
    run_host(
        equivalent.bytes(),
        &retained.root,
        "equivalent-feature-selection",
    );
    assert_ne!(
        equivalent.provenance().selection().features(),
        a.provenance().selection().features(),
    );
    assert_ne!(equivalent.provenance().digest(), original_provenance);
    drop(equivalent);

    workspace.write(
        GENERATION,
        "compile_error!(\"intentional bad B consumer\");\n",
    );
    let failed_b = build_executable(&request, |_| Ok(())).unwrap_err();
    assert!(
        matches!(failed_b, CargoBuildFailure::ProcessFailed),
        "{failed_b:?}",
    );
    assert_retained_a(&a, &original_bytes, original_digest, original_provenance);
    run_host(a.bytes(), &retained.root, "a-after-b-compile-failed");

    workspace.write(GENERATION, GOOD_GENERATION);
    // Satisfy every local source condition; only dependency-qualified target eligibility is absent.
    let missing_feature = selection(
        HOST_PACKAGE,
        HOST_BIN,
        &tools.host,
        &["host-entry", "expanded"],
    );
    let missing_feature = build_executable(
        &workspace.request(&tools, "host-kit/Cargo.toml", missing_feature, HOST_INPUTS),
        |_| Ok(()),
    )
    .unwrap_err();
    assert!(
        matches!(missing_feature, CargoBuildFailure::ProcessFailed,),
        "{missing_feature:?}",
    );
    assert_retained_a(&a, &original_bytes, original_digest, original_provenance);
    run_host(
        a.bytes(),
        &retained.root,
        "a-after-b-required-feature-failed",
    );

    // The compiled module is declared explicitly. Mutating it after capture cannot escape as A.
    let mut changed_before_return = false;
    let stale = build_executable(&request, |point| {
        if point == CargoBuildPoint::BeforeReturn {
            changed_before_return = true;
            workspace.write(
                GENERATION,
                "pub const GENERATION: &str = \"stale-before-return\";\n",
            );
        }
        Ok(())
    })
    .unwrap_err();
    assert!(changed_before_return);
    assert!(matches!(stale, CargoBuildFailure::Stale), "{stale:?}");
    assert_retained_a(&a, &original_bytes, original_digest, original_provenance);

    let mut abort_points = Vec::new();
    let aborted = build_executable(&request, |point| {
        abort_points.push(point);
        Err(CargoBuildFailure::Aborted)
    })
    .unwrap_err();
    assert_eq!(abort_points, [CargoBuildPoint::BeforeMetadata]);
    assert!(matches!(aborted, CargoBuildFailure::Aborted), "{aborted:?}");
    assert!(!workspace.root.join(HOST_MARKER).exists());

    // Host tools and Player are separate selections, not a shared preset or implicit bin choice.
    let player_selection = selection(PLAYER_PACKAGE, PLAYER_BIN, &tools.host, &["trial-player"]);
    let player = build_executable(
        &workspace.request(
            &tools,
            "player-kit/Cargo.toml",
            player_selection.clone(),
            PLAYER_INPUTS,
        ),
        |_| Ok(()),
    )
    .expect("build the actual renamed-engine processed-input Player");
    assert_provenance(
        &player,
        &workspace,
        &tools,
        &player_selection,
        &["expanded", "player-entry", "trial-player"],
        PLAYER_INPUTS,
    );
    assert_original_output(&workspace, &player_selection, &player);
    assert_ne!(player.provenance().selection(), a.provenance().selection());
    assert_ne!(player.digest(), a.digest());
    assert!(!workspace.root.join(HOST_MARKER).exists());

    workspace.replace_output(&host_selection);
    workspace.replace_output(&player_selection);
    assert_retained_a(&a, &original_bytes, original_digest, original_provenance);
    assert_ne!(file_digest(&workspace.output(&host_selection)), a.digest());
    assert_ne!(
        file_digest(&workspace.output(&player_selection)),
        player.digest(),
    );
    workspace.remove_source();
    assert!(!workspace.root.exists());
    run_host(
        a.bytes(),
        &retained.root,
        "a-after-output-replacement-and-source-removal",
    );

    // The existing reader and all seven existing oracles run only from the captured native bytes.
    // The content expectation names the actual selected triple/profile, not the old fixture label.
    let target = RuntimeContentTarget::new(
        player.provenance().selection().target_triple(),
        player.provenance().selection().profile(),
    )
    .unwrap();
    assert_eq!(target.triple(), tools.host);
    assert_eq!(target.profile(), PROFILE);
    let executable_name = format!("{PLAYER_BIN}{}", std::env::consts::EXE_SUFFIX);
    processed_input_fixture::run_cases(&executable_name, &target, |executable| {
        install_native(player.bytes(), executable);
    });
    assert!(
        !workspace.root.exists(),
        "Runtime must not recreate or build its source workspace",
    );
    assert_eq!(ContentDigest::of_bytes(player.bytes()), player.digest());
}

#[test]
#[ignore = "actual installed wasm32-wasip1 build only; run serially with the shared repository target"]
fn foreign_wasi_build_returns_bytes_without_execution_and_keeps_native_host_tool_usable() {
    let tools = Tools::pinned();
    tools.require_foreign_std();
    // This workspace deliberately has no Nara dependency. It qualifies the Cargo build boundary,
    // not Nara's WASI Runtime or a foreign target's content delivery and execution.
    let workspace = Workspace::new(&tools, false);
    let host_selection = selection(HOST_PACKAGE, HOST_BIN, &tools.host, &["trial-host"]);
    let host = build_executable(
        &workspace.request(
            &tools,
            "host-kit/Cargo.toml",
            host_selection.clone(),
            HOST_INPUTS,
        ),
        |_| Ok(()),
    )
    .unwrap();
    assert_provenance(
        &host,
        &workspace,
        &tools,
        &host_selection,
        &["expanded", "host-entry", "trial-host"],
        HOST_INPUTS,
    );
    let native_bytes = host.bytes().to_vec();
    let foreign_selection = selection(
        FOREIGN_PACKAGE,
        FOREIGN_BIN,
        FOREIGN_TARGET,
        &["trial-wasi"],
    );
    let foreign = build_executable(
        &workspace.request(
            &tools,
            "foreign-kit/Cargo.toml",
            foreign_selection.clone(),
            FOREIGN_INPUTS,
        ),
        |_| Ok(()),
    )
    .expect("actually compile the selected custom binary to installed wasm32-wasip1");
    assert_provenance(
        &foreign,
        &workspace,
        &tools,
        &foreign_selection,
        &["expanded", "trial-wasi", "wasi-entry"],
        FOREIGN_INPUTS,
    );
    assert!(foreign.bytes().len() > 8);
    assert_eq!(&foreign.bytes()[..8], b"\0asm\x01\0\0\0");
    assert_ne!(foreign.provenance().selection().target_triple(), tools.host);
    assert_eq!(foreign.provenance().toolchain().host_target(), tools.host);
    assert_original_output(&workspace, &foreign_selection, &foreign);
    assert_original_output(&workspace, &host_selection, &host);
    assert_eq!(host.bytes(), native_bytes);
    assert!(!workspace.root.join(FOREIGN_MARKER).exists());
    assert!(!workspace.root.join(HOST_MARKER).exists());
    let retained = Temporary::new();
    workspace.remove_source();
    run_host(
        host.bytes(),
        &retained.root,
        "native-host-after-foreign-build",
    );
    assert_eq!(ContentDigest::of_bytes(foreign.bytes()), foreign.digest());
    // Deliberately no execution, runner lookup, Runtime admission or Ready assertion for foreign.
}

fn assert_mandatory_inventory(
    workspace: &Workspace,
    tools: &Tools,
    selected: &CargoBuildSelection,
) {
    for missing in ["Cargo.toml", "Cargo.lock", "host-kit/Cargo.toml"] {
        let result = CargoBuildRequest::new(
            &workspace.root,
            RelativePath::new("host-kit/Cargo.toml").unwrap(),
            selected.clone(),
            tools.build_tools(),
            HOST_INPUTS
                .iter()
                .copied()
                .filter(|path| *path != missing)
                .map(|path| RelativePath::new(path).unwrap()),
            CargoBuildLimits::default(),
        );
        let error = result
            .err()
            .expect("a manifest/lockfile omission must reject the request");
        assert!(
            matches!(error, CargoBuildFailure::MissingDeclaredInput),
            "{missing}: {error:?}",
        );
    }
    let inputs: Vec<_> = HOST_INPUTS
        .iter()
        .copied()
        .filter(|path| *path != "host-kit/src/main.rs")
        .collect();
    let mut built = false;
    let error = build_executable(
        &workspace.request(tools, "host-kit/Cargo.toml", selected.clone(), &inputs),
        |point| {
            built |= point == CargoBuildPoint::BeforeBuild;
            Ok(())
        },
    )
    .unwrap_err();
    assert!(
        matches!(error, CargoBuildFailure::MissingDeclaredInput),
        "{error:?}",
    );
    assert!(
        !built,
        "metadata's actual selected source is mandatory before a build starts",
    );
}

fn assert_provenance(
    captured: &CapturedCargoExecutable,
    workspace: &Workspace,
    tools: &Tools,
    selected: &CargoBuildSelection,
    resolved: &[&str],
    inventory: &[&str],
) {
    let provenance = captured.provenance();
    assert_eq!(provenance.selection(), selected);
    assert_eq!(provenance.selection().profile(), PROFILE);
    assert!(!provenance.selection().default_features());
    assert_eq!(
        provenance
            .resolved_features()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        resolved,
    );
    assert_ne!(
        provenance.selection().features(),
        provenance.resolved_features()
    );
    assert_eq!(provenance.toolchain().host_target(), tools.host);
    assert_eq!(
        provenance.toolchain().cargo_digest(),
        file_digest(&tools.cargo)
    );
    assert_eq!(
        provenance.toolchain().rustc_digest(),
        file_digest(&tools.rustc)
    );
    assert!(!captured.bytes().is_empty());
    assert_eq!(captured.digest(), ContentDigest::of_bytes(captured.bytes()));
    assert_eq!(captured.digest().length(), captured.bytes().len() as u64);
    assert!(provenance.metadata_digest().length() > 0);
    assert!(provenance.digest().length() > 0);
    assert_ne!(provenance.digest(), captured.digest());
    let mut expected = inventory.to_vec();
    expected.sort_unstable();
    assert_eq!(
        provenance
            .declared_inputs()
            .iter()
            .map(|input| input.path())
            .collect::<Vec<_>>(),
        expected,
    );
    for input in provenance.declared_inputs() {
        assert_eq!(
            input.digest(),
            file_digest(&workspace.root.join(input.path())),
            "{}",
            input.path(),
        );
    }
}

fn assert_original_output(
    workspace: &Workspace,
    selected: &CargoBuildSelection,
    captured: &CapturedCargoExecutable,
) {
    assert_eq!(file_digest(&workspace.output(selected)), captured.digest());
}

fn assert_retained_a(
    captured: &CapturedCargoExecutable,
    bytes: &[u8],
    digest: ContentDigest,
    provenance: ContentDigest,
) {
    assert_eq!(captured.bytes(), bytes);
    assert_eq!(captured.digest(), digest);
    assert_eq!(ContentDigest::of_bytes(captured.bytes()), digest);
    assert_eq!(captured.provenance().digest(), provenance);
}

fn file_digest(path: &Path) -> ContentDigest {
    ContentDigest::of_bytes(&fs::read(path).unwrap())
}
