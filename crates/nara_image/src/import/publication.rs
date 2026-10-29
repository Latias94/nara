use std::fmt::{self, Debug, Formatter};

use nara_asset::{
    AssetEvents, AssetMutationBaseline, AssetPath, AssetRecord, AssetServer, AssetStateError,
    AssetStates, AssetVersion, Assets, Handle, ImportArtifactRecord, StableAssetId,
};
use nara_fs::ContentDigest;

use crate::budget::{ImageImportCharge, ImageImportPixelLease, ImageImportReservation};
use crate::limits::{ImageImportBudgetError, ImageImportMemoryPlan};
use crate::{
    ImageAsset, ImageImportBudgetHost, ImageImportLimitKind, ImageImportSettings,
    ImageSourceMetadata,
};

use super::{ImageImportError, ImageImportStage, ImagePublicationFailureKind};

#[derive(Debug, Clone)]
enum PreviousImageSlot {
    Vacant,
    Occupied {
        rgba_bytes: usize,
        retained_pixels: Option<ImageImportPixelLease>,
    },
}

impl PreviousImageSlot {
    fn capture_observed(previous: Option<&ImageAsset>) -> Self {
        match previous {
            Some(image) => Self::Occupied {
                rgba_bytes: image.pixels().len(),
                retained_pixels: image.shared_import_pixel_lease(),
            },
            None => Self::Vacant,
        }
    }

    fn publication_overlap_bytes(&self, budget_host: &ImageImportBudgetHost) -> usize {
        match self {
            Self::Vacant => 0,
            Self::Occupied {
                rgba_bytes,
                retained_pixels,
                ..
            } => {
                if retained_pixels
                    .as_ref()
                    .is_some_and(|lease| lease.accounts_for(budget_host, *rgba_bytes))
                {
                    0
                } else {
                    *rgba_bytes
                }
            }
        }
    }

    const fn is_occupied(&self) -> bool {
        matches!(self, Self::Occupied { .. })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ImagePublicationSnapshot {
    handle: Handle<ImageAsset>,
    source_stable_id: StableAssetId,
    source_path: AssetPath,
    expected_version: AssetVersion,
    previous_slot: PreviousImageSlot,
    baseline: AssetMutationBaseline<ImageAsset>,
}

impl ImagePublicationSnapshot {
    pub(crate) fn capture(
        source: &AssetRecord,
        handle: Handle<ImageAsset>,
        expected_version: AssetVersion,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<Self, ImagePublicationFailureKind> {
        Self::capture_observed(
            source,
            handle,
            expected_version,
            images.get(handle),
            images,
            states,
        )
    }

    fn capture_observed(
        source: &AssetRecord,
        handle: Handle<ImageAsset>,
        expected_version: AssetVersion,
        previous: Option<&ImageAsset>,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<Self, ImagePublicationFailureKind> {
        if states.version(handle.id()).unwrap_or(AssetVersion::ZERO) != expected_version {
            return Err(ImagePublicationFailureKind::Stale);
        }
        if previous.is_some_and(|image| {
            image.source().is_none_or(|metadata| {
                metadata.stable_id() != source.stable_id() || metadata.path() != source.path()
            })
        }) {
            return Err(ImagePublicationFailureKind::TargetMismatch);
        }
        let baseline = images
            .mutation_baseline(handle, states)
            .map_err(map_asset_state_error)?;
        Ok(Self {
            handle,
            source_stable_id: source.stable_id(),
            source_path: source.path().clone(),
            expected_version,
            previous_slot: PreviousImageSlot::capture_observed(previous),
            baseline,
        })
    }

    pub(crate) fn capture_with_settings(
        source: &AssetRecord,
        handle: Handle<ImageAsset>,
        expected_version: AssetVersion,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<(Self, Option<ImageImportSettings>), ImagePublicationFailureKind> {
        let previous = images.get(handle);
        let snapshot =
            Self::capture_observed(source, handle, expected_version, previous, images, states)?;
        let settings = states
            .state(handle.id())
            .and_then(|state| state.usable_version())
            .and(previous)
            .and_then(ImageAsset::source)
            .map(ImageSourceMetadata::settings);
        Ok((snapshot, settings))
    }

    pub(super) fn admit(
        self,
        source: &AssetRecord,
        asset_server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
        budget_host: &ImageImportBudgetHost,
    ) -> Result<ImagePublicationAdmission, ImageImportError> {
        if source.stable_id() != self.source_stable_id || source.path() != &self.source_path {
            return Err(ImageImportError::Publication(
                ImagePublicationFailureKind::TargetMismatch,
            ));
        }
        self.validate_binding(asset_server)
            .map_err(ImageImportError::Publication)?;
        self.validate_state(states)
            .map_err(ImageImportError::Publication)?;
        self.validate_baseline(images, states)
            .map_err(ImageImportError::Publication)?;
        let reserved_overlap_bytes = budget_host.publication_overlap_limit().get();
        let observed = self.previous_slot.publication_overlap_bytes(budget_host);
        if observed > reserved_overlap_bytes {
            return Err(ImageImportError::budget(
                ImageImportStage::Admission,
                ImageImportBudgetError::per_image(
                    ImageImportLimitKind::RgbaBytes,
                    Some(observed as u64),
                    reserved_overlap_bytes as u64,
                ),
            ));
        }
        Ok(ImagePublicationAdmission {
            snapshot: self,
            publication_overlap_bytes: observed,
        })
    }

    fn validate_binding(
        &self,
        asset_server: &AssetServer,
    ) -> Result<(), ImagePublicationFailureKind> {
        let bound_stable_id = asset_server
            .stable_id(self.handle.id())
            .ok_or(ImagePublicationFailureKind::UnknownAsset)?;
        if bound_stable_id != self.source_stable_id {
            return Err(ImagePublicationFailureKind::TargetMismatch);
        }
        let bound_path = asset_server
            .path(self.handle.id())
            .ok_or(ImagePublicationFailureKind::UnknownAsset)?;
        if bound_path != self.source_path.as_str() {
            return Err(ImagePublicationFailureKind::TargetMismatch);
        }
        if asset_server.get_handle::<ImageAsset>(self.handle.id()) != Some(self.handle) {
            return Err(ImagePublicationFailureKind::TargetMismatch);
        }
        Ok(())
    }

    fn validate_state(&self, states: &AssetStates) -> Result<(), ImagePublicationFailureKind> {
        let current_version = states
            .version(self.handle.id())
            .unwrap_or(AssetVersion::ZERO);
        if current_version != self.expected_version {
            return Err(ImagePublicationFailureKind::Stale);
        }
        Ok(())
    }

    fn baseline_error(&self, error: AssetStateError) -> ImagePublicationFailureKind {
        match error {
            AssetStateError::SlotSuperseded { .. } if self.previous_slot.is_occupied() => {
                ImagePublicationFailureKind::ReloadValueChanged
            }
            AssetStateError::SlotSuperseded { .. } => ImagePublicationFailureKind::SlotChanged,
            error => map_asset_state_error(error),
        }
    }

    fn validate_baseline(
        &self,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<(), ImagePublicationFailureKind> {
        if self.previous_slot.is_occupied() && images.get(self.handle).is_none() {
            return Err(ImagePublicationFailureKind::ReloadValueMissing);
        }
        if !self.previous_slot.is_occupied() && images.get(self.handle).is_some() {
            return Err(ImagePublicationFailureKind::AlreadyLoaded);
        }
        self.baseline
            .validate(images, states)
            .map_err(|error| self.baseline_error(error))
    }

    pub(crate) fn mutation(
        &self,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<AssetMutationBaseline<ImageAsset>, ImagePublicationFailureKind> {
        self.validate_baseline(images, states)?;
        Ok(self.baseline.clone())
    }

    pub(crate) fn is_current(
        &self,
        asset_server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> bool {
        self.validate_binding(asset_server).is_ok()
            && self.validate_state(states).is_ok()
            && self.validate_baseline(images, states).is_ok()
    }
}

#[derive(Debug)]
pub(super) struct ImagePublicationAdmission {
    snapshot: ImagePublicationSnapshot,
    publication_overlap_bytes: usize,
}

impl ImagePublicationAdmission {
    pub(super) const fn overlap_bytes(&self) -> usize {
        self.publication_overlap_bytes
    }
}

pub struct ImageImportedAsset {
    image: ImageAsset,
    source_digest: ContentDigest,
    memory_plan: ImageImportMemoryPlan,
    publication: ImagePublicationAdmission,
    publication_reservation: ImageImportReservation,
}

impl ImageImportedAsset {
    pub(super) fn new(
        image: ImageAsset,
        source_digest: ContentDigest,
        memory_plan: ImageImportMemoryPlan,
        publication: ImagePublicationAdmission,
        publication_reservation: ImageImportReservation,
    ) -> Self {
        Self {
            image,
            source_digest,
            memory_plan,
            publication,
            publication_reservation,
        }
    }

    #[must_use]
    pub fn artifact(&self) -> &ImportArtifactRecord {
        self.source().artifact()
    }

    /// Returns actual source provenance guaranteed by the private PNG import constructor.
    #[must_use]
    pub fn source(&self) -> &ImageSourceMetadata {
        self.image
            .source()
            .expect("private image import results always contain actual source provenance")
    }

    #[must_use]
    pub const fn image(&self) -> &ImageAsset {
        &self.image
    }

    #[must_use]
    pub const fn source_digest(&self) -> ContentDigest {
        self.source_digest
    }

    #[must_use]
    pub const fn memory_plan(&self) -> ImageImportMemoryPlan {
        self.memory_plan
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn into_image(self) -> ImageAsset {
        self.image
    }

    pub fn commit(
        self,
        asset_server: &AssetServer,
        images: &mut Assets<ImageAsset>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> Result<AssetVersion, ImagePublicationFailureKind> {
        let Self {
            image,
            source_digest: _,
            memory_plan: _,
            publication,
            publication_reservation,
        } = self;
        let result =
            commit_published_image(image, publication, asset_server, images, states, events);
        drop(publication_reservation);
        result
    }

    /// Commits the image while retaining its exact shared pixel charge.
    ///
    /// The attached owner follows immutable pixel clones into prepared resources and releases the
    /// charge only after the final retained image disappears.
    pub fn commit_retained(
        self,
        asset_server: &AssetServer,
        images: &mut Assets<ImageAsset>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> Result<AssetVersion, ImageImportError> {
        let Self {
            mut image,
            source_digest: _,
            memory_plan,
            publication,
            publication_reservation,
        } = self;
        let rgba_bytes = image.pixels().len();
        if rgba_bytes != memory_plan.rgba_bytes() {
            return Err(ImageImportError::Publication(
                ImagePublicationFailureKind::TargetMismatch,
            ));
        }
        let (retention, publication_overlap_guard) =
            ImageImportPixelLease::from_publication(publication_reservation, rgba_bytes).map_err(
                |_| ImageImportError::Publication(ImagePublicationFailureKind::TargetMismatch),
            )?;
        if !image.try_attach_import_pixel_lease(retention) {
            return Err(ImageImportError::Publication(
                ImagePublicationFailureKind::TargetMismatch,
            ));
        }
        let result =
            commit_published_image(image, publication, asset_server, images, states, events)
                .map_err(ImageImportError::Publication);
        drop(publication_overlap_guard);
        result
    }

    pub(super) fn retain_publication_charge(mut self) -> Result<Self, ImageImportError> {
        self.publication_reservation
            .resize(ImageImportCharge::publication(self.memory_plan))
            .map_err(|error| ImageImportError::budget(ImageImportStage::Publication, error))?;
        Ok(self)
    }
}

fn commit_published_image(
    image: ImageAsset,
    publication: ImagePublicationAdmission,
    asset_server: &AssetServer,
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
    events: &mut AssetEvents,
) -> Result<AssetVersion, ImagePublicationFailureKind> {
    let snapshot = &publication.snapshot;
    snapshot.validate_binding(asset_server)?;
    let source = image
        .source()
        .ok_or(ImagePublicationFailureKind::TargetMismatch)?;
    if source.stable_id() != snapshot.source_stable_id || source.path() != &snapshot.source_path {
        return Err(ImagePublicationFailureKind::TargetMismatch);
    }
    snapshot.validate_state(states)?;
    snapshot.validate_baseline(images, states)?;

    let source_hash = source.source_hash();
    let artifact_hash = source.artifact().key().digest();
    let version = snapshot
        .mutation(images, states)?
        .commit_loaded(
            image,
            images,
            states,
            events,
            Some(source_hash),
            Some(artifact_hash),
        )
        .map_err(map_asset_state_error)?;
    states.acknowledge_metadata_publication(snapshot.handle.id());
    Ok(version)
}

impl Debug for ImageImportedAsset {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageImportedAsset")
            .field("artifact", self.artifact())
            .field("source", self.source())
            .field("extent", &self.image.extent())
            .field("format", &self.image.format())
            .field("color_space", &self.image.color_space())
            .field("pixel_len", &self.image.pixels().len())
            .field("memory_plan", &self.memory_plan)
            .finish_non_exhaustive()
    }
}

fn map_asset_state_error(error: AssetStateError) -> ImagePublicationFailureKind {
    match error {
        AssetStateError::UnknownAsset { .. } => ImagePublicationFailureKind::UnknownAsset,
        AssetStateError::VersionExhausted { .. } => ImagePublicationFailureKind::VersionExhausted,
        AssetStateError::StaleReload { .. } => ImagePublicationFailureKind::Stale,
        AssetStateError::StorageMismatch { .. } => ImagePublicationFailureKind::TargetMismatch,
        AssetStateError::SlotSuperseded { .. } => ImagePublicationFailureKind::SlotChanged,
        AssetStateError::MutationSuperseded { .. } => ImagePublicationFailureKind::StateChanged,
    }
}

#[cfg(test)]
mod source_admission;
