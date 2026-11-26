use std::sync::Arc;

use nara_asset::{
    AssetRecord, AssetSourceKind, ImportDependencyDigest, ImportRecipe, ImporterId,
    ImporterVersion, SourceHash, StableAssetId,
};

use super::*;
use crate::image::{
    ImageColorSpace, ImageExtent, ImageFormat, ImageImportSettings, ImageSourceMetadata,
};

const SOURCE: &str = "aed61cf0-6e45-4078-8793-1fa45c794fd3";
const FOREIGN: &str = "c93d5412-de9f-41fe-913d-369e5f8b9f38";
const AUXILIARY: &str = "70edaf59-91c2-450d-96eb-7a31094b92c7";

fn product(source: &str, auxiliary: bool) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(source).unwrap(),
        if auxiliary {
            ImportedProductId::parse_subproduct(AUXILIARY).unwrap()
        } else {
            ImportedProductId::PRIMARY
        },
    )
    .unwrap()
}

fn image(product: AssetProductRef, color: u8) -> ImageAsset {
    ImageAsset::from_product(
        product,
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        vec![color, 0, 0, 255],
    )
    .unwrap()
    .share_with_retention_owner(Arc::new(()))
}

fn imported(record: &AssetRecord, color: u8) -> ImageAsset {
    imported_with_source(record, color, color)
}

fn imported_with_source(record: &AssetRecord, color: u8, source_byte: u8) -> ImageAsset {
    let settings = ImageImportSettings::default();
    let recipe = ImportRecipe::new(
        record.stable_id(),
        SourceHash::from_bytes(&[source_byte]),
        ImportDependencyDigest::empty(),
        ImporterId::new("test.image").unwrap(),
        ImporterVersion::new(1),
        settings.settings_hash(),
    );
    let source = ImageSourceMetadata::new(record.path().clone(), settings, recipe).unwrap();
    ImageAsset::from_import(
        source,
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        vec![color, 0, 0, 255],
    )
    .unwrap()
    .share_with_retention_owner(Arc::new(()))
}

#[derive(Default)]
struct Publications {
    server: AssetServer,
    images: Assets<ImageAsset>,
    states: AssetStates,
    events: AssetEvents,
}

impl Publications {
    fn input(&mut self, image: ImageAsset) -> ImagePublicationInput {
        let product = image.product().unwrap();
        let handle = self.server.reserve_product::<ImageAsset>(product).unwrap();
        ImagePublicationInput::new(handle, product, None, image)
    }

    fn prepare(
        &self,
        inputs: Vec<ImagePublicationInput>,
        owned: &[ImageContentReceipt],
        mode: ImagePublicationMode,
    ) -> Result<PreparedImagePublications, ImagePublicationError> {
        PreparedImagePublications::prepare(
            inputs,
            owned.iter(),
            mode,
            &self.server,
            &self.server,
            &self.images,
            &self.states,
        )
    }

    fn commit(
        &mut self,
        prepared: PreparedImagePublications,
    ) -> Result<Vec<ImageContentReceipt>, ImagePublicationError> {
        prepared.commit(
            &self.server,
            &mut self.images,
            &mut self.states,
            &mut self.events,
        )
    }

    fn publish(&mut self, image: ImageAsset) -> Vec<ImageContentReceipt> {
        let input = self.input(image);
        let prepared = self
            .prepare(vec![input], &[], ImagePublicationMode::Add)
            .unwrap();
        self.commit(prepared).unwrap()
    }
}

#[test]
fn product_index_preserves_primary_auxiliary_and_enclosing_source() {
    let mut values = Publications::default();
    let auxiliary = values.input(image(product(SOURCE, true), 1));
    let foreign = values.input(image(product(FOREIGN, true), 2));
    let primary = values.input(image(product(SOURCE, false), 3));
    let mismatched = ImagePublicationInput::new(
        primary.binding.handle,
        product(SOURCE, false),
        None,
        image(product(SOURCE, true), 3),
    );
    assert!(matches!(
        values.prepare(vec![mismatched], &[], ImagePublicationMode::Add),
        Err(ImagePublicationError::IdentityMismatch)
    ));
    let prepared = values
        .prepare(
            vec![auxiliary, foreign, primary],
            &[],
            ImagePublicationMode::Add,
        )
        .unwrap();
    for (product, color) in [
        (product(SOURCE, true), 1),
        (product(FOREIGN, true), 2),
        (product(SOURCE, false), 3),
    ] {
        let (handle, image) = prepared
            .retained_image_for_product(product, &values.images)
            .unwrap();
        assert_eq!(prepared.handle_for_product(product), Some(handle));
        assert_eq!(image.product(), Some(product));
        assert_eq!(image.pixels()[0], color);
    }
    assert!(
        prepared
            .handle_for_product(product(FOREIGN, false))
            .is_none()
    );
    assert_eq!(values.commit(prepared).unwrap().len(), 3);
}

#[test]
fn source_relocation_preserves_product_handle_and_publishes_actual_provenance() {
    let mut values = Publications::default();
    let product = product(FOREIGN, false);
    let original_path = AssetSourcePath::new("game/art/source.png").unwrap();
    let record = AssetRecord::new(
        product.source(),
        original_path.clone(),
        AssetSourceKind::Image,
    );
    let handle = values.server.reserve_record(&record).unwrap();
    let input = ImagePublicationInput::new(
        handle,
        product,
        Some(original_path.clone()),
        imported(&record, 1),
    );
    let initial = values
        .prepare(vec![input], &[], ImagePublicationMode::Add)
        .unwrap();
    let original = values.commit(initial).unwrap();
    assert_eq!(
        original[0].binding.source_path.as_ref(),
        Some(&original_path)
    );

    let path = AssetSourcePath::new("game/art/renamed.png").unwrap();
    let record = AssetRecord::new(product.source(), path.clone(), AssetSourceKind::Image);
    let mut staged_server = values.server.clone();
    staged_server
        .relocate_primary_source(&original_path, &record)
        .unwrap();
    let previous_image = values.images.get(handle).unwrap().share_retained().unwrap();
    let stale_input =
        ImagePublicationInput::new(handle, product, Some(path.clone()), previous_image);
    assert!(matches!(
        PreparedImagePublications::prepare(
            vec![stale_input],
            original.iter(),
            ImagePublicationMode::Replace,
            &values.server,
            &staged_server,
            &values.images,
            &values.states,
        ),
        Err(ImagePublicationError::IdentityMismatch)
    ));
    let image = imported(&record, 2);
    let source = image.source().unwrap().clone();
    let input = ImagePublicationInput::new(handle, product, Some(path.clone()), image);
    let prepared = PreparedImagePublications::prepare(
        vec![input],
        original.iter(),
        ImagePublicationMode::Replace,
        &values.server,
        &staged_server,
        &values.images,
        &values.states,
    )
    .unwrap();
    prepared
        .validate(&values.server, &values.images, &values.states)
        .unwrap();
    assert_eq!(prepared.handle_for_product(product), Some(handle));
    assert!(prepared.retiring_products().next().is_none());
    values.server = staged_server;
    let replacement = values.commit(prepared).unwrap();
    assert_eq!(replacement[0].binding.source_path.as_ref(), Some(&path));
    assert_eq!(values.server.path(handle.id()), Some(&path));
    assert!(!original[0].matches(&values.server, &values.images, &values.states));
    let state = values.states.state(handle.id()).unwrap();
    assert_eq!(state.source_hash(), Some(source.source_hash()));
    assert_eq!(
        state.import_recipe_digest(),
        Some(source.import_recipe().digest())
    );
}

#[test]
fn pathless_publication_keeps_complete_identity_without_source_hashes() {
    let mut values = Publications::default();
    let receipts = values.publish(image(product(SOURCE, true), 1));
    let receipt = &receipts[0];
    assert_eq!(receipt.product(), product(SOURCE, true));
    assert!(receipt.binding.source_path.is_none());
    assert!(values.server.path(receipt.handle().id()).is_none());
    assert!(receipt.matches(&values.server, &values.images, &values.states));
    let state = values.states.state(receipt.handle().id()).unwrap();
    assert!(state.source_hash().is_none());
    assert!(state.import_recipe_digest().is_none());
}

#[test]
fn pending_publication_refuses_new_value_and_state_owners_even_for_empty_slots() {
    let mut values = Publications::default();
    let input = values.input(image(product(SOURCE, true), 1));
    let prepared = values
        .prepare(vec![input], &[], ImagePublicationMode::Add)
        .unwrap();
    assert!(
        prepared
            .validate(&values.server, &values.images, &values.states)
            .is_ok()
    );
    assert_eq!(
        prepared.validate(&values.server, &Assets::default(), &values.states),
        Err(ImagePublicationError::Superseded)
    );
    assert_eq!(
        prepared.validate(&values.server, &values.images, &AssetStates::default()),
        Err(ImagePublicationError::Superseded)
    );
}

#[test]
fn owned_receipt_follows_the_rust_store_and_rejects_same_value_mutation() {
    let mut values = Publications::default();
    let receipts = values.publish(image(product(SOURCE, true), 1));
    let receipt = &receipts[0];
    let mut moved = std::mem::take(&mut values.images);
    assert!(receipt.matches(&values.server, &moved, &values.states));
    assert!(!receipt.matches(&values.server, &values.images, &values.states));
    assert!(moved.get_mut(receipt.handle()).is_some());
    assert!(!receipt.matches(&values.server, &moved, &values.states));
}

#[test]
fn state_only_changes_invalidate_owned_image_and_retirement() {
    let mut values = Publications::default();
    let owned = values.publish(image(product(SOURCE, true), 1));
    let retirement = values
        .prepare(Vec::new(), &owned, ImagePublicationMode::Replace)
        .unwrap();
    retirement
        .validate(&values.server, &values.images, &values.states)
        .unwrap();
    values.states.set_loading(owned[0].handle().id());
    assert_eq!(
        retirement.validate(&values.server, &values.images, &values.states),
        Err(ImagePublicationError::Superseded)
    );
}

#[test]
fn additive_retirement_uses_the_last_good_image_after_reload_failure() {
    let mut values = Publications::default();
    let owned = values.publish(image(product(SOURCE, true), 1));
    let handle = owned[0].handle();
    values
        .images
        .record_reload_failure(
            handle,
            &mut values.states,
            &mut values.events,
            "invalid replacement",
        )
        .unwrap();
    assert_eq!(values.images.get(handle).unwrap().pixels(), [1, 0, 0, 255]);
    let retirement = values
        .prepare(Vec::new(), &owned, ImagePublicationMode::Replace)
        .unwrap();
    assert!(values.commit(retirement).unwrap().is_empty());
    assert!(values.images.get(handle).is_none());
    assert!(matches!(
        values.states.state(handle.id()).unwrap().load_state(),
        LoadState::Removed
    ));
}

#[test]
fn prepared_retirement_keeps_the_last_good_publication_after_a_failed_reload() {
    let mut values = Publications::default();
    let owned = values.publish(image(product(SOURCE, true), 1));
    let handle = owned[0].handle();
    let retirement = values
        .prepare(Vec::new(), &owned, ImagePublicationMode::Replace)
        .unwrap();
    values
        .images
        .record_reload_failure(
            handle,
            &mut values.states,
            &mut values.events,
            "invalid replacement while retirement is pending",
        )
        .unwrap();
    assert_eq!(values.images.get(handle).unwrap().pixels(), [1, 0, 0, 255]);
    retirement
        .validate(&values.server, &values.images, &values.states)
        .unwrap();
    assert!(values.commit(retirement).unwrap().is_empty());
    assert!(values.images.get(handle).is_none());
    assert!(matches!(
        values.states.state(handle.id()).unwrap().load_state(),
        LoadState::Removed
    ));
}

#[test]
fn escaped_pixels_retain_the_exact_owner_after_publication_and_retirement() {
    let mut values = Publications::default();
    let owner = Arc::new(());
    let weak = Arc::downgrade(&owner);
    let shared = image(product(SOURCE, true), 1).share_with_retention_owner(Arc::clone(&owner));
    drop(owner);
    let receipts = values.publish(shared);
    let escaped = values
        .images
        .get(receipts[0].handle())
        .unwrap()
        .share_retained()
        .unwrap();
    let retirement = values
        .prepare(Vec::new(), &receipts, ImagePublicationMode::Replace)
        .unwrap();
    assert!(values.commit(retirement).unwrap().is_empty());
    assert!(values.images.is_empty());
    assert!(weak.upgrade().is_some());
    assert_eq!(escaped.pixels(), &[1, 0, 0, 255]);
    drop(escaped);
    assert!(weak.upgrade().is_none());
}

#[test]
fn additive_reuse_keeps_exact_allocation_and_rejects_changed_values_or_store() {
    let mut values = Publications::default();
    let product = product(SOURCE, true);
    let owned = values.publish(image(product, 1));
    let handle = owned[0].handle();
    let version = values.states.version(handle.id());
    let event_count = values.events.len();

    let changed = values.input(image(product, 2));
    assert!(matches!(
        values.prepare(vec![changed], &owned, ImagePublicationMode::Add),
        Err(ImagePublicationError::Superseded)
    ));
    let same = values.input(image(product, 1));
    let prepared = values
        .prepare(vec![same], &owned, ImagePublicationMode::Add)
        .unwrap();
    assert_eq!(prepared.additive_products(), vec![product]);
    let (_, reused) = prepared
        .retained_image_for_product(product, &values.images)
        .unwrap();
    assert!(reused.same_allocation(values.images.get(handle).unwrap()));
    assert!(values.commit(prepared).unwrap().is_empty());
    assert_eq!(values.events.len(), event_count);
    assert_eq!(values.states.version(handle.id()), version);

    let same = values.input(image(product, 1));
    let prepared = values
        .prepare(vec![same], &owned, ImagePublicationMode::Add)
        .unwrap();
    assert!(values.images.get_mut(handle).is_some());
    assert!(
        prepared
            .retained_image_for_product(product, &values.images)
            .is_none()
    );
    assert!(matches!(
        values.commit(prepared),
        Err(ImagePublicationError::Superseded)
    ));
}

#[test]
fn additive_reuse_rejects_changed_import_provenance_with_identical_pixels() {
    let mut values = Publications::default();
    let product = product(SOURCE, false);
    let path = AssetSourcePath::new("game/art/atlas.png").unwrap();
    let record = AssetRecord::new(product.source(), path.clone(), AssetSourceKind::Image);
    let handle = values.server.reserve_record(&record).unwrap();
    let original = imported_with_source(&record, 1, 10);
    let changed = imported_with_source(&record, 1, 11);
    assert_eq!(original.content_hash(), changed.content_hash());
    let source = original.source().unwrap().clone();
    let input = ImagePublicationInput::new(handle, product, Some(path.clone()), original);
    let initial = values
        .prepare(vec![input], &[], ImagePublicationMode::Add)
        .unwrap();
    let owned = values.commit(initial).unwrap();
    let version = values.states.version(handle.id());
    let event_count = values.events.len();

    let input = ImagePublicationInput::new(handle, product, Some(path), changed);
    assert!(matches!(
        values.prepare(vec![input], &owned, ImagePublicationMode::Add),
        Err(ImagePublicationError::Superseded)
    ));
    assert_eq!(values.images.get(handle).unwrap().source(), Some(&source));
    assert_eq!(values.states.version(handle.id()), version);
    assert_eq!(values.events.len(), event_count);
}

#[test]
fn rejection_of_a_stale_binding_precedes_every_publication_and_retirement() {
    let mut values = Publications::default();
    let owned = values.publish(image(product(SOURCE, true), 1));
    let original_handle = owned[0].handle();
    let event_count = values.events.len();
    let live_server = values.server.clone();
    let next = values.input(image(product(FOREIGN, true), 2));
    let next_handle = next.binding.handle;
    let prepared = PreparedImagePublications::prepare(
        vec![next],
        owned.iter(),
        ImagePublicationMode::Replace,
        &live_server,
        &values.server,
        &values.images,
        &values.states,
    )
    .unwrap();
    assert_eq!(
        prepared.retiring_products().collect::<Vec<_>>(),
        [owned[0].product()]
    );
    values.server = live_server;
    assert!(matches!(
        values.commit(prepared),
        Err(ImagePublicationError::Superseded)
    ));
    assert_eq!(values.events.len(), event_count);
    assert!(values.states.state(next_handle.id()).is_none());
    assert!(values.images.get(next_handle).is_none());
    assert_eq!(values.images.get(original_handle).unwrap().pixels()[0], 1);
}

#[test]
fn duplicate_destinations_or_retirements_fail_during_preparation() {
    let mut values = Publications::default();
    let product = product(SOURCE, true);
    let first = values.input(image(product, 1));
    let duplicate = values.input(image(product, 2));
    assert!(matches!(
        values.prepare(vec![first, duplicate], &[], ImagePublicationMode::Add),
        Err(ImagePublicationError::IdentityMismatch)
    ));
    assert!(values.images.is_empty());
    assert!(values.events.is_empty());

    let owned = values.publish(image(product, 1));
    let receipts = [owned[0].clone(), owned[0].clone()];
    assert!(matches!(
        values.prepare(Vec::new(), &receipts, ImagePublicationMode::Replace),
        Err(ImagePublicationError::IdentityMismatch)
    ));
    assert_eq!(values.images.len(), 1);
    assert_eq!(values.events.len(), 1);
}
