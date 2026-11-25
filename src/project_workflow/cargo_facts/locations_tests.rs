use super::super::messages::CargoDiagnosticsBuilder;
use super::*;

#[test]
fn first_primary_location_follows_secondary_spans_and_ignores_later_primaries() {
    let mut secondary = primary_span("secondary.rs", 2, 3);
    secondary["is_primary"] = json!(false);
    let mut primary = primary_span("src/main.rs", 27, 9);
    primary["text"] = json!([{"text": "source canary", "highlight_start": 1}]);
    primary["suggested_replacement"] = json!("replacement canary");
    primary["expansion"] = json!({"span": primary_span("macro_source.rs", 99, 98)});
    let diagnostic = parsed_diagnostic(diagnostic_with_spans(json!([
        secondary,
        primary,
        primary_span("later_primary.rs", 77, 88),
    ])));

    let location: &CargoDiagnosticLocation = diagnostic.primary_location().unwrap();
    assert_eq!(location.path(), Path::new("src/main.rs"));
    assert_eq!(location.line(), 27_u32);
    assert_eq!(location.column(), 9_u32);
    assert!(!diagnostic.truncated());
}

#[test]
fn absent_null_empty_and_secondary_only_spans_are_not_location_loss() {
    let mut secondary = primary_span("secondary.rs", 2, 3);
    secondary["is_primary"] = json!(false);
    secondary["expansion"] = json!({"span": primary_span("macro_source.rs", 99, 98)});
    for spans in [
        None,
        Some(Value::Null),
        Some(json!([])),
        Some(json!([secondary])),
    ] {
        let mut value = diagnostic_message("error", Some("E0308"), "expected a value");
        if let Some(spans) = spans {
            value["message"]["spans"] = spans;
        }
        value["message"]["rendered"] = json!("error --> private/source.rs:44:55");
        value["message"]["children"] = json!([{
            "spans": [primary_span("child_source.rs", 66, 77)]
        }]);
        let diagnostic = parsed_diagnostic(value);
        assert!(diagnostic.primary_location().is_none());
        assert!(!diagnostic.truncated());
    }
}

#[test]
fn malformed_optional_span_containers_preserve_compiler_failure() {
    for spans in [json!(false), json!(17), json!("invalid spans"), json!({})] {
        let mut decoder = CargoBuildDecoder::new(test_invocation());
        push_value(&mut decoder, diagnostic_with_spans(spans)).unwrap();
        push_value(&mut decoder, build_finished(false)).unwrap();
        assert!(matches!(
            decoder.finish(),
            Err(CargoBuildError::BuildFailed)
        ));
        let diagnostics = decoder.diagnostics();
        assert_eq!(diagnostics.entries().len(), 1);
        assert_eq!(diagnostics.dropped_count(), 0);
        assert_eq!(diagnostics.truncated_count(), 1);
        assert!(diagnostics.entries()[0].primary_location().is_none());
        assert_eq!(diagnostics.entries()[0].code(), Some("E0308"));
        assert_eq!(diagnostics.entries()[0].message(), "expected a value");
    }
}

#[test]
fn invalid_first_primary_fields_only_omit_location_without_falling_back() {
    let mut cases = vec![
        ("file_name", None),
        ("file_name", Some(Value::Null)),
        ("file_name", Some(json!(false))),
        ("file_name", Some(json!(17))),
        ("file_name", Some(json!(""))),
        ("file_name", Some(json!("src/\0main.rs"))),
        ("file_name", Some(json!("src/\nmain.rs"))),
        ("file_name", Some(json!("src/\u{7f}main.rs"))),
        ("file_name", Some(json!("src/\u{85}main.rs"))),
    ];
    for field in ["line_start", "column_start"] {
        for invalid in [
            None,
            Some(Value::Null),
            Some(json!(false)),
            Some(json!(0)),
            Some(json!(-1)),
            Some(json!(1.5)),
            Some(json!("1")),
            Some(json!(u64::from(u32::MAX) + 1)),
            Some(json!(u64::MAX)),
        ] {
            cases.push((field, invalid));
        }
    }
    for (field, invalid) in cases {
        let mut primary = primary_span("src/main.rs", 1, 1);
        if let Some(invalid) = invalid {
            primary[field] = invalid;
        } else {
            primary.as_object_mut().unwrap().remove(field);
        }
        let mut decoder = CargoBuildDecoder::new(test_invocation());
        push_value(
            &mut decoder,
            diagnostic_with_spans(json!([primary, primary_span("fallback.rs", 2, 3)])),
        )
        .unwrap();
        push_value(&mut decoder, build_finished(false)).unwrap();
        assert!(matches!(
            decoder.finish(),
            Err(CargoBuildError::BuildFailed)
        ));
        let diagnostics = decoder.diagnostics();
        assert_eq!(diagnostics.entries().len(), 1);
        assert_eq!(diagnostics.dropped_count(), 0);
        assert_eq!(diagnostics.truncated_count(), 1);
        let diagnostic = &diagnostics.entries()[0];
        assert_eq!(diagnostic.level(), CargoDiagnosticLevel::Error);
        assert_eq!(diagnostic.code(), Some("E0308"));
        assert_eq!(diagnostic.message(), "expected a value");
        assert!(diagnostic.primary_location().is_none());
        assert!(diagnostic.truncated());
    }
}

#[test]
fn primary_coordinates_retain_the_full_positive_u32_range() {
    let diagnostic = parsed_diagnostic(diagnostic_with_spans(json!([primary_span(
        "src/main.rs",
        u32::MAX,
        u32::MAX,
    )])));
    let location = diagnostic.primary_location().unwrap();
    assert_eq!(location.line(), u32::MAX);
    assert_eq!(location.column(), u32::MAX);
    assert!(!diagnostic.truncated());
}

#[test]
fn unicode_primary_paths_are_retained_whole_or_omitted_at_the_existing_byte_limit() {
    let prefix = "src/";
    let available = MAX_PATH_BYTES - prefix.len();
    let path = format!(
        "{prefix}{}{}",
        "界".repeat(available / "界".len()),
        "x".repeat(available % "界".len()),
    );
    assert_eq!(path.len(), MAX_PATH_BYTES);
    let diagnostic = parsed_diagnostic(diagnostic_with_spans(json!([primary_span(&path, 1, 2)])));
    assert_eq!(
        diagnostic.primary_location().unwrap().path(),
        Path::new(&path)
    );
    assert!(!diagnostic.truncated());

    let oversized = format!("{path}界");
    let diagnostic = parsed_diagnostic(diagnostic_with_spans(json!([primary_span(
        &oversized, 1, 2,
    )])));
    assert!(diagnostic.primary_location().is_none());
    assert_eq!(diagnostic.code(), Some("E0308"));
    assert_eq!(diagnostic.message(), "expected a value");
    assert!(diagnostic.truncated());
}

#[test]
fn bounded_large_external_native_paths_remain_raw_generic_observations() {
    let external = test_root().with_file_name("external");
    let mut spelling = external.to_str().unwrap().to_owned();
    spelling.push(std::path::MAIN_SEPARATOR);
    spelling.push_str(&"x".repeat(MAX_PATH_BYTES - spelling.len()));
    let diagnostic = parsed_diagnostic(diagnostic_with_spans(json!([primary_span(
        &spelling, 4, 5,
    )])));
    let location = diagnostic.primary_location().unwrap();
    assert!(location.path().is_absolute());
    assert_eq!(
        location.path().as_os_str().as_encoded_bytes().len(),
        MAX_PATH_BYTES
    );
    assert_eq!(location.path(), Path::new(&spelling));
    assert!(!diagnostic.truncated());

    let mut builder = CargoDiagnosticsBuilder::default();
    builder.push(diagnostic);
    let projected = builder
        .snapshot()
        .with_project_relative_locations(&test_root(), &test_root());
    assert!(projected.entries()[0].primary_location().is_none());
    assert_eq!(projected.truncated_count(), 1);
    assert_eq!(projected.dropped_count(), 0);
}

#[test]
fn retained_location_payload_counts_toward_the_aggregate_byte_budget() {
    let mut builder = CargoDiagnosticsBuilder::default();
    let path = "x".repeat(MAX_PATH_BYTES);
    let message = "m".repeat(MAX_DIAGNOSTIC_MESSAGE_BYTES);
    let mut value = diagnostic_message("warning", None, &message);
    value["message"]["spans"] = json!([primary_span(&path, 1, 2)]);
    builder.push(parsed_diagnostic(value));
    for _ in 0..7 {
        builder.push(parsed_diagnostic(diagnostic_message(
            "warning", None, &message,
        )));
    }
    assert_eq!(builder.retained_bytes, MAX_DIAGNOSTIC_BYTES);
    assert_eq!(builder.entries.len(), 8);
    assert!(builder.entries[0].primary_location().is_some());
    assert_eq!(builder.truncated_count, 0);

    builder.push(parsed_diagnostic(diagnostic_message("warning", None, "x")));
    assert_eq!(builder.retained_bytes, MAX_DIAGNOSTIC_BYTES);
    assert_eq!(builder.entries.len(), 8);
    assert_eq!(builder.dropped_count, 1);
}

#[test]
fn aggregate_budget_omits_new_location_before_dropping_its_message_or_code() {
    let mut builder = CargoDiagnosticsBuilder::default();
    let message = "m".repeat(MAX_DIAGNOSTIC_MESSAGE_BYTES);
    for _ in 0..15 {
        builder.push(parsed_diagnostic(diagnostic_message(
            "warning", None, &message,
        )));
    }
    let message = "m".repeat(MAX_DIAGNOSTIC_MESSAGE_BYTES - "E0308".len());
    let mut value = diagnostic_message("error", Some("E0308"), &message);
    value["message"]["spans"] = json!([primary_span("src/main.rs", 1, 2)]);
    builder.push(parsed_diagnostic(value));
    assert_eq!(builder.retained_bytes, MAX_DIAGNOSTIC_BYTES);
    assert_eq!(builder.entries.len(), 16);
    assert_eq!(builder.dropped_count, 0);
    assert_eq!(builder.truncated_count, 1);
    let diagnostic = builder.entries.last().unwrap();
    assert_eq!(diagnostic.message(), message);
    assert_eq!(diagnostic.code(), Some("E0308"));
    assert!(diagnostic.primary_location().is_none());
    assert!(diagnostic.truncated());
}

#[test]
fn count_limit_and_projection_preserve_drop_counts_and_count_each_location_loss_once() {
    let mut builder = CargoDiagnosticsBuilder::default();
    let external = test_root().with_file_name("external").join("private.rs");
    let value = diagnostic_with_spans(json!([primary_span(external.to_str().unwrap(), 1, 2)]));
    for _ in 0..=MAX_DIAGNOSTIC_COUNT {
        builder.push(parsed_diagnostic(value.clone()));
    }
    let raw = builder.snapshot();
    assert_eq!(raw.entries().len(), MAX_DIAGNOSTIC_COUNT);
    assert_eq!(raw.dropped_count(), 1);
    assert_eq!(raw.truncated_count(), 0);
    let projected = raw.with_project_relative_locations(&test_root(), &test_root());
    assert_eq!(projected.dropped_count(), 1);
    assert_eq!(projected.truncated_count(), MAX_DIAGNOSTIC_COUNT as u64);
    assert!(projected.entries().iter().all(|diagnostic| {
        diagnostic.primary_location().is_none()
            && diagnostic.truncated()
            && diagnostic.code() == Some("E0308")
            && diagnostic.message() == "expected a value"
    }));
    assert_eq!(
        projected
            .clone()
            .with_project_relative_locations(&test_root(), &test_root()),
        projected,
    );
}

#[test]
fn projection_retains_admitted_relative_and_known_project_absolute_locations() {
    let project = test_root();
    for path in [
        PathBuf::from("src/main.rs"),
        PathBuf::from("src/场景.rs"),
        project.join("src/main.rs"),
    ] {
        let mut builder = CargoDiagnosticsBuilder::default();
        builder.push(parsed_diagnostic(diagnostic_with_spans(json!([
            primary_span(path.to_str().unwrap(), 12, 34,)
        ]))));
        let raw = builder.snapshot();
        let projected = raw
            .clone()
            .with_project_relative_locations(&project, &project);
        let location = projected.entries()[0].primary_location().unwrap();
        let expected = if path.is_absolute() {
            Path::new("src/main.rs")
        } else {
            &path
        };
        assert_eq!(location.path(), expected);
        assert!(crate::fs::RelativePath::preflight(location.path()).is_ok());
        assert_eq!(location.line(), 12);
        assert_eq!(location.column(), 34);
        assert_eq!(projected.truncated_count(), 0);
        assert!(!projected.entries()[0].truncated());
        assert_eq!(raw.entries()[0].primary_location().unwrap().path(), path);
    }
}

#[test]
fn workspace_relative_locations_select_the_project_and_withhold_sibling_packages() {
    let workspace = test_root();
    let project = workspace.join("game");
    let mut builder = CargoDiagnosticsBuilder::default();
    for path in [
        PathBuf::from("game/src/lib.rs"),
        project.join("src/lib.rs"),
        PathBuf::from("sibling/src/lib.rs"),
        workspace.join("game-other/src/lib.rs"),
    ] {
        builder.push(parsed_diagnostic(diagnostic_with_spans(json!([
            primary_span(path.to_str().unwrap(), 3, 9,)
        ]))));
    }
    let projected = builder
        .snapshot()
        .with_project_relative_locations(&project, &workspace);
    for diagnostic in &projected.entries()[..2] {
        let location = diagnostic.primary_location().unwrap();
        assert_eq!(location.path(), Path::new("src/lib.rs"));
        assert_eq!((location.line(), location.column()), (3, 9));
        assert!(!diagnostic.truncated());
    }
    for diagnostic in &projected.entries()[2..] {
        assert!(diagnostic.primary_location().is_none());
        assert!(diagnostic.truncated());
        assert_eq!(diagnostic.code(), Some("E0308"));
        assert_eq!(diagnostic.message(), "expected a value");
    }
    assert_eq!(projected.truncated_count(), 2);
    assert_eq!(projected.dropped_count(), 0);
}

#[test]
fn projection_withholds_external_parent_traversal_and_nonportable_paths() {
    let project = test_root();
    for path in [
        project.clone(),
        project.with_file_name("project-other").join("private.rs"),
        project.join("../private.rs"),
        PathBuf::from("../private.rs"),
        PathBuf::from("src/../../private.rs"),
        PathBuf::from("src/file.rs:stream"),
        PathBuf::from("src/CON.rs"),
        PathBuf::from("src//main.rs"),
        PathBuf::from("src/trailing.rs "),
        PathBuf::from("<anon>"),
        PathBuf::from("x".repeat(256)),
    ] {
        let mut builder = CargoDiagnosticsBuilder::default();
        builder.push(parsed_diagnostic(diagnostic_with_spans(json!([
            primary_span(path.to_str().unwrap(), 12, 34,)
        ]))));
        let projected = builder
            .snapshot()
            .with_project_relative_locations(&project, &project);
        let diagnostic = &projected.entries()[0];
        assert!(diagnostic.primary_location().is_none(), "path: {path:?}");
        assert!(diagnostic.truncated());
        assert_eq!(diagnostic.code(), Some("E0308"));
        assert_eq!(diagnostic.message(), "expected a value");
        assert_eq!(projected.truncated_count(), 1);
        assert_eq!(projected.dropped_count(), 0);
    }
}

#[test]
fn projection_does_not_double_count_text_or_invalid_location_loss() {
    let mut builder = CargoDiagnosticsBuilder::default();
    let external = test_root().with_file_name("external").join("private.rs");
    let message = "m".repeat(MAX_DIAGNOSTIC_MESSAGE_BYTES + 1);
    let mut value = diagnostic_message("error", Some("E0308"), &message);
    value["message"]["spans"] = json!([primary_span(external.to_str().unwrap(), 1, 2)]);
    builder.push(parsed_diagnostic(value));
    builder.push(parsed_diagnostic(diagnostic_with_spans(json!([
        primary_span("src/\nmain.rs", 1, 2,)
    ]))));
    builder.push(parsed_diagnostic(diagnostic_message(
        "error",
        None,
        "no location",
    )));
    assert_eq!(builder.truncated_count, 2);
    let raw = builder.snapshot();
    let projected = raw
        .clone()
        .with_project_relative_locations(&test_root(), &test_root());
    assert_eq!(projected.truncated_count(), 2);
    assert_eq!(projected.dropped_count(), 0);
    for (original, projected) in raw.entries().iter().zip(projected.entries()) {
        assert_eq!(projected.message(), original.message());
        assert_eq!(projected.code(), original.code());
        assert!(projected.primary_location().is_none());
    }
    assert!(!projected.entries()[2].truncated());
}

#[test]
fn absolute_locations_require_an_absolute_known_project_root() {
    let mut builder = CargoDiagnosticsBuilder::default();
    builder.push(parsed_diagnostic(diagnostic_with_spans(json!([
        primary_span(test_root().join("src/main.rs").to_str().unwrap(), 1, 2,)
    ]))));
    let projected = builder
        .snapshot()
        .with_project_relative_locations(Path::new("project"), &test_root());
    assert!(projected.entries()[0].primary_location().is_none());
    assert_eq!(projected.truncated_count(), 1);
}

#[cfg(windows)]
#[test]
fn windows_projection_matches_verbatim_and_ordinary_drive_roots_and_separators() {
    for (project, raw_path) in [
        (r"\\?\C:\project", "C:/project/src/main.rs"),
        (r"\\?\C:\project", r"c:\PROJECT\src\main.rs"),
        (r"C:\project", r"\\?\C:\project\src\main.rs"),
        (r"C:\project", r"src\main.rs"),
    ] {
        let mut builder = CargoDiagnosticsBuilder::default();
        builder.push(parsed_diagnostic(diagnostic_with_spans(json!([
            primary_span(raw_path, 1, 2,)
        ]))));
        let projected = builder
            .snapshot()
            .with_project_relative_locations(Path::new(project), Path::new(project));
        assert_eq!(
            projected.entries()[0].primary_location().unwrap().path(),
            Path::new("src/main.rs"),
        );
        assert_eq!(projected.truncated_count(), 0);
    }
}

#[cfg(windows)]
#[test]
fn windows_projection_withholds_drive_relative_rooted_and_ambiguous_unc_paths() {
    for (project, raw_path) in [
        (r"C:\project", r"C:src\main.rs"),
        (r"C:\project", r"\src\main.rs"),
        (r"C:\project", r"src\..\private.rs"),
        (r"C:\project", r"\\server\share\project\src\main.rs"),
        (
            r"\\?\UNC\server\share\project",
            r"\\server\share\project\src\main.rs",
        ),
    ] {
        let mut builder = CargoDiagnosticsBuilder::default();
        builder.push(parsed_diagnostic(diagnostic_with_spans(json!([
            primary_span(raw_path, 1, 2,)
        ]))));
        let projected = builder
            .snapshot()
            .with_project_relative_locations(Path::new(project), Path::new(project));
        assert!(projected.entries()[0].primary_location().is_none());
        assert_eq!(projected.truncated_count(), 1);
    }
}

fn primary_span(file_name: &str, line: u32, column: u32) -> Value {
    json!({
        "file_name": file_name,
        "line_start": line,
        "column_start": column,
        "is_primary": true,
    })
}

fn diagnostic_with_spans(spans: Value) -> Value {
    let mut value = diagnostic_message("error", Some("E0308"), "expected a value");
    value["message"]["spans"] = spans;
    value
}

fn parsed_diagnostic(value: Value) -> CargoCompilerDiagnostic {
    let CargoJsonMessage::CompilerDiagnostic(diagnostic) = CargoJsonMessage::parse(value).unwrap()
    else {
        panic!("expected a compiler diagnostic");
    };
    diagnostic
}
