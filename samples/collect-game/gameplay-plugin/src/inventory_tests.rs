#[test]
fn current_inventory_preserves_the_committed_schema_fingerprint_and_canonical_members() {
    let catalog = super::catalog().expect("the current gameplay catalog is valid");
    assert_eq!(
        catalog.fingerprint().to_string(),
        "782244dac503de9fc04e2ad7ad94c9c8a399f005eb6581927867f13e2dd8b4cb"
    );
    assert_eq!(
        catalog
            .components
            .iter()
            .map(|schema| schema.id().as_str())
            .collect::<Vec<_>>(),
        ["collect-game.Collectible", "collect-game.PlayerConfig"]
    );
}
