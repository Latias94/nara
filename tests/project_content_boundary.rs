#[cfg(all(feature = "serde", feature = "runtime-2d"))]
#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

#[cfg(all(feature = "serde", feature = "runtime-2d"))]
use nara::{
    asset::{AssetPath, AssetRef, AssetSourceKind, StableAssetId},
    project_host::{ProjectContentErrorKind, ProjectContentLoader},
    reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{
        PrefabDocument, PrefabInstance, SceneComponentRecord, SceneDocument, SceneEntityRecord,
        ScenePatchDocument,
    },
};

#[cfg(all(feature = "serde", feature = "runtime-2d"))]
use project_content_fixture::{
    TestProject, image_meta, player_image_meta, scene_id, sprite_value, valid_png_bytes,
};

#[cfg(all(feature = "serde", feature = "runtime-2d"))]
#[test]
fn stable_id_prefab_references_reject_without_asset_root_scanning() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let stable_id = StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap();
    let mut anchor = SceneEntityRecord::new(scene_id("enemy-anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::StableId(stable_id),
        overrides: ScenePatchDocument::default(),
    });
    let scene = SceneDocument::new([anchor]);
    std::fs::write(
        project.path().join("scenes/startup.scene.json"),
        scene.to_json_string().unwrap(),
    )
    .unwrap();
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(
        error.kind(),
        ProjectContentErrorKind::UnsupportedStableAssetReference
    );
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn settings_from_another_project_root_reject_before_content_access() {
    let source = TestProject::with_prefab_startup();
    let other = TestProject::with_prefab_startup();
    let (candidate, plan, _source_root) = source.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(other.root_capability()).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::ProjectRootMismatch);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn settings_and_runtime_plan_from_different_lineages_reject_before_content_access() {
    let project = TestProject::with_prefab_startup();
    let other = TestProject::with_prefab_startup();
    let (candidate, _plan, root) = project.candidate_plan_and_root();
    let (_other_candidate, other_plan, _other_root) = other.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &other_plan).unwrap_err();

    assert_eq!(
        error.kind(),
        ProjectContentErrorKind::ProjectLineageMismatch
    );
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn stable_id_component_asset_references_reject_without_catalogue_scanning() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let stable_id = StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap();
    let enemy = SceneEntityRecord::new(scene_id("enemy")).with_component(
        ComponentTypeId::new("nara.sprite.Sprite"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            sprite_value(&AssetRef::StableId(stable_id)),
        ),
    );
    project.write_prefab_source(&PrefabDocument::new([enemy]));
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(
        error.kind(),
        ProjectContentErrorKind::UnsupportedStableAssetReference
    );
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn escaping_component_asset_paths_reject_before_source_access() {
    let project = TestProject::with_prefab_startup();
    let prefab_path = project.path().join("prefabs/enemy.prefab.json");
    let mut encoded =
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(prefab_path).unwrap()).unwrap();
    replace_json_string(&mut encoded, "textures/player.png", "../outside.png");
    project.write_prefab_bytes("enemy.prefab.json", &serde_json::to_vec(&encoded).unwrap());
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::AssetReference);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn document_catalog_mismatch_rejects_without_partial_publication() {
    let project = TestProject::with_prefab_startup();
    let unknown = SceneEntityRecord::new(scene_id("unknown")).with_component(
        ComponentTypeId::new("nara.test.MissingFromFrozenCatalog"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
    );
    project.write_scene_source(&SceneDocument::new([unknown]));
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::ScenePublication);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn image_metadata_must_match_the_referenced_path_and_source_kind() {
    let path_mismatch = TestProject::with_prefab_startup();
    let (candidate, plan, root) = path_mismatch.candidate_plan_and_root();
    let mut meta = player_image_meta();
    meta.path = AssetPath::new("textures/other.png").unwrap();
    path_mismatch.write_player_image_meta(&meta);
    let loader = ProjectContentLoader::new(root).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::AssetMetaMismatch);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);

    let kind_mismatch = TestProject::with_prefab_startup();
    let (candidate, plan, root) = kind_mismatch.candidate_plan_and_root();
    let mut meta = player_image_meta();
    meta.source_kind = AssetSourceKind::Other("binary".to_owned());
    kind_mismatch.write_player_image_meta(&meta);
    let loader = ProjectContentLoader::new(root).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::UnsupportedAssetKind);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn duplicate_stable_asset_ids_reject_the_complete_closure_atomically() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let duplicate = image_meta("3c7c5be4-fd4e-4b65-b8d4-c671f5982186", "textures/other.png");
    project.write_asset("textures/other.png", valid_png_bytes(), &duplicate);
    let player = AssetRef::path("textures/player.png").unwrap();
    let other = AssetRef::path("textures/other.png").unwrap();
    project.write_prefab_source(&PrefabDocument::new([
        SceneEntityRecord::new(scene_id("enemy-player")).with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, sprite_value(&player)),
        ),
        SceneEntityRecord::new(scene_id("enemy-other")).with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, sprite_value(&other)),
        ),
    ]));
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::AssetMetaMismatch);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn cyclic_prefab_closure_rejects_without_requeue_or_partial_publication() {
    let project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let mut cyclic = SceneEntityRecord::new(scene_id("cycle"));
    cyclic.prefab = Some(PrefabInstance {
        source: AssetRef::path("enemy.prefab.json").unwrap(),
        overrides: ScenePatchDocument::default(),
    });
    project.write_prefab_source(&PrefabDocument::new([cyclic]));
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::PrefabExpansion);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn hostile_scene_and_image_inputs_publish_no_snapshot_or_residency() {
    let hostile_scene = TestProject::with_prefab_startup();
    let (candidate, plan, root) = hostile_scene.candidate_plan_and_root();
    hostile_scene.write_scene_bytes(br#"{"kind":"scene","format_version":1,"payload":[]}"#);
    let loader = ProjectContentLoader::new(root).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::SceneFormat);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);

    let hostile_image = TestProject::with_prefab_startup();
    let (candidate, plan, root) = hostile_image.candidate_plan_and_root();
    hostile_image.write_asset("textures/player.png", b"not a png", &player_image_meta());
    let loader = ProjectContentLoader::new(root).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::ImageImport);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn strict_content_loader_rejects_a_multi_link_source_before_publication() {
    let project = TestProject::with_prefab_startup();
    std::fs::hard_link(
        project.path().join("scenes/startup.scene.json"),
        project.path().join("scenes/startup.alias.json"),
    )
    .unwrap();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::HostAuthorityRejected);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[cfg(target_os = "linux")]
#[test]
fn strict_content_loader_rejects_a_symlinked_source_before_publication() {
    use std::os::unix::fs::symlink;

    let project = TestProject::with_prefab_startup();
    std::fs::rename(
        project.path().join("scenes/startup.scene.json"),
        project.path().join("scenes/startup.target.json"),
    )
    .unwrap();
    symlink(
        "startup.target.json",
        project.path().join("scenes/startup.scene.json"),
    )
    .unwrap();
    let (candidate, plan, root) = project.candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();

    let error = loader.load(&candidate, &plan).unwrap_err();

    assert_eq!(error.kind(), ProjectContentErrorKind::HostAuthorityRejected);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn parented_transform_loads_while_inherited_visibility_remains_fail_closed() {
    let transform_project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = transform_project.candidate_plan_and_root();
    let transform_record = |x: f64, y: f64| {
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([
                (
                    "translation",
                    ComponentValue::map([
                        ("x", ComponentValue::f64(x).unwrap()),
                        ("y", ComponentValue::f64(y).unwrap()),
                    ]),
                ),
                ("rotation", ComponentValue::f64(0.0).unwrap()),
                (
                    "scale",
                    ComponentValue::map([
                        ("x", ComponentValue::f64(1.0).unwrap()),
                        ("y", ComponentValue::f64(1.0).unwrap()),
                    ]),
                ),
            ]),
        )
    };
    let transform_id = ComponentTypeId::new("nara.transform.Transform2d");
    let parent = SceneEntityRecord::new(scene_id("parent"))
        .with_component(transform_id.clone(), transform_record(10.0, 20.0));
    let child = SceneEntityRecord::new(scene_id("child"))
        .with_parent(scene_id("parent"))
        .with_component(transform_id.clone(), transform_record(3.0, 4.0));
    transform_project.write_scene_source(&SceneDocument::new([parent, child]));
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let expanded = snapshot.expanded_startup_scene();
    let child = expanded
        .entities
        .iter()
        .find(|entity| entity.id == scene_id("child"))
        .unwrap();
    assert_eq!(child.parent.as_ref(), Some(&scene_id("parent")));
    assert!(child.components.contains_key(&transform_id));
    drop(snapshot);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);

    let visibility_project = TestProject::with_prefab_startup();
    let (candidate, plan, root) = visibility_project.candidate_plan_and_root();
    let parent = SceneEntityRecord::new(scene_id("parent")).with_component(
        ComponentTypeId::new("nara.scene.Visibility"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String("hidden".to_owned()),
        ),
    );
    let child = SceneEntityRecord::new(scene_id("child")).with_parent(scene_id("parent"));
    visibility_project.write_scene_source(&SceneDocument::new([parent, child]));
    let loader = ProjectContentLoader::new(root).unwrap();
    let error = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectContentErrorKind::UnsupportedHierarchySemantics
    );
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn project_content_source_has_no_ambient_or_runtime_authority() {
    assert_project_content_module_uses_conventional_path();

    let sources = project_content_source_files()
        .into_iter()
        .map(|path| {
            let source = std::fs::read_to_string(&path).unwrap();
            let syntax = syn::parse_file(&source).unwrap();
            (path, source, syntax)
        })
        .collect::<Vec<_>>();

    for (path, _, syntax) in &sources {
        assert_project_content_syntax_boundary(path, syntax);
    }

    let source = sources
        .iter()
        .map(|(_, source, _)| source.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "PathBuf",
        "read_dir(",
        "RuntimeInstance",
        "RuntimeCandidate",
        "ComponentSchemaProviderReceipt",
        "ComponentSchemaProviderBindingId",
        "RuntimeFaultReporter",
        "OnceLock",
        "nara_app::App",
        "bevy_ecs::World",
        "AssetServer",
        "AssetStates",
        "Assets::<ImageAsset>",
        "AssetVersion",
        "wgpu::",
        "winit::",
    ] {
        assert!(
            !source.contains(forbidden),
            "project content boundary contains forbidden token {forbidden:?}"
        );
    }

    assert_snapshot_field_allowlist(&sources);
    assert_snapshot_public_api_allowlist(&sources);
}

#[test]
fn owner_lineage_slice_does_not_publish_unavailable_schema_readiness() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let forbidden_exports = [
        "AuthoringSchemaCatalog",
        "KnownUnbound",
        "OptionalNativeBinding",
        "PlaceholderComponent",
        "UnavailableSchema",
        "UnknownSchema",
    ];
    for relative in ["crates/nara_reflect/src/lib.rs", "src/lib.rs"] {
        let path = root.join(relative);
        let source = std::fs::read_to_string(&path).unwrap();
        let syntax = syn::parse_file(&source).unwrap();
        let mut public_imports = Vec::new();
        for item in &syntax.items {
            let syn::Item::Use(item) = item else {
                continue;
            };
            if !matches!(item.vis, syn::Visibility::Public(_)) {
                continue;
            }
            flatten_use_tree(&item.tree, &mut Vec::new(), &mut public_imports);
        }
        for forbidden in forbidden_exports {
            assert!(
                public_imports
                    .iter()
                    .all(|import| import.binding() != Some(forbidden)),
                "{} publicly exports unavailable-schema type {forbidden}",
                path.display()
            );
        }
    }

    let registry_path = root.join("crates/nara_reflect/src/registry.rs");
    let registry_source = std::fs::read_to_string(&registry_path).unwrap();
    let registry = syn::parse_file(&registry_source).unwrap();
    let registry_data = registry
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Struct(item) if item.ident == "RegistryData" => Some(item),
            _ => None,
        })
        .expect("RegistryData remains the concrete Runtime registry storage");
    let bindings = registry_data
        .fields
        .iter()
        .find(|field| {
            field
                .ident
                .as_ref()
                .is_some_and(|ident| ident == "bindings")
        })
        .expect("RegistryData retains native binding storage");
    assert_eq!(
        type_shape(&bindings.ty),
        "BTreeMap<ComponentTypeId,NativeComponentBinding>",
        "Runtime bindings must remain concrete and complete rather than optional readiness wrappers"
    );

    let reflect_manifest_path = root.join("crates/nara_reflect/Cargo.toml");
    let reflect_manifest_source = std::fs::read_to_string(&reflect_manifest_path).unwrap();
    let reflect_manifest = toml::from_str::<toml::Value>(&reflect_manifest_source).unwrap();
    let reflect_dependencies = reflect_manifest["dependencies"]
        .as_table()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        reflect_dependencies,
        [
            "bevy_ecs",
            "bevy_reflect",
            "blake3",
            "nara_app",
            "nara_asset",
            "nara_core",
            "nara_ecs",
            "nara_identity",
            "nara_reflect_derive",
            "ron",
            "serde",
            "serde_json",
        ]
        .into_iter()
        .collect(),
        "owner lineage must not grow package, VM, or unavailable-authoring dependencies"
    );

    let root_manifest_source = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let root_manifest = toml::from_str::<toml::Value>(&root_manifest_source).unwrap();
    let dependency_tables = [
        root_manifest.get("dependencies"),
        root_manifest
            .get("workspace")
            .and_then(|workspace| workspace.get("dependencies")),
        reflect_manifest.get("dependencies"),
    ];
    let forbidden_dependencies = [
        "extism",
        "mlua",
        "nara_authoring_schema",
        "nara_package",
        "nara_script",
        "netcorehost",
        "rhai",
        "wasmer",
        "wasmtime",
    ];
    for table in dependency_tables.into_iter().flatten() {
        let table = table.as_table().unwrap();
        for forbidden in forbidden_dependencies {
            assert!(
                !table.contains_key(forbidden),
                "owner lineage unexpectedly admits dependency {forbidden}"
            );
        }
    }
}

#[test]
fn project_content_paths_are_charged_before_relative_path_allocation() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/project_content.rs"),
    )
    .unwrap();
    let start = source.find("fn observe_path(").unwrap();
    let tail = &source[start..];
    let end = tail.find("\n    fn observe_depth(").unwrap();
    let body = &tail[..end];
    let preflight = body.find("RelativePath::preflight").unwrap();
    let entries = body
        .find("ProjectContentBudgetKind::DirectoryEntries")
        .unwrap();
    let bytes = body.find("ProjectContentBudgetKind::PathBytes").unwrap();
    let allocation = body.find("RelativePath::new").unwrap();

    assert!(preflight < entries);
    assert!(entries < bytes);
    assert!(bytes < allocation);
}

#[test]
fn project_content_image_payload_cannot_escape_its_snapshot_lease_by_clone() {
    static_assertions::assert_not_impl_any!(nara::image::ImageAsset: Clone);
}

#[test]
fn project_content_ast_guard_rejects_alias_and_macro_indirection() {
    let hostile_sources = [
        r#"
            extern crate std as system;
            fn read() { let _ = system::fs::read("hidden"); }
        "#,
        r#"
            use std::include_str as hidden;
            fn read() { let _ = hidden!("hidden.rs"); }
        "#,
        r#"
            macro_rules! hidden {
                () => { let _ = std::fs::read("hidden"); };
            }
            fn read() { hidden!(); }
        "#,
        r#"
            fn read() { matches!(std::fs::read("hidden"), Ok(_)); }
        "#,
        r#"
            mod hidden { pub struct Box<T: ?Sized> { ptr: *const T } }
            use self::hidden::*;
            struct Snapshot { value: Box<[u8]> }
        "#,
        r#"
            use external::ambient as matches;
            fn read() { matches!(); }
        "#,
    ];

    for source in hostile_sources {
        let syntax = syn::parse_file(source).unwrap();
        let rejected = std::panic::catch_unwind(|| {
            assert_project_content_syntax_boundary(std::path::Path::new("hostile.rs"), &syntax);
        });
        assert!(rejected.is_err(), "AST boundary accepted {source}");
    }
}

#[test]
fn snapshot_type_identity_guard_rejects_renamed_authority() {
    let syntax = syn::parse_file(
        r#"
            use std::sync::Arc;
            use nara_fs::ContentDigest;
            use nara_reflect::SchemaCompositionFingerprint;
            use nara_scene::SceneDocument;
            use crate::project_host::Owner as ProjectSettingsLineage;
            use budget::ProjectContentLease;
        "#,
    )
    .unwrap();

    let rejected = std::panic::catch_unwind(|| {
        assert_snapshot_import_identities(std::path::Path::new("hostile.rs"), &syntax);
    });
    assert!(rejected.is_err());
}

#[test]
fn snapshot_method_allowlist_rejects_unreviewed_surface() {
    let syntax = syn::parse_file(
        r#"
            pub struct ProjectContentSnapshot;
            impl ProjectContentSnapshot {
                pub fn startup_scene(&self) -> &SceneDocument { todo!() }
                pub fn authority(&self) -> ComponentRegistrySnapshotWitness { todo!() }
            }
        "#,
    )
    .unwrap();

    let rejected = std::panic::catch_unwind(|| {
        assert_snapshot_inherent_method_allowlist(
            std::path::Path::new("hostile.rs"),
            &syntax.items,
            &[("startup_scene", "&SceneDocument")],
        );
    });

    assert!(rejected.is_err());
}

#[test]
fn snapshot_api_guard_allows_shared_world_independent_content() {
    let syntax = syn::parse_file(
        r#"
            pub struct ProjectContentSnapshot;
            impl ProjectContentSnapshot {
                pub fn startup_scene(&self) -> Arc<SceneDocument> { todo!() }
            }
        "#,
    )
    .unwrap();

    assert_snapshot_inherent_method_allowlist(
        std::path::Path::new("allowed.rs"),
        &syntax.items,
        &[("startup_scene", "Arc<SceneDocument>")],
    );
}

#[derive(Debug)]
struct UseImport {
    path: Vec<String>,
    alias: Option<String>,
    is_glob: bool,
}

impl UseImport {
    fn binding(&self) -> Option<&str> {
        if self.is_glob {
            None
        } else {
            self.alias
                .as_deref()
                .or_else(|| self.path.last().map(String::as_str))
        }
    }
}

struct ProjectContentBoundaryVisitor<'a> {
    source_path: &'a std::path::Path,
    std_roots: std::collections::BTreeSet<String>,
    imports: Vec<UseImport>,
}

const ALLOWED_BUILTIN_MACROS: [&str; 5] = [
    "assert_eq",
    "debug_assert",
    "debug_assert_eq",
    "matches",
    "write",
];

fn is_reserved_macro_binding(binding: &str) -> bool {
    ALLOWED_BUILTIN_MACROS.contains(&binding) || binding == "define_project_content_budget_kinds"
}

impl ProjectContentBoundaryVisitor<'_> {
    fn assert_macro_tokens(&self, name: &str, tokens: proc_macro2::TokenStream) {
        let mut identifiers = Vec::new();
        collect_token_identifiers(tokens, &mut identifiers);
        for forbidden in ["fs", "path", "include", "include_str", "include_bytes"] {
            assert!(
                !identifiers.iter().any(|identifier| identifier == forbidden),
                "{} hides forbidden identifier {forbidden} inside macro {name}",
                self.source_path.display()
            );
        }
        for root in &self.std_roots {
            assert!(
                !identifiers.iter().any(|identifier| identifier == root),
                "{} hides ambient std root {root} inside macro {name}",
                self.source_path.display()
            );
        }
    }
}

impl<'ast> syn::visit::Visit<'ast> for ProjectContentBoundaryVisitor<'_> {
    fn visit_file(&mut self, file: &'ast syn::File) {
        for import in &self.imports {
            assert!(
                !import.is_glob,
                "{} uses a glob import outside the inspected authority surface: {:?}",
                self.source_path.display(),
                import
            );
            if let Some(binding) = import.binding() {
                assert!(
                    !is_reserved_macro_binding(binding),
                    "{} imports over reserved macro binding {binding}",
                    self.source_path.display()
                );
            }
            assert!(
                !uses_forbidden_import(&import.path, &self.std_roots),
                "{} imports forbidden authority through {:?}",
                self.source_path.display(),
                import
            );
        }
        syn::visit::visit_file(self, file);
    }

    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        panic!(
            "{} declares extern crate {} outside the inspected import surface",
            self.source_path.display(),
            item.ident
        );
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        assert_no_module_path_override(self.source_path, item);
        syn::visit::visit_item_mod(self, item);
    }

    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if item.mac.path.is_ident("macro_rules") {
            let name = item.ident.as_ref().map(identifier_text).unwrap_or_default();
            assert_eq!(
                name,
                "define_project_content_budget_kinds",
                "{} defines non-allowlisted macro {name}",
                self.source_path.display()
            );
            self.assert_macro_tokens(&name, item.mac.tokens.clone());
            return;
        }
        syn::visit::visit_item_macro(self, item);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let name = mac
            .path
            .segments
            .last()
            .map(|segment| identifier_text(&segment.ident))
            .unwrap_or_default();
        let is_budget_kind_invocation = name == "define_project_content_budget_kinds";
        assert!(
            ALLOWED_BUILTIN_MACROS.contains(&name.as_str()) || is_budget_kind_invocation,
            "{} uses non-allowlisted macro {name}",
            self.source_path.display()
        );
        self.assert_macro_tokens(&name, mac.tokens.clone());
        syn::visit::visit_macro(self, mac);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        assert!(
            !uses_forbidden_import(&segments, &self.std_roots),
            "{} references an ambient std module through {}",
            self.source_path.display(),
            segments.join("::")
        );
        syn::visit::visit_path(self, path);
    }
}

fn assert_project_content_syntax_boundary(source_path: &std::path::Path, syntax: &syn::File) {
    let imports = collect_use_imports(syntax);
    let mut visitor = ProjectContentBoundaryVisitor {
        source_path,
        std_roots: collect_std_roots(&imports),
        imports,
    };
    syn::visit::Visit::visit_file(&mut visitor, syntax);
}

fn collect_use_imports(syntax: &syn::File) -> Vec<UseImport> {
    struct Collector {
        imports: Vec<UseImport>,
    }

    impl<'ast> syn::visit::Visit<'ast> for Collector {
        fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
            flatten_use_tree(&item.tree, &mut Vec::new(), &mut self.imports);
        }
    }

    let mut collector = Collector {
        imports: Vec::new(),
    };
    syn::visit::Visit::visit_file(&mut collector, syntax);
    collector.imports
}

fn flatten_use_tree(tree: &syn::UseTree, prefix: &mut Vec<String>, imports: &mut Vec<UseImport>) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(identifier_text(&path.ident));
            flatten_use_tree(&path.tree, prefix, imports);
            prefix.pop();
        }
        syn::UseTree::Name(name) => {
            let mut path = prefix.clone();
            path.push(identifier_text(&name.ident));
            normalize_self_import(&mut path);
            imports.push(UseImport {
                path,
                alias: None,
                is_glob: false,
            });
        }
        syn::UseTree::Rename(rename) => {
            let mut path = prefix.clone();
            path.push(identifier_text(&rename.ident));
            normalize_self_import(&mut path);
            imports.push(UseImport {
                path,
                alias: Some(identifier_text(&rename.rename)),
                is_glob: false,
            });
        }
        syn::UseTree::Glob(_) => imports.push(UseImport {
            path: prefix.clone(),
            alias: None,
            is_glob: true,
        }),
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                flatten_use_tree(tree, prefix, imports);
            }
        }
    }
}

fn normalize_self_import(path: &mut Vec<String>) {
    if path.last().is_some_and(|segment| segment == "self") {
        path.pop();
    }
}

fn collect_std_roots(imports: &[UseImport]) -> std::collections::BTreeSet<String> {
    let mut roots = std::collections::BTreeSet::from(["std".to_owned()]);
    loop {
        let before = roots.len();
        for import in imports {
            if import.path.len() == 1
                && roots.contains(&import.path[0])
                && let Some(alias) = &import.alias
            {
                roots.insert(alias.clone());
            }
        }
        if roots.len() == before {
            return roots;
        }
    }
}

fn uses_forbidden_import(path: &[String], std_roots: &std::collections::BTreeSet<String>) -> bool {
    let imports_hidden_macro = path.last().is_some_and(|segment| {
        matches!(
            segment.as_str(),
            "include" | "include_str" | "include_bytes"
        )
    });
    imports_hidden_macro
        || (path.len() >= 2
            && std_roots.contains(&path[0])
            && matches!(path[1].as_str(), "fs" | "path"))
}

fn collect_token_identifiers(tokens: proc_macro2::TokenStream, identifiers: &mut Vec<String>) {
    for token in tokens {
        match token {
            proc_macro2::TokenTree::Group(group) => {
                collect_token_identifiers(group.stream(), identifiers);
            }
            proc_macro2::TokenTree::Ident(identifier) => {
                identifiers.push(identifier_text(&identifier));
            }
            proc_macro2::TokenTree::Punct(_) | proc_macro2::TokenTree::Literal(_) => {}
        }
    }
}

fn identifier_text(identifier: &proc_macro2::Ident) -> String {
    let identifier = identifier.to_string();
    identifier
        .strip_prefix("r#")
        .unwrap_or(&identifier)
        .to_owned()
}

fn assert_project_content_module_uses_conventional_path() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_path = manifest.join("src/lib.rs");
    let source = std::fs::read_to_string(&source_path).unwrap();
    let syntax = syn::parse_file(&source).unwrap();
    let modules = syntax
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Mod(item) if item.ident == "project_content" => Some(item),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        modules.len(),
        1,
        "root must declare one project_content module"
    );
    assert!(
        modules[0].content.is_none(),
        "project_content must use its conventional external module file"
    );
    assert_no_module_path_override(&source_path, modules[0]);
}

fn assert_no_module_path_override(source_path: &std::path::Path, item: &syn::ItemMod) {
    for attribute in &item.attrs {
        assert!(
            !attribute.path().is_ident("path"),
            "{} redirects module {} with #[path]",
            source_path.display(),
            item.ident
        );
        if attribute.path().is_ident("cfg_attr") {
            let nested = attribute
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{} has an invalid cfg_attr on module {}: {error}",
                        source_path.display(),
                        item.ident
                    )
                });
            assert!(
                !nested.iter().skip(1).any(meta_contains_path_override),
                "{} redirects module {} through cfg_attr(path)",
                source_path.display(),
                item.ident
            );
        }
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
    list.parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
        .is_ok_and(|nested| nested.iter().skip(1).any(meta_contains_path_override))
}

fn assert_snapshot_field_allowlist(sources: &[(std::path::PathBuf, String, syn::File)]) {
    const EXPECTED_FIELDS: [(&str, &str); 11] = [
        ("lineage", "ProjectSettingsLineage"),
        ("schema_fingerprint", "SchemaCompositionFingerprint"),
        ("schema_authority", "ComponentRegistrySnapshotWitness"),
        ("revision", "ProjectContentRevision"),
        ("content_digest", "ContentDigest"),
        ("source_upgrade_required", "bool"),
        ("startup_scene", "Arc<SceneDocument>"),
        ("expanded_startup_scene", "Arc<SceneDocument>"),
        ("prefabs", "Box<[ProjectPrefabContent]>"),
        ("images", "Box<[ProjectImageContent]>"),
        ("_lease", "ProjectContentLease"),
    ];

    let snapshots = sources
        .iter()
        .flat_map(|(path, _, syntax)| {
            syntax.items.iter().filter_map(move |item| match item {
                syn::Item::Struct(item) if item.ident == "ProjectContentSnapshotInner" => {
                    Some((path, item))
                }
                _ => None,
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        snapshots.len(),
        1,
        "snapshot storage must have one definition"
    );

    let (path, snapshot) = snapshots[0];
    let syntax = sources
        .iter()
        .find_map(|(source_path, _, syntax)| (source_path == path).then_some(syntax))
        .unwrap();
    assert_snapshot_import_identities(path, syntax);
    let fields = snapshot.fields.iter().collect::<Vec<_>>();
    assert_eq!(
        fields.len(),
        EXPECTED_FIELDS.len(),
        "{} changed the snapshot authority field set",
        path.display()
    );
    for (field, (expected_name, expected_type)) in fields.into_iter().zip(EXPECTED_FIELDS) {
        let name = field.ident.as_ref().map(ToString::to_string);
        assert_eq!(name.as_deref(), Some(expected_name));
        assert_eq!(
            type_shape(&field.ty),
            expected_type,
            "snapshot field {expected_name} changed authority type"
        );
    }
}

fn assert_snapshot_public_api_allowlist(sources: &[(std::path::PathBuf, String, syn::File)]) {
    const EXPECTED_METHODS: [(&str, &str); 9] = [
        ("lineage", "ProjectSettingsLineage"),
        ("schema_fingerprint", "SchemaCompositionFingerprint"),
        ("revision", "ProjectContentRevision"),
        ("content_digest", "ContentDigest"),
        ("source_upgrade_required", "bool"),
        ("startup_scene", "&SceneDocument"),
        ("expanded_startup_scene", "&SceneDocument"),
        ("prefabs", "&[ProjectPrefabContent]"),
        ("images", "&[ProjectImageContent]"),
    ];

    let public_snapshots = sources
        .iter()
        .flat_map(|(path, _, syntax)| {
            syntax.items.iter().filter_map(move |item| match item {
                syn::Item::Struct(item) if item.ident == "ProjectContentSnapshot" => {
                    Some((path, item))
                }
                _ => None,
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        public_snapshots.len(),
        1,
        "ProjectContentSnapshot must have one public wrapper definition"
    );
    let (snapshot_path, snapshot) = public_snapshots[0];
    assert!(matches!(snapshot.vis, syn::Visibility::Public(_)));
    let fields = snapshot.fields.iter().collect::<Vec<_>>();
    assert_eq!(fields.len(), 1, "snapshot wrapper changed field count");
    assert_eq!(
        fields[0].ident.as_ref().map(ToString::to_string).as_deref(),
        Some("inner")
    );
    assert!(matches!(fields[0].vis, syn::Visibility::Inherited));
    assert_eq!(
        type_shape(&fields[0].ty),
        "Arc<ProjectContentSnapshotInner>"
    );

    let derives = snapshot
        .attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("derive"))
        .flat_map(|attribute| {
            attribute
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
                )
                .unwrap()
        })
        .map(|path| path_shape(&path))
        .collect::<Vec<_>>();
    assert_eq!(derives, ["Clone"]);

    let snapshot_syntax = sources
        .iter()
        .find_map(|(path, _, syntax)| (path == snapshot_path).then_some(syntax))
        .unwrap();
    assert_snapshot_inherent_method_allowlist(
        snapshot_path,
        &snapshot_syntax.items,
        &EXPECTED_METHODS,
    );
    let mut snapshot_trait_impls = Vec::new();
    let mut loader_load_count = 0;
    for (_, _, syntax) in sources {
        for item in flattened_items(&syntax.items) {
            let syn::Item::Impl(item_impl) = item else {
                continue;
            };
            let self_shape = type_shape(&item_impl.self_ty);
            if self_shape == "ProjectContentSnapshot" {
                if let Some((_, trait_path, _)) = &item_impl.trait_ {
                    snapshot_trait_impls.push(path_shape(trait_path));
                }
            } else if self_shape == "ProjectContentLoader" && item_impl.trait_.is_none() {
                for impl_item in &item_impl.items {
                    if let syn::ImplItem::Fn(method) = impl_item
                        && method.sig.ident == "load"
                    {
                        assert_project_content_loader_load(method);
                        loader_load_count += 1;
                    }
                }
            }
        }
    }

    snapshot_trait_impls.sort();
    assert_eq!(snapshot_trait_impls, ["fmt::Debug"]);
    assert_eq!(
        loader_load_count, 1,
        "ProjectContentLoader::load must have one reviewed definition"
    );
    assert!(
        snapshot_path.ends_with("project_content.rs"),
        "ProjectContentSnapshot moved out of its owning module"
    );
}

fn assert_snapshot_inherent_method_allowlist(
    source_path: &std::path::Path,
    items: &[syn::Item],
    expected: &[(&str, &str)],
) {
    let mut actual = std::collections::BTreeMap::new();
    for item in flattened_items(items) {
        let syn::Item::Impl(item_impl) = item else {
            continue;
        };
        if item_impl.trait_.is_some() || type_shape(&item_impl.self_ty) != "ProjectContentSnapshot"
        {
            continue;
        }
        for item in &item_impl.items {
            let syn::ImplItem::Fn(method) = item else {
                continue;
            };
            if !matches!(method.vis, syn::Visibility::Public(_)) {
                continue;
            }
            assert_shared_receiver_only(source_path, &method.sig);
            let return_shape = match &method.sig.output {
                syn::ReturnType::Type(_, ty) => type_shape(ty),
                syn::ReturnType::Default => "()".to_owned(),
            };
            assert!(
                actual
                    .insert(method.sig.ident.to_string(), return_shape)
                    .is_none(),
                "{} duplicates public ProjectContentSnapshot method {}",
                source_path.display(),
                method.sig.ident
            );
        }
    }
    let expected = expected
        .iter()
        .map(|(name, output)| ((*name).to_owned(), (*output).to_owned()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        actual,
        expected,
        "{} changed the reviewed ProjectContentSnapshot SDK",
        source_path.display()
    );
}

fn assert_shared_receiver_only(source_path: &std::path::Path, signature: &syn::Signature) {
    assert!(signature.constness.is_none());
    assert!(signature.asyncness.is_none());
    assert!(signature.unsafety.is_none());
    assert!(signature.abi.is_none());
    assert!(signature.generics.params.is_empty());
    assert!(signature.generics.where_clause.is_none());
    assert!(signature.variadic.is_none());
    let inputs = signature.inputs.iter().collect::<Vec<_>>();
    assert_eq!(
        inputs.len(),
        1,
        "{} changed parameters for snapshot method {}",
        source_path.display(),
        signature.ident
    );
    let syn::FnArg::Receiver(receiver) = inputs[0] else {
        panic!("snapshot methods must use a shared receiver");
    };
    assert!(receiver.reference.is_some());
    assert!(receiver.mutability.is_none());
    assert!(receiver.colon_token.is_none());
}

fn assert_project_content_loader_load(method: &syn::ImplItemFn) {
    assert!(method.sig.constness.is_none());
    assert!(method.sig.asyncness.is_none());
    assert!(method.sig.unsafety.is_none());
    assert!(method.sig.abi.is_none());
    assert!(method.sig.generics.params.is_empty());
    assert!(method.sig.generics.where_clause.is_none());
    assert!(method.sig.variadic.is_none());
    let inputs = method.sig.inputs.iter().collect::<Vec<_>>();
    assert_eq!(inputs.len(), 3);
    assert!(matches!(
        inputs[0],
        syn::FnArg::Receiver(receiver)
            if receiver.reference.is_some()
                && receiver.mutability.is_none()
                && receiver.colon_token.is_none()
    ));
    let argument_types = inputs[1..]
        .iter()
        .map(|input| match input {
            syn::FnArg::Typed(input) => type_shape(&input.ty),
            syn::FnArg::Receiver(_) => "<unexpected-receiver>".to_owned(),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        argument_types,
        ["&ProjectSettingsCandidate", "&RuntimePlan"]
    );
    let syn::ReturnType::Type(_, output) = &method.sig.output else {
        panic!("ProjectContentLoader::load must return the bounded snapshot result");
    };
    assert_eq!(
        type_shape(output),
        "Result<ProjectContentSnapshot,ProjectContentError>"
    );
}

fn flattened_items(items: &[syn::Item]) -> Vec<&syn::Item> {
    fn collect<'a>(items: &'a [syn::Item], output: &mut Vec<&'a syn::Item>) {
        for item in items {
            output.push(item);
            if let syn::Item::Mod(item_mod) = item
                && let Some((_, nested)) = &item_mod.content
            {
                collect(nested, output);
            }
        }
    }

    let mut output = Vec::new();
    collect(items, &mut output);
    output
}

fn path_shape(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| identifier_text(&segment.ident))
        .collect::<Vec<_>>()
        .join("::")
}

fn assert_snapshot_import_identities(source_path: &std::path::Path, syntax: &syn::File) {
    let expected_imports: [(&str, &[&str]); 8] = [
        ("Arc", &["std", "sync", "Arc"]),
        ("ContentDigest", &["nara_fs", "ContentDigest"]),
        (
            "ComponentRegistrySnapshot",
            &["nara_reflect", "ComponentRegistrySnapshot"],
        ),
        (
            "ComponentRegistrySnapshotWitness",
            &[
                "nara_reflect",
                "__private",
                "ComponentRegistrySnapshotWitness",
            ],
        ),
        (
            "SchemaCompositionFingerprint",
            &["nara_reflect", "SchemaCompositionFingerprint"],
        ),
        ("SceneDocument", &["nara_scene", "SceneDocument"]),
        (
            "ProjectSettingsLineage",
            &["crate", "project_host", "ProjectSettingsLineage"],
        ),
        ("ProjectContentLease", &["budget", "ProjectContentLease"]),
    ];
    let imports = collect_use_imports(syntax);
    for (binding, expected_path) in expected_imports {
        let matches = imports
            .iter()
            .filter(|import| import.binding() == Some(binding))
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            1,
            "{} must bind {binding} exactly once",
            source_path.display()
        );
        assert_eq!(
            matches[0]
                .path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            expected_path,
            "{} binds snapshot type {binding} from the wrong authority",
            source_path.display()
        );
        assert!(
            matches[0].alias.is_none(),
            "{} renames canonical snapshot type {binding}",
            source_path.display()
        );
    }

    for local_type in [
        "ProjectContentRevision",
        "ProjectPrefabContent",
        "ProjectImageContent",
    ] {
        let definitions = syntax
            .items
            .iter()
            .filter(|item| matches!(item, syn::Item::Struct(item) if item.ident == local_type))
            .count();
        assert_eq!(
            definitions,
            1,
            "{} must define snapshot type {local_type} as one local struct",
            source_path.display()
        );
    }

    assert!(
        !imports.iter().any(|import| import.binding() == Some("Box")),
        "{} shadows the prelude Box type",
        source_path.display()
    );
    assert!(
        !syntax.items.iter().any(|item| {
            matches!(item, syn::Item::Struct(item) if item.ident == "Box")
                || matches!(item, syn::Item::Enum(item) if item.ident == "Box")
                || matches!(item, syn::Item::Type(item) if item.ident == "Box")
                || matches!(item, syn::Item::Union(item) if item.ident == "Box")
        }),
        "{} shadows the prelude Box type",
        source_path.display()
    );
}

fn type_shape(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) if path.qself.is_none() => path
            .path
            .segments
            .iter()
            .map(|segment| {
                let mut shape = segment.ident.to_string();
                match &segment.arguments {
                    syn::PathArguments::None => {}
                    syn::PathArguments::AngleBracketed(arguments) => {
                        let arguments = arguments
                            .args
                            .iter()
                            .map(|argument| match argument {
                                syn::GenericArgument::Type(ty) => type_shape(ty),
                                _ => "<unsupported-argument>".to_owned(),
                            })
                            .collect::<Vec<_>>()
                            .join(",");
                        shape.push('<');
                        shape.push_str(&arguments);
                        shape.push('>');
                    }
                    syn::PathArguments::Parenthesized(_) => {
                        shape.push_str("<unsupported-parenthesized-arguments>");
                    }
                }
                shape
            })
            .collect::<Vec<_>>()
            .join("::"),
        syn::Type::Slice(slice) => format!("[{}]", type_shape(&slice.elem)),
        syn::Type::Reference(reference) if reference.mutability.is_none() => {
            format!("&{}", type_shape(&reference.elem))
        }
        _ => "<unsupported-type>".to_owned(),
    }
}

fn project_content_source_files() -> Vec<std::path::PathBuf> {
    let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut paths = vec![source_root.join("project_content.rs")];
    collect_rust_source_files(&source_root.join("project_content"), &mut paths);
    paths.sort();
    assert!(
        paths.len() >= 2,
        "project content source scan is incomplete"
    );
    paths
}

fn collect_rust_source_files(directory: &std::path::Path, paths: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let file_type = entry.file_type().unwrap();
        assert!(
            !file_type.is_symlink(),
            "project content source modules must not hide behind symlinks"
        );
        let path = entry.path();
        if file_type.is_dir() {
            collect_rust_source_files(&path, paths);
        } else if file_type.is_file() && path.extension().is_some_and(|value| value == "rs") {
            paths.push(path);
        }
    }
}

fn replace_json_string(value: &mut serde_json::Value, expected: &str, replacement: &str) {
    match value {
        serde_json::Value::String(current) if current == expected => {
            *current = replacement.to_owned();
        }
        serde_json::Value::Array(values) => {
            for value in values {
                replace_json_string(value, expected, replacement);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                replace_json_string(value, expected, replacement);
            }
        }
        _ => {}
    }
}
