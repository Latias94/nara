use nara_ui_kernel::{
    UiKey, UiLocalKey, UiOpaqueId, UiPersistentIdentity, UiProjectionPath, UiProjectionStep,
    UiRootKey,
};

#[test]
fn projected_identity_keeps_source_separate_from_instance_location() {
    let source = UiOpaqueId::new("retry-button").unwrap();
    let document = UiOpaqueId::new("asset:terminal-prefab").unwrap();
    let source_entity = UiOpaqueId::new("terminal/actions/retry").unwrap();

    let first_path = UiProjectionPath::new(UiOpaqueId::new("terminal-instance-a").unwrap())
        .with_step(UiProjectionStep::new(
            document.clone(),
            source_entity.clone(),
        ))
        .unwrap();
    let second_path = UiProjectionPath::new(UiOpaqueId::new("terminal-instance-b").unwrap())
        .with_step(UiProjectionStep::new(document, source_entity))
        .unwrap();

    let first =
        UiRootKey::persistent(UiPersistentIdentity::projected(source.clone(), first_path)).unwrap();
    let second =
        UiRootKey::persistent(UiPersistentIdentity::projected(source, second_path)).unwrap();

    assert_ne!(first, second);
}

#[test]
fn scoped_key_exposes_its_structured_persistent_local_identity() {
    let source = UiOpaqueId::new("retry-button").unwrap();
    let path = UiProjectionPath::new(UiOpaqueId::new("reference-ui").unwrap())
        .with_step(UiProjectionStep::new(
            UiOpaqueId::new("reference-ui.prefab.json").unwrap(),
            source.clone(),
        ))
        .unwrap();
    let identity = UiPersistentIdentity::projected(source, path);
    let local = UiLocalKey::persistent(identity.clone()).unwrap();
    let key = UiKey::scoped(UiRootKey::runtime("reference", "root").unwrap(), local).unwrap();

    assert_eq!(key.persistent_identity(), Some(&identity));
    assert!(
        UiKey::scoped(
            UiRootKey::runtime("reference", "root").unwrap(),
            UiLocalKey::runtime("reference", "runtime").unwrap(),
        )
        .unwrap()
        .persistent_identity()
        .is_none()
    );
}

#[test]
fn exact_scoped_keys_do_not_use_a_digest_or_root_global_namespace() {
    let first_root = UiRootKey::runtime("reference", "hud-a").unwrap();
    let second_root = UiRootKey::runtime("reference", "hud-b").unwrap();
    let local = UiLocalKey::runtime("reference", "score").unwrap();
    let first = UiKey::scoped(first_root, local.clone()).unwrap();
    let second = UiKey::scoped(second_root, local).unwrap();

    assert_ne!(first, second);
    assert_eq!(first, first.clone());
}

#[test]
fn opaque_atoms_preserve_path_like_content_without_parsing_it() {
    let atom = UiOpaqueId::new("anchor/source_entity/health-fill").unwrap();
    assert_eq!(atom.as_str(), "anchor/source_entity/health-fill");
    assert!(UiOpaqueId::new("bad\nkey").is_err());
}

#[test]
fn total_key_size_is_capped_at_256_utf8_bytes() {
    assert!(UiOpaqueId::new("x".repeat(257)).is_err());
    let large = "x".repeat(127);
    assert!(UiRootKey::runtime(large.clone(), large).is_err());
}
