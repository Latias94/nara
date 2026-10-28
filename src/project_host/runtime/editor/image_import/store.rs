use std::collections::{BTreeMap, BTreeSet};

use nara_asset::{AssetId, AssetPath, AssetVersion, Handle, ImportSettingsHash, StableAssetId};
use nara_fs::{ContentDigest, FileIdentity};
use nara_image::ImageAsset;

use super::hydration::EditorImageHydrationSource;

use crate::project_host::authoring::{
    AuthoringProjectAttemptGeneration, ProjectSourceIndexGeneration,
};

pub(super) use crate::project_host::authoring::MAX_AUTHORING_IMAGE_IDENTITIES as MAX_LAST_GOOD_IMAGES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorImageSourceProof {
    Unverified,
    SourceIndex {
        generation: ProjectSourceIndexGeneration,
    },
    AuthoringClosure {
        attempt_generation: AuthoringProjectAttemptGeneration,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditorImageLastGood {
    stable_id: StableAssetId,
    handle: Handle<ImageAsset>,
    source_identity: FileIdentity,
    source_proof: EditorImageSourceProof,
    source_digest: ContentDigest,
    settings_hash: ImportSettingsHash,
    version: AssetVersion,
    capture_source: Option<Box<EditorImageHydrationSource>>,
}

impl EditorImageLastGood {
    #[must_use]
    pub(super) const fn new(
        stable_id: StableAssetId,
        handle: Handle<ImageAsset>,
        source_identity: FileIdentity,
        source_proof: EditorImageSourceProof,
        source_digest: ContentDigest,
        settings_hash: ImportSettingsHash,
        version: AssetVersion,
    ) -> Self {
        Self {
            stable_id,
            handle,
            source_identity,
            source_proof,
            source_digest,
            settings_hash,
            version,
            capture_source: None,
        }
    }

    pub(super) fn with_capture_source(mut self, source: EditorImageHydrationSource) -> Self {
        self.capture_source = Some(Box::new(source));
        self
    }

    pub(super) fn capture_source(&self) -> Option<&EditorImageHydrationSource> {
        self.capture_source.as_deref()
    }

    pub(super) fn refresh_capture_source(&mut self, source: EditorImageHydrationSource) {
        self.capture_source = Some(Box::new(source));
    }

    #[must_use]
    pub(crate) const fn stable_id(&self) -> StableAssetId {
        self.stable_id
    }

    #[must_use]
    pub(crate) const fn handle(&self) -> Handle<ImageAsset> {
        self.handle
    }

    #[must_use]
    pub(crate) const fn source_identity(&self) -> FileIdentity {
        self.source_identity
    }

    #[must_use]
    pub(crate) const fn source_proof(&self) -> EditorImageSourceProof {
        self.source_proof
    }

    #[must_use]
    pub(crate) const fn source_digest(&self) -> ContentDigest {
        self.source_digest
    }

    #[must_use]
    pub(crate) const fn settings_hash(&self) -> ImportSettingsHash {
        self.settings_hash
    }

    #[must_use]
    pub(crate) const fn version(&self) -> AssetVersion {
        self.version
    }

    pub(super) fn verify_source_index(&mut self, generation: ProjectSourceIndexGeneration) {
        self.source_proof = EditorImageSourceProof::SourceIndex { generation };
    }
}

#[derive(Default)]
pub(super) struct EditorImageLastGoodStore {
    images: BTreeMap<AssetPath, EditorImageLastGood>,
    changed: bool,
}

impl EditorImageLastGoodStore {
    /// A successor index starts its own generations; keep pixels without predecessor authority.
    pub(super) fn revoke_source_index_verification(&mut self) {
        for image in self.images.values_mut() {
            if matches!(
                image.source_proof,
                EditorImageSourceProof::SourceIndex { .. }
            ) {
                image.source_proof = EditorImageSourceProof::Unverified;
            }
        }
    }

    #[must_use]
    pub(super) fn get(&self, asset_path: &AssetPath) -> Option<&EditorImageLastGood> {
        self.images.get(asset_path)
    }

    pub(super) fn get_mut(&mut self, asset_path: &AssetPath) -> Option<&mut EditorImageLastGood> {
        self.images.get_mut(asset_path)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&AssetPath, &EditorImageLastGood)> {
        self.images.iter()
    }

    pub(super) fn remove_if_matches(
        &mut self,
        asset_path: &AssetPath,
        expected: &EditorImageLastGood,
    ) {
        if self.images.get(asset_path) == Some(expected) {
            self.images.remove(asset_path);
            self.changed = true;
        }
    }

    #[must_use]
    pub(super) fn can_insert(&self, asset_path: &AssetPath) -> bool {
        self.images.len() < MAX_LAST_GOOD_IMAGES || self.images.contains_key(asset_path)
    }

    pub(super) fn insert(&mut self, asset_path: AssetPath, image: EditorImageLastGood) -> bool {
        if !self.can_insert(&asset_path) {
            return false;
        }
        let changed = self.images.get(&asset_path) != Some(&image);
        self.images.insert(asset_path, image);
        self.changed |= changed;
        true
    }

    #[must_use]
    pub(super) fn asset_ids(&self) -> BTreeSet<AssetId> {
        self.images
            .values()
            .map(|image| image.handle().id())
            .collect()
    }

    pub(super) fn ineligible_handles(
        &self,
        mut retain: impl FnMut(&AssetPath, &EditorImageLastGood) -> bool,
    ) -> Vec<Handle<ImageAsset>> {
        self.images
            .iter()
            .filter_map(|(path, image)| (!retain(path, image)).then_some(image.handle()))
            .collect()
    }

    pub(super) fn retain_catalog_eligible(
        &mut self,
        mut retain: impl FnMut(&AssetPath, &EditorImageLastGood) -> bool,
    ) {
        let previous_len = self.images.len();
        self.images
            .retain(|asset_path, image| retain(asset_path, image));
        self.changed |= self.images.len() != previous_len;
    }

    #[must_use]
    pub(super) const fn changed(&self) -> bool {
        self.changed
    }

    pub(super) fn acknowledge_changed(&mut self) {
        self.changed = false;
    }
}
