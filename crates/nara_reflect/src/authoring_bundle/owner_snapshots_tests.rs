use std::cell::Cell;

use super::*;
use crate::{
    ComponentCatalogFileBudgetKind, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaVersion, ComponentValueKind, ComponentValueShape,
};

fn owner_id(name: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(name).unwrap()
}

fn component(id: &str, version: u32) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new(id),
        "Value",
        ComponentSchemaVersion::new(version).unwrap(),
    )
}

fn snapshot(name: &str, catalog: ComponentSchemaCatalog) -> ComponentSchemaOwnerSnapshot {
    ComponentSchemaOwnerSnapshot::admit(owner_id(name), catalog, None).unwrap()
}

fn basic_snapshot(name: &str, id: &str) -> ComponentSchemaOwnerSnapshot {
    snapshot(
        name,
        ComponentSchemaCatalog {
            components: vec![component(id, 1)],
            ..ComponentSchemaCatalog::default()
        },
    )
}

fn evolved_snapshot() -> ComponentSchemaOwnerSnapshot {
    let previous = ComponentSchemaCatalog {
        components: vec![component("test.Retired", 1), component("test.Evolved", 1)],
        ..ComponentSchemaCatalog::default()
    };
    let mut current = ComponentSchemaCatalog::successor_of(&previous).unwrap();
    current.components = vec![component("test.Evolved", 2), component("test.Added", 1)];
    current
        .type_tombstones
        .push(ComponentTypeId::new("test.Retired"));
    ComponentSchemaOwnerSnapshot::admit(owner_id("test.evolved"), current, Some(previous)).unwrap()
}

#[test]
fn snapshots_preserve_authority_and_match_raw_and_wire_content() {
    let first = basic_snapshot("test.first", "test.First");
    let evolved = evolved_snapshot();
    let snapshots = [first.clone(), evolved.clone()];
    let bundle = AuthoringSchemaBundle::from_owner_snapshots(snapshots.clone()).unwrap();
    assert_eq!(bundle.owners()[0].owner_id(), evolved.owner_id());
    for original in &snapshots {
        let retained = bundle.owner(original.owner_id()).unwrap();
        assert!(retained.ptr_eq(original));
        assert!(std::ptr::eq(retained.current(), original.current()));
        match (retained.predecessor(), original.predecessor()) {
            (Some(retained), Some(original)) => assert!(std::ptr::eq(retained, original)),
            (None, None) => {}
            _ => panic!("owner predecessor changed"),
        }
    }
    let raw =
        AuthoringSchemaBundle::new(snapshots.iter().map(AuthoringSchemaBundleOwner::from)).unwrap();
    assert_eq!(bundle, raw);
    assert_eq!(bundle.fingerprint(), raw.fingerprint());
    assert_eq!(
        bundle.to_json_string().unwrap(),
        raw.to_json_string().unwrap()
    );
    let decoded = AuthoringSchemaBundle::from_json_str(&bundle.to_json_string().unwrap()).unwrap();
    assert_eq!(decoded, bundle);
    assert_eq!(decoded.fingerprint(), bundle.fingerprint());
    for original in &snapshots {
        assert!(!raw.owner(original.owner_id()).unwrap().ptr_eq(original));
        assert!(!decoded.owner(original.owner_id()).unwrap().ptr_eq(original));
        // Neither independent admission path has replaced the original bundle's authority.
        assert!(bundle.owner(original.owner_id()).unwrap().ptr_eq(original));
    }
}

#[test]
fn empty_snapshot_composition_matches_the_existing_empty_bundle() {
    let bundle = AuthoringSchemaBundle::from_owner_snapshots([]).unwrap();
    assert!(bundle.owners().is_empty());
    assert_eq!(bundle, AuthoringSchemaBundle::new([]).unwrap());
    assert_eq!(
        AuthoringSchemaBundle::from_json_str(&bundle.to_json_string().unwrap()).unwrap(),
        bundle
    );
}

#[test]
fn raw_candidates_still_canonicalize_and_wire_inputs_still_require_canonical_order() {
    let evolved = evolved_snapshot();
    let first = basic_snapshot("test.first", "test.First");
    let bundle =
        AuthoringSchemaBundle::from_owner_snapshots([first.clone(), evolved.clone()]).unwrap();
    let mut raw_evolved = AuthoringSchemaBundleOwner::from(&evolved);
    raw_evolved.current.components.reverse();
    assert_eq!(
        AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::from(&first), raw_evolved,])
            .unwrap(),
        bundle
    );

    let encoded = bundle.to_json_string().unwrap();
    let mut wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    wire["payload"]["owners"].as_array_mut().unwrap().reverse();
    assert!(matches!(
        AuthoringSchemaBundle::from_json_str(&wire.to_string()),
        Err(AuthoringSchemaBundleError::NonCanonicalOwnerOrder { .. })
    ));

    let mut wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    wire["payload"]["owners"][0]["current"]["components"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert!(matches!(
        AuthoringSchemaBundle::from_json_str(&wire.to_string()),
        Err(AuthoringSchemaBundleError::NonCanonicalCatalog { .. })
    ));
}

#[test]
fn duplicate_owner_rejects_both_shared_and_independently_admitted_authorities() {
    let original = basic_snapshot("test.owner", "test.First");
    let independent = basic_snapshot("test.owner", "test.Second");
    assert!(!original.ptr_eq(&independent));
    for duplicate in [original.clone(), independent] {
        assert!(matches!(
            AuthoringSchemaBundle::from_owner_snapshots([original.clone(), duplicate]),
            Err(AuthoringSchemaBundleError::DuplicateOwner { owner }) if owner == *original.owner_id()
        ));
    }
}

#[derive(Clone, Copy, Debug)]
enum Claim {
    Current,
    PredecessorComponent,
    CurrentTombstone,
    PredecessorTombstone,
}

fn claimed_snapshot(name: &str, claim: Claim) -> ComponentSchemaOwnerSnapshot {
    let shared = ComponentTypeId::new("test.Shared");
    let mut previous = ComponentSchemaCatalog::default();
    match claim {
        Claim::Current | Claim::PredecessorComponent => {
            previous.components.push(component(shared.as_str(), 1))
        }
        Claim::CurrentTombstone | Claim::PredecessorTombstone => {
            previous.type_tombstones.push(shared.clone())
        }
    }
    match claim {
        Claim::Current | Claim::CurrentTombstone => snapshot(name, previous),
        Claim::PredecessorComponent | Claim::PredecessorTombstone => {
            let mut current = ComponentSchemaCatalog::successor_of(&previous).unwrap();
            if matches!(claim, Claim::PredecessorComponent) {
                current.type_tombstones.push(shared);
            }
            ComponentSchemaOwnerSnapshot::admit(owner_id(name), current, Some(previous)).unwrap()
        }
    }
}

#[test]
fn all_current_predecessor_and_tombstone_claim_combinations_share_the_ownership_validator() {
    let claims = [
        Claim::Current,
        Claim::PredecessorComponent,
        Claim::CurrentTombstone,
        Claim::PredecessorTombstone,
    ];
    for first_claim in claims {
        for second_claim in claims {
            let first = claimed_snapshot("test.first", first_claim);
            let second = claimed_snapshot("test.second", second_claim);
            let raw_error = AuthoringSchemaBundle::new([
                AuthoringSchemaBundleOwner::from(&first),
                AuthoringSchemaBundleOwner::from(&second),
            ])
            .unwrap_err();
            let error = AuthoringSchemaBundle::from_owner_snapshots([second, first]).unwrap_err();
            match (error, raw_error) {
                (
                    AuthoringSchemaBundleError::DivergentComponentOwnership {
                        component_id,
                        first_owner,
                        second_owner,
                    },
                    AuthoringSchemaBundleError::DivergentComponentOwnership {
                        component_id: raw_component,
                        first_owner: raw_first,
                        second_owner: raw_second,
                    },
                ) => {
                    assert_eq!(component_id, ComponentTypeId::new("test.Shared"));
                    assert_eq!(first_owner, owner_id("test.first"));
                    assert_eq!(second_owner, owner_id("test.second"));
                    assert_eq!(
                        (component_id, first_owner, second_owner),
                        (raw_component, raw_first, raw_second)
                    );
                }
                errors => {
                    panic!("unexpected errors for {first_claim:?}/{second_claim:?}: {errors:?}")
                }
            }
        }
    }
}

struct UntrustedOwnerIterator<'a> {
    owner: ComponentSchemaOwnerSnapshot,
    maximum_calls: usize,
    calls: &'a Cell<usize>,
}

impl Iterator for UntrustedOwnerIterator<'_> {
    type Item = ComponentSchemaOwnerSnapshot;

    fn next(&mut self) -> Option<Self::Item> {
        let calls = self.calls.get() + 1;
        assert!(
            calls <= self.maximum_calls,
            "iterator consumed after owner-count refusal"
        );
        self.calls.set(calls);
        Some(self.owner.clone())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        panic!("owner composition must not trust the input size hint")
    }
}

#[test]
fn public_constructor_bounds_collection_without_consulting_size_hint() {
    let maximum = AuthoringSchemaBundleLimits::default().owners().get();
    let calls = Cell::new(0);
    let owners = UntrustedOwnerIterator {
        owner: snapshot("test.empty", ComponentSchemaCatalog::default()),
        maximum_calls: maximum + 1,
        calls: &calls,
    };
    assert!(
        matches!(AuthoringSchemaBundle::from_owner_snapshots(owners),
        Err(AuthoringSchemaBundleError::OwnerLimitExceeded { observed, maximum: actual })
            if observed == maximum + 1 && actual == maximum)
    );
    assert_eq!(calls.get(), maximum + 1);
}

#[test]
fn snapshot_count_is_checked_before_the_next_owners_catalog_budget() {
    let first = basic_snapshot("test.first", "test.First");
    let second = basic_snapshot("test.second", "test.Second");
    let limits = AuthoringSchemaBundleLimits::default()
        .with_owners(ItemLimit::ONE)
        .with_catalogs(ComponentCatalogFileLimits::default().with_components(ItemLimit::ONE));
    assert!(matches!(
        bundle_from_owner_snapshots([first, second], limits),
        Err(AuthoringSchemaBundleError::OwnerLimitExceeded {
            observed: 2,
            maximum: 1
        })
    ));
}

#[test]
fn composed_catalog_budget_spans_snapshot_owners_and_their_predecessors() {
    let limits = AuthoringSchemaBundleLimits::default()
        .with_catalogs(ComponentCatalogFileLimits::default().with_components(ItemLimit::ONE));
    let first = basic_snapshot("test.first", "test.First");
    let second = basic_snapshot("test.second", "test.Second");
    let previous = first.current().clone();
    let mut current = ComponentSchemaCatalog::successor_of(&previous).unwrap();
    current.components = previous.components.clone();
    let evolved =
        ComponentSchemaOwnerSnapshot::admit(owner_id("test.evolved"), current, Some(previous))
            .unwrap();
    for owners in [vec![first, second], vec![evolved]] {
        assert!(matches!(bundle_from_owner_snapshots(owners, limits),
            Err(AuthoringSchemaBundleError::Budget(error))
                if error.kind == ComponentCatalogFileBudgetKind::Components
                    && error.observed == 2 && error.maximum == 1));
    }
}

#[test]
fn aggregate_field_shape_alias_and_tombstone_limits_are_not_bypassed_by_admitted_snapshots() {
    let with_field = |owner: &str, id: &str| {
        let mut schema = component(id, 1);
        schema.fields.push(ComponentFieldSchema::required(
            ComponentFieldId::new("field"),
            "Field",
            ComponentFieldPath::from_fields(["field"]),
            ComponentValueKind::Bool,
        ));
        snapshot(
            owner,
            ComponentSchemaCatalog {
                components: vec![schema],
                ..ComponentSchemaCatalog::default()
            },
        )
    };
    let owners = [
        with_field("test.first", "test.First"),
        with_field("test.second", "test.Second"),
    ];
    for (catalogs, kind, observed) in [
        (
            ComponentCatalogFileLimits::default().with_fields(ItemLimit::ONE),
            ComponentCatalogFileBudgetKind::Fields,
            2,
        ),
        (
            ComponentCatalogFileLimits::default().with_aliases(ItemLimit::ONE),
            ComponentCatalogFileBudgetKind::Aliases,
            2,
        ),
    ] {
        assert!(matches!(bundle_from_owner_snapshots(owners.clone(),
            AuthoringSchemaBundleLimits::default().with_catalogs(catalogs)),
            Err(AuthoringSchemaBundleError::Budget(error))
                if error.kind == kind && error.observed == observed && error.maximum == 1));
    }
    let mut catalog = owners[0].current().clone();
    catalog.components[0].fields[0].shape =
        ComponentValueShape::list(ComponentValueShape::scalar(ComponentValueKind::Bool));
    let nested = snapshot("test.nested", catalog);
    assert!(
        matches!(bundle_from_owner_snapshots([nested], AuthoringSchemaBundleLimits::default()
        .with_catalogs(ComponentCatalogFileLimits::default().with_fields(ItemLimit::ONE))),
        Err(AuthoringSchemaBundleError::Budget(error))
            if error.kind == ComponentCatalogFileBudgetKind::ShapeNodes
                && error.observed == 2 && error.maximum == 1)
    );
    assert!(
        matches!(bundle_from_owner_snapshots([owners[0].clone()], AuthoringSchemaBundleLimits::default()
        .with_catalogs(ComponentCatalogFileLimits::default().with_diagnostic_sources(ItemLimit::ONE))),
        Err(AuthoringSchemaBundleError::Budget(error))
            if error.kind == ComponentCatalogFileBudgetKind::DiagnosticSources
                && error.observed == 2 && error.maximum == 1)
    );
    let tombstone = claimed_snapshot("test.retired", Claim::PredecessorTombstone);
    assert!(
        matches!(bundle_from_owner_snapshots([tombstone], AuthoringSchemaBundleLimits::default()
        .with_catalogs(ComponentCatalogFileLimits::default().with_tombstones(ItemLimit::ONE))),
        Err(AuthoringSchemaBundleError::Budget(error))
            if error.kind == ComponentCatalogFileBudgetKind::Tombstones
                && error.observed == 2 && error.maximum == 1)
    );
}
