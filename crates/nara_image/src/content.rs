use std::{error::Error, fmt, sync::Arc};

use nara_asset::{
    AssetProductRef, AssetPublicationRevision, AssetServer, AssetSlotRevision, AssetStates,
    AssetVersion, Assets, Handle,
};

use crate::{
    ImageAsset, ImageColorSpace, ImageContentHash, ImageExtent, ImageFormat, ImageSourceMetadata,
};

/// Immutable Image content and the authority under which a derived asset admitted it.
#[derive(Debug, Clone)]
pub struct ImageContentSnapshot {
    handle: Handle<ImageAsset>,
    product: AssetProductRef,
    version: AssetVersion,
    slot: AssetSlotRevision,
    publication: AssetPublicationRevision,
    image: Arc<ImageAsset>,
}

impl ImageContentSnapshot {
    pub fn capture(
        handle: Handle<ImageAsset>,
        server: &AssetServer,
        states: &AssetStates,
        images: &Assets<ImageAsset>,
    ) -> Result<Self, ImageContentError> {
        let image = images.get(handle).ok_or(ImageContentError::Unavailable)?;
        let product = image.product().ok_or(ImageContentError::IdentityMismatch)?;
        if !catalog_matches(handle, product, server) {
            return Err(ImageContentError::IdentityMismatch);
        }
        let version = states
            .state(handle.id())
            .and_then(|state| state.usable_version())
            .ok_or(ImageContentError::Unavailable)?;
        if states.published_slot_revision(handle.id()) != Some(&images.slot_revision(handle)) {
            return Err(ImageContentError::Unavailable);
        }
        Ok(Self {
            handle,
            product,
            version,
            slot: images.slot_revision(handle),
            publication: states.publication_revision(handle.id()),
            image: Arc::new(image.share_immutable()),
        })
    }

    #[must_use]
    pub const fn handle(&self) -> Handle<ImageAsset> {
        self.handle
    }

    #[must_use]
    pub const fn version(&self) -> AssetVersion {
        self.version
    }

    #[must_use]
    pub const fn slot_revision(&self) -> &AssetSlotRevision {
        &self.slot
    }

    #[must_use]
    pub fn source(&self) -> Option<&ImageSourceMetadata> {
        self.image.source()
    }

    #[must_use]
    pub const fn product(&self) -> AssetProductRef {
        self.product
    }

    #[must_use]
    pub fn content_hash(&self) -> ImageContentHash {
        self.image.content_hash()
    }

    #[must_use]
    pub fn format(&self) -> ImageFormat {
        self.image.format()
    }

    #[must_use]
    pub fn color_space(&self) -> ImageColorSpace {
        self.image.color_space()
    }

    #[must_use]
    pub fn extent(&self) -> ImageExtent {
        self.image.extent()
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        self.image.pixels()
    }

    #[must_use]
    pub fn same_allocation(&self, other: &Self) -> bool {
        self.image.same_allocation(&other.image)
    }

    /// Retains a concrete consumer's allocation charge with every clone and prepared projection.
    #[must_use]
    pub fn with_retention_owner<R>(mut self, owner: Arc<R>) -> Self
    where
        R: Send + Sync + 'static,
    {
        self.image = Arc::new(self.image.share_with_retention_owner(owner));
        self
    }

    #[must_use]
    pub fn is_current(
        &self,
        server: &AssetServer,
        states: &AssetStates,
        images: &Assets<ImageAsset>,
    ) -> bool {
        self.is_authorized(server, states, images)
            && images.slot_revision(self.handle) == self.slot
            && states
                .state(self.handle.id())
                .and_then(|state| state.usable_version())
                == Some(self.version)
    }

    /// Allows a coherent last-good derived value across valid replacement, never across revocation.
    #[must_use]
    pub fn is_authorized(
        &self,
        server: &AssetServer,
        states: &AssetStates,
        images: &Assets<ImageAsset>,
    ) -> bool {
        let Some(state) = states.state(self.handle.id()) else {
            return false;
        };
        if state.usable_version().is_none() {
            return false;
        }
        self.publication == states.publication_revision(self.handle.id())
            && states.published_slot_revision(self.handle.id())
                == Some(&images.slot_revision(self.handle))
            && catalog_matches(self.handle, self.product, server)
            && images
                .get(self.handle)
                .is_some_and(|image| image.product() == Some(self.product))
    }

    #[cfg(any(feature = "render-prepare", test))]
    #[must_use]
    pub fn prepared_snapshot(&self) -> crate::PreparedImageSnapshot {
        crate::PreparedImageSnapshot::from_image(
            self.handle,
            self.version,
            self.slot.clone(),
            &self.image,
        )
    }
}

impl PartialEq for ImageContentSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.handle == other.handle
            && self.product == other.product
            && self.version == other.version
            && self.slot == other.slot
            && self.publication == other.publication
            && self.same_allocation(other)
    }
}

impl Eq for ImageContentSnapshot {}

fn catalog_matches(
    handle: Handle<ImageAsset>,
    product: AssetProductRef,
    server: &AssetServer,
) -> bool {
    server.get_handle::<ImageAsset>(handle.id()) == Some(handle)
        && server.product(handle.id()) == Some(product)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageContentError {
    Unavailable,
    IdentityMismatch,
}

impl fmt::Display for ImageContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "image has no usable published content",
            Self::IdentityMismatch => "image does not match its typed catalog identity",
        })
    }
}

impl Error for ImageContentError {}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::{
        ArtifactFormatVersion, ArtifactLabel, AssetEvents, AssetPath, AssetRecord, AssetSourceKind,
        ImportArtifactKey, ImportArtifactRecord, ImportDependencyDigest, ImportProfile,
        ImportedAssetType, ImportedProductId, ImporterId, ImporterVersion, SourceHash,
        StableAssetId,
    };

    use crate::ImageImportSettings;

    fn loaded() -> (
        AssetServer,
        Handle<ImageAsset>,
        Assets<ImageAsset>,
        AssetStates,
        AssetEvents,
    ) {
        let record = AssetRecord::new(
            StableAssetId::parse_str("20000000-0000-4000-8000-000000000001").unwrap(),
            AssetPath::new("image.png").unwrap(),
            AssetSourceKind::Image,
        );
        let settings = ImageImportSettings::default();
        let key = ImportArtifactKey::new(
            record.stable_id(),
            SourceHash::from_bytes(b"pixels"),
            ImportDependencyDigest::empty(),
            ImporterId::new("test.image").unwrap(),
            ImporterVersion::new(1),
            settings.settings_hash(),
            ImportProfile::default(),
            ImportedAssetType::new("image").unwrap(),
            ArtifactLabel::default(),
            ArtifactFormatVersion::new(1),
        );
        let image = ImageAsset::from_import(
            ImageSourceMetadata::new(
                record.path().clone(),
                settings,
                ImportArtifactRecord::new(key).unwrap(),
            )
            .unwrap(),
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            vec![1, 2, 3, 4],
        )
        .unwrap();
        let mut server = AssetServer::new();
        let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
        let mut images = Assets::default();
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();
        images
            .commit_loaded(handle, image, &mut states, &mut events, None, None)
            .unwrap();
        (server, handle, images, states, events)
    }

    #[test]
    fn exact_snapshot_retains_pixels_during_loading_failure_and_after_removal() {
        let (server, handle, mut images, mut states, mut events) = loaded();
        let snapshot = ImageContentSnapshot::capture(handle, &server, &states, &images).unwrap();
        states.set_loading(handle.id());
        assert!(snapshot.is_current(&server, &states, &images));
        images
            .record_reload_failure(handle, &mut states, &mut events, "reload failed")
            .unwrap();
        assert!(snapshot.is_current(&server, &states, &images));
        let image = images
            .remove_with_state(handle, &mut states, &mut events)
            .unwrap()
            .unwrap();
        assert!(!snapshot.is_authorized(&server, &states, &images));
        images
            .commit_loaded(handle, image, &mut states, &mut events, None, None)
            .unwrap();
        assert!(!snapshot.is_authorized(&server, &states, &images));
        assert_eq!(snapshot.pixels(), &[1, 2, 3, 4]);
    }

    #[test]
    fn same_version_direct_slot_replacement_cannot_authorize_a_previous_snapshot() {
        let (server, handle, mut images, states, _) = loaded();
        let snapshot = ImageContentSnapshot::capture(handle, &server, &states, &images).unwrap();
        let removed = images.remove(handle).unwrap();
        images.insert(handle, removed);
        assert!(!snapshot.is_current(&server, &states, &images));
        assert!(!snapshot.is_authorized(&server, &states, &images));
    }

    #[test]
    fn valid_replacement_preserves_old_authority_but_a_later_raw_slot_aba_cannot_revive_it() {
        let (server, handle, mut images, mut states, mut events) = loaded();
        let old = ImageContentSnapshot::capture(handle, &server, &states, &images).unwrap();
        let replacement = images.get(handle).unwrap().share_immutable();
        images
            .commit_loaded(handle, replacement, &mut states, &mut events, None, None)
            .unwrap();
        assert!(old.is_authorized(&server, &states, &images));
        assert!(!old.is_current(&server, &states, &images));
        let latest = ImageContentSnapshot::capture(handle, &server, &states, &images).unwrap();
        let removed = images.remove(handle).unwrap();
        images.insert(handle, removed);
        assert!(!old.is_authorized(&server, &states, &images));
        assert!(!latest.is_authorized(&server, &states, &images));
        assert_eq!(
            ImageContentSnapshot::capture(handle, &server, &states, &images),
            Err(ImageContentError::Unavailable)
        );
    }

    #[test]
    fn prepared_projection_keeps_the_consumers_retention_owner_until_its_last_clone() {
        let (server, handle, images, states, _) = loaded();
        let owner = Arc::new(());
        let retained = Arc::downgrade(&owner);
        let snapshot = ImageContentSnapshot::capture(handle, &server, &states, &images)
            .unwrap()
            .with_retention_owner(owner);
        let prepared = snapshot.prepared_snapshot();
        let clone = prepared.clone();
        drop(snapshot);
        assert!(retained.upgrade().is_some());
        assert_eq!(prepared.resource().pixels(), &[1, 2, 3, 4]);
        drop(prepared);
        assert!(retained.upgrade().is_some());
        drop(clone);
        assert!(retained.upgrade().is_none());
    }

    #[test]
    fn anonymous_content_has_no_managed_authority_but_supports_code_first_preparation() {
        let mut server = AssetServer::new();
        let handle = server.reserve_anonymous::<ImageAsset>().unwrap();
        let mut images = Assets::default();
        let mut states = AssetStates::default();
        let image = ImageAsset::new(
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Linear,
            vec![1, 2, 3, 4],
        )
        .unwrap();
        assert!(image.product().is_none());
        assert!(image.source().is_none());
        images
            .commit_loaded(
                handle,
                image,
                &mut states,
                &mut AssetEvents::default(),
                None,
                None,
            )
            .unwrap();
        assert!(server.product(handle.id()).is_none());
        assert_eq!(
            ImageContentSnapshot::capture(handle, &server, &states, &images),
            Err(ImageContentError::IdentityMismatch)
        );
        let prepared = crate::PreparedImageSnapshot::capture(handle, &images, &states).unwrap();
        assert_eq!(prepared.resource().pixels(), &[1, 2, 3, 4]);
    }

    #[test]
    fn anonymous_successor_revokes_managed_derivation_with_a_remaining_product_binding() {
        let (server, handle, mut images, mut states, mut events) = loaded();
        let previous = ImageContentSnapshot::capture(handle, &server, &states, &images).unwrap();
        let image = ImageAsset::new(
            previous.extent(),
            previous.format(),
            previous.color_space(),
            vec![4, 3, 2, 1],
        )
        .unwrap();
        images
            .commit_loaded(handle, image, &mut states, &mut events, None, None)
            .unwrap();
        assert_eq!(server.product(handle.id()), Some(previous.product()));
        assert!(!previous.is_authorized(&server, &states, &images));
        assert_eq!(
            ImageContentSnapshot::capture(handle, &server, &states, &images),
            Err(ImageContentError::IdentityMismatch)
        );
        let prepared = crate::PreparedImageSnapshot::capture(handle, &images, &states).unwrap();
        assert_eq!(prepared.resource().pixels(), &[4, 3, 2, 1]);
    }

    #[test]
    fn pathless_primary_and_auxiliary_products_support_capture_last_good_and_revocation() {
        for selection in [
            ImportedProductId::PRIMARY,
            ImportedProductId::parse_subproduct("30000000-0000-4000-8000-000000000001").unwrap(),
        ] {
            let product = AssetProductRef::new(
                StableAssetId::parse_str("20000000-0000-4000-8000-000000000001").unwrap(),
                selection,
            )
            .unwrap();
            let mut server = AssetServer::new();
            let handle = server.reserve_product::<ImageAsset>(product).unwrap();
            let mut images = Assets::default();
            let mut states = AssetStates::default();
            let mut events = AssetEvents::default();
            let image = ImageAsset::from_product(
                product,
                ImageExtent::new(1, 1),
                ImageFormat::Rgba8,
                ImageColorSpace::Linear,
                vec![1, 2, 3, 4],
            )
            .unwrap();
            let hash = image.content_hash();
            images
                .commit_loaded(handle, image, &mut states, &mut events, None, None)
                .unwrap();
            assert!(server.path(handle.id()).is_none());
            let snapshot =
                ImageContentSnapshot::capture(handle, &server, &states, &images).unwrap();
            assert!(snapshot.source().is_none());
            assert_eq!(snapshot.product(), product);
            assert_eq!(snapshot.content_hash(), hash);
            assert_eq!(snapshot.prepared_snapshot().resource().content_hash(), hash);
            assert!(snapshot.is_current(&server, &states, &images));
            states.set_loading(handle.id());
            images
                .record_reload_failure(handle, &mut states, &mut events, "decode failed")
                .unwrap();
            assert!(snapshot.is_current(&server, &states, &images));

            let replacement = ImageAsset::from_product(
                product,
                ImageExtent::new(1, 1),
                ImageFormat::Rgba8,
                ImageColorSpace::Srgb,
                vec![4, 3, 2, 1],
            )
            .unwrap();
            images
                .commit_loaded(handle, replacement, &mut states, &mut events, None, None)
                .unwrap();
            assert!(snapshot.is_authorized(&server, &states, &images));
            assert!(!snapshot.is_current(&server, &states, &images));
            assert_eq!(snapshot.pixels(), &[1, 2, 3, 4]);
            let latest = ImageContentSnapshot::capture(handle, &server, &states, &images).unwrap();
            assert!(latest.is_current(&server, &states, &images));
            assert_ne!(latest.content_hash(), snapshot.content_hash());
            let removed = images
                .remove_with_state(handle, &mut states, &mut events)
                .unwrap()
                .unwrap();
            assert!(!snapshot.is_authorized(&server, &states, &images));
            assert!(!latest.is_authorized(&server, &states, &images));
            images
                .commit_loaded(handle, removed, &mut states, &mut events, None, None)
                .unwrap();
            assert!(!snapshot.is_authorized(&server, &states, &images));
            assert!(!latest.is_authorized(&server, &states, &images));
        }
    }

    #[test]
    fn catalog_matching_uses_complete_product_including_auxiliary_selection() {
        let (mut server, primary_handle, mut images, mut states, mut events) = loaded();
        let primary = images.get(primary_handle).unwrap().product().unwrap();
        let auxiliary = AssetProductRef::new(
            primary.source(),
            ImportedProductId::parse_subproduct("30000000-0000-4000-8000-000000000001").unwrap(),
        )
        .unwrap();
        let handle = server.reserve_product::<ImageAsset>(auxiliary).unwrap();
        let wrong_selection = ImageAsset::from_product(
            primary,
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            vec![1, 2, 3, 4],
        )
        .unwrap();
        images
            .commit_loaded(
                handle,
                wrong_selection,
                &mut states,
                &mut events,
                None,
                None,
            )
            .unwrap();
        assert_eq!(
            ImageContentSnapshot::capture(handle, &server, &states, &images),
            Err(ImageContentError::IdentityMismatch)
        );
    }

    #[test]
    fn source_and_processed_replacements_share_product_authority() {
        let (server, handle, mut images, mut states, mut events) = loaded();
        let imported = ImageContentSnapshot::capture(handle, &server, &states, &images).unwrap();
        let processed = ImageAsset::from_product(
            imported.product(),
            imported.extent(),
            imported.format(),
            imported.color_space(),
            imported.pixels().to_vec(),
        )
        .unwrap();
        images
            .commit_loaded(handle, processed, &mut states, &mut events, None, None)
            .unwrap();
        assert!(imported.is_authorized(&server, &states, &images));
        let current = ImageContentSnapshot::capture(handle, &server, &states, &images).unwrap();
        assert!(current.source().is_none());
        assert_eq!(imported.content_hash(), current.content_hash());
        assert!(current.is_current(&server, &states, &images));

        let restored_import = ImageAsset::from_import(
            imported.source().unwrap().clone(),
            imported.extent(),
            imported.format(),
            imported.pixels().to_vec(),
        )
        .unwrap();
        images
            .commit_loaded(
                handle,
                restored_import,
                &mut states,
                &mut events,
                None,
                None,
            )
            .unwrap();
        assert!(current.is_authorized(&server, &states, &images));
        assert!(
            ImageContentSnapshot::capture(handle, &server, &states, &images)
                .unwrap()
                .source()
                .is_some()
        );
    }

    #[test]
    fn source_path_alias_is_not_image_content_identity() {
        let (server, handle, mut images, mut states, mut events) = loaded();
        let image = images.get(handle).unwrap();
        let source = image.source().unwrap();
        let alternate_path = ImageAsset::from_import(
            ImageSourceMetadata::new(
                AssetPath::new("other/source.png").unwrap(),
                source.settings(),
                source.artifact().clone(),
            )
            .unwrap(),
            image.extent(),
            image.format(),
            image.pixels().to_vec(),
        )
        .unwrap();
        images
            .commit_loaded(handle, alternate_path, &mut states, &mut events, None, None)
            .unwrap();
        let snapshot = ImageContentSnapshot::capture(handle, &server, &states, &images).unwrap();
        assert_eq!(
            snapshot.source().unwrap().path().as_str(),
            "other/source.png"
        );
        assert_eq!(server.path(handle.id()), Some("image.png"));
        assert!(snapshot.is_current(&server, &states, &images));
    }
}
