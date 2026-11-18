//! Actual independent Host/Player delivery; no nested build runs in ordinary fixture tests.

#![cfg(any(windows, target_os = "linux"))]
#![forbid(unsafe_code)]

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

use engine::{
    asset::{AssetProductRef, ImportedProductId, StableAssetId},
    fs::ContentDigest,
    project_tooling::cargo_build::{CargoBuildLimits, build_executable},
    runtime_content::{RuntimeContentCatalog, RuntimeContentCatalogLimits},
};

#[path = "support/cooked_workspace.rs"]
mod cooked_workspace;
#[path = "support/workspace.rs"]
#[allow(dead_code)]
mod workspace;

use cooked_workspace::{CookedWorkspace, GENERATED, HOST_BIN, HOST_PACKAGE, PLAYER_BIN};
use workspace::{Temporary, Tools, install_native, selection};

#[test]
#[ignore = "actual serial Cargo Host/Player builds plus source-free native execution; shared root target required"]
fn compiled_cooker_publishes_a_relocatable_player_with_compiled_expectations_and_physical_travel() {
    let tools = Tools::pinned();
    let owner = Temporary::new();
    let generated = CookedWorkspace::new(&owner.root, &tools);
    let host_selection = selection(HOST_PACKAGE, HOST_BIN, &tools.host, &["cook"]);
    let host = build_executable(
        &generated.host_request(&tools, host_selection.clone(), CargoBuildLimits::default()),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(host.provenance().selection(), &host_selection);
    assert_eq!(
        host.provenance().resolved_features(),
        ["cook", "cook-entry"]
    );
    assert!(host.provenance().generated_rust_input().is_none());
    let host_directory = owner.root.join("host-execution");
    fs::create_dir(&host_directory).unwrap();
    let host_path = host_directory.join(format!("{HOST_BIN}{}", std::env::consts::EXE_SUFFIX));
    install_native(host.bytes(), &host_path);
    let output = owner.root.join("delivery-output");
    fs::create_dir(&output).unwrap();
    let mut command = Command::new(&host_path);
    command
        .current_dir(&host_directory)
        .arg(&generated.root)
        .arg(&output)
        .arg(&tools.cargo)
        .arg(&tools.rustc)
        .arg(&tools.host)
        .arg(&tools.target)
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_TARGET_DIR", &tools.target)
        .env(
            "NARA_CARGO_GENERATED_RUST_SOURCE",
            "compile_error!(\"inherited generated source must be cleared\");\n",
        );
    hide_window(&mut command);
    let (status, stdout, stderr) = run(
        &mut command,
        &owner.root,
        "cook",
        Duration::from_secs(25 * 60),
    );
    assert!(
        status.success(),
        "{status}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        stdout,
        "cooked-host ok actual-import cpu-target compound-aux deterministic-pack request-generated-a-b-a no-input-refused binding-fences published\n",
        "{stderr}"
    );
    let executable_name = format!("{PLAYER_BIN}{}", std::env::consts::EXE_SUFFIX);
    // Move the complete published directory; no member is patched with its new native location.
    let relocation = owner.root.join("unrelated-installation");
    fs::create_dir(&relocation).unwrap();
    assert!(relocation.canonicalize().unwrap().starts_with(&owner.root));
    let mut installations = Vec::new();
    let mut generated_inputs = Vec::new();
    for (published, name, profile) in [
        ("published", "Aurora", "cpu4-shipping"),
        ("bounded-a-preserved", "Aurora-captured-A", "cpu4-shipping"),
        ("captured-b", "Aurora-B", "cpu4-alternate-content"),
        ("captured-a-repeat", "Aurora-A-repeat", "cpu4-shipping"),
    ] {
        let published = output.join(published);
        assert_payload(&published, &executable_name);
        let package = fs::read(published.join("package.json")).unwrap();
        let envelope: serde_json::Value = serde_json::from_slice(&package).unwrap();
        let manifest = &envelope["payload"];
        assert_eq!(envelope["kind"], "player_package");
        assert_eq!(envelope["format_version"], 1);
        assert_eq!(
            manifest["build"]["selection"]["package"],
            "aurora-native-game"
        );
        assert_eq!(manifest["build"]["selection"]["bin"], PLAYER_BIN);
        assert_eq!(
            manifest["build"]["selection"]["cargo_profile"],
            "consumer-checked"
        );
        assert_eq!(manifest["build"]["selection"]["target_triple"], tools.host);
        assert_eq!(manifest["build"]["selection"]["default_features"], false);
        assert_eq!(
            manifest["build"]["selection"]["requested_features"],
            serde_json::json!(["ship"])
        );
        assert_eq!(
            manifest["build"]["resolved_features"],
            serde_json::json!(["player-entry", "quality", "ship"])
        );
        assert_eq!(manifest["generated_input_name"], GENERATED);
        assert_eq!(manifest["content_target"]["profile"], profile);
        assert!(
            manifest["build"]["declared_inputs"]
                .as_array()
                .unwrap()
                .iter()
                .all(|input| input["path"] != GENERATED)
        );
        let generated = &manifest["build"]["generated_rust_input"];
        assert_eq!(generated["path"], GENERATED);
        assert_eq!(
            generated["digest"][0],
            manifest["generated_input_digest"]["length"]
        );
        assert_eq!(
            generated["digest"][1],
            manifest["generated_input_digest"]["hash"]
        );
        generated_inputs.push(manifest["generated_input_digest"].clone());
        let relocated = relocation.join(name);
        assert!(published.canonicalize().unwrap().starts_with(&owner.root));
        assert!(!relocated.exists());
        fs::rename(&published, &relocated).unwrap();
        assert!(!published.exists());
        assert_eq!(fs::read(relocated.join("package.json")).unwrap(), package);
        installations.push((relocated, package));
    }
    assert_eq!(generated_inputs[0], generated_inputs[1]);
    assert_ne!(generated_inputs[1], generated_inputs[2]);
    assert_eq!(generated_inputs[1], generated_inputs[3]);
    // Authoring files, source sidecars, dependency lock/config and all generated store files go.
    assert!(generated.root.join("source/stripe.png.meta").exists());
    assert!(generated.root.join("source/suffix.txt").exists());
    assert!(generated.root.join("source-cache").is_dir());
    owner.remove_owned(&generated.root);
    owner.remove_owned(&host_directory);
    assert!(!generated.root.exists());
    let unrelated_cwd = owner.root.join("empty-working-directory");
    fs::create_dir(&unrelated_cwd).unwrap();
    let no_tools = owner.root.join("no-cargo-or-rustc");
    assert!(!no_tools.exists());
    for (index, (relocated, _)) in installations.iter().enumerate() {
        let (status, stdout, stderr) = run_player(
            &relocated.join(&executable_name),
            &unrelated_cwd,
            &no_tools,
            &owner.root,
            &format!("good-{index}"),
        );
        assert!(
            status.success(),
            "{status}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert_eq!(
            stdout,
            "cooked-player ok physical-starts=2/2 cancels=1/1 commits=3 rejected=0 source-free\n"
        );
    }
    let relocated = &installations[0].0;
    let alternate = &installations[2].0;
    let pack_a = fs::read(relocated.join("content.pack")).unwrap();
    let pack_b = fs::read(alternate.join("content.pack")).unwrap();
    assert_ne!(
        ContentDigest::of_bytes(&pack_a),
        ContentDigest::of_bytes(&pack_b)
    );
    assert_ne!(&pack_a[12..20], &pack_b[12..20]);
    for (directory, wrong_pack, original_pack, name) in [
        (relocated, &pack_b, &pack_a, "a-refuses-pack-b"),
        (alternate, &pack_a, &pack_b, "b-refuses-pack-a"),
    ] {
        fs::write(directory.join("content.pack"), wrong_pack).unwrap();
        let (status, stdout, stderr) = run_player(
            &directory.join(&executable_name),
            &unrelated_cwd,
            &no_tools,
            &owner.root,
            name,
        );
        assert_eq!(
            status.code(),
            Some(1),
            "{status}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert!(stdout.is_empty(), "{stdout}");
        assert_eq!(stderr, "cooked-player rejected InvalidPackHeader\n");
        fs::write(directory.join("content.pack"), original_pack).unwrap();
    }

    // Complete publication was valid. Only now corrupt the not-yet-demanded third Scene, keeping
    // its exact catalog and extent intact. Startup and two successful travels must still work.
    corrupt_deferred_scene(&relocated.join("content.pack"));
    let (status, stdout, stderr) = run_player(
        &relocated.join(&executable_name),
        &unrelated_cwd,
        &no_tools,
        &owner.root,
        "deferred-corruption",
    );
    assert!(
        status.success(),
        "{status}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        stdout,
        "cooked-player ok physical-starts=2/2 cancels=1/1 commits=2 rejected=1 source-free\n"
    );
    assert!(!generated.root.exists());
    for (relocated, package) in &installations {
        assert_eq!(&fs::read(relocated.join("package.json")).unwrap(), package);
        assert_payload(relocated, &executable_name);
    }
    assert!(fs::read_dir(&unrelated_cwd).unwrap().next().is_none());
}

fn assert_payload(directory: &Path, executable_name: &str) {
    let mut files = fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            entry.file_name().into_string().unwrap()
        })
        .collect::<Vec<_>>();
    let mut expected = vec![
        executable_name.to_owned(),
        "content.pack".to_owned(),
        "package.json".to_owned(),
    ];
    files.sort();
    expected.sort();
    assert_eq!(files, expected);
}

fn corrupt_deferred_scene(path: &Path) {
    let mut bytes = fs::read(path).unwrap();
    let original = ContentDigest::of_bytes(&bytes);
    assert_eq!(&bytes[..8], b"NARAPACK");
    let catalog_length =
        usize::try_from(u64::from_le_bytes(bytes[12..20].try_into().unwrap())).unwrap();
    let end = 20 + catalog_length;
    let catalog_digest = ContentDigest::of_bytes(&bytes[20..end]);
    let catalog = RuntimeContentCatalog::from_json_bytes(
        &bytes[20..end],
        RuntimeContentCatalogLimits::default(),
    )
    .unwrap();
    let deferred = AssetProductRef::new(
        StableAssetId::parse_str("9dc10000-0000-4000-8000-000000000003").unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap();
    let mut offset = end;
    let mut damaged = false;
    for member in catalog.members() {
        let length = usize::try_from(member.digest().length()).unwrap();
        if member.products().contains(&deferred) {
            assert!(!damaged && length > 0);
            bytes[offset + length / 2] ^= 1;
            damaged = true;
        }
        offset += length;
    }
    assert_eq!(offset, bytes.len());
    assert!(damaged);
    assert_eq!(ContentDigest::of_bytes(&bytes[20..end]), catalog_digest);
    assert_ne!(ContentDigest::of_bytes(&bytes), original);
    fs::write(path, bytes).unwrap();
}

fn run_player(
    executable: &Path,
    cwd: &Path,
    no_tools: &Path,
    logs: &Path,
    name: &str,
) -> (ExitStatus, String, String) {
    let mut command = Command::new(executable);
    command.env_clear().env("PATH", no_tools).current_dir(cwd);
    #[cfg(windows)]
    for variable in ["SystemRoot", "WINDIR"] {
        if let Some(value) = std::env::var_os(variable) {
            command.env(variable, value);
        }
    }
    hide_window(&mut command);
    run(&mut command, logs, name, Duration::from_secs(60))
}

fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = command;
}

fn run(
    command: &mut Command,
    logs: &Path,
    name: &str,
    timeout: Duration,
) -> (ExitStatus, String, String) {
    let stdout: PathBuf = logs.join(format!("{name}.stdout.log"));
    let stderr: PathBuf = logs.join(format!("{name}.stderr.log"));
    command
        .stdin(Stdio::null())
        .stdout(File::create(&stdout).unwrap())
        .stderr(File::create(&stderr).unwrap());
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            polled => {
                let killed = child.kill();
                let reaped = child.wait();
                panic!(
                    "{name}: watchdog {polled:?}; kill={killed:?}; wait={reaped:?}\nstdout:\n{}\nstderr:\n{}",
                    fs::read_to_string(&stdout).unwrap(),
                    fs::read_to_string(&stderr).unwrap()
                );
            }
        }
    };
    drop(child);
    for path in [&stdout, &stderr] {
        assert!(
            fs::metadata(path).unwrap().len() <= 256 * 1024,
            "finite consumer output"
        );
    }
    (
        status,
        fs::read_to_string(stdout).unwrap(),
        fs::read_to_string(stderr).unwrap(),
    )
}
