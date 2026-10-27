//! Real managed images exercise bootstrap framing and graph admission directly.
//! Session execution and its process supervision are qualified by the other consumers.

#![cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]

use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

use serde_json::Value;

const ENTRY: &str = "Nara.SourceToolProbe.dll";
const DEPENDENCY: &str = "Nara.SourceToolProbeDependency.dll";
const FAILURE: &[u8] = b"Nara managed tool bootstrap failed.\n";

fn environment_path(name: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(name)
            .unwrap_or_else(|| panic!("Missing {name}; run the behavior-language fixture runner")),
    );
    assert!(path.is_absolute(), "{name} must be an absolute path");
    path
}

fn image(directory_variable: &str, name: &str) -> Vec<u8> {
    let path = environment_path(directory_variable).join(name);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("Cannot read fixture {}: {error}", path.display()));
    assert!(
        !bytes.is_empty() && bytes.len() <= 16 * 1024 * 1024,
        "Fixture image must fit the bootstrap image limit"
    );
    bytes
}

fn header(version: u32, count: u32, entry: u32) -> Vec<u8> {
    let mut bytes = b"NARATOOL".to_vec();
    for value in [version, count, entry] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn envelope(images: &[&[u8]], entry: u32, request: &[u8]) -> Vec<u8> {
    let mut bytes = header(1, u32::try_from(images.len()).unwrap(), entry);
    for image in images {
        bytes.extend_from_slice(&u64::try_from(image.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(image);
    }
    bytes.extend_from_slice(request);
    bytes
}

fn run(case: &str, input: &[u8]) -> Output {
    let directory = environment_path("NARA_SOURCE_TOOL_BOOTSTRAP_DIR");
    let framework = std::env::var("NARA_SOURCE_TOOL_FRAMEWORK")
        .expect("Runner must select an exact Microsoft.NETCore.App version");
    assert!(!framework.is_empty());
    let mut command = Command::new(environment_path("NARA_SOURCE_TOOL_DOTNET"));
    command
        .env_clear()
        .env("DOTNET_EnableDiagnostics", "0")
        .env("DOTNET_MULTILEVEL_LOOKUP", "0")
        .args([
            "exec",
            "--fx-version",
            &framework,
            "--roll-forward",
            "Disable",
        ])
        .arg(directory.join("Nara.SourceToolHost.dll"))
        .current_dir(&directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let mut child = command
        .spawn()
        .unwrap_or_else(|error| panic!("{case}: cannot spawn the managed bootstrap: {error}"));
    // This is the sole stdin writer. Drop it before waiting so the unchanged JSON tail
    // and deliberately truncated envelopes both terminate at EOF. These fixtures emit
    // only a probe line, one description, or the fixed rejection diagnostic.
    let written = {
        let mut stdin = child.stdin.take().expect("Bootstrap stdin is piped");
        stdin.write_all(input)
    };
    let output = child
        .wait_with_output()
        .unwrap_or_else(|error| panic!("{case}: cannot collect the managed bootstrap: {error}"));
    assert!(written.is_ok(), "{case}: cannot write fixture: {written:?}");
    assert!(
        output.stdout.len() <= 64 * 1024,
        "{case}: stdout exceeds 64 KiB"
    );
    assert!(
        output.stderr.len() <= 16 * 1024,
        "{case}: stderr exceeds 16 KiB"
    );
    output
}

fn assert_rejected(case: &str, input: &[u8]) {
    let output = run(case, input);
    assert!(
        !output.status.success(),
        "{case}: bootstrap accepted the fixture"
    );
    assert!(
        output.stdout.is_empty(),
        "{case}: entry ran before rejection"
    );
    assert_eq!(
        output.stderr, FAILURE,
        "{case}: rejection diagnostic changed"
    );
}

#[test]
fn managed_bootstrap_runs_private_main_with_exact_dependency_images() {
    for (variant, expected) in [
        ("V1", b"managed-probe:1\r\n".as_slice()),
        ("V2", b"managed-probe:2\r\n".as_slice()),
    ] {
        let variable = format!("NARA_MANAGED_PROBE_{variant}_DIR");
        let entry = image(&variable, ENTRY);
        let dependency = image(&variable, DEPENDENCY);
        // Select a nonzero entry index; the dependency is an ordinary library image.
        let output = run(variant, &envelope(&[&dependency, &entry], 1, &[]));
        assert!(output.status.success(), "{variant}: {:?}", output.status);
        assert_eq!(
            output.stdout, expected,
            "{variant}: dependency did not execute"
        );
        assert!(
            output.stderr.is_empty(),
            "{variant}: unexpected diagnostics"
        );
    }
}

#[test]
fn managed_bootstrap_rejects_missing_mismatched_and_duplicate_dependency_images() {
    let entry = image("NARA_MANAGED_PROBE_V1_DIR", ENTRY);
    let v1 = image("NARA_MANAGED_PROBE_V1_DIR", DEPENDENCY);
    let v2 = image("NARA_MANAGED_PROBE_V2_DIR", DEPENDENCY);
    // Both revisions are real, runnable DLLs with the same simple assembly name.
    // ALC's higher-version unification must not admit the entry's 1.0.0.0 edge.
    for (case, images) in [
        ("missing dependency", vec![entry.as_slice()]),
        (
            "higher dependency version",
            vec![entry.as_slice(), v2.as_slice()],
        ),
        (
            "duplicate identical image",
            vec![entry.as_slice(), v1.as_slice(), v1.as_slice()],
        ),
        (
            "duplicate simple name",
            vec![entry.as_slice(), v1.as_slice(), v2.as_slice()],
        ),
    ] {
        assert_rejected(case, &envelope(&images, 0, &[]));
    }
}

#[test]
fn managed_bootstrap_rejects_async_and_sta_entries_before_their_output() {
    for variant in ["ASYNC", "STA"] {
        let variable = format!("NARA_MANAGED_PROBE_{variant}_DIR");
        let entry = image(&variable, ENTRY);
        let dependency = image(&variable, DEPENDENCY);
        assert_rejected(variant, &envelope(&[&entry, &dependency], 0, &[]));
    }
}

#[test]
fn managed_bootstrap_rejects_malformed_and_truncated_envelopes() {
    let entry = image("NARA_MANAGED_PROBE_V1_DIR", ENTRY);
    let dependency = image("NARA_MANAGED_PROBE_V1_DIR", DEPENDENCY);
    let valid = envelope(&[&entry, &dependency], 0, &[]);
    let mut bad_magic = header(1, 1, 0);
    bad_magic[0] = b'X';
    let mut truncated_length = header(1, 1, 0);
    truncated_length.extend_from_slice(&[1, 0, 0, 0]);
    // Declare an oversized image without allocating or sending its body.
    let mut oversized = header(1, 1, 0);
    oversized.extend_from_slice(&(16_u64 * 1024 * 1024 + 1).to_le_bytes());
    for (case, input) in [
        ("truncated header", header(1, 1, 0)[..19].to_vec()),
        ("bad magic", bad_magic),
        ("unsupported envelope version", header(2, 1, 0)),
        ("zero image count", header(1, 0, 0)),
        ("too many images", header(1, 65, 0)),
        ("entry outside package", header(1, 1, 1)),
        ("truncated image length", truncated_length),
        ("truncated image", valid[..valid.len() - 1].to_vec()),
        ("oversized declared image", oversized),
    ] {
        assert_rejected(case, &input);
    }
}

#[test]
fn managed_bootstrap_preserves_enemy_stats_describe_request_after_images() {
    let entry = image("NARA_SOURCE_TOOL_V1_DIR", "EnemyStatsTool.dll");
    // Keep ordinary wire bytes after the last image, including surrounding whitespace.
    let request = br#"  {"protocol_version":1,"request":739,"operation":"describe",
"name":"","fields":{},"parameters":{}}
"#;
    let output = run("EnemyStats describe", &envelope(&[&entry], 0, request));
    assert!(output.status.success(), "{:?}", output.status);
    assert!(output.stderr.is_empty(), "Description emitted diagnostics");
    // Parsing the entire stdout also rejects any bootstrap/probe debug output.
    let response: Value = serde_json::from_slice(&output.stdout)
        .expect("Bootstrap must return only the ordinary EnemyStats JSON response");
    assert_eq!(response["protocol_version"], 1);
    assert_eq!(response["request"], 739);
    assert_eq!(response["result"]["kind"], "description");
    assert_eq!(
        response["result"]["catalog"]["kind"],
        "component_schema_catalog"
    );
    assert_eq!(
        response["result"]["catalog"]["payload"]["components"][0]["id"],
        "trial.enemy.EnemyStats"
    );
}
