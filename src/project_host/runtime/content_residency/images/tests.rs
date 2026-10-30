use super::*;

use nara_asset::{AssetSourcePath, StableAssetId};
use nara_image::{ImageColorSpace, ImageExtent, ImageFormat};

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

#[derive(Default)]
struct Publications {
    server: AssetServer,
    images: Assets<ImageAsset>,
    states: AssetStates,
    events: AssetEvents,
}

impl Publications {
    fn prepare(&mut self, image: ImageAsset, record: Option<AssetRecord>) -> ImagePublications {
        let product = image.product().unwrap();
        let handle = match &record {
            Some(record) => self.server.reserve_record::<ImageAsset>(record).unwrap(),
            None => self.server.reserve_product::<ImageAsset>(product).unwrap(),
        };
        self.states.next_version(handle.id()).unwrap();
        ImagePublications {
            publish: BTreeMap::from([(
                product,
                ImagePublication {
                    handle,
                    product,
                    record,
                    image,
                    baseline: self.images.prepare_mutation(handle, &self.states).unwrap(),
                },
            )]),
            retire: Vec::new(),
        }
    }

    fn commit(&mut self, prepared: ImagePublications) -> Vec<OwnedImage> {
        prepared.commit(
            &self.server,
            &mut self.images,
            &mut self.states,
            &mut self.events,
        )
    }
}

#[test]
fn product_index_preserves_primary_auxiliary_and_enclosing_source() {
    let mut values = Publications::default();
    let auxiliary = values.prepare(image(product(SOURCE, true), 1), None);
    let foreign = values.prepare(image(product(FOREIGN, true), 2), None);
    let mut prepared = auxiliary;
    prepared.publish.extend(foreign.publish);
    assert_eq!(
        prepared
            .image_for_product(product(SOURCE, true))
            .unwrap()
            .1
            .pixels()[0],
        1
    );
    assert_eq!(
        prepared
            .image_for_product(product(FOREIGN, true))
            .unwrap()
            .1
            .pixels()[0],
        2
    );
    assert!(prepared.image_for_product(product(SOURCE, false)).is_none());
    let primary = values.prepare(image(product(SOURCE, false), 3), None);
    prepared.publish.extend(primary.publish);
    assert_eq!(
        prepared
            .image_for_product(product(SOURCE, false))
            .unwrap()
            .1
            .pixels()[0],
        3
    );
    assert_eq!(prepared.publish.len(), 3);
}

#[test]
fn source_publication_keeps_its_actual_canonical_record() {
    let mut values = Publications::default();
    let mut prepared = values.prepare(image(product(SOURCE, true), 1), None);
    let path = AssetSourcePath::new("game/art/source.png").unwrap();
    let record = AssetRecord::new(
        product(FOREIGN, false).source(),
        path.clone(),
        AssetSourceKind::Image,
    );
    let source = values.prepare(image(product(FOREIGN, false), 2), Some(record));
    prepared.publish.extend(source.publish);
    let (handle, image) = prepared.image_for_product(product(FOREIGN, false)).unwrap();
    assert_eq!(image.product(), Some(product(FOREIGN, false)));
    assert_eq!(values.server.path(handle.id()), Some(&path));
    assert_eq!(
        prepared.publish[&product(FOREIGN, false)]
            .record
            .as_ref()
            .unwrap()
            .path(),
        &path
    );
    assert!(prepared.image_for_product(product(FOREIGN, true)).is_none());
}

#[test]
fn pathless_publication_keeps_complete_identity_without_source_hashes() {
    let mut values = Publications::default();
    let prepared = values.prepare(image(product(SOURCE, true), 1), None);
    let receipts = values.commit(prepared);
    let receipt = &receipts[0];
    assert_eq!(receipt.product, product(SOURCE, true));
    assert!(receipt.record.is_none());
    assert!(values.server.path(receipt.handle.id()).is_none());
    assert!(receipt.matches(&values.server, &values.images, &values.states));
    let state = values.states.state(receipt.handle.id()).unwrap();
    assert!(state.source_hash().is_none());
    assert!(state.import_hash().is_none());
}

#[test]
fn pending_publication_refuses_new_value_and_state_owners_even_for_empty_slots() {
    let mut values = Publications::default();
    let prepared = values.prepare(image(product(SOURCE, true), 1), None);
    assert!(
        prepared
            .validate(&values.server, &values.images, &values.states)
            .is_ok()
    );
    assert!(
        prepared
            .validate(&values.server, &Assets::default(), &values.states)
            .is_err()
    );
    assert!(
        prepared
            .validate(&values.server, &values.images, &AssetStates::default())
            .is_err()
    );
}

#[test]
fn owned_receipt_follows_the_rust_store_and_rejects_same_value_mutation() {
    let mut values = Publications::default();
    let prepared = values.prepare(image(product(SOURCE, true), 1), None);
    let receipts = values.commit(prepared);
    let receipt = &receipts[0];
    let mut moved = std::mem::take(&mut values.images);
    assert!(receipt.matches(&values.server, &moved, &values.states));
    assert!(!receipt.matches(&values.server, &values.images, &values.states));
    assert!(moved.get_mut(receipt.handle).is_some());
    assert!(!receipt.matches(&values.server, &moved, &values.states));
}

#[test]
fn state_only_changes_invalidate_owned_image_and_retirement() {
    let mut values = Publications::default();
    let prepared = values.prepare(image(product(SOURCE, true), 1), None);
    let receipts = values.commit(prepared);
    let handle = receipts[0].handle;
    let retirement = ImagePublications {
        publish: BTreeMap::new(),
        retire: receipts,
    };
    assert!(
        retirement
            .validate(&values.server, &values.images, &values.states)
            .is_ok()
    );
    values.states.set_loading(handle.id());
    assert!(
        retirement
            .validate(&values.server, &values.images, &values.states)
            .is_err()
    );
}

#[test]
fn escaped_pixels_retain_the_exact_owner_after_publication_and_retirement() {
    let mut values = Publications::default();
    let owner = Arc::new(());
    let weak = Arc::downgrade(&owner);
    let shared = image(product(SOURCE, true), 1).share_with_retention_owner(Arc::clone(&owner));
    let prepared = values.prepare(shared, None);
    drop(owner);
    let receipts = values.commit(prepared);
    let escaped = values
        .images
        .get(receipts[0].handle)
        .unwrap()
        .share_retained()
        .unwrap();
    let retirement = ImagePublications {
        publish: BTreeMap::new(),
        retire: receipts,
    };
    assert!(values.commit(retirement).is_empty());
    assert!(values.images.is_empty());
    assert!(weak.upgrade().is_some());
    assert_eq!(escaped.pixels(), &[1, 0, 0, 255]);
    drop(escaped);
    assert!(weak.upgrade().is_none());
}
