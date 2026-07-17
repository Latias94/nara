use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use nara::app::{App, RuntimeAdmissionError, RuntimeCandidate, RuntimeInstance};
use serde_json::Value;
use syn::{Fields, ImplItem, Item, Signature, TraitItem, Type, Visibility, visit::Visit};

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

struct PublicFunction {
    name: String,
    signature: Signature,
}

fn parse_rust(source: &str) -> syn::File {
    syn::parse_file(source).expect("Rust boundary source must parse")
}

fn is_public(visibility: &Visibility) -> bool {
    matches!(visibility, Visibility::Public(_))
}

fn public_functions(source: &str) -> Vec<PublicFunction> {
    let file = parse_rust(source);
    let mut functions = Vec::new();
    collect_public_functions(&file.items, &mut functions);
    functions
}

fn collect_public_functions(items: &[Item], functions: &mut Vec<PublicFunction>) {
    for item in items {
        match item {
            Item::Fn(function) if is_public(&function.vis) => functions.push(PublicFunction {
                name: function.sig.ident.to_string(),
                signature: function.sig.clone(),
            }),
            Item::Impl(item_impl) => {
                functions.extend(item_impl.items.iter().filter_map(|item| {
                    let ImplItem::Fn(function) = item else {
                        return None;
                    };
                    is_public(&function.vis).then(|| PublicFunction {
                        name: function.sig.ident.to_string(),
                        signature: function.sig.clone(),
                    })
                }));
            }
            Item::Trait(item_trait) if is_public(&item_trait.vis) => {
                functions.extend(item_trait.items.iter().filter_map(|item| {
                    let TraitItem::Fn(function) = item else {
                        return None;
                    };
                    Some(PublicFunction {
                        name: function.sig.ident.to_string(),
                        signature: function.sig.clone(),
                    })
                }));
            }
            Item::Mod(module) => {
                if let Some((_, items)) = &module.content {
                    collect_public_functions(items, functions);
                }
            }
            _ => {}
        }
    }
}

struct MutableReferenceVisitor<'target> {
    target: &'target str,
    found: bool,
}

impl<'ast> Visit<'ast> for MutableReferenceVisitor<'_> {
    fn visit_type_reference(&mut self, reference: &'ast syn::TypeReference) {
        if reference.mutability.is_some() && type_ends_with(&reference.elem, self.target) {
            self.found = true;
        }
        syn::visit::visit_type_reference(self, reference);
    }
}

fn type_ends_with(ty: &Type, target: &str) -> bool {
    matches!(ty, Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == target))
}

fn signature_has_mutable_reference_to(signature: &Signature, target: &str) -> bool {
    let mut visitor = MutableReferenceVisitor {
        target,
        found: false,
    };
    visitor.visit_signature(signature);
    visitor.found
}

fn public_data_surfaces_with_mutable_world(items: &[Item], found: &mut Vec<String>) {
    for item in items {
        match item {
            Item::Struct(item_struct) if is_public(&item_struct.vis) => {
                for field in &item_struct.fields {
                    if is_public(&field.vis) && type_has_mutable_world(&field.ty) {
                        found.push(item_struct.ident.to_string());
                    }
                }
            }
            Item::Enum(item_enum) if is_public(&item_enum.vis) => {
                for variant in &item_enum.variants {
                    if fields_have_mutable_world(&variant.fields) {
                        found.push(format!("{}::{}", item_enum.ident, variant.ident));
                    }
                }
            }
            Item::Type(item_type)
                if is_public(&item_type.vis) && type_has_mutable_world(&item_type.ty) =>
            {
                found.push(item_type.ident.to_string());
            }
            Item::Mod(module) => {
                if let Some((_, items)) = &module.content {
                    public_data_surfaces_with_mutable_world(items, found);
                }
            }
            _ => {}
        }
    }
}

fn fields_have_mutable_world(fields: &Fields) -> bool {
    fields.iter().any(|field| type_has_mutable_world(&field.ty))
}

fn type_has_mutable_world(ty: &Type) -> bool {
    let mut visitor = MutableReferenceVisitor {
        target: "World",
        found: false,
    };
    visitor.visit_type(ty);
    visitor.found
}

fn impl_self_name(item_impl: &syn::ItemImpl) -> Option<String> {
    let Type::Path(path) = item_impl.self_ty.as_ref() else {
        return None;
    };
    path.path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn forbidden_conversion_impls(items: &[Item], found: &mut Vec<String>) {
    const MANAGED_TYPES: [&str; 6] = [
        "RuntimeCandidate",
        "ReadyRuntimeCandidate",
        "RuntimeInstance",
        "RuntimeRetirement",
        "RuntimeDriverScope",
        "RuntimeCloseContext",
    ];
    for item in items {
        match item {
            Item::Impl(item_impl) => {
                let Some((_, trait_path, _)) = &item_impl.trait_ else {
                    continue;
                };
                let Some(self_name) = impl_self_name(item_impl) else {
                    continue;
                };
                let Some(trait_name) = trait_path
                    .segments
                    .last()
                    .map(|segment| segment.ident.to_string())
                else {
                    continue;
                };
                if MANAGED_TYPES.contains(&self_name.as_str())
                    && matches!(trait_name.as_str(), "DerefMut" | "AsMut")
                {
                    found.push(format!("{trait_name} for {self_name}"));
                }
            }
            Item::Mod(module) => {
                if let Some((_, items)) = &module.content {
                    forbidden_conversion_impls(items, found);
                }
            }
            _ => {}
        }
    }
}

fn public_inherent_methods(items: &[Item], target: &str, methods: &mut Vec<String>) {
    for item in items {
        match item {
            Item::Impl(item_impl)
                if item_impl.trait_.is_none()
                    && impl_self_name(item_impl).as_deref() == Some(target) =>
            {
                methods.extend(item_impl.items.iter().filter_map(|item| {
                    let ImplItem::Fn(function) = item else {
                        return None;
                    };
                    is_public(&function.vis).then(|| function.sig.ident.to_string())
                }));
            }
            Item::Mod(module) => {
                if let Some((_, items)) = &module.content {
                    public_inherent_methods(items, target, methods);
                }
            }
            _ => {}
        }
    }
}

struct IdentifierVisitor<'target> {
    target: &'target str,
    found: bool,
}

impl<'ast> Visit<'ast> for IdentifierVisitor<'_> {
    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        if ident == self.target {
            self.found = true;
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if call.method == self.target {
            self.found = true;
        }
        syn::visit::visit_expr_method_call(self, call);
    }

    fn visit_path_segment(&mut self, segment: &'ast syn::PathSegment) {
        if segment.ident == self.target {
            self.found = true;
        }
        syn::visit::visit_path_segment(self, segment);
    }
}

fn file_uses_identifier(source: &str, target: &str) -> bool {
    let file = parse_rust(source);
    let mut visitor = IdentifierVisitor {
        target,
        found: false,
    };
    visitor.visit_file(&file);
    visitor.found
}

struct IdentifierCollector {
    identifiers: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for IdentifierCollector {
    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        self.identifiers.insert(ident.to_string());
    }
}

fn rust_identifiers(source: &str) -> BTreeSet<String> {
    let file = parse_rust(source);
    let mut collector = IdentifierCollector {
        identifiers: BTreeSet::new(),
    };
    collector.visit_file(&file);
    collector.identifiers
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
            public_functions(&source)
        })
        .collect::<Vec<_>>();

    let raw_app_entries = functions
        .iter()
        .filter(|function| signature_has_mutable_reference_to(&function.signature, "App"))
        .map(|function| function.name.as_str())
        .collect::<Vec<_>>();
    assert!(
        raw_app_entries.is_empty(),
        "nara_winit exposes public raw App driver entries: {raw_app_entries:?}"
    );
    assert!(
        functions.iter().any(|function| {
            function.name == "run"
                && signature_has_mutable_reference_to(&function.signature, "RuntimeInstance")
        }),
        "nara_winit must expose a public RuntimeInstance runner entry"
    );
}

#[test]
fn managed_runtime_scopes_do_not_expose_raw_world_mutation() {
    let runtime_source =
        fs::read_to_string(workspace_root().join("crates/nara_app/src/runtime.rs"))
            .expect("runtime source must be readable");
    let runtime_public_functions = public_functions(&runtime_source);
    let forbidden = runtime_public_functions
        .iter()
        .filter(|function| matches!(function.name.as_str(), "world_mut" | "scope_world_mut"))
        .map(|function| function.name.as_str())
        .collect::<Vec<_>>();
    assert!(
        forbidden.is_empty(),
        "managed runtime scopes expose raw World mutation: {forbidden:?}"
    );
    let raw_world_signatures = runtime_public_functions
        .iter()
        .filter(|function| signature_has_mutable_reference_to(&function.signature, "World"))
        .map(|function| function.name.as_str())
        .collect::<Vec<_>>();
    assert!(
        raw_world_signatures.is_empty(),
        "managed runtime public signatures expose raw mutable World access: {raw_world_signatures:?}"
    );
    let runtime_file = parse_rust(&runtime_source);
    let mut public_data_surfaces = Vec::new();
    public_data_surfaces_with_mutable_world(&runtime_file.items, &mut public_data_surfaces);
    assert!(
        public_data_surfaces.is_empty(),
        "managed runtime public data exposes mutable World access: {public_data_surfaces:?}"
    );
    let mut driver_scope_methods = Vec::new();
    public_inherent_methods(
        &runtime_file.items,
        "RuntimeDriverScope",
        &mut driver_scope_methods,
    );
    for structural_mutator in [
        "insert_resource",
        "remove_resource",
        "remove_non_send_resource",
        "for_each_component_mut",
        "spawn_component",
        "remove_component",
        "with_component_mut",
    ] {
        assert!(
            !driver_scope_methods
                .iter()
                .any(|method| method == structural_mutator),
            "managed runtime driver scope regained generic structural mutation through {structural_mutator}"
        );
    }
    let mut conversion_impls = Vec::new();
    forbidden_conversion_impls(&runtime_file.items, &mut conversion_impls);
    assert!(
        conversion_impls.is_empty(),
        "managed runtime scope exposes mutation through conversion traits: {conversion_impls:?}"
    );

    let winit_source = fs::read_to_string(workspace_root().join("crates/nara_winit/src/lib.rs"))
        .expect("winit source must be readable");
    assert!(
        !file_uses_identifier(&winit_source, "world_mut"),
        "nara_winit regained ambient runtime World mutation"
    );
}

#[test]
fn public_signature_parser_detects_renamed_raw_world_paths() {
    let functions = public_functions(
        "pub fn as_world_mut(&mut self) -> &mut World { todo!() }\n\
         pub fn with_world(operation: impl FnOnce(&mut World)) { todo!() }",
    );

    assert!(
        signature_has_mutable_reference_to(&functions[0].signature, "World"),
        "renamed raw World return escaped signature parsing"
    );
    assert!(
        signature_has_mutable_reference_to(&functions[1].signature, "World"),
        "World-bearing closure escaped signature parsing"
    );
}

#[test]
fn ast_guard_detects_conversion_fields_and_later_inherent_impls() {
    let file = parse_rust(
        "pub struct Escape<'a> { pub world: &'a mut World }\n\
         impl RuntimeDriverScope<'_> { fn harmless(&self) {} }\n\
         impl RuntimeDriverScope<'_> { pub fn insert_resource(&mut self) {} }\n\
         impl<'w> DerefMut for RuntimeDriverScope<'w> {\n\
             fn deref_mut(&mut self) -> &mut Self::Target { todo!() }\n\
         }",
    );

    let mut data_surfaces = Vec::new();
    public_data_surfaces_with_mutable_world(&file.items, &mut data_surfaces);
    assert_eq!(data_surfaces, ["Escape"]);

    let mut methods = Vec::new();
    public_inherent_methods(&file.items, "RuntimeDriverScope", &mut methods);
    assert!(methods.iter().any(|method| method == "insert_resource"));

    let mut conversions = Vec::new();
    forbidden_conversion_impls(&file.items, &mut conversions);
    assert_eq!(conversions, ["DerefMut for RuntimeDriverScope"]);
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
        let identifiers = rust_identifiers(&source);
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
    let functions = public_functions(
        "pub\nunsafe fn drive(runtime: & 'runtime mut nara_app::App) {}\n\
         pub(crate) fn internal(app: &mut App) {}",
    );
    assert_eq!(functions.len(), 1);
    assert!(signature_has_mutable_reference_to(
        &functions[0].signature,
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
