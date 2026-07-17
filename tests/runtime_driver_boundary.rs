use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use nara::app::{App, RuntimeAdmissionError, RuntimeCandidate, RuntimeInstance};
use serde_json::Value;

const NARA_APP_DEPENDENCY_ALLOWLIST: [&str; 4] = ["bevy_ecs", "blake3", "nara_ecs", "thiserror"];

fn start_runtime(app: App) -> RuntimeInstance {
    let candidate = RuntimeCandidate::admit(app.seal().unwrap()).unwrap();
    match candidate.complete_startup() {
        Ok(ready) => ready.promote(),
        Err(failure) => {
            panic!("candidate startup failed: {:?}", failure.fault())
        }
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn cargo_metadata() -> Value {
    let root = workspace_root();
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--locked",
            "--manifest-path",
        ])
        .arg(root.join("Cargo.toml"))
        .current_dir(&root)
        .output()
        .expect("cargo metadata must be executable");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata must return JSON")
}

fn rust_source_files(root: &Path) -> Vec<PathBuf> {
    fn collect(directory: &Path, files: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()));
        for entry in entries {
            let path = entry
                .expect("source directory entry must be readable")
                .path();
            if path.is_dir() {
                collect(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect(root, &mut files);
    files.sort();
    files
}

// This lexer removes comments and literals, then preserves identifiers and punctuation as tokens.
// The boundary assertions therefore survive formatting changes without matching prose or diagnostics.
fn rust_tokens(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index..].starts_with(b"//") {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes[index..].starts_with(b"/*") {
            index += 2;
            let mut depth = 1usize;
            while index < bytes.len() && depth > 0 {
                if bytes[index..].starts_with(b"/*") {
                    depth += 1;
                    index += 2;
                } else if bytes[index..].starts_with(b"*/") {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
            continue;
        }
        if let Some((content_start, hashes)) = raw_string_start(bytes, index) {
            index = skip_raw_string(bytes, content_start, hashes);
            continue;
        }
        if bytes[index] == b'"'
            || (bytes[index] == b'b' && bytes.get(index + 1).is_some_and(|byte| *byte == b'"'))
        {
            index += usize::from(bytes[index] == b'b') + 1;
            while index < bytes.len() {
                match bytes[index] {
                    b'\\' => index = (index + 2).min(bytes.len()),
                    b'"' => {
                        index += 1;
                        break;
                    }
                    _ => index += 1,
                }
            }
            continue;
        }
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(source[start..index].to_owned());
            continue;
        }
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        tokens.push(char::from(bytes[index]).to_string());
        index += 1;
    }

    tokens
}

fn raw_string_start(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    let mut cursor = index;
    if bytes.get(cursor) == Some(&b'b') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'r') {
        return None;
    }
    cursor += 1;
    let hashes_start = cursor;
    while bytes.get(cursor) == Some(&b'#') {
        cursor += 1;
    }
    (bytes.get(cursor) == Some(&b'"')).then_some((cursor + 1, cursor - hashes_start))
}

fn skip_raw_string(bytes: &[u8], mut index: usize, hashes: usize) -> usize {
    while index < bytes.len() {
        if bytes[index] == b'"'
            && bytes
                .get(index + 1..index + 1 + hashes)
                .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
        {
            return index + 1 + hashes;
        }
        index += 1;
    }
    bytes.len()
}

#[derive(Debug)]
struct PublicFunction {
    name: String,
    parameters: Vec<String>,
}

fn public_functions(tokens: &[String]) -> Vec<PublicFunction> {
    let mut functions = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index] != "pub" || tokens.get(index + 1).is_some_and(|token| token == "(") {
            index += 1;
            continue;
        }

        let mut function_index = index + 1;
        while tokens
            .get(function_index)
            .is_some_and(|token| matches!(token.as_str(), "async" | "const" | "unsafe" | "extern"))
        {
            function_index += 1;
        }
        if tokens.get(function_index).is_none_or(|token| token != "fn") {
            index += 1;
            continue;
        }
        let Some(name) = tokens.get(function_index + 1) else {
            break;
        };
        let Some(open) = tokens[function_index + 2..]
            .iter()
            .position(|token| token == "(")
            .map(|offset| function_index + 2 + offset)
        else {
            break;
        };
        let Some(close) = matching_delimiter(tokens, open, "(", ")") else {
            break;
        };
        functions.push(PublicFunction {
            name: name.clone(),
            parameters: tokens[open + 1..close].to_vec(),
        });
        index = close + 1;
    }
    functions
}

fn matching_delimiter(tokens: &[String], open: usize, left: &str, right: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        if token == left {
            depth += 1;
        } else if token == right {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn accepts_mutable_reference_to(parameters: &[String], target: &str) -> bool {
    parameters.iter().enumerate().any(|(index, token)| {
        if token != "&" {
            return false;
        }
        let mut cursor = index + 1;
        if parameters.get(cursor).is_some_and(|token| token == "'") {
            cursor += 2;
        }
        if parameters.get(cursor).is_none_or(|token| token != "mut") {
            return false;
        }
        parameters[cursor + 1..]
            .iter()
            .take_while(|token| token.as_str() != ",")
            .any(|token| token == target)
    })
}

#[test]
fn managed_runtime_rejects_the_raw_app_runner_path() {
    let mut app = App::new();
    app.set_runner(|_| Ok(nara::app::AppExit::Success)).unwrap();
    let sealed = app.seal().unwrap();

    let failure = RuntimeCandidate::admit(sealed).unwrap_err();
    assert_eq!(failure.error(), RuntimeAdmissionError::RawRunnerInstalled);
}

#[test]
fn platform_public_runner_accepts_runtime_without_raw_app_authority() {
    let source_root = workspace_root().join("crates/nara_winit/src");
    let functions = rust_source_files(&source_root)
        .into_iter()
        .flat_map(|path| {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            public_functions(&rust_tokens(&source))
        })
        .collect::<Vec<_>>();

    let raw_app_entries = functions
        .iter()
        .filter(|function| accepts_mutable_reference_to(&function.parameters, "App"))
        .map(|function| function.name.as_str())
        .collect::<Vec<_>>();
    assert!(
        raw_app_entries.is_empty(),
        "nara_winit exposes public raw App driver entries: {raw_app_entries:?}"
    );
    assert!(
        functions.iter().any(|function| {
            function.name == "run"
                && accepts_mutable_reference_to(&function.parameters, "RuntimeInstance")
        }),
        "nara_winit must expose a public RuntimeInstance runner entry"
    );
}

#[test]
fn runtime_core_uses_only_its_dependency_and_source_allowlists() {
    let metadata = cargo_metadata();
    let packages = metadata["packages"]
        .as_array()
        .expect("cargo metadata packages must be an array");
    let app = packages
        .iter()
        .find(|package| package["name"] == "nara_app")
        .expect("cargo metadata must contain nara_app");
    let actual_dependencies = app["dependencies"]
        .as_array()
        .expect("package dependencies must be an array")
        .iter()
        .filter(|dependency| {
            dependency["kind"].is_null()
                || matches!(dependency["kind"].as_str(), Some("normal" | "build"))
        })
        .map(|dependency| {
            dependency["name"]
                .as_str()
                .expect("dependency names must be strings")
                .to_owned()
        })
        .collect::<BTreeSet<_>>();
    let allowed_dependencies = NARA_APP_DEPENDENCY_ALLOWLIST
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let unexpected_dependencies = actual_dependencies
        .difference(&allowed_dependencies)
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        unexpected_dependencies.is_empty(),
        "nara_app has dependencies outside its allowlist: {unexpected_dependencies:?}"
    );

    let mut forbidden_identifiers = packages
        .iter()
        .filter_map(|package| package["name"].as_str())
        .filter(|name| name.starts_with("nara_") && !matches!(*name, "nara_app" | "nara_ecs"))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    forbidden_identifiers.extend([
        "RuntimeRecipe".to_owned(),
        "RuntimeStartAttempt".to_owned(),
        "egui".to_owned(),
        "wgpu".to_owned(),
        "winit".to_owned(),
    ]);

    for path in rust_source_files(&workspace_root().join("crates/nara_app/src")) {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let identifiers = rust_tokens(&source).into_iter().collect::<BTreeSet<_>>();
        let forbidden = identifiers
            .intersection(&forbidden_identifiers)
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            forbidden.is_empty(),
            "{} contains forbidden runtime-core identifiers: {forbidden:?}",
            path.display()
        );
    }
}

#[test]
fn signature_guard_is_insensitive_to_formatting_and_lifetimes() {
    let functions = public_functions(&rust_tokens(
        "pub\nunsafe fn drive(runtime: & 'runtime mut nara_app::App) {}\n\
         pub(crate) fn internal(app: &mut App) {}",
    ));
    assert_eq!(functions.len(), 1);
    assert!(accepts_mutable_reference_to(
        &functions[0].parameters,
        "App"
    ));
}

#[test]
fn driver_scope_is_short_lived_and_does_not_expose_app() {
    let mut runtime = start_runtime(App::new());
    let frame = runtime
        .with_driver_scope(|scope| scope.world().resource::<nara::app::RealTime>().frame)
        .unwrap();
    assert_eq!(frame, 0);
}
