use std::{
    collections::{BTreeSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

use nara_ui_kernel::{
    UiActionId, UiAuthoringError, UiAuthoringSnapshotBuilder, UiImageId, UiKey, UiKeyError,
    UiLayer, UiLimits, UiLocalKey, UiNode, UiOpaqueId, UiPersistentIdentity, UiRoot, UiRootKey,
    UiStyle,
};

fn identity(context: impl AsRef<[u8]>, placement: impl AsRef<[u8]>) -> UiPersistentIdentity {
    UiPersistentIdentity::new(
        UiOpaqueId::new(context).unwrap(),
        UiOpaqueId::new(placement).unwrap(),
    )
}

#[test]
fn external_image_identity_is_provider_scoped_and_disjoint_from_names() {
    struct ImageProvider;
    struct OtherProvider;
    let image = UiImageId::external::<ImageProvider>(17);
    assert_eq!(image, UiImageId::external::<ImageProvider>(17));
    assert_ne!(image, UiImageId::external::<OtherProvider>(17));
    assert_ne!(image, UiImageId::external::<ImageProvider>(18));
    assert_ne!(image, UiImageId::new(image.to_string()).unwrap());
    assert!(image.as_name().is_none());
    assert_eq!(
        UiImageId::new("product:opaque/primary").unwrap().as_name(),
        Some("product:opaque/primary")
    );
    let mut values = BTreeSet::new();
    values.insert(image.clone());
    values.insert(UiImageId::external::<OtherProvider>(17));
    values.insert(UiImageId::new(image.to_string()).unwrap());
    assert_eq!(values.len(), 3);
}

#[test]
fn complete_root_cost_is_admitted_before_ready_or_rejected_body_validation() {
    let root = UiRootKey::runtime("a", "b").unwrap();
    assert_eq!(UiKey::root(root.clone()).unwrap().encoded_len(), 10);
    for rejected_body in [false, true] {
        let mut authored = UiRoot::new(root.clone(), UiLayer::Base, UiStyle::fill());
        if rejected_body {
            authored = authored.with_child(
                UiNode::new(UiStyle::fill())
                    .with_text(nara_ui_kernel::UiText::new("unkeyed", 16.0).unwrap()),
            );
        }
        let result = UiAuthoringSnapshotBuilder::new(1)
            .with_limits(UiLimits::default().with_max_key_bytes(9).unwrap())
            .with_root(authored.clone())
            .build();
        assert!(matches!(
            result,
            Err(UiAuthoringError::KeyLimitExceeded {
                actual: 10,
                maximum: 9
            })
        ));
        let accepted = UiAuthoringSnapshotBuilder::new(1)
            .with_limits(UiLimits::default().with_max_key_bytes(10).unwrap())
            .with_root(authored)
            .build()
            .unwrap();
        assert_eq!(accepted.roots().len(), usize::from(!rejected_body));
    }
}

#[test]
fn complete_persistent_addresses_keep_context_separate_from_placement() {
    let first =
        UiRootKey::persistent(identity(b"source-a/slot-a", b"anchor-a/source/entity")).unwrap();
    let second =
        UiRootKey::persistent(identity(b"source-b/slot-a", b"anchor-a/source/entity")).unwrap();
    let occurrence =
        UiRootKey::persistent(identity(b"source-a/slot-b", b"anchor-a/source/entity")).unwrap();
    let placement =
        UiRootKey::persistent(identity(b"source-a/slot-a", b"anchor-b/source/entity")).unwrap();
    assert_eq!(
        BTreeSet::from([first, second, occurrence, placement]).len(),
        4
    );
}

#[test]
fn exact_scoped_keys_do_not_use_a_digest_or_root_global_namespace() {
    let first_root = UiRootKey::runtime("reference", "hud-a").unwrap();
    let second_root = UiRootKey::runtime("reference", "hud-b").unwrap();
    let local = UiLocalKey::persistent(identity(b"foreign-source/slot", b"score")).unwrap();
    let first = UiKey::scoped(first_root, local.clone()).unwrap();
    let second = UiKey::scoped(second_root, local).unwrap();
    assert_ne!(first, second);
    assert_eq!(first, first.clone());
    assert_eq!(first.local_key(), second.local_key());
    let reconstructed = UiKey::scoped(
        first.root_key().clone(),
        second.local_key().unwrap().clone(),
    )
    .unwrap();
    assert_eq!(first, reconstructed);
}

#[test]
fn binary_identity_is_distinct_from_text_action_and_image_atoms() {
    let bytes = [0, 9, 10, 255];
    assert_eq!(UiOpaqueId::new(bytes).unwrap().as_bytes(), bytes);
    assert!(UiActionId::new("bad\nkey").is_err());
    assert!(UiImageId::new("bad\nkey").is_err());
    assert!(UiOpaqueId::new([]).is_err());
    assert!(UiOpaqueId::new([0; 257]).is_err());
}

#[test]
fn sharing_independent_construction_and_recomposition_have_identical_value_semantics() {
    let context = UiOpaqueId::new([7; 100]).unwrap();
    let root = UiRootKey::persistent(UiPersistentIdentity::new(
        context.clone(),
        UiOpaqueId::new("root").unwrap(),
    ))
    .unwrap();
    let shared = UiLocalKey::persistent(UiPersistentIdentity::new(
        context,
        UiOpaqueId::new("child").unwrap(),
    ))
    .unwrap();
    let independent = UiLocalKey::persistent(identity([7; 100], "child")).unwrap();
    let first = UiKey::scoped(root.clone(), shared).unwrap();
    let second = UiKey::scoped(root, independent).unwrap();
    let hash = |key: &UiKey| {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    };
    assert_eq!(first, second);
    assert_eq!(first.cmp(&second), std::cmp::Ordering::Equal);
    assert_eq!(hash(&first), hash(&second));
    assert_eq!(first.encoded_len(), second.encoded_len());
}

#[test]
fn canonical_cost_and_reduced_limits_agree_at_255_256_257_bytes() {
    let root = UiRootKey::persistent(identity(b"ctx", [1; 121])).unwrap();
    for (leaf, expected) in [(117, 255), (118, 256), (119, 257)] {
        let local = UiLocalKey::persistent(identity(b"ctx", vec![2; leaf])).unwrap();
        let result = UiKey::scoped(root.clone(), local.clone());
        if expected == 257 {
            assert_eq!(
                result,
                Err(UiKeyError::KeyTooLong {
                    actual: 257,
                    maximum: 256
                })
            );
            continue;
        }
        assert_eq!(result.unwrap().encoded_len(), expected);
        let authored = UiRoot::new(root.clone(), UiLayer::Base, UiStyle::fill())
            .with_child(UiNode::new(UiStyle::fill()).with_key(local));
        for maximum in [expected - 1, expected] {
            let snapshot = UiAuthoringSnapshotBuilder::new(1)
                .with_limits(UiLimits::default().with_max_key_bytes(maximum).unwrap())
                .with_root(authored.clone())
                .build()
                .unwrap();
            assert_eq!(snapshot.roots().len(), usize::from(maximum == expected));
        }
    }
    let large = "x".repeat(127);
    assert!(UiRootKey::runtime(large.clone(), large).is_err());
}

#[test]
fn variable_placement_lengths_preserve_the_exact_shared_prefix_budget() {
    let mut root_placement = vec![7; 120];
    root_placement.extend_from_slice(b"root");
    let mut child_placement = vec![7; 120];
    child_placement.extend_from_slice(b"label");
    let root = UiRootKey::persistent(identity(b"ctx", root_placement)).unwrap();
    let local = UiLocalKey::persistent(identity(b"ctx", child_placement)).unwrap();
    let key = UiKey::scoped(root, local)
        .expect("a varying leaf length must not repeat the entire shared path");
    assert_eq!(key.encoded_len(), 146);
}
