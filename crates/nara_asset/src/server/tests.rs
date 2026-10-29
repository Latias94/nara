use std::collections::HashSet;

use super::*;
use crate::{
    AssetRef, AssetRefError, AssetRefExportPolicy, AssetSourceKind, Assets, ProjectAssetDatabase,
};

fn product(source: u128, selection: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::from_uuid(uuid::Uuid::from_u128(source)),
        selection,
    )
    .unwrap()
}

fn auxiliary() -> ImportedProductId {
    ImportedProductId::parse_subproduct("011a8cc0-343a-4b95-8adb-3aa6d81977ed").unwrap()
}

fn source_record() -> AssetRecord {
    AssetRecord::new(
        product(1, ImportedProductId::PRIMARY).source(),
        AssetPath::new("textures/player.png").unwrap(),
        AssetSourceKind::Image,
    )
}

#[test]
fn primary_product_and_source_record_share_handles_in_both_orders() {
    let reference = product(1, ImportedProductId::PRIMARY);
    let record = source_record();
    for product_first in [false, true] {
        let mut server = AssetServer::new();
        let first = if product_first {
            server.reserve_product::<String>(reference).unwrap()
        } else {
            server.reserve_record::<String>(&record).unwrap()
        };

        assert_eq!(server.reserve_record::<String>(&record), Ok(first));
        assert_eq!(server.reserve_product::<String>(reference), Ok(first));
        assert_eq!(server.reserve_record_id(&record), Ok(first.id()));
        let baseline = server.clone();
        assert_eq!(server.resolve_product::<String>(reference), Ok(first));
        assert_eq!(server.product(first.id()), Some(reference));
        assert_eq!(server.path(first.id()), Some(record.path().as_str()));
        assert_eq!(server.stable_id(first.id()), Some(reference.source()));
        assert_eq!(
            server.id_for_stable_id(reference.source()),
            Some(first.id())
        );
        assert_eq!(server, baseline);
    }
}

#[test]
fn source_path_alias_joins_its_primary_product_without_changing_handle() {
    let mut server = AssetServer::new();
    let record = source_record();
    let reference = product(1, ImportedProductId::PRIMARY);
    let handle = server.reserve::<String>(record.path().as_str()).unwrap();
    assert_eq!(server.product(handle.id()), None);

    assert_eq!(server.reserve_record::<String>(&record), Ok(handle));
    assert_eq!(server.reserve_product::<String>(reference), Ok(handle));
    assert_eq!(server.product(handle.id()), Some(reference));
    assert_eq!(server.id_for_path(record.path()), Some(handle.id()));
}

#[test]
fn product_ids_are_scoped_by_source_and_distinguish_primary_from_auxiliary() {
    let mut server = AssetServer::new();
    let primary = product(1, ImportedProductId::PRIMARY);
    let selected = product(1, auxiliary());
    let other_primary = product(2, ImportedProductId::PRIMARY);
    let other_selected = product(2, auxiliary());
    let primary_handle = server.reserve_product::<String>(primary).unwrap();
    let selected_handle = server.reserve_product::<u32>(selected).unwrap();
    let other_primary_handle = server.reserve_product::<String>(other_primary).unwrap();
    let other_selected_handle = server.reserve_product::<u32>(other_selected).unwrap();

    assert_eq!(
        [
            primary_handle.id(),
            selected_handle.id(),
            other_primary_handle.id(),
            other_selected_handle.id(),
        ]
        .into_iter()
        .collect::<HashSet<_>>()
        .len(),
        4
    );
    for (reference, id) in [
        (primary, primary_handle.id()),
        (selected, selected_handle.id()),
        (other_primary, other_primary_handle.id()),
        (other_selected, other_selected_handle.id()),
    ] {
        assert_eq!(server.product(id), Some(reference));
        assert_eq!(server.path(id), None);
    }
    assert_eq!(server.stable_id(selected_handle.id()), None);
    assert_eq!(server.stable_id(other_selected_handle.id()), None);
    assert_eq!(
        server.id_for_stable_id(primary.source()),
        Some(primary_handle.id())
    );
    assert_eq!(
        server.id_for_stable_id(other_primary.source()),
        Some(other_primary_handle.id())
    );
    assert_eq!(server.get_handle::<u32>(primary_handle.id()), None);
    assert_eq!(server.get_handle::<String>(selected_handle.id()), None);
}

#[test]
fn unknown_products_and_wrong_types_leave_the_catalogue_unchanged() {
    let mut server = AssetServer::new();
    let primary = product(1, ImportedProductId::PRIMARY);
    let selected = product(1, auxiliary());
    let primary_handle = server.reserve_product::<String>(primary).unwrap();
    let selected_handle = server.reserve_product::<String>(selected).unwrap();
    let baseline = server.clone();

    for unknown in [
        product(2, ImportedProductId::PRIMARY),
        product(2, auxiliary()),
        product(
            1,
            ImportedProductId::parse_subproduct("a84ad884-46f9-4178-b0b1-711e851c201c").unwrap(),
        ),
    ] {
        assert_eq!(
            server.resolve_product::<String>(unknown),
            Err(AssetError::UnknownProduct(unknown))
        );
        assert_eq!(server, baseline);
        assert_eq!(
            AssetRef::Product(unknown).resolve::<u32>(&mut server),
            Err(AssetRefError::Asset(AssetError::UnknownProduct(unknown)))
        );
        assert_eq!(server, baseline);
    }
    for (reference, handle) in [(primary, primary_handle), (selected, selected_handle)] {
        let mismatch = AssetError::AssetTypeMismatch {
            id: handle.id(),
            existing: std::any::type_name::<String>(),
            requested: std::any::type_name::<u32>(),
        };
        assert_eq!(
            server.reserve_product::<u32>(reference),
            Err(mismatch.clone())
        );
        assert_eq!(server, baseline);
        assert_eq!(
            server.resolve_product::<u32>(reference),
            Err(mismatch.clone())
        );
        assert_eq!(server, baseline);
        assert_eq!(
            AssetRef::Product(reference).resolve::<u32>(&mut server),
            Err(AssetRefError::Asset(mismatch.clone()))
        );
        assert_eq!(server, baseline);
        assert_eq!(
            AssetRef::from_handle(&server, Handle::<u32>::new(handle.id())),
            Err(AssetRefError::Asset(mismatch))
        );
        assert_eq!(server, baseline);
    }
}

#[test]
fn lookup_claims_an_existing_untyped_primary_without_allocating_another_identity() {
    let mut server = AssetServer::new();
    let reference = product(1, ImportedProductId::PRIMARY);
    let id = server.reserve_record_id(&source_record()).unwrap();
    let baseline = server.clone();
    assert_eq!(server.get_handle::<String>(id), None);

    let handle = server.resolve_product::<String>(reference).unwrap();

    assert_eq!(handle.id(), id);
    assert_eq!(server.get_handle::<String>(id), Some(handle));
    assert_eq!(baseline.get_handle::<String>(id), None);
    assert_ne!(server, baseline);
    let claimed = server.clone();
    assert_eq!(server.reserve_product::<String>(reference), Ok(handle));
    assert_eq!(server, claimed);
}

#[test]
fn independently_reserved_path_and_product_cannot_be_merged_by_a_source_record() {
    let mut server = AssetServer::new();
    let record = source_record();
    let reference = product(1, ImportedProductId::PRIMARY);
    let path_handle = server.reserve::<String>(record.path().as_str()).unwrap();
    let product_handle = server.reserve_product::<String>(reference).unwrap();
    assert_ne!(path_handle, product_handle);
    let baseline = server.clone();
    let conflict = AssetError::ConflictingAssetIdentity {
        path: record.path().clone(),
        stable_id: reference.source(),
    };

    assert_eq!(server.reserve_record_id(&record), Err(conflict.clone()));
    assert_eq!(server, baseline);
    assert_eq!(server.reserve_record::<String>(&record), Err(conflict));
    assert_eq!(server, baseline);
    assert_eq!(server.product(path_handle.id()), None);
    assert_eq!(server.path(product_handle.id()), None);
}

#[test]
fn a_conflicting_product_type_does_not_add_a_source_path() {
    let mut server = AssetServer::new();
    let record = source_record();
    let reference = product(1, ImportedProductId::PRIMARY);
    let handle = server.reserve_product::<String>(reference).unwrap();
    let baseline = server.clone();

    assert!(matches!(
        server.reserve_record::<u32>(&record),
        Err(AssetError::AssetTypeMismatch { id, .. }) if id == handle.id()
    ));
    assert_eq!(server, baseline);
    assert_eq!(server.path(handle.id()), None);
    assert_eq!(server.id_for_path(record.path()), None);
    assert_eq!(server.reserve_record::<String>(&record), Ok(handle));
}

#[test]
fn nil_source_records_reject_before_any_alias_or_type_mutation() {
    let nil = StableAssetId::from_uuid(uuid::Uuid::nil());
    let record = AssetRecord::new(
        nil,
        AssetPath::new("textures/nil.png").unwrap(),
        AssetSourceKind::Image,
    );
    let invalid = AssetError::InvalidProductIdentity(AssetProductIdentityError::NilSource);
    for path_exists in [false, true] {
        let mut server = AssetServer::new();
        if path_exists {
            server.reserve::<u32>(record.path().as_str()).unwrap();
        }
        let baseline = server.clone();

        assert_eq!(
            server.reserve_record::<String>(&record),
            Err(invalid.clone())
        );
        assert_eq!(server, baseline);
        assert_eq!(server.reserve_record_id(&record), Err(invalid.clone()));
        assert_eq!(server, baseline);
        assert_eq!(server.id_for_stable_id(nil), None);
    }
}

#[test]
fn reverse_export_preserves_auxiliary_and_pathless_primary_product_identity() {
    let mut server = AssetServer::new();
    for selection in [ImportedProductId::PRIMARY, auxiliary()] {
        let reference = product(1, selection);
        let handle = server.reserve_product::<String>(reference).unwrap();
        let baseline = server.clone();
        assert_eq!(
            AssetRef::from_handle(&server, handle),
            Ok(AssetRef::Product(reference))
        );
        for policy in [
            AssetRefExportPolicy::Path,
            AssetRefExportPolicy::StableIdWhenKnown,
        ] {
            let exported = AssetRef::from_handle_with_policy(&server, handle, policy).unwrap();
            assert_eq!(exported, AssetRef::Product(reference));
            assert_eq!(exported.resolve::<String>(&mut server), Ok(handle));
            assert_eq!(server, baseline);
        }
        assert_eq!(server.product(handle.id()), Some(reference));
        assert_eq!(server.path(handle.id()), None);
    }
}

#[test]
fn source_bound_primary_exports_keep_the_source_policy_and_database_requirement() {
    let mut server = AssetServer::new();
    let record = source_record();
    let reference = product(1, ImportedProductId::PRIMARY);
    let handle = server.reserve_product::<String>(reference).unwrap();
    assert_eq!(server.reserve_record::<String>(&record), Ok(handle));
    assert_eq!(
        AssetRef::from_handle(&server, handle),
        Ok(AssetRef::Path(record.path().clone()))
    );
    assert_eq!(
        AssetRef::from_handle_with_policy(&server, handle, AssetRefExportPolicy::StableIdWhenKnown),
        Ok(AssetRef::StableId(reference.source()))
    );
    let selected = product(1, auxiliary());
    let auxiliary_handle = server.reserve_product::<String>(selected).unwrap();
    for policy in [
        AssetRefExportPolicy::Path,
        AssetRefExportPolicy::StableIdWhenKnown,
    ] {
        assert_eq!(
            AssetRef::from_handle_with_policy(&server, auxiliary_handle, policy),
            Ok(AssetRef::Product(selected))
        );
    }
    assert_eq!(server.path(auxiliary_handle.id()), None);
    let baseline = server.clone();
    assert_eq!(
        AssetRef::StableId(reference.source()).resolve::<String>(&mut server),
        Err(AssetRefError::MissingProjectDatabase(reference.source()))
    );
    assert_eq!(
        AssetRef::Product(reference)
            .resolve_with_database::<String>(&mut server, &ProjectAssetDatabase::default()),
        Err(AssetRefError::Database(
            crate::AssetDatabaseError::UnknownStableId(reference.source())
        ))
    );
    assert_eq!(server, baseline);
}

#[test]
fn product_identity_does_not_imply_asset_residency() {
    let mut server = AssetServer::new();
    let reference = product(1, auxiliary());
    let handle = server.reserve_product::<String>(reference).unwrap();
    let baseline = server.clone();
    let mut assets = Assets::<String>::default();

    assert_eq!(server.stable_id(handle.id()), None);
    assert_eq!(server.id_for_stable_id(reference.source()), None);
    assert_eq!(assets.get(handle), None);
    assert_eq!(server.resolve_product::<String>(reference), Ok(handle));
    assert_eq!(assets.get(handle), None);
    assets.insert(handle, String::from("prepared by the concrete owner"));
    assert_eq!(
        assets.get(handle).map(String::as_str),
        Some("prepared by the concrete owner")
    );
    assert_eq!(server, baseline);
}

#[test]
fn cloned_catalogues_keep_product_indices_path_aliases_and_type_claims_independent() {
    let mut original = AssetServer::new();
    let primary = product(1, ImportedProductId::PRIMARY);
    let selected = product(1, auxiliary());
    let primary_handle = original.reserve_product::<String>(primary).unwrap();
    let selected_handle = original.reserve_product::<String>(selected).unwrap();
    let untyped = original
        .reserve_record_id(&AssetRecord::new(
            product(2, ImportedProductId::PRIMARY).source(),
            AssetPath::new("textures/untyped.png").unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();
    let baseline = original.clone();
    let mut clone = original.clone();
    assert_eq!(original, clone);

    let new_reference = product(3, auxiliary());
    let clone_new = clone.reserve_product::<String>(new_reference).unwrap();
    assert_eq!(original, baseline);
    assert_eq!(original.product(clone_new.id()), None);
    let original_new = original.reserve_product::<String>(new_reference).unwrap();
    assert_ne!(original_new, clone_new);
    assert_eq!(clone.product(original_new.id()), None);

    assert_eq!(
        original.reserve_record::<String>(&source_record()),
        Ok(primary_handle)
    );
    assert_eq!(clone.path(primary_handle.id()), None);
    assert_eq!(baseline.path(primary_handle.id()), None);
    assert_eq!(original.claim::<String>(untyped).unwrap().id(), untyped);
    assert_eq!(clone.claim::<u32>(untyped).unwrap().id(), untyped);
    assert_eq!(baseline.get_handle::<String>(untyped), None);
    assert_eq!(baseline.get_handle::<u32>(untyped), None);
    assert_eq!(original.get_handle::<u32>(untyped), None);
    assert_eq!(clone.get_handle::<String>(untyped), None);
    assert_eq!(
        original.resolve_product::<String>(selected),
        Ok(selected_handle)
    );
    assert_eq!(
        clone.resolve_product::<String>(selected),
        Ok(selected_handle)
    );
    assert_eq!(original.product(primary_handle.id()), Some(primary));
    assert_eq!(clone.product(primary_handle.id()), Some(primary));
    assert_ne!(original, clone);
}
