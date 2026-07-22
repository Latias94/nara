use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::Value as JsonValue;
use syn::{Attribute, ItemExternCrate, ItemTrait, ItemUse, UseTree, visit::Visit};
use toml::Value as TomlValue;

const FIXTURE_PACKAGE: &str = "renamed-nara-runtime-runner-fixture";
const FIXTURE_RELATIVE_PATH: &str = "tests/fixtures/runtime-runner/renamed-root";
const ALLOWED_SOURCE_ROOTS: [&str; 2] = ["engine", "std"];
const FORBIDDEN_SOURCE_IDENTIFIERS: [&str; 8] = [
    "__RuntimeDriverPort",
    "__apply_port",
    "RuntimeDriverScope",
    "RuntimeCandidateScope",
    "run_once",
    "world",
    "world_mut",
    "World",
];

#[test]
fn renamed_root_fixture_has_an_independent_locked_manifest() {
    let repository = repository_root();
    let fixture = fixture_root(&repository);
    let manifest_source = fs::read_to_string(fixture.join("Cargo.toml"))
        .expect("RGD-U6 requires the external runtime-runner fixture manifest");
    let manifest = toml::from_str::<TomlValue>(&manifest_source)
        .expect("runtime-runner fixture manifest must be valid TOML");

    assert_manifest_boundary(&manifest).unwrap();
    assert!(
        fixture.join("Cargo.lock").is_file(),
        "the external fixture must retain its own lockfile"
    );

    let output = Command::new(cargo_executable())
        .current_dir(&repository)
        .args(["metadata", "--manifest-path"])
        .arg(fixture.join("Cargo.toml"))
        .args(["--locked", "--no-deps", "--format-version", "1"])
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("failed to read runtime-runner fixture metadata");
    assert_command_succeeded("runtime-runner metadata", &output);

    let metadata: JsonValue =
        serde_json::from_slice(&output.stdout).expect("Cargo metadata must be valid JSON");
    let packages = metadata["packages"]
        .as_array()
        .expect("Cargo metadata packages must be an array");
    assert_eq!(packages.len(), 1);
    let package = &packages[0];
    assert_eq!(package["name"].as_str(), Some(FIXTURE_PACKAGE));

    let dependencies = package["dependencies"]
        .as_array()
        .expect("fixture dependencies must be an array");
    assert_eq!(dependencies.len(), 1, "{dependencies:#?}");
    let dependency = &dependencies[0];
    assert_eq!(dependency["name"].as_str(), Some("nara"));
    assert_eq!(dependency["rename"].as_str(), Some("engine"));
    assert!(dependency["source"].is_null());
    assert!(dependency["kind"].is_null());
    assert!(dependency["target"].is_null());
    assert_eq!(dependency["optional"].as_bool(), Some(false));
    assert_eq!(dependency["uses_default_features"].as_bool(), Some(false));
    assert_eq!(dependency["features"], serde_json::json!(["runtime-core"]));
    let dependency_path = dependency["path"]
        .as_str()
        .map(Path::new)
        .expect("the renamed Nara dependency must expose a local path")
        .canonicalize()
        .expect("the renamed Nara dependency path must resolve");
    assert_eq!(dependency_path, repository.canonicalize().unwrap());

    let workspace_members = metadata["workspace_members"]
        .as_array()
        .expect("fixture workspace members must be an array");
    assert_eq!(workspace_members, &[package["id"].clone()]);
    assert_eq!(
        Path::new(
            metadata["workspace_root"]
                .as_str()
                .expect("fixture workspace root must be a path"),
        )
        .canonicalize()
        .unwrap(),
        fixture.canonicalize().unwrap()
    );
}

#[test]
fn renamed_root_fixture_runs_its_concrete_managed_runtime_loop() {
    let repository = repository_root();
    let output = Command::new(cargo_executable())
        .current_dir(&repository)
        .args(["test", "--manifest-path"])
        .arg(fixture_root(&repository).join("Cargo.toml"))
        .args(["--locked", "--jobs", "1", "--", "--test-threads=1"])
        .env("CARGO_TERM_COLOR", "never")
        .env("CARGO_TARGET_DIR", repository.join("target"))
        .output()
        .expect("failed to run the external runtime-runner fixture");
    assert_command_succeeded("runtime-runner fixture", &output);
}

#[test]
fn runtime_runner_fixture_rejects_private_runtime_and_runner_shortcuts() {
    let fixture = fixture_root(&repository_root());
    let sources = rust_source_files(&fixture.join("src"));
    assert!(
        !sources.is_empty(),
        "fixture must contain Rust source files"
    );

    let mut combined = String::new();
    for path in sources {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert_source_boundary(&source)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        combined.push_str(&source);
    }

    for required in [
        "RuntimeAdmissionReservation",
        "RuntimeInstance",
        "RuntimeControl::Pause",
        "RuntimeControl::StepFixedTick",
        "RuntimeControl::Stop",
    ] {
        assert!(
            combined.contains(required),
            "the concrete external loop must use {required}"
        );
    }

    for forbidden in [
        "use engine::app::__RuntimeDriverPort;",
        "fn bypass(app: &mut engine::app::App) { let _ = app.run_once(std::time::Duration::ZERO); }",
        "fn mutate(runtime: &mut engine::app::RuntimeInstance) { let _ = runtime.world(); }",
        "use bevy_ecs::world::World;",
        "use nara_ecs::world::World;",
        "pub trait Runner {}",
        "#[path = \"hidden.rs\"] mod hidden;",
        "include!(\"hidden.rs\");",
    ] {
        assert!(
            assert_source_boundary(forbidden).is_err(),
            "runtime-runner boundary must reject {forbidden:?}"
        );
    }
}

#[test]
fn runtime_runner_manifest_rejects_workspace_and_private_dependency_shortcuts() {
    let fixture = fixture_root(&repository_root());
    let source = fs::read_to_string(fixture.join("Cargo.toml")).unwrap();
    let manifest = toml::from_str::<TomlValue>(&source).unwrap();

    let mut inherited = manifest.clone();
    inherited["dependencies"]["engine"]
        .as_table_mut()
        .unwrap()
        .insert("workspace".to_owned(), TomlValue::Boolean(true));
    assert!(assert_manifest_boundary(&inherited).is_err());

    let mut patched = manifest.clone();
    patched
        .as_table_mut()
        .unwrap()
        .insert("patch".to_owned(), TomlValue::Table(toml::map::Map::new()));
    assert!(assert_manifest_boundary(&patched).is_err());

    for private_dependency in ["nara_ecs", "bevy_ecs"] {
        let mut private = manifest.clone();
        private["dependencies"].as_table_mut().unwrap().insert(
            private_dependency.to_owned(),
            TomlValue::Table(toml::map::Map::new()),
        );
        assert!(
            assert_manifest_boundary(&private).is_err(),
            "manifest must reject {private_dependency}"
        );
    }
}

#[test]
fn managed_runtime_runner_guide_documents_the_public_contract() {
    let guide = fs::read_to_string(
        repository_root()
            .join("docs")
            .join("guides")
            .join("managed-runtime-runner.md"),
    )
    .expect("RGD-U6 requires the public managed-runtime runner guide");

    for required in [
        "RuntimeAdmissionReservation",
        "RuntimeInstance",
        "request_control",
        "control_status",
        "RuntimeState::Stopped",
        "does not provide a universal `Runner` trait",
    ] {
        assert!(guide.contains(required), "guide is missing {required:?}");
    }
    assert!(guide.contains("Do not call\n`App::run_once`"));
    assert!(!guide.contains("__RuntimeDriverPort"));
}

fn assert_manifest_boundary(manifest: &TomlValue) -> Result<(), String> {
    let package = manifest
        .get("package")
        .and_then(TomlValue::as_table)
        .ok_or_else(|| "fixture must declare [package]".to_owned())?;
    if package.get("name").and_then(TomlValue::as_str) != Some(FIXTURE_PACKAGE) {
        return Err("fixture package name changed".to_owned());
    }
    if manifest.get("patch").is_some() {
        return Err("[patch] is forbidden".to_owned());
    }
    if manifest.get("dev-dependencies").is_some() || manifest.get("build-dependencies").is_some() {
        return Err("the external runner must have exactly one normal dependency".to_owned());
    }
    let workspace = manifest
        .get("workspace")
        .and_then(TomlValue::as_table)
        .ok_or_else(|| "fixture must own an independent workspace".to_owned())?;
    if workspace.get("resolver").and_then(TomlValue::as_str) != Some("3") {
        return Err("fixture must select resolver 3".to_owned());
    }
    let dependencies = manifest
        .get("dependencies")
        .and_then(TomlValue::as_table)
        .ok_or_else(|| "fixture must declare [dependencies]".to_owned())?;
    if dependencies.len() != 1 || !dependencies.contains_key("engine") {
        return Err("fixture must depend only on the renamed public root".to_owned());
    }
    let engine = dependencies["engine"]
        .as_table()
        .ok_or_else(|| "renamed root dependency must use table syntax".to_owned())?;
    if engine.get("package").and_then(TomlValue::as_str) != Some("nara") {
        return Err("renamed root dependency must name package nara".to_owned());
    }
    if engine.get("path").and_then(TomlValue::as_str) != Some("../../../..") {
        return Err("renamed root dependency must use the fixture-relative root path".to_owned());
    }
    if engine.get("default-features").and_then(TomlValue::as_bool) != Some(false) {
        return Err("renamed root dependency must disable default features".to_owned());
    }
    let features = engine
        .get("features")
        .and_then(TomlValue::as_array)
        .ok_or_else(|| "renamed root dependency must declare features".to_owned())?;
    if features.len() != 1 || features[0].as_str() != Some("runtime-core") {
        return Err("renamed root dependency must request only runtime-core".to_owned());
    }
    if engine.get("workspace").and_then(TomlValue::as_bool) == Some(true) {
        return Err("renamed root dependency must not inherit workspace metadata".to_owned());
    }
    Ok(())
}

fn assert_source_boundary(source: &str) -> Result<(), String> {
    let syntax = syn::parse_file(source).map_err(|error| error.to_string())?;
    let mut visitor = SourceBoundaryVisitor::default();
    visitor.visit_file(&syntax);

    let allowed = ALLOWED_SOURCE_ROOTS.into_iter().collect::<BTreeSet<_>>();
    let unexpected_roots = visitor
        .roots
        .iter()
        .filter(|root| !allowed.contains(root.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unexpected_roots.is_empty() {
        return Err(format!("undeclared source roots: {unexpected_roots:?}"));
    }
    if !visitor.redirects.is_empty() {
        return Err(format!(
            "uninspectable source redirects: {:?}",
            visitor.redirects
        ));
    }
    if visitor.declares_trait {
        return Err("the fixture must not declare a Runner trait".to_owned());
    }
    let forbidden = FORBIDDEN_SOURCE_IDENTIFIERS
        .into_iter()
        .filter(|identifier| visitor.identifiers.contains(*identifier))
        .collect::<Vec<_>>();
    if !forbidden.is_empty() {
        return Err(format!(
            "forbidden runtime shortcut identifiers: {forbidden:?}"
        ));
    }
    Ok(())
}

#[derive(Default)]
struct SourceBoundaryVisitor {
    roots: BTreeSet<String>,
    local_bindings: BTreeSet<String>,
    redirects: BTreeSet<String>,
    identifiers: BTreeSet<String>,
    declares_trait: bool,
}

impl<'ast> Visit<'ast> for SourceBoundaryVisitor {
    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        collect_use_roots(&item.tree, &mut self.roots);
        collect_use_bindings(&item.tree, &mut self.local_bindings);
        if use_tree_has_glob(&item.tree) {
            self.redirects.insert("glob import".to_owned());
        }
        syn::visit::visit_item_use(self, item);
    }

    fn visit_item_extern_crate(&mut self, item: &'ast ItemExternCrate) {
        self.redirects.insert("extern crate".to_owned());
        self.roots.insert(item.ident.to_string());
        syn::visit::visit_item_extern_crate(self, item);
    }

    fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
        self.declares_trait = true;
        syn::visit::visit_item_trait(self, item);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        if path.segments.len() > 1 {
            let root = path.segments.first().unwrap().ident.to_string();
            if root.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                && !self.local_bindings.contains(&root)
                && !matches!(root.as_str(), "crate" | "self" | "super")
            {
                self.roots.insert(root);
            }
        }
        syn::visit::visit_path(self, path);
    }

    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        if let Some(identifier) = item
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            && matches!(
                identifier.as_str(),
                "include" | "include_str" | "include_bytes"
            )
        {
            self.redirects.insert(identifier);
        }
        syn::visit::visit_macro(self, item);
    }

    fn visit_attribute(&mut self, attribute: &'ast Attribute) {
        if attribute.path().is_ident("path") || cfg_attr_redirects_path(attribute) {
            self.redirects
                .insert(attribute.path().segments.last().unwrap().ident.to_string());
        }
        syn::visit::visit_attribute(self, attribute);
    }

    fn visit_ident(&mut self, identifier: &'ast syn::Ident) {
        self.identifiers.insert(identifier.to_string());
    }
}

fn collect_use_roots(tree: &UseTree, roots: &mut BTreeSet<String>) {
    match tree {
        UseTree::Path(path) => {
            if !matches!(path.ident.to_string().as_str(), "crate" | "self" | "super") {
                roots.insert(path.ident.to_string());
            }
        }
        UseTree::Name(name) => {
            roots.insert(name.ident.to_string());
        }
        UseTree::Rename(rename) => {
            roots.insert(rename.ident.to_string());
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_use_roots(item, roots);
            }
        }
        UseTree::Glob(_) => {}
    }
}

fn collect_use_bindings(tree: &UseTree, bindings: &mut BTreeSet<String>) {
    match tree {
        UseTree::Path(path) => {
            if use_tree_imports_self(&path.tree) {
                bindings.insert(path.ident.to_string());
            }
            collect_use_bindings(&path.tree, bindings);
        }
        UseTree::Name(name) => {
            if name.ident != "self" {
                bindings.insert(name.ident.to_string());
            }
        }
        UseTree::Rename(rename) => {
            bindings.insert(rename.rename.to_string());
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_use_bindings(item, bindings);
            }
        }
        UseTree::Glob(_) => {}
    }
}

fn use_tree_imports_self(tree: &UseTree) -> bool {
    match tree {
        UseTree::Path(path) => use_tree_imports_self(&path.tree),
        UseTree::Name(name) => name.ident == "self",
        UseTree::Rename(_) | UseTree::Glob(_) => false,
        UseTree::Group(group) => group.items.iter().any(use_tree_imports_self),
    }
}

fn use_tree_has_glob(tree: &UseTree) -> bool {
    match tree {
        UseTree::Path(path) => use_tree_has_glob(&path.tree),
        UseTree::Name(_) | UseTree::Rename(_) => false,
        UseTree::Group(group) => group.items.iter().any(use_tree_has_glob),
        UseTree::Glob(_) => true,
    }
}

fn cfg_attr_redirects_path(attribute: &Attribute) -> bool {
    if !attribute.path().is_ident("cfg_attr") {
        return false;
    }
    match attribute
        .parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
    {
        Ok(nested) => nested.iter().skip(1).any(meta_contains_path_override),
        Err(_) => true,
    }
}

fn meta_contains_path_override(meta: &syn::Meta) -> bool {
    if meta.path().is_ident("path") {
        return true;
    }
    let syn::Meta::List(list) = meta else {
        return false;
    };
    if !list.path.is_ident("cfg_attr") {
        return false;
    }
    match list
        .parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
    {
        Ok(nested) => nested.iter().skip(1).any(meta_contains_path_override),
        Err(_) => true,
    }
}

fn rust_source_files(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut sources = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
        {
            let entry = entry.expect("fixture source directory entry must be readable");
            let file_type = entry
                .file_type()
                .expect("fixture file type must be readable");
            assert!(
                !file_type.is_symlink(),
                "fixture source links are forbidden"
            );
            let path = entry.path();
            if file_type.is_dir() && entry.file_name() != "target" {
                pending.push(path);
            } else if file_type.is_file()
                && path.extension().is_some_and(|extension| extension == "rs")
            {
                sources.push(path);
            }
        }
    }
    sources.sort();
    sources
}

fn assert_command_succeeded(label: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn cargo_executable() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_root(repository: &Path) -> PathBuf {
    repository.join(FIXTURE_RELATIVE_PATH)
}
