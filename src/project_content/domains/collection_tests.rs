use super::*;

const SOURCE_A: &str = "df2cb24c-43e1-4f20-bf75-c9193ff5b6b1";
const SOURCE_B: &str = "b0be758c-1a1e-4742-bba2-7219cf7abc7e";
const FOREIGN_SOURCE: &str = "4049b717-72c5-4c1b-a519-87b3cb529c52";
const AUXILIARY_A: &str = "da182a7f-7418-49fa-bc9c-3e4669313486";
const AUXILIARY_B: &str = "1a1ca3f8-0ae2-465c-a244-004e969acaa9";

struct CompoundDomain;

#[derive(Debug)]
struct Bundle {
    value: &'static str,
    primary: bool,
    auxiliary: Vec<ImportedProductId>,
}

impl AssetDomain for CompoundDomain {
    type Product = Bundle;

    fn contains_product(product: &Bundle, id: ImportedProductId) -> bool {
        if id == ImportedProductId::PRIMARY {
            product.primary
        } else {
            product.auxiliary.contains(&id)
        }
    }
}

fn stable_id(source: &str) -> StableAssetId {
    StableAssetId::parse_str(source).unwrap()
}

fn auxiliary(id: &str) -> ImportedProductId {
    ImportedProductId::parse_subproduct(id).unwrap()
}

fn product_ref(source: &str, product: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(stable_id(source), product).unwrap()
}

fn record(source: &str, path: &str) -> AssetRecord {
    AssetRecord::new(
        stable_id(source),
        AssetSourcePath::new(path).unwrap(),
        AssetSourceKind::Other("collection-test".to_owned()),
    )
}

fn bundle(value: &'static str, auxiliary: &[ImportedProductId]) -> Bundle {
    Bundle {
        value,
        primary: true,
        auxiliary: auxiliary.to_vec(),
    }
}

fn source_collection(
    scope: AssetPathScope,
    records: Vec<AssetRecord>,
    products: Vec<Bundle>,
) -> ProjectAssetCollection<Bundle> {
    ProjectAssetCollection {
        storage: CollectionStorage::Source {
            scope,
            records: records.into_boxed_slice(),
            products: products.into_boxed_slice(),
            contains_product: CompoundDomain::contains_product,
        },
    }
}

#[test]
fn source_lookup_checks_primary_and_actual_auxiliary_membership() {
    let present = auxiliary(AUXILIARY_A);
    let absent = auxiliary(AUXILIARY_B);
    let collection = source_collection(
        AssetPathScope::ROOT,
        vec![record(SOURCE_A, "bundles/main.bundle")],
        vec![bundle("main", &[present])],
    );
    let stored = collection.iter().next().unwrap();
    for reference in [
        AssetRef::path("bundles/main.bundle").unwrap(),
        AssetRef::StableId(stable_id(SOURCE_A)),
        AssetRef::Product(product_ref(SOURCE_A, ImportedProductId::PRIMARY)),
        AssetRef::Product(product_ref(SOURCE_A, present)),
    ] {
        assert!(std::ptr::eq(collection.get(&reference).unwrap(), stored));
    }
    assert!(
        collection
            .get(&AssetRef::Product(product_ref(SOURCE_A, absent)))
            .is_none()
    );
}

#[test]
fn source_primary_aliases_require_actual_primary_membership() {
    let auxiliary = auxiliary(AUXILIARY_A);
    let mut product = bundle("auxiliary only", &[auxiliary]);
    product.primary = false;
    let collection = source_collection(
        AssetPathScope::ROOT,
        vec![record(SOURCE_A, "bundles/main.bundle")],
        vec![product],
    );
    for reference in [
        AssetRef::path("bundles/main.bundle").unwrap(),
        AssetRef::StableId(stable_id(SOURCE_A)),
        AssetRef::Product(product_ref(SOURCE_A, ImportedProductId::PRIMARY)),
    ] {
        assert!(collection.get(&reference).is_none());
    }
    assert!(
        collection
            .get(&AssetRef::Product(product_ref(SOURCE_A, auxiliary)))
            .is_some()
    );
}

#[test]
fn source_paths_alias_only_actual_source_records() {
    let auxiliary = auxiliary(AUXILIARY_A);
    let collection = source_collection(
        AssetPathScope::ROOT,
        vec![record(SOURCE_A, "bundles/main.bundle")],
        vec![bundle("main", &[auxiliary])],
    );
    for path in [
        "bundles/unknown.bundle".to_owned(),
        SOURCE_A.to_owned(),
        AUXILIARY_A.to_owned(),
        format!("bundles/main.bundle/{auxiliary}"),
    ] {
        assert!(collection.get(&AssetRef::path(path).unwrap()).is_none());
    }
    assert_eq!(
        collection
            .get(&AssetRef::path("bundles/main.bundle").unwrap())
            .unwrap()
            .value,
        "main"
    );
}

#[test]
fn equal_auxiliary_ids_remain_scoped_to_the_actual_source() {
    let shared = auxiliary(AUXILIARY_A);
    let only_a = auxiliary(AUXILIARY_B);
    let collection = source_collection(
        AssetPathScope::ROOT,
        vec![
            record(SOURCE_A, "bundles/a.bundle"),
            record(SOURCE_B, "bundles/b.bundle"),
        ],
        vec![bundle("a", &[shared, only_a]), bundle("b", &[shared])],
    );
    for (source, expected) in [(SOURCE_A, "a"), (SOURCE_B, "b")] {
        assert_eq!(
            collection
                .get(&AssetRef::Product(product_ref(source, shared)))
                .unwrap()
                .value,
            expected
        );
    }
    for reference in [
        AssetRef::Product(product_ref(SOURCE_B, only_a)),
        AssetRef::Product(product_ref(FOREIGN_SOURCE, shared)),
        AssetRef::StableId(stable_id(FOREIGN_SOURCE)),
    ] {
        assert!(collection.get(&reference).is_none());
    }
}

#[test]
fn iteration_visits_values_once_despite_multiple_product_selections() {
    let auxiliary_a = auxiliary(AUXILIARY_A);
    let auxiliary_b = auxiliary(AUXILIARY_B);
    let collection = source_collection(
        AssetPathScope::ROOT,
        vec![
            record(SOURCE_A, "bundles/a.bundle"),
            record(SOURCE_B, "bundles/b.bundle"),
        ],
        vec![
            bundle("a", &[auxiliary_a, auxiliary_b]),
            bundle("b", &[auxiliary_a]),
        ],
    );
    for selected in [ImportedProductId::PRIMARY, auxiliary_a, auxiliary_b] {
        assert!(
            collection
                .get(&AssetRef::Product(product_ref(SOURCE_A, selected)))
                .is_some()
        );
    }
    let mut values = collection.iter();
    assert_eq!(values.len(), 2);
    assert_eq!(values.next().unwrap().value, "a");
    assert_eq!(values.len(), 1);
    assert_eq!(values.next().unwrap().value, "b");
    assert_eq!(values.len(), 0);
    assert!(values.next().is_none());
}

#[test]
fn source_record_view_preserves_only_actual_records_in_storage_order() {
    let records = vec![
        record(SOURCE_A, "bundles/a.bundle"),
        record(SOURCE_B, "bundles/b.bundle"),
    ];
    let collection = source_collection(
        AssetPathScope::ROOT,
        records.clone(),
        vec![bundle("a", &[]), bundle("b", &[])],
    );
    assert_eq!(collection.source_records().len(), 2);
    assert_eq!(
        collection.source_records().cloned().collect::<Vec<_>>(),
        records
    );
}

#[test]
fn selection_revalidation_uses_source_entries_and_actual_membership() {
    let present = auxiliary(AUXILIARY_A);
    let collection = DomainCollection::<CompoundDomain>(source_collection(
        AssetPathScope::ROOT,
        vec![
            record(SOURCE_A, "bundles/a.bundle"),
            record(SOURCE_B, "bundles/b.bundle"),
        ],
        vec![bundle("a", &[present]), bundle("b", &[])],
    ));
    let mut required = BTreeMap::from([
        (
            stable_id(SOURCE_A),
            BTreeSet::from([product_ref(SOURCE_A, present)]),
        ),
        (
            stable_id(SOURCE_B),
            BTreeSet::from([product_ref(SOURCE_B, ImportedProductId::PRIMARY)]),
        ),
    ]);
    collection
        .validate_product_selections(&mut required)
        .unwrap();
    assert!(required.is_empty());

    let missing = product_ref(SOURCE_B, present);
    let mut required = BTreeMap::from([(stable_id(SOURCE_B), BTreeSet::from([missing]))]);
    assert_eq!(
        collection.validate_product_selections(&mut required),
        Err(missing)
    );
}

#[test]
fn source_path_aliases_use_the_actual_configured_scope() {
    let present = auxiliary(AUXILIARY_A);
    let scoped_record = record(SOURCE_A, "game/content/imports/bundles/main.bundle");
    let collection = source_collection(
        AssetPathScope::under(AssetSourcePath::new("game/content/imports").unwrap()),
        vec![scoped_record.clone()],
        vec![bundle("main", &[present])],
    );
    let stored = collection.iter().next().unwrap();
    for reference in [
        AssetRef::path("bundles/main.bundle").unwrap(),
        AssetRef::StableId(stable_id(SOURCE_A)),
        AssetRef::Product(product_ref(SOURCE_A, ImportedProductId::PRIMARY)),
        AssetRef::Product(product_ref(SOURCE_A, present)),
    ] {
        assert!(std::ptr::eq(collection.get(&reference).unwrap(), stored));
    }
    for path in [
        "game/content/imports/bundles/main.bundle",
        "assets/bundles/main.bundle",
        "imports/bundles/main.bundle",
        "bundles/unknown.bundle",
    ] {
        assert!(collection.get(&AssetRef::path(path).unwrap()).is_none());
    }
    assert_eq!(collection.source_records().next(), Some(&scoped_record));
}

#[test]
fn root_source_path_aliases_keep_the_complete_canonical_spelling() {
    let collection = source_collection(
        AssetPathScope::ROOT,
        vec![record(SOURCE_A, "game/content/imports/bundles/main.bundle")],
        vec![bundle("main", &[])],
    );
    assert_eq!(
        collection
            .get(&AssetRef::path("game/content/imports/bundles/main.bundle").unwrap())
            .unwrap()
            .value,
        "main"
    );
    assert!(
        collection
            .get(&AssetRef::path("bundles/main.bundle").unwrap())
            .is_none()
    );
}

#[test]
fn source_path_aliases_reject_foreign_scopes_and_partial_prefixes() {
    let scope = AssetPathScope::under(AssetSourcePath::new("content/imports").unwrap());
    let present = auxiliary(AUXILIARY_A);
    for path in [
        "content/imports-extra/bundles/main.bundle",
        "content/prefabs/bundles/main.bundle",
        "other/imports/bundles/main.bundle",
        "content/Imports/bundles/main.bundle",
        "content/imports",
    ] {
        let collection = source_collection(
            scope.clone(),
            vec![record(SOURCE_A, path)],
            vec![bundle("foreign", &[present])],
        );
        for alias in ["bundles/main.bundle", path] {
            assert!(collection.get(&AssetRef::path(alias).unwrap()).is_none());
        }
        // Stable and complete Product identity are independent of the collection's Path scope.
        for reference in [
            AssetRef::StableId(stable_id(SOURCE_A)),
            AssetRef::Product(product_ref(SOURCE_A, ImportedProductId::PRIMARY)),
            AssetRef::Product(product_ref(SOURCE_A, present)),
        ] {
            assert_eq!(collection.get(&reference).unwrap().value, "foreign");
        }
    }
}

#[test]
fn same_named_sources_keep_distinct_scope_aliases_and_product_identity() {
    let records = vec![
        record(SOURCE_A, "content/imports/bundles/main.bundle"),
        record(SOURCE_B, "content/shared/bundles/main.bundle"),
    ];
    for (root, expected) in [("content/imports", "a"), ("content/shared", "b")] {
        let collection = source_collection(
            AssetPathScope::under(AssetSourcePath::new(root).unwrap()),
            records.clone(),
            vec![bundle("a", &[]), bundle("b", &[])],
        );
        assert_eq!(
            collection
                .get(&AssetRef::path("bundles/main.bundle").unwrap())
                .unwrap()
                .value,
            expected
        );
        for (source, value) in [(SOURCE_A, "a"), (SOURCE_B, "b")] {
            assert_eq!(
                collection
                    .get(&AssetRef::Product(product_ref(
                        source,
                        ImportedProductId::PRIMARY
                    )))
                    .unwrap()
                    .value,
                value
            );
        }
        assert_eq!(
            collection.source_records().cloned().collect::<Vec<_>>(),
            records
        );
    }
}

#[test]
fn empty_processed_collection_has_no_source_records_or_aliases() {
    let erased = runtime_collection::<CompoundDomain>(Vec::new(), Vec::new()).unwrap();
    let collection = erased
        .as_any()
        .downcast_ref::<ProjectAssetCollection<Bundle>>()
        .unwrap();
    assert!(erased.records().is_empty());
    assert_eq!(collection.source_records().len(), 0);
    assert_eq!(collection.iter().len(), 0);
    for reference in [
        AssetRef::path("bundles/main.bundle").unwrap(),
        AssetRef::StableId(stable_id(SOURCE_A)),
        AssetRef::Product(product_ref(SOURCE_A, ImportedProductId::PRIMARY)),
        AssetRef::Product(product_ref(SOURCE_A, auxiliary(AUXILIARY_A))),
    ] {
        assert!(collection.get(&reference).is_none());
    }
}

#[test]
fn processed_mapping_requires_an_acquired_owner() {
    let reference = product_ref(SOURCE_A, auxiliary(AUXILIARY_A));
    assert!(matches!(
        runtime_collection::<CompoundDomain>(Vec::new(), vec![(reference, 0)]),
        Err(RuntimeContentError::ProductIdentityMismatch)
    ));
}
