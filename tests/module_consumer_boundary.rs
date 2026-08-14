use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value as JsonValue;
use syn::{ItemExternCrate, ItemUse, UseTree, visit::Visit};
use toml::Value as TomlValue;

const EXPECTED_DEPENDENCIES: [&str; 3] = [
    "dependencies.nara_reflect",
    "dependencies.nara_scene",
    "dev-dependencies.bevy_ecs",
];
const ALLOWED_SOURCE_ROOTS: [&str; 7] = [
    "alloc",
    "bevy_ecs",
    "core",
    "nara_reflect",
    "nara_scene",
    "nara_scene_module_consumer",
    "std",
];
const FS_EXPECTED_DEPENDENCIES: [&str; 1] = ["dependencies.nara_fs"];
const PROJECT_WORKFLOW_EXPECTED_DEPENDENCIES: [&str; 1] = ["dependencies.nara"];
const PROJECT_WORKFLOW_ALLOWED_SOURCE_ROOTS: [&str; 1] = ["nara"];

#[test]
fn module_consumer_workspace_preserves_direct_modules_and_one_workflow_facade() {
    let repository = repository_root();
    let consumer = module_consumer_root();
    let source = fs::read_to_string(consumer.join("Cargo.toml"))
        .expect("RGF-U18 requires module-consumer/Cargo.toml");
    let manifest = toml::from_str::<TomlValue>(&source)
        .expect("module-consumer/Cargo.toml must be valid TOML");

    assert_manifest_boundary(&manifest).unwrap();
    assert!(consumer.join("Cargo.lock").is_file());

    let workspace = manifest
        .get("workspace")
        .and_then(TomlValue::as_table)
        .expect("module consumer must own an independent workspace");
    assert_eq!(
        workspace.get("resolver").and_then(TomlValue::as_str),
        Some("3")
    );
    assert_eq!(
        workspace.get("members").and_then(TomlValue::as_array),
        Some(&vec![
            TomlValue::String("fs-consumer".to_owned()),
            TomlValue::String("project-workflow-consumer".to_owned()),
        ])
    );
    assert_eq!(
        workspace
            .get("default-members")
            .and_then(TomlValue::as_array),
        Some(&vec![
            TomlValue::String(".".to_owned()),
            TomlValue::String("fs-consumer".to_owned()),
            TomlValue::String("project-workflow-consumer".to_owned()),
        ])
    );

    let root_manifest = fs::read_to_string(repository.join("Cargo.toml")).unwrap();
    let root_manifest = toml::from_str::<TomlValue>(&root_manifest).unwrap();
    let excluded = root_manifest["workspace"]["exclude"]
        .as_array()
        .expect("root workspace exclude must be an array");
    assert!(
        excluded
            .iter()
            .any(|entry| entry.as_str() == Some("module-consumer")),
        "the independent consumer must be excluded from the root workspace"
    );

    assert_path_dependency(&manifest, "nara_scene", "../crates/nara_scene");
    assert_path_dependency(&manifest, "nara_reflect", "../crates/nara_reflect");
    let scene = &manifest["dependencies"]["nara_scene"];
    assert_eq!(
        scene.get("default-features").and_then(TomlValue::as_bool),
        Some(false)
    );
    assert_eq!(
        scene.get("features").and_then(TomlValue::as_array),
        Some(&vec![TomlValue::String("serde".to_owned())])
    );
    assert_eq!(
        manifest["dev-dependencies"]["bevy_ecs"].as_str(),
        Some("0.19")
    );

    let filesystem_source = fs::read_to_string(consumer.join("fs-consumer/Cargo.toml"))
        .expect("CDPF-U3 requires the direct filesystem consumer manifest");
    let filesystem = toml::from_str::<TomlValue>(&filesystem_source)
        .expect("filesystem consumer manifest must be valid TOML");
    assert_filesystem_manifest_boundary(&filesystem).unwrap();
    assert_path_dependency(&filesystem, "nara_fs", "../../crates/nara_fs");

    let project_workflow_source =
        fs::read_to_string(consumer.join("project-workflow-consumer/Cargo.toml"))
            .expect("FPER-U0 requires the non-CLI project-workflow consumer manifest");
    let project_workflow = toml::from_str::<TomlValue>(&project_workflow_source)
        .expect("project-workflow consumer manifest must be valid TOML");
    assert_project_workflow_manifest_boundary(&project_workflow).unwrap();
    assert_path_dependency(&project_workflow, "nara", "../..");
}

#[test]
fn locked_metadata_keeps_direct_modules_separate_from_the_project_workflow_facade() {
    let repository = repository_root();
    let manifest = module_consumer_root().join("Cargo.toml");
    let output = Command::new(cargo_executable())
        .current_dir(&repository)
        .args(["metadata", "--manifest-path"])
        .arg(&manifest)
        .args(["--locked", "--format-version", "1"])
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("failed to read module-consumer Cargo metadata");

    assert!(
        output.status.success(),
        "module-consumer metadata failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: JsonValue =
        serde_json::from_slice(&output.stdout).expect("Cargo metadata must be valid JSON");
    let packages = metadata["packages"]
        .as_array()
        .expect("Cargo metadata packages must be an array");
    assert!(
        packages
            .iter()
            .any(|package| package["name"] == "nara_scene")
    );
    assert!(packages.iter().any(|package| package["name"] == "nara"));

    let members = metadata["workspace_members"]
        .as_array()
        .expect("workspace_members must be an array");
    assert_eq!(
        members.len(),
        3,
        "the two direct module consumers and one workflow consumer must be members"
    );
    let member_ids = members
        .iter()
        .map(|member| member.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    for package_name in [
        "nara-scene-module-consumer",
        "nara-fs-module-consumer",
        "nara-project-workflow-consumer",
    ] {
        let package = packages
            .iter()
            .find(|package| package["name"] == package_name)
            .unwrap_or_else(|| panic!("{package_name} must be present"));
        assert!(
            member_ids.contains(package["id"].as_str().unwrap()),
            "{package_name} must be a workspace member"
        );
    }
    assert_eq!(
        Path::new(metadata["workspace_root"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        module_consumer_root().canonicalize().unwrap()
    );
}

#[test]
fn source_imports_are_declared_and_stay_on_the_supported_surface() {
    let consumer = module_consumer_root();
    for root in [consumer.join("src"), consumer.join("tests")] {
        for path in rust_sources(&root) {
            let source = fs::read_to_string(&path).unwrap();
            assert_source_boundary(&source, &ALLOWED_SOURCE_ROOTS)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        }
    }
    for forbidden in [
        "use nara::scene::SceneDocument;",
        "const _: Option<nara_app::PrivateType> = None;",
        "#[path = \"../hidden.rs\"] mod hidden;",
        "include!(\"../hidden.rs\");",
        "macro_rules! hidden { () => { use nara_ecs::World; } }",
    ] {
        assert!(assert_source_boundary(forbidden, &ALLOWED_SOURCE_ROOTS).is_err());
    }
}

#[test]
fn project_workflow_consumer_uses_only_the_public_root_module() {
    let root = module_consumer_root().join("project-workflow-consumer/src");
    for path in rust_sources(&root) {
        let source = fs::read_to_string(&path).unwrap();
        assert_source_boundary(&source, &PROJECT_WORKFLOW_ALLOWED_SOURCE_ROOTS)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }

    for forbidden in [
        "use nara_project::ProjectManifest;",
        "use nara_fs::DirectoryCapability;",
        "use nara_winit::WinitPlugin;",
        "include!(\"../hidden.rs\");",
    ] {
        assert!(assert_source_boundary(forbidden, &PROJECT_WORKFLOW_ALLOWED_SOURCE_ROOTS).is_err());
    }
}

#[test]
fn manifest_boundary_rejects_product_and_workspace_shortcuts() {
    let source = fs::read_to_string(module_consumer_root().join("Cargo.toml")).unwrap();
    let manifest = toml::from_str::<TomlValue>(&source).unwrap();

    let mut root_facade = manifest.clone();
    root_facade["dependencies"].as_table_mut().unwrap().insert(
        "engine".to_owned(),
        dependency_table([
            ("package", TomlValue::String("nara".to_owned())),
            ("path", TomlValue::String("..".to_owned())),
        ]),
    );
    assert!(assert_manifest_boundary(&root_facade).is_err());

    let mut inherited = manifest.clone();
    inherited["dependencies"]["nara_scene"] =
        dependency_table([("workspace", TomlValue::Boolean(true))]);
    assert!(assert_manifest_boundary(&inherited).is_err());

    let mut patched = manifest.clone();
    patched
        .as_table_mut()
        .unwrap()
        .insert("patch".to_owned(), TomlValue::Table(toml::map::Map::new()));
    assert!(assert_manifest_boundary(&patched).is_err());

    let mut private_crate = manifest;
    private_crate["dependencies"]
        .as_table_mut()
        .unwrap()
        .insert(
            "engine_ecs".to_owned(),
            dependency_table([
                ("package", TomlValue::String("nara_ecs".to_owned())),
                ("path", TomlValue::String("../crates/nara_ecs".to_owned())),
            ]),
        );
    assert!(assert_manifest_boundary(&private_crate).is_err());
}

#[test]
fn filesystem_manifest_boundary_rejects_facade_workspace_and_private_shortcuts() {
    let source = fs::read_to_string(module_consumer_root().join("fs-consumer/Cargo.toml")).unwrap();
    let manifest = toml::from_str::<TomlValue>(&source).unwrap();
    assert_filesystem_manifest_boundary(&manifest).unwrap();

    let mut root_facade = manifest.clone();
    root_facade["dependencies"].as_table_mut().unwrap().insert(
        "engine".to_owned(),
        dependency_table([
            ("package", TomlValue::String("nara".to_owned())),
            ("path", TomlValue::String("../..".to_owned())),
        ]),
    );
    assert!(assert_filesystem_manifest_boundary(&root_facade).is_err());

    let mut inherited = manifest.clone();
    inherited["dependencies"]["nara_fs"] =
        dependency_table([("workspace", TomlValue::Boolean(true))]);
    assert!(assert_filesystem_manifest_boundary(&inherited).is_err());

    let mut private_crate = manifest;
    private_crate["dependencies"]
        .as_table_mut()
        .unwrap()
        .insert(
            "nara_project".to_owned(),
            dependency_table([(
                "path",
                TomlValue::String("../../crates/nara_project".to_owned()),
            )]),
        );
    assert!(assert_filesystem_manifest_boundary(&private_crate).is_err());
}

#[test]
fn project_workflow_manifest_boundary_rejects_extra_features_and_private_shortcuts() {
    let source =
        fs::read_to_string(module_consumer_root().join("project-workflow-consumer/Cargo.toml"))
            .unwrap();
    let manifest = toml::from_str::<TomlValue>(&source).unwrap();
    assert_project_workflow_manifest_boundary(&manifest).unwrap();

    let mut inherited = manifest.clone();
    inherited["dependencies"]["nara"] = dependency_table([("workspace", TomlValue::Boolean(true))]);
    assert!(assert_project_workflow_manifest_boundary(&inherited).is_err());

    let mut desktop = manifest.clone();
    desktop["dependencies"]["nara"]["features"] = TomlValue::Array(vec![
        TomlValue::String("project-workflow".to_owned()),
        TomlValue::String("desktop-winit".to_owned()),
    ]);
    assert!(assert_project_workflow_manifest_boundary(&desktop).is_err());

    let mut private_crate = manifest;
    private_crate["dependencies"]
        .as_table_mut()
        .unwrap()
        .insert(
            "nara_project".to_owned(),
            dependency_table([(
                "path",
                TomlValue::String("../../crates/nara_project".to_owned()),
            )]),
        );
    assert!(assert_project_workflow_manifest_boundary(&private_crate).is_err());
}

#[test]
fn removing_documented_registry_prerequisite_fails_at_the_import() {
    let repository = repository_root();
    let consumer = module_consumer_root();
    let temporary = TemporaryDirectory::new(&repository, "missing-nara-reflect");
    fs::create_dir_all(temporary.path().join("src")).unwrap();

    let source = fs::read_to_string(consumer.join("src/lib.rs")).unwrap();
    fs::write(temporary.path().join("src/lib.rs"), source).unwrap();

    let manifest_source = fs::read_to_string(consumer.join("Cargo.toml")).unwrap();
    let mut manifest = toml::from_str::<TomlValue>(&manifest_source).unwrap();
    let workspace = manifest["workspace"]
        .as_table_mut()
        .expect("module-consumer must own its isolated workspace");
    workspace.remove("members");
    workspace.remove("default-members");
    manifest["dependencies"]
        .as_table_mut()
        .unwrap()
        .remove("nara_reflect");
    manifest.as_table_mut().unwrap().remove("dev-dependencies");
    manifest["dependencies"]["nara_scene"]["path"] = TomlValue::String(
        repository
            .join("crates/nara_scene")
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
    );
    fs::write(
        temporary.path().join("Cargo.toml"),
        toml::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let output = Command::new(cargo_executable())
        .current_dir(&repository)
        .args(["check", "--manifest-path"])
        .arg(temporary.path().join("Cargo.toml"))
        .args(["--offline", "--lib", "--jobs", "1"])
        .env("CARGO_TERM_COLOR", "never")
        .env(
            "CARGO_TARGET_DIR",
            repository.join("target/module-consumer-boundary-cargo"),
        )
        .output()
        .expect("failed to execute the missing-prerequisite compile probe");

    assert!(
        !output.status.success(),
        "removing nara_reflect unexpectedly compiled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nara_reflect")
            && (stderr.contains("unresolved import") || stderr.contains("unlinked crate")),
        "missing prerequisite must fail at the nara_reflect import:\n{stderr}"
    );
}

#[test]
fn scene_module_documentation_names_the_exact_supported_prerequisites() {
    let readme = fs::read_to_string(repository_root().join("crates/nara_scene/README.md"))
        .expect("nara_scene must document its direct-consumer surface");

    for required in [
        "nara_scene",
        "nara_reflect",
        "bevy_ecs",
        "does not promise",
        "arbitrary cross-engine compatibility",
    ] {
        assert!(readme.contains(required), "README is missing {required:?}");
    }
    assert!(
        !readme.contains("nara ="),
        "README must not route through facade"
    );
    assert!(!readme.contains("workspace = true"));
    assert!(!readme.contains("[patch"));
}

#[test]
fn filesystem_module_documentation_names_the_bounded_public_contract() {
    let readme = fs::read_to_string(repository_root().join("crates/nara_fs/README.md"))
        .expect("nara_fs must document its direct-consumer surface");

    for required in [
        "Host-issued handles",
        "RelativeComponent",
        "production Host",
        "high-entropy name",
        "fixed names are suitable only for fixtures",
        "DirectoryPublicationTransaction::publish",
        "recoverable orphan",
        "module-consumer/fs-consumer",
    ] {
        assert!(readme.contains(required), "README is missing {required:?}");
    }
    for forbidden in ["nara =", "workspace = true", "[patch"] {
        assert!(
            !readme.contains(forbidden),
            "README contains the forbidden shortcut {forbidden:?}"
        );
    }
}

#[test]
fn filesystem_consumer_production_has_no_ambient_path_authority() {
    let source = fs::read_to_string(module_consumer_root().join("fs-consumer/src/lib.rs"))
        .expect("filesystem consumer production source must exist");
    let production = source
        .split_once("#[cfg(all(test,")
        .map_or(source.as_str(), |(production, _)| production);

    for forbidden in [
        "std::fs",
        "std::path",
        "File::open",
        "OpenOptions",
        "current_dir",
        "current_exe",
        "CARGO_MANIFEST_DIR",
    ] {
        assert!(
            !production.contains(forbidden),
            "filesystem consumer production code contains ambient authority {forbidden:?}"
        );
    }
}

fn assert_manifest_boundary(manifest: &TomlValue) -> Result<(), String> {
    assert_direct_manifest_boundary(
        manifest,
        &EXPECTED_DEPENDENCIES,
        &["nara_scene", "nara_reflect"],
    )
}

fn assert_filesystem_manifest_boundary(manifest: &TomlValue) -> Result<(), String> {
    assert_direct_manifest_boundary(manifest, &FS_EXPECTED_DEPENDENCIES, &["nara_fs"])
}

fn assert_project_workflow_manifest_boundary(manifest: &TomlValue) -> Result<(), String> {
    if manifest.get("patch").is_some() {
        return Err("[patch] is forbidden".to_owned());
    }
    let mut dependencies = Vec::new();
    collect_dependency_entries(manifest, &mut Vec::new(), &mut dependencies)?;
    dependencies.sort_by(|left, right| left.0.cmp(&right.0));
    let observed = dependencies
        .iter()
        .map(|(path, _, _)| path.as_str())
        .collect::<Vec<_>>();
    if observed != PROJECT_WORKFLOW_EXPECTED_DEPENDENCIES {
        return Err(format!("unexpected direct dependencies: {observed:?}"));
    }

    let dependency = dependencies[0]
        .2
        .as_table()
        .ok_or_else(|| "dependencies.nara must use an explicit dependency table".to_owned())?;
    if dependency.get("workspace").is_some() {
        return Err("the root facade dependency cannot inherit from a workspace".to_owned());
    }
    if dependency
        .get("default-features")
        .and_then(TomlValue::as_bool)
        != Some(false)
    {
        return Err("default root features must remain disabled".to_owned());
    }
    if dependency.get("features").and_then(TomlValue::as_array)
        != Some(&vec![TomlValue::String("project-workflow".to_owned())])
    {
        return Err("only the project-workflow feature is allowed".to_owned());
    }
    Ok(())
}

fn assert_direct_manifest_boundary(
    manifest: &TomlValue,
    expected_dependencies: &[&str],
    allowed_nara_packages: &[&str],
) -> Result<(), String> {
    if manifest.get("patch").is_some() {
        return Err("[patch] is forbidden".to_owned());
    }
    let mut dependencies = Vec::new();
    collect_dependency_entries(manifest, &mut Vec::new(), &mut dependencies)?;
    dependencies.sort_by(|left, right| left.0.cmp(&right.0));
    let observed = dependencies
        .iter()
        .map(|(path, _, _)| path.as_str())
        .collect::<Vec<_>>();
    if observed != expected_dependencies {
        return Err(format!("unexpected direct dependencies: {observed:?}"));
    }

    for (path, dependency_name, value) in dependencies {
        let table = value.as_table();
        if table
            .and_then(|table| table.get("workspace"))
            .and_then(TomlValue::as_bool)
            == Some(true)
        {
            return Err(format!("{path} inherits from a workspace"));
        }
        let package_name = table
            .and_then(|table| table.get("package"))
            .and_then(TomlValue::as_str)
            .unwrap_or(dependency_name);
        if package_name == "nara" {
            return Err("the root facade is forbidden".to_owned());
        }
        if package_name.starts_with("nara_") && !allowed_nara_packages.contains(&package_name) {
            return Err(format!(
                "private Nara dependency is forbidden: {package_name}"
            ));
        }
    }
    Ok(())
}

fn collect_dependency_entries<'a>(
    value: &'a TomlValue,
    path: &mut Vec<String>,
    entries: &mut Vec<(String, &'a str, &'a TomlValue)>,
) -> Result<(), String> {
    let Some(table) = value.as_table() else {
        return Ok(());
    };
    for (key, child) in table {
        path.push(key.clone());
        if matches!(
            key.as_str(),
            "dependencies" | "dev-dependencies" | "build-dependencies"
        ) {
            let dependencies = child
                .as_table()
                .ok_or_else(|| format!("{} must be a table", path.join(".")))?;
            for (dependency, value) in dependencies {
                entries.push((
                    format!("{}.{}", path.join("."), dependency),
                    dependency,
                    value,
                ));
            }
        } else {
            collect_dependency_entries(child, path, entries)?;
        }
        path.pop();
    }
    Ok(())
}

fn assert_source_boundary(source: &str, allowed_roots: &[&str]) -> Result<(), String> {
    let syntax = syn::parse_file(source).map_err(|error| error.to_string())?;
    let mut visitor = ImportRootVisitor::default();
    visitor.visit_file(&syntax);
    let allowed = allowed_roots.iter().copied().collect::<BTreeSet<_>>();
    let undeclared = visitor
        .roots
        .iter()
        .filter(|root| !allowed.contains(root.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if undeclared.is_empty() && visitor.redirects.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "undeclared imports {undeclared:?}; uninspectable redirects {:?}",
            visitor.redirects
        ))
    }
}

#[derive(Default)]
struct ImportRootVisitor {
    roots: BTreeSet<String>,
    local_bindings: BTreeSet<String>,
    redirects: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for ImportRootVisitor {
    fn visit_file(&mut self, file: &'ast syn::File) {
        for item in &file.items {
            match item {
                syn::Item::Use(item) => self.record_use(item),
                syn::Item::ExternCrate(item) => self.record_extern_crate(item),
                _ => {}
            }
        }
        syn::visit::visit_file(self, file);
    }

    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        self.record_use(item);
    }

    fn visit_item_extern_crate(&mut self, item: &'ast ItemExternCrate) {
        self.record_extern_crate(item);
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
        let name = item.path.segments.last().unwrap().ident.to_string();
        if matches!(name.as_str(), "include" | "macro_rules") {
            self.redirects.insert(name);
        }
        syn::visit::visit_macro(self, item);
    }

    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if attribute.path().is_ident("path") || cfg_attr_redirects_path(attribute) {
            self.redirects
                .insert(attribute.path().segments.last().unwrap().ident.to_string());
        }
        syn::visit::visit_attribute(self, attribute);
    }
}

impl ImportRootVisitor {
    fn record_use(&mut self, item: &ItemUse) {
        collect_use_roots(&item.tree, &mut self.roots);
        collect_use_bindings(&item.tree, &mut self.local_bindings);
        if use_tree_contains_include(&item.tree) {
            self.redirects.insert("imported include".to_owned());
        }
    }

    fn record_extern_crate(&mut self, item: &ItemExternCrate) {
        self.roots.insert(item.ident.to_string());
        if let Some((_, rename)) = &item.rename {
            self.local_bindings.insert(rename.to_string());
        }
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
        UseTree::Path(path) => collect_use_bindings(&path.tree, bindings),
        UseTree::Name(name) => {
            bindings.insert(name.ident.to_string());
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

fn use_tree_contains_include(tree: &UseTree) -> bool {
    match tree {
        UseTree::Path(path) => use_tree_contains_include(&path.tree),
        UseTree::Name(name) => name.ident == "include",
        UseTree::Rename(rename) => rename.ident == "include",
        UseTree::Group(group) => group.items.iter().any(use_tree_contains_include),
        UseTree::Glob(_) => false,
    }
}

fn cfg_attr_redirects_path(attribute: &syn::Attribute) -> bool {
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

fn assert_path_dependency(manifest: &TomlValue, dependency: &str, expected_path: &str) {
    assert_eq!(
        manifest["dependencies"][dependency]
            .get("path")
            .and_then(TomlValue::as_str),
        Some(expected_path)
    );
}

fn dependency_table<const N: usize>(entries: [(&str, TomlValue); N]) -> TomlValue {
    TomlValue::Table(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut sources = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
        {
            let entry = entry.unwrap();
            let file_type = entry.file_type().unwrap();
            assert!(!file_type.is_symlink(), "source links are forbidden");
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

fn cargo_executable() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn module_consumer_root() -> PathBuf {
    repository_root().join("module-consumer")
}

struct TemporaryDirectory {
    path: PathBuf,
    permitted_root: PathBuf,
}

impl TemporaryDirectory {
    fn new(repository: &Path, purpose: &str) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let permitted_root = repository.join("target/module-consumer-boundary");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = permitted_root.join(format!(
            "{purpose}-{}-{timestamp}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self {
            path,
            permitted_root,
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        assert!(self.path.starts_with(&self.permitted_root));
        assert_ne!(self.path, self.permitted_root);
        let _ = fs::remove_dir_all(&self.path);
    }
}
