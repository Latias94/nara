use super::*;

use nara_asset::{ImportedProductId, StableAssetId};
use nara_core::SourceUniverseId;
use serde_json::{Value, json};

fn product(source: u64, auxiliary: Option<u64>) -> AssetProductRef {
    let source =
        StableAssetId::parse_str(format!("00000000-0000-4000-8000-{source:012x}")).unwrap();
    let product = auxiliary.map_or(ImportedProductId::PRIMARY, |id| {
        ImportedProductId::parse_subproduct(&format!("10000000-0000-4000-8000-{id:012x}")).unwrap()
    });
    AssetProductRef::new(source, product).unwrap()
}

fn dependency(
    product: AssetProductRef,
    role: RuntimeContentDependencyRole,
) -> RuntimeContentDependency {
    RuntimeContentDependency::new(product, role)
}

fn scene(
    path: &str,
    product: AssetProductRef,
    dependencies: Vec<RuntimeContentDependency>,
) -> RuntimeContentMember {
    RuntimeContentMember::new(
        path,
        ContentDigest::of_bytes(path.as_bytes()),
        RuntimeContentMemberKind::Scene {
            source: SourceDocumentKey::stable_asset(product.source(), SourceDocumentKind::Scene),
        },
        vec![product],
        dependencies,
    )
    .unwrap()
}

fn asset(
    path: &str,
    products: Vec<AssetProductRef>,
    dependencies: Vec<RuntimeContentDependency>,
) -> RuntimeContentMember {
    RuntimeContentMember::new(
        path,
        ContentDigest::of_bytes(path.as_bytes()),
        RuntimeContentMemberKind::Asset {
            format: FormatKind::new("text_bundle").unwrap(),
            version: FormatVersion::ONE,
        },
        products,
        dependencies,
    )
    .unwrap()
}

fn definition() -> RuntimeContentCatalogDefinition {
    let startup = product(1, None);
    let later = product(2, None);
    let text_a = product(3, Some(10));
    let text_b = product(3, Some(20));
    let external_soft = product(90, Some(90));
    RuntimeContentCatalogDefinition {
        target: RuntimeContentTarget::new("x86_64-pc-windows-msvc", "release").unwrap(),
        configuration: RuntimeConfiguration::default(),
        plugin_plan_fingerprint: [31; 32],
        schema_fingerprint: [47; 32],
        startup_scene: startup,
        scenes: vec![later, startup],
        addressable_roots: vec![text_b, later],
        members: vec![
            scene(
                "scenes/start.runtime.json",
                startup,
                vec![dependency(text_b, RuntimeContentDependencyRole::Required)],
            ),
            asset(
                "assets/text.bundle",
                vec![text_b, text_a],
                vec![
                    dependency(external_soft, RuntimeContentDependencyRole::Soft),
                    dependency(later, RuntimeContentDependencyRole::Required),
                ],
            ),
            scene("scenes/later.runtime.json", later, vec![]),
        ],
    }
}

fn catalog() -> RuntimeContentCatalog {
    RuntimeContentCatalog::new(definition(), RuntimeContentCatalogLimits::default()).unwrap()
}

fn value() -> Value {
    serde_json::from_slice(&catalog().to_json_bytes().unwrap()).unwrap()
}

fn decode(value: &Value) -> Result<RuntimeContentCatalog, RuntimeContentCatalogError> {
    RuntimeContentCatalog::from_json_bytes(
        &serde_json::to_vec(value).unwrap(),
        RuntimeContentCatalogLimits::default(),
    )
}

fn budget(error: RuntimeContentCatalogError, name: &'static str) {
    assert!(
        matches!(&error, RuntimeContentCatalogError::BudgetExceeded { budget, .. } if *budget == name),
        "{error}"
    );
}

#[test]
fn complete_catalog_round_trip_retains_configuration_identity_and_compound_products() {
    let catalog = catalog();
    let bytes = catalog.to_json_bytes().unwrap();
    let decoded =
        RuntimeContentCatalog::from_json_bytes(&bytes, RuntimeContentCatalogLimits::default())
            .unwrap();
    assert_eq!(decoded.to_json_bytes().unwrap(), bytes);
    assert_eq!(catalog.encoded_len(), bytes.len());
    assert_eq!(decoded.retained_bytes(), catalog.retained_bytes());
    assert!(decoded.modeled_work_bytes() > decoded.retained_bytes());
    assert_eq!(decoded.target().triple(), "x86_64-pc-windows-msvc");
    assert_eq!(decoded.target().profile(), "release");
    assert_eq!(decoded.configuration(), catalog.configuration());
    assert_eq!(decoded.plugin_plan_fingerprint(), [31; 32]);
    assert_eq!(decoded.schema_fingerprint(), [47; 32]);
    assert_eq!(decoded.startup_scene(), product(1, None));
    assert_eq!(decoded.scenes(), [product(1, None), product(2, None)]);
    assert_eq!(
        decoded.addressable_roots(),
        [product(2, None), product(3, Some(20))]
    );
    let text = decoded.member_for_product(product(3, Some(20))).unwrap();
    assert_eq!(text.path(), "assets/text.bundle");
    assert_eq!(
        text.products(),
        [product(3, Some(10)), product(3, Some(20))]
    );
    assert_eq!(
        text.digest(),
        ContentDigest::of_bytes(b"assets/text.bundle")
    );
    assert!(std::ptr::eq(
        text,
        decoded.member_for_product(product(3, Some(10))).unwrap()
    ));
    assert!(decoded.member_for_product(product(3, None)).is_none());
    assert!(
        matches!(text.kind(), RuntimeContentMemberKind::Asset { format, version } if format.as_str() == "text_bundle" && *version == FormatVersion::ONE)
    );
    assert_eq!(text.dependencies()[0].product(), product(2, None));
    assert_eq!(
        text.dependencies()[0].role(),
        RuntimeContentDependencyRole::Required
    );
    assert!(
        text.dependencies()
            .binary_search_by_key(&product(2, None), RuntimeContentDependency::product)
            .is_ok()
    );
}

#[test]
fn canonical_bytes_do_not_depend_on_collection_insertion_order() {
    let mut first = definition();
    first.configuration.capabilities.requested = vec![
        nara_project::ProductCapability::Runtime2d,
        nara_project::ProductCapability::RuntimeCore,
    ];
    let mut second = definition();
    second.members.reverse();
    second.scenes.reverse();
    second.addressable_roots.reverse();
    second.configuration.capabilities.requested = vec![
        nara_project::ProductCapability::RuntimeCore,
        nara_project::ProductCapability::Runtime2d,
    ];
    for member in &mut second.members {
        member.products.reverse();
        member.dependencies.reverse();
    }
    let first = RuntimeContentCatalog::new(first, RuntimeContentCatalogLimits::default()).unwrap();
    let second =
        RuntimeContentCatalog::new(second, RuntimeContentCatalogLimits::default()).unwrap();
    assert_eq!(
        first.to_json_bytes().unwrap(),
        second.to_json_bytes().unwrap()
    );
}

#[test]
fn shared_encoded_member_may_contain_products_from_distinct_logical_sources() {
    let mut input = definition();
    input.members[1].products.push(product(4, Some(40)));
    let catalog =
        RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()).unwrap();
    assert!(std::ptr::eq(
        catalog.member_for_product(product(3, Some(10))).unwrap(),
        catalog.member_for_product(product(4, Some(40))).unwrap(),
    ));
}

#[test]
fn absent_soft_edges_are_retained_without_inventing_members() {
    let catalog = catalog();
    let text = catalog.member_for_product(product(3, Some(10))).unwrap();
    assert_eq!(
        text.dependencies()[1],
        dependency(product(90, Some(90)), RuntimeContentDependencyRole::Soft)
    );
    assert!(catalog.member_for_product(product(90, Some(90))).is_none());
    let encoded = catalog.to_json_bytes().unwrap();
    assert!(
        RuntimeContentCatalog::from_json_bytes(&encoded, RuntimeContentCatalogLimits::default())
            .is_ok()
    );
}

#[test]
fn required_edges_must_resolve_to_complete_membership() {
    let mut input = definition();
    input.members[0].dependencies = vec![dependency(
        product(3, None),
        RuntimeContentDependencyRole::Required,
    )];
    assert!(
        matches!(RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()), Err(RuntimeContentCatalogError::MissingRequiredProduct { product: missing, .. }) if missing == product(3, None))
    );
}

#[test]
fn required_cycles_and_self_edges_are_finite_catalog_data() {
    let mut input = definition();
    input.members[2].dependencies = vec![
        dependency(product(1, None), RuntimeContentDependencyRole::Required),
        dependency(product(2, None), RuntimeContentDependencyRole::Required),
    ];
    let catalog =
        RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()).unwrap();
    let decoded = RuntimeContentCatalog::from_json_bytes(
        &catalog.to_json_bytes().unwrap(),
        RuntimeContentCatalogLimits::default(),
    )
    .unwrap();
    assert_eq!(decoded.members().len(), 3);
}

#[test]
fn duplicate_products_are_rejected_within_or_across_members() {
    let mut input = definition();
    input.members[1].products.push(product(3, Some(10)));
    assert!(matches!(
        RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()),
        Err(RuntimeContentCatalogError::DuplicateProduct { .. })
    ));
    let mut input = definition();
    input.members.push(asset(
        "assets/other.bundle",
        vec![product(3, Some(10))],
        vec![],
    ));
    assert!(matches!(
        RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()),
        Err(RuntimeContentCatalogError::DuplicateProduct { .. })
    ));
}

#[test]
fn repeated_dependency_is_rejected_even_when_its_roles_differ() {
    for role in [
        RuntimeContentDependencyRole::Required,
        RuntimeContentDependencyRole::Soft,
    ] {
        let mut input = definition();
        input.members[0]
            .dependencies
            .push(dependency(product(3, Some(20)), role));
        assert!(matches!(
            RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()),
            Err(RuntimeContentCatalogError::DuplicateDependency { .. })
        ));
    }
}

#[test]
fn path_case_aliases_and_file_ancestor_collisions_are_rejected() {
    let mut input = definition();
    input
        .members
        .push(asset("ASSETS/TEXT.BUNDLE", vec![product(4, None)], vec![]));
    assert!(matches!(
        RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()),
        Err(RuntimeContentCatalogError::DuplicateMemberPath { .. })
    ));
    let mut input = definition();
    input
        .members
        .push(asset("ASSETS", vec![product(4, None)], vec![]));
    input
        .members
        .push(asset("assets-other", vec![product(5, None)], vec![]));
    assert!(matches!(
        RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()),
        Err(RuntimeContentCatalogError::AncestorMemberPath { .. })
    ));
}

#[test]
fn member_paths_reject_traversal_rooting_platform_aliases_and_reserved_catalog_file() {
    for path in [
        "",
        "/asset",
        "C:/asset",
        "asset\\name",
        "//server/share",
        "../asset",
        "asset/../file",
        "asset/./file",
        "asset//file",
        "asset/",
        "asset/name.",
        "asset/name ",
        " asset/file",
        "asset/NUL.txt",
        "asset/CON .txt",
        "CoM1.bin",
        "asset/lpt9",
        "catalog.json",
        "CATALOG.JSON/data",
        "asset?x",
        "asset:x",
        "asset\0name",
        "é/member",
    ] {
        assert!(
            validate_member_path(path).is_err(),
            "path must be refused: {path:?}"
        );
    }
    assert!(validate_member_path(&format!("{}.bin", "a".repeat(252))).is_err());
    assert!(validate_member_path("assets/normal file-1_v2.bin").is_ok());
}

#[test]
fn member_path_with_256_components_is_admitted_with_sufficient_path_bytes() {
    let path = vec!["part"; 256].join("/");
    assert!(path.len() > RuntimeContentCatalogLimits::default().path_bytes().get());
    assert_eq!(RelativePath::preflight(&path).unwrap().components(), 256);
    assert!(RelativePath::new(&path).is_ok());
    let limits =
        RuntimeContentCatalogLimits::default().with_path_bytes(ByteLimit::new(path.len()).unwrap());
    let mut input = definition();
    input.members[0] = scene(&path, product(1, None), vec![]);
    let catalog = RuntimeContentCatalog::new(input, limits).unwrap();
    let decoded =
        RuntimeContentCatalog::from_json_bytes(&catalog.to_json_bytes().unwrap(), limits).unwrap();
    assert_eq!(
        decoded.member_for_product(product(1, None)).unwrap().path(),
        path
    );
}

#[test]
fn member_path_with_257_components_is_rejected_before_filesystem_construction() {
    let path = vec!["part"; 257].join("/");
    let limits =
        RuntimeContentCatalogLimits::default().with_path_bytes(ByteLimit::new(path.len()).unwrap());
    assert!(path.len() <= MAX_MEMBER_PATH_BYTES);
    assert!(RelativePath::preflight(&path).is_err());
    assert!(matches!(
        RuntimeContentMember::new(
            &path,
            ContentDigest::of_bytes(b"scene"),
            RuntimeContentMemberKind::Scene {
                source: SourceDocumentKey::stable_asset(
                    product(1, None).source(),
                    SourceDocumentKind::Scene
                ),
            },
            vec![product(1, None)],
            vec![],
        ),
        Err(RuntimeContentCatalogError::InvalidMemberPath { .. })
    ));
    let mut input = definition();
    input.members[0].path = path.clone();
    assert!(matches!(
        RuntimeContentCatalog::new(input, limits),
        Err(RuntimeContentCatalogError::InvalidMemberPath { .. })
    ));
    let mut wire = value();
    wire["payload"]["members"][0]["path"] = json!(path);
    assert!(matches!(
        RuntimeContentCatalog::from_json_bytes(&serde_json::to_vec(&wire).unwrap(), limits),
        Err(RuntimeContentCatalogError::InvalidMemberPath { .. })
    ));
}

#[test]
fn target_coordinates_are_bounded_explicit_identifiers() {
    for (triple, profile) in [
        ("", "release"),
        ("x86_64-pc-windows-msvc", "../debug"),
        (" target", "release"),
        ("host/target", "release"),
        ("a", ""),
    ] {
        assert!(RuntimeContentTarget::new(triple, profile).is_err());
    }
    assert!(
        RuntimeContentTarget::new(
            "a".repeat(RuntimeContentTarget::MAX_TRIPLE_BYTES + 1),
            "release"
        )
        .is_err()
    );
    assert!(
        RuntimeContentTarget::new("a", "b".repeat(RuntimeContentTarget::MAX_PROFILE_BYTES + 1))
            .is_err()
    );
    assert!(RuntimeContentTarget::new("wasm32-unknown-unknown", "custom-profile").is_ok());
}

#[test]
fn scene_member_requires_one_product_and_real_matching_scene_key() {
    let base = product(1, None);
    for (kind, products) in [
        (
            RuntimeContentMemberKind::Scene {
                source: SourceDocumentKey::stable_asset(base.source(), SourceDocumentKind::Prefab),
            },
            vec![base],
        ),
        (
            RuntimeContentMemberKind::Scene {
                source: SourceDocumentKey::stable_asset(
                    product(2, None).source(),
                    SourceDocumentKind::Scene,
                ),
            },
            vec![base],
        ),
        (
            RuntimeContentMemberKind::Scene {
                source: SourceDocumentKey::stable_asset(
                    StableAssetId::parse_str("00000000-0000-0000-0000-000000000000").unwrap(),
                    SourceDocumentKind::Scene,
                ),
            },
            vec![base],
        ),
        (
            RuntimeContentMemberKind::Scene {
                source: SourceDocumentKey::stable_asset(base.source(), SourceDocumentKind::Scene),
            },
            vec![base, product(2, None)],
        ),
    ] {
        assert!(matches!(
            RuntimeContentMember::new(
                "scene.runtime.json",
                ContentDigest::of_bytes(b"data"),
                kind,
                products,
                vec![]
            ),
            Err(RuntimeContentCatalogError::InvalidSceneMember { .. })
        ));
    }
    assert!(matches!(
        RuntimeContentMember::new(
            "empty.bin",
            ContentDigest::of_bytes(b""),
            RuntimeContentMemberKind::Asset {
                format: FormatKind::new("binary").unwrap(),
                version: FormatVersion::ONE
            },
            vec![],
            vec![],
        ),
        Err(RuntimeContentCatalogError::EmptyMemberProducts { .. })
    ));
}

#[test]
fn named_source_identity_is_preserved_without_looking_up_authoring_paths() {
    let mut input = definition();
    let universe: SourceUniverseId = "20000000-0000-4000-8000-000000000001".parse().unwrap();
    input.members[0].kind = RuntimeContentMemberKind::Scene {
        source: SourceDocumentKey::named(
            universe,
            "code-authored startup",
            SourceDocumentKind::Scene,
        )
        .unwrap(),
    };
    let catalog =
        RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()).unwrap();
    let bytes = catalog.to_json_bytes().unwrap();
    let decoded =
        RuntimeContentCatalog::from_json_bytes(&bytes, RuntimeContentCatalogLimits::default())
            .unwrap();
    let RuntimeContentMemberKind::Scene { source } =
        decoded.member_for_product(product(1, None)).unwrap().kind()
    else {
        panic!("Scene member expected");
    };
    assert!(
        matches!(source.origin(), SourceDocumentOrigin::Named { name, .. } if name.as_ref() == "code-authored startup")
    );
}

#[test]
fn declared_scene_and_addressable_roots_require_the_selected_product() {
    let mut input = definition();
    input.scenes.push(product(3, Some(10)));
    assert!(matches!(
        RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()),
        Err(RuntimeContentCatalogError::InvalidSceneRoot { .. })
    ));
    let mut input = definition();
    input
        .scenes
        .retain(|selected| *selected != product(1, None));
    assert!(matches!(
        RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()),
        Err(RuntimeContentCatalogError::UndeclaredStartupScene { .. })
    ));
    let mut input = definition();
    input.addressable_roots.push(product(3, None));
    assert!(matches!(
        RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()),
        Err(RuntimeContentCatalogError::MissingAddressableProduct { .. })
    ));
    for scenes in [true, false] {
        let mut input = definition();
        if scenes {
            input.scenes.push(product(2, None));
        } else {
            input.addressable_roots.push(product(2, None));
        }
        assert!(matches!(
            RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()),
            Err(RuntimeContentCatalogError::DuplicateRoot { .. })
        ));
    }
}

#[test]
fn constructor_and_decoder_apply_all_catalog_count_limits() {
    let bytes = catalog().to_json_bytes().unwrap();
    for (name, limits) in [
        (
            "members",
            RuntimeContentCatalogLimits::default().with_members(ItemLimit::new(2).unwrap()),
        ),
        (
            "products",
            RuntimeContentCatalogLimits::default().with_products(ItemLimit::new(3).unwrap()),
        ),
        (
            "dependencies",
            RuntimeContentCatalogLimits::default().with_dependencies(ItemLimit::new(2).unwrap()),
        ),
        (
            "scenes",
            RuntimeContentCatalogLimits::default().with_scenes(ItemLimit::ONE),
        ),
        (
            "addressable-roots",
            RuntimeContentCatalogLimits::default().with_addressable_roots(ItemLimit::ONE),
        ),
    ] {
        budget(
            RuntimeContentCatalog::new(definition(), limits).unwrap_err(),
            name,
        );
        budget(
            RuntimeContentCatalog::from_json_bytes(&bytes, limits).unwrap_err(),
            name,
        );
    }
}

#[test]
fn encoded_and_retained_limits_have_exact_inclusive_boundaries() {
    let original = catalog();
    let bytes = original.to_json_bytes().unwrap();
    let exact = RuntimeContentCatalogLimits::default()
        .with_encoded_bytes(ByteLimit::new(bytes.len()).unwrap())
        .with_retained_bytes(ByteLimit::new(original.retained_bytes()).unwrap());
    assert!(RuntimeContentCatalog::new(definition(), exact).is_ok());
    assert!(RuntimeContentCatalog::from_json_bytes(&bytes, exact).is_ok());
    for (name, limits) in [
        (
            "encoded-bytes",
            exact.with_encoded_bytes(ByteLimit::new(bytes.len() - 1).unwrap()),
        ),
        (
            "retained-bytes",
            exact.with_retained_bytes(ByteLimit::new(original.retained_bytes() - 1).unwrap()),
        ),
    ] {
        budget(
            RuntimeContentCatalog::new(definition(), limits).unwrap_err(),
            name,
        );
        budget(
            RuntimeContentCatalog::from_json_bytes(&bytes, limits).unwrap_err(),
            name,
        );
    }
}

#[test]
fn cumulative_modeled_work_has_an_exact_boundary_for_each_admission_path() {
    let original = catalog();
    let limits = RuntimeContentCatalogLimits::default()
        .with_working_bytes(ByteLimit::new(original.modeled_work_bytes()).unwrap());
    assert!(RuntimeContentCatalog::new(definition(), limits).is_ok());
    budget(
        RuntimeContentCatalog::new(
            definition(),
            limits.with_working_bytes(ByteLimit::new(original.modeled_work_bytes() - 1).unwrap()),
        )
        .unwrap_err(),
        "working-bytes",
    );
    let bytes = original.to_json_bytes().unwrap();
    let decoded =
        RuntimeContentCatalog::from_json_bytes(&bytes, RuntimeContentCatalogLimits::default())
            .unwrap();
    let limits = RuntimeContentCatalogLimits::default()
        .with_working_bytes(ByteLimit::new(decoded.modeled_work_bytes()).unwrap());
    assert!(RuntimeContentCatalog::from_json_bytes(&bytes, limits).is_ok());
    budget(
        RuntimeContentCatalog::from_json_bytes(
            &bytes,
            limits.with_working_bytes(ByteLimit::new(decoded.modeled_work_bytes() - 1).unwrap()),
        )
        .unwrap_err(),
        "working-bytes",
    );
    budget(
        RuntimeContentCatalog::from_json_bytes(&bytes, limits.with_working_bytes(ByteLimit::ONE))
            .unwrap_err(),
        "working-bytes",
    );
}

#[test]
fn raw_count_admission_precedes_semantic_payload_construction() {
    let mut wire = value();
    wire["payload"]["members"][2]["products"][0] =
        json!({"invalid": "would fail Product deserialization"});
    let limits = RuntimeContentCatalogLimits::default().with_members(ItemLimit::new(2).unwrap());
    budget(
        RuntimeContentCatalog::from_json_bytes(&serde_json::to_vec(&wire).unwrap(), limits)
            .unwrap_err(),
        "members",
    );
}

#[test]
fn shape_and_path_limits_are_enforced_before_semantic_decode() {
    let bytes = catalog().to_json_bytes().unwrap();
    let ordinary = RuntimeContentCatalogLimits::default();
    let shape = ordinary.shape();
    for (name, limits) in [
        (
            "path-bytes",
            ordinary.with_path_bytes(ByteLimit::new(8).unwrap()),
        ),
        (
            "wire-depth",
            ordinary.with_shape(SerdeShapeLimits::new(
                DepthLimit::ONE,
                shape.nodes(),
                shape.container_items(),
                shape.string_bytes(),
                shape.total_string_bytes(),
            )),
        ),
        (
            "wire-nodes",
            ordinary.with_shape(SerdeShapeLimits::new(
                shape.depth(),
                ItemLimit::ONE,
                shape.container_items(),
                shape.string_bytes(),
                shape.total_string_bytes(),
            )),
        ),
        (
            "wire-container-items",
            ordinary.with_shape(SerdeShapeLimits::new(
                shape.depth(),
                shape.nodes(),
                ItemLimit::ONE,
                shape.string_bytes(),
                shape.total_string_bytes(),
            )),
        ),
        (
            "string-bytes",
            ordinary.with_shape(SerdeShapeLimits::new(
                shape.depth(),
                shape.nodes(),
                shape.container_items(),
                ByteLimit::ONE,
                shape.total_string_bytes(),
            )),
        ),
        (
            "total-string-bytes",
            ordinary.with_shape(SerdeShapeLimits::new(
                shape.depth(),
                shape.nodes(),
                shape.container_items(),
                shape.string_bytes(),
                ByteLimit::ONE,
            )),
        ),
    ] {
        budget(
            RuntimeContentCatalog::from_json_bytes(&bytes, limits).unwrap_err(),
            name,
        );
    }
}

#[test]
fn strict_envelope_checks_kind_version_and_engine_requirement() {
    let mut wire = value();
    assert_eq!(wire["kind"], "runtime_content_catalog");
    assert_eq!(wire["format_version"], 1);
    wire["kind"] = json!("scene");
    assert!(matches!(
        decode(&wire),
        Err(RuntimeContentCatalogError::Contract(
            PersistentFileContractError::WrongKind { .. }
        ))
    ));
    let mut wire = value();
    wire["format_version"] = json!(2);
    assert!(matches!(
        decode(&wire),
        Err(RuntimeContentCatalogError::Contract(
            PersistentFileContractError::UnsupportedFormatVersion { .. }
        ))
    ));
    let mut wire = value();
    wire["engine_min_version"] = json!("999.0.0");
    assert!(matches!(
        decode(&wire),
        Err(RuntimeContentCatalogError::Contract(
            PersistentFileContractError::EngineVersionTooOld { .. }
        ))
    ));
}

#[test]
fn unknown_fields_are_rejected_at_every_owned_wire_record() {
    for pointer in [
        "",
        "/payload",
        "/generator",
        "/payload/target",
        "/payload/configuration",
        "/payload/members/0",
        "/payload/members/0/digest",
        "/payload/members/0/kind",
        "/payload/members/0/dependencies/0",
    ] {
        let mut wire = value();
        wire.pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        assert!(decode(&wire).is_err(), "unknown field at {pointer}");
    }
}

#[test]
fn owned_wire_records_reject_positional_arrays() {
    for pointer in [
        "",
        "/payload",
        "/generator",
        "/payload/target",
        "/payload/configuration",
        "/payload/members/0",
        "/payload/members/0/digest",
        "/payload/members/0/kind",
        "/payload/members/0/dependencies/0",
    ] {
        let mut wire = value();
        let record = wire.pointer_mut(pointer).unwrap();
        let fields = record
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        *record = Value::Array(fields);
        assert!(decode(&wire).is_err(), "positional record at {pointer}");
    }
}

#[test]
fn duplicate_keys_are_rejected_even_in_unknown_and_configuration_records() {
    let bytes = catalog().to_json_bytes().unwrap();
    let text = String::from_utf8(bytes).unwrap();
    for text in [
        text.replacen(
            "\"format_version\":1",
            "\"format_version\":1,\"format_version\":1",
            1,
        ),
        text.replacen("\"triple\":", "\"profile\":\"release\",\"triple\":", 1),
        text.replacen(
            "\"configuration\":{",
            "\"configuration\":{\"unknown\":1,\"unknown\":2,",
            1,
        ),
        text.replacen("\"digest\":{", "\"digest\":{\"length\":0,", 1),
    ] {
        assert!(matches!(
            RuntimeContentCatalog::from_json_bytes(
                text.as_bytes(),
                RuntimeContentCatalogLimits::default()
            ),
            Err(RuntimeContentCatalogError::Shape(
                SerdeShapeError::DuplicateMapKey
            ))
        ));
    }
}

#[test]
fn corrupt_or_ambiguous_identity_and_format_records_are_refused() {
    for replacement in [
        json!({"source": "00000000-0000-0000-0000-000000000000", "product": {"kind":"primary"}}),
        json!({"source": "00000000-0000-4000-8000-000000000001", "product": {"kind":"subproduct", "id":"00000000-0000-0000-0000-000000000000"}}),
        json!({"source": "00000000-0000-4000-8000-000000000001", "product": {"kind":"primary", "id":"10000000-0000-4000-8000-000000000001"}}),
    ] {
        let mut wire = value();
        wire["payload"]["members"][0]["products"][0] = replacement;
        assert!(decode(&wire).is_err());
    }
    for (key, replacement) in [("version", json!(0)), ("format", json!("../invalid"))] {
        let mut wire = value();
        wire["payload"]["members"][0]["kind"][key] = replacement;
        assert!(decode(&wire).is_err());
    }
    let mut wire = value();
    wire["payload"]["members"][0]["digest"]["hash"] = json!([1, 2, 3]);
    assert!(decode(&wire).is_err());
}

#[test]
fn trailing_json_and_truncated_payloads_are_refused() {
    let mut bytes = catalog().to_json_bytes().unwrap();
    bytes.extend_from_slice(b" null");
    assert!(
        RuntimeContentCatalog::from_json_bytes(&bytes, RuntimeContentCatalogLimits::default())
            .is_err()
    );
    assert!(
        RuntimeContentCatalog::from_json_bytes(
            &bytes[..bytes.len() / 2],
            RuntimeContentCatalogLimits::default()
        )
        .is_err()
    );
}

#[test]
fn catalog_does_not_turn_configuration_records_into_admitted_runtime_values() {
    let mut input = definition();
    input.configuration.window.width = 0;
    let catalog =
        RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()).unwrap();
    assert!(catalog.configuration().resolve().is_err());
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut input = definition();
        input.configuration.runtime.time_scale = value;
        assert!(matches!(
            RuntimeContentCatalog::new(input, RuntimeContentCatalogLimits::default()),
            Err(RuntimeContentCatalogError::NonfiniteConfiguration)
        ));
    }
}
