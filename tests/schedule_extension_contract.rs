use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use syn::{UseTree, visit::Visit};

#[test]
fn fixture_depends_only_on_the_renamed_public_root_package() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = fixture_root(repository);
    assert!(fixture.join("Cargo.lock").is_file());
    let output = Command::new(cargo_executable())
        .current_dir(repository)
        .args(["metadata", "--manifest-path"])
        .arg(fixture.join("Cargo.toml"))
        .args(["--locked", "--no-deps", "--format-version", "1"])
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("failed to read schedule-extension fixture metadata");

    assert_command_succeeded("schedule-extension metadata", &output);
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Cargo metadata must be valid JSON");
    let packages = metadata["packages"]
        .as_array()
        .expect("Cargo metadata packages must be an array");
    assert_eq!(packages.len(), 1);
    let dependencies = packages[0]["dependencies"]
        .as_array()
        .expect("Cargo metadata dependencies must be an array");
    assert_eq!(dependencies.len(), 1, "{dependencies:#?}");
    let dependency = &dependencies[0];
    assert_eq!(dependency["name"], "nara");
    assert_eq!(dependency["rename"], "engine");
    assert_eq!(dependency["uses_default_features"], false);
    assert_eq!(dependency["features"], serde_json::json!(["runtime-core"]));
    let dependency_path = dependency["path"]
        .as_str()
        .map(Path::new)
        .expect("the renamed nara dependency must expose a local path")
        .canonicalize()
        .expect("the renamed nara dependency path must resolve");
    assert_eq!(dependency_path, repository.canonicalize().unwrap());
}

#[test]
fn renamed_root_fixture_observes_the_public_anchor_contract() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(cargo_executable())
        .current_dir(repository)
        .args(["test", "--manifest-path"])
        .arg(fixture_root(repository).join("Cargo.toml"))
        .args(["--locked", "--jobs", "1"])
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("failed to run schedule-extension fixture");

    assert_command_succeeded("schedule-extension fixture", &output);
}

#[test]
fn fixture_claims_exactly_the_documented_compatibility_anchors() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = fixture_root(repository);
    let manifest = fs::read_to_string(fixture.join("Cargo.toml")).unwrap();
    let surface = RustSurface::from_directory(&fixture.join("src"));

    assert!(!manifest.contains("bevy_ecs"));
    assert!(!manifest.contains("nara_ecs"));
    assert!(!manifest.contains("workspace = true"));
    assert!(!manifest.contains("[patch"));
    assert!(!surface.has_root("bevy_ecs"));
    assert!(!surface.has_root_matching(|root| root.starts_with("nara_")));
    for anchor in [
        &["CoreStage", "FixedUpdate"][..],
        &["CoreStage", "Cleanup"],
        &["FixedUpdateSet", "Simulate"],
        &["GameplayCommandSet", "Consume"],
        &["GameplayCommandSet", "Capture"],
    ] {
        assert!(
            surface.contains_path(anchor),
            "fixture omitted {}",
            anchor.join("::")
        );
    }
    assert_only_documented_schedule_variants("schedule-extension fixture", repository, &surface);
    assert!(
        !surface.has_external_glob(),
        "fixture must not hide schedule dependencies behind a public-root glob import"
    );
    assert!(!surface.contains_identifier("include"));
    assert!(!surface.has_attribute_identifier("path"));
    assert!(!surface.has_macro_token_identifier("path"));
    for private_target in [
        &["admit_gameplay_commands"][..],
        &["acknowledge_gameplay_commands"],
    ] {
        assert!(
            !surface.contains_identifier(private_target[0]),
            "fixture ordered against non-anchor {}",
            private_target.join("::")
        );
    }
    assert!(surface.contains_method("before_ignore_deferred"));
    for managed_runtime_type in ["RuntimeCandidate", "RuntimeInstance"] {
        assert!(
            !surface.contains_identifier(managed_runtime_type),
            "fixture must not claim managed-runtime escalation through {managed_runtime_type}"
        );
    }
}

#[test]
fn reference_game_uses_only_documented_schedule_anchors() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let surface = RustSurface::from_directory(&repository.join("reference-game/src"));
    let test_surface =
        RustSurface::from_schedule_files_in_directory(&repository.join("reference-game/tests"));

    assert!(surface.contains_path(&["CoreStage", "FixedUpdate"]));
    assert!(surface.contains_path(&["CoreStage", "Cleanup"]));
    assert!(surface.contains_path(&["FixedUpdateSet", "Simulate"]));
    assert_only_documented_schedule_variants("reference game", repository, &surface);
    assert_only_documented_schedule_variants("reference game tests", repository, &test_surface);
    for private_schedule_type in ["StartupStage", "TaskUpdateSet", "AssetTaskUpdateSet"] {
        assert!(
            !surface.contains_identifier(private_schedule_type)
                && !test_surface.contains_identifier(private_schedule_type),
            "reference game depends on non-anchor schedule type {private_schedule_type}"
        );
    }
    assert!(
        !surface.has_external_glob(),
        "reference game must not hide schedule dependencies behind a public-root glob import"
    );
    assert!(!surface.contains_identifier("include"));
    assert!(!surface.has_attribute_identifier("path"));
    assert!(!surface.has_macro_token_identifier("path"));
    for non_anchor in [
        &["admit_gameplay_commands"][..],
        &["acknowledge_gameplay_commands"],
    ] {
        assert!(
            !surface.contains_identifier(non_anchor[0])
                && !test_surface.contains_identifier(non_anchor[0]),
            "reference game depends on non-anchor {}",
            non_anchor.join("::")
        );
    }
}

#[test]
fn rust_surface_fails_closed_on_alias_and_macro_indirection() {
    let surface = RustSurface::from_source(
        r#"
        extern crate engine as runtime;
        use runtime::app::{FixedUpdateSet as FixedSets, RuntimeCandidate as Candidate};
        use runtime::app::FixedUpdateSet::Prepare;
        use runtime::gameplay::admit_gameplay_commands as admit;
        use runtime::app::*;

        type AliasedSets = FixedSets;

        macro_rules! hidden_set {
            ($variant:ident) => { let _ = crate::AliasedSets::$variant; };
        }

        macro_rules! hidden_source {
            () => {
                include!("hidden.inc");
                #[path = "hidden.rs"]
                mod generated;
            };
        }

        #[cfg_attr(any(), path = "hidden.inc")]
        mod hidden;

        fn probe(candidate: Option<Candidate>) {
            let _ = candidate;
            let _ = Prepare;
            let _ = runtime::app::FixedUpdateSet::r#Prepare;
            let _ = AliasedSets::Finalize;
            let _ = runtime::app::CoreStage::ALL[4];
            let _ = admit;
            hidden_set!(Admit);
            include!("hidden.inc");
        }
        "#,
    );

    assert!(surface.contains_path(&["FixedUpdateSet", "Prepare"]));
    assert!(surface.contains_identifier("Finalize"));
    assert!(surface.contains_identifier("Admit"));
    assert!(surface.contains_identifier("ALL"));
    assert!(surface.contains_identifier("RuntimeCandidate"));
    assert!(surface.contains_identifier("admit_gameplay_commands"));
    assert!(surface.has_external_glob());
    assert!(surface.contains_macro("include"));
    assert!(surface.has_attribute_identifier("path"));
    assert!(surface.has_macro_token_identifier("include"));
    assert!(surface.has_macro_token_identifier("path"));
}

#[test]
fn rust_surface_recurses_into_module_files() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let surface = RustSurface::from_directory(
        &repository.join("tests/fixtures/schedule-extension/surface-probe"),
    );

    assert!(surface.contains_path(&["FixedUpdateSet", "Prepare"]));
    assert!(surface.contains_identifier("Prepare"));
    assert_eq!(
        unsupported_enum_members(
            &surface,
            &repository.join("crates/nara_app/src/lib.rs"),
            "FixedUpdateSet",
            &["Simulate"],
        ),
        ["FixedUpdateSet::Prepare", "FixedUpdateSet::Finalize"]
    );
}

#[test]
fn schedule_allowlist_rejects_aggregate_enum_values() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let surface = RustSurface::from_source(
        "fn configure(app: &mut App) { app.add_systems(CoreStage::r#ALL[4], system); }",
    );

    assert_eq!(
        unsupported_enum_members(
            &surface,
            &repository.join("crates/nara_app/src/lib.rs"),
            "CoreStage",
            &["FixedUpdate"],
        ),
        ["CoreStage::ALL"]
    );
}

fn assert_only_documented_schedule_variants(label: &str, repository: &Path, surface: &RustSurface) {
    assert_enum_variant_allowlist(
        label,
        surface,
        &repository.join("crates/nara_app/src/lib.rs"),
        "CoreStage",
        &["FixedUpdate", "Cleanup"],
    );
    assert_enum_variant_allowlist(
        label,
        surface,
        &repository.join("crates/nara_app/src/lib.rs"),
        "FixedUpdateSet",
        &["Simulate"],
    );
    assert_enum_variant_allowlist(
        label,
        surface,
        &repository.join("crates/nara_gameplay/src/lib.rs"),
        "GameplayCommandSet",
        &["Consume", "Capture"],
    );
}

fn assert_enum_variant_allowlist(
    label: &str,
    surface: &RustSurface,
    declaration: &Path,
    enum_name: &str,
    allowed: &[&str],
) {
    let unsupported = unsupported_enum_members(surface, declaration, enum_name, allowed);

    assert!(
        unsupported.is_empty(),
        "{label} depends on non-anchor {enum_name} members: {unsupported:?}"
    );
}

fn unsupported_enum_members(
    surface: &RustSurface,
    declaration: &Path,
    enum_name: &str,
    allowed: &[&str],
) -> Vec<String> {
    let (variants, associated) = enum_members(declaration, enum_name);
    let type_names = surface.aliases_for(enum_name);
    variants
        .into_iter()
        .filter(|variant| {
            !allowed.contains(&variant.as_str())
                && surface.contains_member_access(&type_names, variant)
        })
        .chain(
            associated
                .into_iter()
                .filter(|member| surface.contains_member_access(&type_names, member)),
        )
        .map(|member| format!("{enum_name}::{member}"))
        .collect()
}

fn enum_members(declaration: &Path, enum_name: &str) -> (Vec<String>, Vec<String>) {
    let source = fs::read_to_string(declaration).expect("enum declaration source must be readable");
    let syntax = syn::parse_file(&source).expect("enum declaration source must parse");
    let variants = syntax
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Enum(item) if item.ident == enum_name => Some(
                item.variants
                    .iter()
                    .map(|variant| identifier_text(&variant.ident))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "missing enum declaration {enum_name} in {}",
                declaration.display()
            )
        });
    let associated = syntax
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Impl(item)
                if item.trait_.is_none()
                    && matches!(
                        item.self_ty.as_ref(),
                        syn::Type::Path(path)
                            if path.path.segments.last().is_some_and(|segment| segment.ident == enum_name)
                    ) => Some(&item.items),
            _ => None,
        })
        .flatten()
        .filter_map(|item| match item {
            syn::ImplItem::Const(item) => Some(identifier_text(&item.ident)),
            syn::ImplItem::Fn(item) => Some(identifier_text(&item.sig.ident)),
            syn::ImplItem::Type(item) => Some(identifier_text(&item.ident)),
            _ => None,
        })
        .collect();
    (variants, associated)
}

#[derive(Default)]
struct RustSurface {
    paths: Vec<Vec<String>>,
    aliases: Vec<(String, String)>,
    methods: Vec<String>,
    glob_imports: Vec<Vec<String>>,
    identifiers: Vec<String>,
    macros: Vec<String>,
    macro_identifiers: Vec<String>,
    attribute_identifiers: Vec<String>,
}

impl RustSurface {
    fn from_source(source: &str) -> Self {
        let syntax = syn::parse_file(source).expect("Rust boundary source must parse");
        let mut surface = Self::default();
        surface.visit_syntax(&syntax);
        surface
    }

    fn from_directory(directory: &Path) -> Self {
        let mut surface = Self::default();
        surface.visit_directory(directory);
        surface
    }

    fn from_schedule_files_in_directory(directory: &Path) -> Self {
        let mut surface = Self::default();
        surface.visit_directory(directory);
        surface
    }

    fn visit_directory(&mut self, directory: &Path) {
        for entry in fs::read_dir(directory).expect("Rust source directory must be readable") {
            let path = entry.expect("Rust source entry must be readable").path();
            if path.is_dir() {
                self.visit_directory(&path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                self.visit_path_file(&path);
            }
        }
    }

    fn visit_path_file(&mut self, path: &Path) {
        let source = fs::read_to_string(path).expect("Rust boundary source must be readable");
        let syntax = syn::parse_file(&source).expect("Rust boundary source must parse");
        self.visit_syntax(&syntax);
    }

    fn visit_syntax(&mut self, syntax: &syn::File) {
        let imports = ImportSurface::from_file(syntax);
        self.identifiers
            .extend(imports.paths.iter().flatten().cloned());
        self.paths.extend(imports.paths);
        self.aliases.extend(imports.aliases);
        self.glob_imports.extend(imports.glob_imports);
        PathSurface { surface: self }.visit_file(syntax);
    }

    fn contains_path(&self, expected: &[&str]) -> bool {
        self.paths.iter().any(|path| {
            path.len() >= expected.len()
                && path[path.len() - expected.len()..]
                    .iter()
                    .map(String::as_str)
                    .eq(expected.iter().copied())
        })
    }

    fn aliases_for(&self, root: &str) -> BTreeSet<String> {
        let mut aliases = BTreeSet::from([root.to_owned()]);
        loop {
            let before = aliases.len();
            for (alias, target) in &self.aliases {
                if aliases.contains(target) {
                    aliases.insert(alias.clone());
                }
            }
            if aliases.len() == before {
                return aliases;
            }
        }
    }

    fn contains_member_access(&self, type_names: &BTreeSet<String>, member: &str) -> bool {
        let qualified = self.paths.iter().any(|path| {
            path.len() >= 2
                && path.last().is_some_and(|segment| segment == member)
                && type_names.contains(&path[path.len() - 2])
        });
        qualified
            || (self
                .macro_identifiers
                .iter()
                .any(|identifier| identifier == member)
                && self
                    .macro_identifiers
                    .iter()
                    .any(|identifier| type_names.contains(identifier)))
    }

    fn has_root(&self, expected: &str) -> bool {
        self.has_root_matching(|root| root == expected)
    }

    fn has_root_matching(&self, predicate: impl Fn(&str) -> bool) -> bool {
        self.paths
            .iter()
            .filter_map(|path| path.first())
            .any(|root| predicate(root))
    }

    fn contains_method(&self, expected: &str) -> bool {
        self.methods.iter().any(|method| method == expected)
    }

    fn has_external_glob(&self) -> bool {
        self.glob_imports.iter().any(|path| {
            path.first().is_some_and(|root| {
                !matches!(
                    root.as_str(),
                    "std" | "core" | "alloc" | "crate" | "self" | "super"
                )
            })
        })
    }

    fn contains_identifier(&self, expected: &str) -> bool {
        self.identifiers
            .iter()
            .any(|identifier| identifier == expected)
    }

    fn contains_macro(&self, expected: &str) -> bool {
        self.macros.iter().any(|name| name == expected)
    }

    fn has_attribute_identifier(&self, expected: &str) -> bool {
        self.attribute_identifiers
            .iter()
            .any(|identifier| identifier == expected)
    }

    fn has_macro_token_identifier(&self, expected: &str) -> bool {
        self.macro_identifiers
            .iter()
            .any(|identifier| identifier == expected)
    }
}

#[derive(Default)]
struct ImportSurface {
    paths: Vec<Vec<String>>,
    glob_imports: Vec<Vec<String>>,
    aliases: Vec<(String, String)>,
}

impl ImportSurface {
    fn from_file(file: &syn::File) -> Self {
        let mut imports = Self::default();
        imports.visit_file(file);
        imports
    }

    fn collect_use_tree(&mut self, prefix: &mut Vec<String>, tree: &UseTree) {
        match tree {
            UseTree::Path(path) => {
                prefix.push(identifier_text(&path.ident));
                self.collect_use_tree(prefix, &path.tree);
                prefix.pop();
            }
            UseTree::Name(name) => {
                let path = if name.ident == "self" {
                    prefix.clone()
                } else {
                    let mut path = prefix.clone();
                    path.push(identifier_text(&name.ident));
                    path
                };
                self.paths.push(path);
            }
            UseTree::Rename(rename) => {
                let mut path = prefix.clone();
                if rename.ident != "self" {
                    path.push(identifier_text(&rename.ident));
                }
                if let Some(target) = path.last() {
                    self.aliases
                        .push((identifier_text(&rename.rename), target.clone()));
                }
                self.paths.push(path);
            }
            UseTree::Glob(_) => self.glob_imports.push(prefix.clone()),
            UseTree::Group(group) => {
                for tree in &group.items {
                    self.collect_use_tree(prefix, tree);
                }
            }
        }
    }
}

impl<'ast> Visit<'ast> for ImportSurface {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.collect_use_tree(&mut Vec::new(), &item.tree);
    }

    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        let path = vec![identifier_text(&item.ident)];
        self.paths.push(path);
    }
}

struct PathSurface<'surface> {
    surface: &'surface mut RustSurface,
}

impl<'ast> Visit<'ast> for PathSurface<'_> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let canonical = path
            .segments
            .iter()
            .map(|segment| identifier_text(&segment.ident))
            .collect::<Vec<_>>();
        self.surface.identifiers.extend(canonical.iter().cloned());
        self.surface.paths.push(canonical);
        syn::visit::visit_path(self, path);
    }

    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        self.surface.identifiers.push(identifier_text(ident));
    }

    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        if let syn::Type::Path(target) = item.ty.as_ref()
            && let Some(target) = target.path.segments.last()
        {
            self.surface
                .aliases
                .push((identifier_text(&item.ident), identifier_text(&target.ident)));
        }
        syn::visit::visit_item_type(self, item);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        self.surface.methods.push(call.method.to_string());
        syn::visit::visit_expr_method_call(self, call);
    }

    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        if let Some(name) = item.path.segments.last() {
            self.surface.macros.push(identifier_text(&name.ident));
        }
        let mut identifiers = Vec::new();
        collect_token_identifiers(&item.tokens.to_string(), &mut identifiers);
        self.surface.identifiers.extend(identifiers.iter().cloned());
        self.surface.macro_identifiers.extend(identifiers);
        syn::visit::visit_macro(self, item);
    }

    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        self.surface.attribute_identifiers.extend(
            attribute
                .path()
                .segments
                .iter()
                .map(|segment| identifier_text(&segment.ident)),
        );
        if let syn::Meta::List(list) = &attribute.meta {
            collect_token_identifiers(
                &list.tokens.to_string(),
                &mut self.surface.attribute_identifiers,
            );
        }
        syn::visit::visit_attribute(self, attribute);
    }
}

fn collect_token_identifiers(tokens: &str, output: &mut Vec<String>) {
    output.extend(
        tokens
            .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .filter(|token| {
                token
                    .chars()
                    .next()
                    .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
            })
            .map(str::to_owned),
    );
}

fn identifier_text(identifier: &syn::Ident) -> String {
    let identifier = identifier.to_string();
    identifier
        .strip_prefix("r#")
        .unwrap_or(&identifier)
        .to_owned()
}

fn fixture_root(repository: &Path) -> PathBuf {
    repository
        .join("tests")
        .join("fixtures")
        .join("schedule-extension")
        .join("renamed-root")
}

fn cargo_executable() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn assert_command_succeeded(label: &str, output: &std::process::Output) {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
