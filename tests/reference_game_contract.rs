use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use syn::visit::Visit;

const FORBIDDEN_LIFECYCLE_IDENTIFIERS: [&str; 13] = [
    "RuntimeAdmissionFailure",
    "RuntimeAdmissionRetirement",
    "RuntimeCandidate",
    "RuntimeCandidateFailure",
    "RuntimeCandidateRetirementState",
    "RuntimeInstance",
    "RuntimeRetirement",
    "RuntimeStartAttempt",
    "begin_retirement",
    "complete_startup",
    "drive_retirement",
    "promote",
    "retirement_state",
];

#[test]
fn reference_game_is_an_isolated_consumer_of_the_public_root_crate() {
    let root = reference_game_root();
    let manifest_source = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let manifest = toml::from_str::<toml::Value>(&manifest_source).unwrap();
    let mut dependencies = Vec::new();
    collect_dependency_entries(&manifest, &mut Vec::new(), &mut dependencies);
    dependencies.sort();

    assert_eq!(dependencies, ["dependencies.nara"]);
    let nara = manifest
        .get("dependencies")
        .and_then(|dependencies| dependencies.get("nara"))
        .and_then(toml::Value::as_table)
        .expect("the reference game must depend on the public root crate");
    assert_eq!(nara.get("path").and_then(toml::Value::as_str), Some(".."));
    assert_eq!(
        nara.get("default-features").and_then(toml::Value::as_bool),
        Some(false)
    );
    assert!(!manifest_source.contains("[patch"));
    assert!(!manifest_source.contains("workspace = true"));
    assert!(root.join("Cargo.lock").is_file());
}

#[test]
fn production_gameplay_does_not_orchestrate_engine_runtime_lifecycle() {
    let mut visitor = LifecycleVisitor::default();
    for path in rust_sources(&reference_game_root().join("src")) {
        let source = fs::read_to_string(&path).unwrap();
        let syntax = syn::parse_file(&source)
            .unwrap_or_else(|error| panic!("{} must parse: {error}", path.display()));
        visitor.visit_file(&syntax);
    }

    assert!(
        visitor.observed.is_empty(),
        "reference game production code names private lifecycle choreography: {:?}",
        visitor.observed
    );
}

#[test]
fn headless_product_surface_is_bundled_typed_and_bounded() {
    let root = reference_game_root();
    let library = fs::read_to_string(root.join("src/lib.rs")).unwrap();
    let snapshot = fs::read_to_string(root.join("src/snapshot.rs")).unwrap();
    let binary = fs::read_to_string(root.join("src/bin/headless.rs")).unwrap();

    for required in [
        "pub fn bundled_wave_run",
        "pub fn wave_headless_run",
        "pub fn movement_command",
    ] {
        assert!(
            library.contains(required),
            "missing product API: {required}"
        );
    }
    assert!(snapshot.contains("pub struct WaveSnapshot"));
    for required in ["bundled_wave_run", "--max-ticks", "MAXIMUM_CLI_TICKS"] {
        assert!(
            binary.contains(required),
            "missing CLI contract: {required}"
        );
    }
    for forbidden in [
        "--scenario",
        "--replay",
        "--commands",
        "scenario_path",
        "command_path",
        "serde_json::from",
        "from_reader",
    ] {
        assert!(
            !binary.contains(forbidden),
            "CLI exposes unadmitted external input: {forbidden}"
        );
    }
}

#[derive(Default)]
struct LifecycleVisitor {
    observed: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for LifecycleVisitor {
    fn visit_path_segment(&mut self, segment: &'ast syn::PathSegment) {
        self.observe(&segment.ident.to_string());
        syn::visit::visit_path_segment(self, segment);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        self.observe(&call.method.to_string());
        syn::visit::visit_expr_method_call(self, call);
    }
}

impl LifecycleVisitor {
    fn observe(&mut self, identifier: &str) {
        if FORBIDDEN_LIFECYCLE_IDENTIFIERS.contains(&identifier) {
            self.observed.insert(identifier.to_owned());
        }
    }
}

fn reference_game_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-game")
}

fn collect_dependency_entries(
    value: &toml::Value,
    path: &mut Vec<String>,
    entries: &mut Vec<String>,
) {
    let Some(table) = value.as_table() else {
        return;
    };
    for (key, child) in table {
        path.push(key.clone());
        if matches!(
            key.as_str(),
            "dependencies" | "dev-dependencies" | "build-dependencies"
        ) {
            let dependencies = child
                .as_table()
                .expect("Cargo dependency sections must be TOML tables");
            for dependency in dependencies.keys() {
                entries.push(format!("{}.{}", path.join("."), dependency));
            }
        } else {
            collect_dependency_entries(child, path, entries);
        }
        path.pop();
    }
}

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_owned()];
    let mut sources = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                sources.push(path);
            }
        }
    }
    sources.sort();
    sources
}
