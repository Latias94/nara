use serde_json::{Value, json};

use super::*;

fn stdout(message: Value) -> OutputLine {
    OutputLine {
        stream: OutputStream::Stdout,
        text: message.to_string(),
    }
}

fn artifact(manifest: &Path, binary: &str, kind: &str, executable: &Path) -> OutputLine {
    stdout(json!({
        "reason": "compiler-artifact",
        "manifest_path": manifest,
        "target": { "name": binary, "kind": [kind] },
        "executable": executable,
        "fresh": true,
    }))
}

#[test]
fn uses_cargos_fresh_executable_only_for_the_selected_binary_and_package() {
    let manifest = fs::canonicalize(env::current_exe().unwrap()).unwrap();
    let executable = manifest.with_file_name("game executable");
    let other = manifest.with_file_name("other");
    let other_manifest = manifest.parent().unwrap();
    let mut messages = BuildMessages::new(&manifest, "game");
    for record in [
        artifact(&manifest, "other", "bin", &other),
        artifact(&manifest, "game", "example", &other),
        artifact(other_manifest, "game", "bin", &other),
        artifact(&manifest, "game", "bin", &executable),
        artifact(&manifest, "game", "bin", &executable),
        stdout(json!({ "reason": "build-finished", "success": true })),
    ] {
        messages.push(record, &mut |event| panic!("unexpected event: {event:?}"));
    }
    assert_eq!(messages.finish(ExitStatus::default()).unwrap(), executable);
}

#[test]
fn reports_compiler_locations_and_rejects_an_unsuccessful_build() {
    let manifest = fs::canonicalize(env::current_exe().unwrap()).unwrap();
    let mut messages = BuildMessages::new(&manifest, "game");
    let mut events = Vec::new();
    for record in [
        artifact(
            &manifest,
            "game",
            "bin",
            &manifest.with_file_name("stale game"),
        ),
        stdout(json!({
            "reason": "compiler-message",
            "package_id": "local game package",
            "message": {
                "level": "error",
                "message": "mismatched types",
                "code": { "code": "E0308", "explanation": "type mismatch" },
                "rendered": "error[E0308]: mismatched types\n",
                "spans": [{
                    "file_name": "src/main.rs", "line_start": 4, "line_end": 4,
                    "column_start": 12, "column_end": 20, "is_primary": true, "label": "expected bool"
                }],
                "children": []
            }
        })),
        OutputLine {
            stream: OutputStream::Stdout,
            text: "unstructured output".into(),
        },
        OutputLine {
            stream: OutputStream::Stderr,
            text: "error: could not compile".into(),
        },
        stdout(json!({ "reason": "build-finished", "success": false })),
    ] {
        messages.push(record, &mut |event| events.push(event));
    }
    assert!(matches!(
        messages.finish(ExitStatus::default()),
        Err(BuildError::Incomplete)
    ));
    let [
        BuildEvent::Diagnostic {
            package_id,
            diagnostic,
        },
        BuildEvent::Output(stdout),
        BuildEvent::Output(stderr),
    ] = events.as_slice()
    else {
        panic!("unexpected events: {events:?}");
    };
    assert_eq!(package_id, "local game package");
    assert_eq!(diagnostic.code.as_deref(), Some("E0308"));
    assert_eq!(diagnostic.spans[0].file_name, Path::new("src/main.rs"));
    assert_eq!(diagnostic.spans[0].line_start, 4);
    assert_eq!(diagnostic.spans[0].column_start, 12);
    assert!(diagnostic.spans[0].is_primary);
    assert_eq!(stdout.stream, OutputStream::Stdout);
    assert_eq!(stderr.stream, OutputStream::Stderr);
}
