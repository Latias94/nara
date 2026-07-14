use std::fmt::{self, Debug, Formatter};

use nara_asset::{
    AssetEvents, AssetRecord, AssetServer, AssetSlotRevision, AssetStateError, AssetStateRevision,
    AssetStates, AssetVersion, Assets, Handle, ImportArtifactRecord, StableAssetId,
};

use crate::budget::{ImageImportCharge, ImageImportReservation};
use crate::limits::{ImageImportBudgetError, ImageImportMemoryPlan};
use crate::{ImageAsset, ImageImportLimitKind};

use super::{ImageImportError, ImageImportStage, ImagePublicationFailureKind};

#[derive(Debug, Clone)]
enum PreviousImageSlot {
    Vacant(AssetSlotRevision),
    Occupied {
        revision: AssetSlotRevision,
        rgba_bytes: usize,
    },
}

impl PreviousImageSlot {
    fn capture(handle: Handle<ImageAsset>, images: &Assets<ImageAsset>) -> Self {
        let revision = images.slot_revision(handle);
        match images.get(handle) {
            Some(image) => Self::Occupied {
                revision,
                rgba_bytes: image.pixels().len(),
            },
            None => Self::Vacant(revision),
        }
    }

    fn validate(
        &self,
        handle: Handle<ImageAsset>,
        images: &Assets<ImageAsset>,
    ) -> Result<(), ImagePublicationFailureKind> {
        match self {
            Self::Vacant(expected_revision) => {
                if images.get(handle).is_some() {
                    return Err(ImagePublicationFailureKind::AlreadyLoaded);
                }
                if &images.slot_revision(handle) != expected_revision {
                    return Err(ImagePublicationFailureKind::SlotChanged);
                }
            }
            Self::Occupied {
                revision: expected_revision,
                ..
            } => {
                if images.get(handle).is_none() {
                    return Err(ImagePublicationFailureKind::ReloadValueMissing);
                }
                if &images.slot_revision(handle) != expected_revision {
                    return Err(ImagePublicationFailureKind::ReloadValueChanged);
                }
            }
        }
        Ok(())
    }

    const fn rgba_bytes(&self) -> usize {
        match self {
            Self::Vacant(_) => 0,
            Self::Occupied { rgba_bytes, .. } => *rgba_bytes,
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
    expected_version: AssetVersion,
    expected_state_revision: AssetStateRevision,
    previous_slot: PreviousImageSlot,
}

impl ImagePublicationSnapshot {
    pub(crate) fn capture(
        source: &AssetRecord,
        handle: Handle<ImageAsset>,
        expected_version: AssetVersion,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Self {
        Self {
            handle,
            source_stable_id: source.stable_id(),
            expected_version,
            expected_state_revision: states.state_revision(handle.id()),
            previous_slot: PreviousImageSlot::capture(handle, images),
        }
    }

    pub(super) fn admit(
        self,
        source: &AssetRecord,
        asset_server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
        reserved_overlap_bytes: usize,
    ) -> Result<ImagePublicationAdmission, ImageImportError> {
        if source.stable_id() != self.source_stable_id {
            return Err(ImageImportError::Publication(
                ImagePublicationFailureKind::TargetMismatch,
            ));
        }
        self.validate_binding(asset_server)
            .map_err(ImageImportError::Publication)?;
        self.validate_state(states)
            .map_err(ImageImportError::Publication)?;
        self.previous_slot
            .validate(self.handle, images)
            .map_err(ImageImportError::Publication)?;
        let observed = self.previous_slot.rgba_bytes();
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
        Ok(())
    }

    fn validate_state(&self, states: &AssetStates) -> Result<(), ImagePublicationFailureKind> {
        let current_version = states
            .version(self.handle.id())
            .unwrap_or(AssetVersion::ZERO);
        if current_version != self.expected_version {
            return Err(ImagePublicationFailureKind::Stale);
        }
        if states.state_revision(self.handle.id()) != self.expected_state_revision {
            return Err(ImagePublicationFailureKind::StateChanged);
        }
        Ok(())
    }

    pub(crate) fn is_current(&self, images: &Assets<ImageAsset>, states: &AssetStates) -> bool {
        self.validate_state(states).is_ok()
            && self.previous_slot.validate(self.handle, images).is_ok()
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
    memory_plan: ImageImportMemoryPlan,
    publication: ImagePublicationAdmission,
    publication_reservation: ImageImportReservation,
}

impl ImageImportedAsset {
    pub(super) fn new(
        image: ImageAsset,
        memory_plan: ImageImportMemoryPlan,
        publication: ImagePublicationAdmission,
        publication_reservation: ImageImportReservation,
    ) -> Self {
        Self {
            image,
            memory_plan,
            publication,
            publication_reservation,
        }
    }

    #[must_use]
    pub const fn artifact(&self) -> &ImportArtifactRecord {
        self.image.source().artifact()
    }

    #[must_use]
    pub const fn image(&self) -> &ImageAsset {
        &self.image
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
        let snapshot = &self.publication.snapshot;
        snapshot.validate_binding(asset_server)?;
        if self.image.source().stable_id() != snapshot.source_stable_id {
            return Err(ImagePublicationFailureKind::TargetMismatch);
        }
        snapshot.validate_state(states)?;
        snapshot.previous_slot.validate(snapshot.handle, images)?;

        let handle = snapshot.handle;
        let expected_version = snapshot.expected_version;
        let was_occupied = snapshot.previous_slot.is_occupied();
        let source_hash = self.image.source().source_hash();
        let artifact_hash = self.image.source().artifact().key().digest();
        let Self {
            image,
            memory_plan: _,
            publication: _,
            publication_reservation,
        } = self;
        let result = if was_occupied {
            images
                .commit_reload(
                    handle,
                    expected_version,
                    image,
                    states,
                    events,
                    Some(source_hash),
                    Some(artifact_hash),
                )
                .map_err(map_asset_state_error)
        } else {
            images
                .commit_loaded(
                    handle,
                    image,
                    states,
                    events,
                    Some(source_hash),
                    Some(artifact_hash),
                )
                .map_err(map_asset_state_error)
        };
        drop(publication_reservation);
        result
    }

    pub(super) fn retain_publication_charge(mut self) -> Result<Self, ImageImportError> {
        self.publication_reservation
            .resize(ImageImportCharge::publication(self.memory_plan))
            .map_err(|error| ImageImportError::budget(ImageImportStage::Publication, error))?;
        Ok(self)
    }
}

impl Debug for ImageImportedAsset {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageImportedAsset")
            .field("artifact", self.artifact())
            .field("source", self.image.source())
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
    }
}
