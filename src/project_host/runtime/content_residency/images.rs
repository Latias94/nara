use std::collections::{BTreeMap, BTreeSet};

use nara_asset::{
    AssetEvents, AssetRecord, AssetRef, AssetServer, AssetSlotRevision, AssetSourceKind,
    AssetStateRevision, AssetStates, Assets, Handle,
};
use nara_image::ImageAsset;
use nara_scene::advanced::SceneAssetBindings;

use super::super::{HostFault, single_error};
use crate::project_content::ProjectContentSnapshot;

pub(super) struct OwnedImage {
    pub(super) handle: Handle<ImageAsset>,
    pub(super) record: AssetRecord,
    slot: AssetSlotRevision,
    state: AssetStateRevision,
}

impl OwnedImage {
    pub(super) fn capture(
        handle: Handle<ImageAsset>,
        record: AssetRecord,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Self {
        Self {
            handle,
            record,
            slot: images.slot_revision(handle),
            state: states.state_revision(handle.id()),
        }
    }

    fn matches(
        &self,
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> bool {
        images.get(self.handle).is_some()
            && server.stable_id(self.handle.id()) == Some(self.record.stable_id())
            && server.path(self.handle.id()) == Some(self.record.path().as_str())
            && images.slot_revision(self.handle) == self.slot
            && states.state_revision(self.handle.id()) == self.state
            && states.published_slot_revision(self.handle.id()) == Some(&self.slot)
            && states
                .state(self.handle.id())
                .is_some_and(|state| state.usable_version().is_some())
    }
}

struct ImagePublication {
    handle: Handle<ImageAsset>,
    record: AssetRecord,
    image: ImageAsset,
    slot: AssetSlotRevision,
    state: AssetStateRevision,
}

/// Finite CPU publications whose only fallible version advances were checked before Scene commit.
pub(super) struct ImagePublications {
    publish: BTreeMap<nara_asset::AssetPath, ImagePublication>,
    retire: Vec<(Handle<ImageAsset>, AssetSlotRevision, AssetStateRevision)>,
}

pub(super) fn validate_startup_images(
    snapshot: &ProjectContentSnapshot,
    server: &AssetServer,
    images: &Assets<ImageAsset>,
    states: &AssetStates,
) -> Result<(), HostFault> {
    let stable_ids = snapshot
        .images()
        .iter()
        .map(|image| image.source().stable_id())
        .collect::<BTreeSet<_>>();
    let paths = snapshot
        .images()
        .iter()
        .map(|image| image.path().as_str())
        .collect::<BTreeSet<_>>();
    let belongs_to_startup = |id| {
        server
            .stable_id(id)
            .is_some_and(|id| stable_ids.contains(&id))
            || server.path(id).is_some_and(|path| paths.contains(path))
    };
    if images
        .iter()
        .any(|(handle, _)| belongs_to_startup(handle.id()))
        || states.iter().any(|(id, state)| {
            belongs_to_startup(id) && !matches!(state.load_state(), nara_asset::LoadState::Removed)
        })
    {
        return Err(superseded());
    }
    Ok(())
}

impl ImagePublications {
    pub(super) fn empty() -> Self {
        Self {
            publish: BTreeMap::new(),
            retire: Vec::new(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.publish.is_empty() && self.retire.is_empty()
    }
    pub(super) fn prepare(
        snapshot: &ProjectContentSnapshot,
        owned: &[OwnedImage],
        bindings: &mut SceneAssetBindings,
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<Self, HostFault> {
        if owned
            .iter()
            .any(|image| !image.matches(server, images, states))
        {
            return Err(superseded());
        }
        let owned_by_id = owned
            .iter()
            .map(|image| (image.handle.id(), image))
            .collect::<BTreeMap<_, _>>();
        let mut publish = BTreeMap::new();
        let mut next_ids = BTreeSet::new();
        for (index, content) in snapshot.images().iter().enumerate() {
            let source = content.source();
            let record = AssetRecord::new(
                source.stable_id(),
                source.path().clone(),
                AssetSourceKind::Image,
            );
            let handle = bindings
                .reserve::<ImageAsset>(
                    &AssetRef::StableId(record.stable_id()),
                    snapshot.asset_database(),
                    &AssetSourceKind::Image,
                )
                .map_err(|_| identity_conflict())?;
            let ours = owned_by_id
                .get(&handle.id())
                .is_some_and(|image| image.record == record);
            if !ours
                && (images.get(handle).is_some()
                    || states.state(handle.id()).is_some_and(|state| {
                        !matches!(state.load_state(), nara_asset::LoadState::Removed)
                    }))
            {
                return Err(superseded());
            }
            states
                .next_version(handle.id())
                .map_err(|_| version_exhausted())?;
            let image = snapshot.share_image_for_consumer(index).ok_or_else(|| {
                HostFault::new(single_error(
                    "project.scene.image-snapshot-invalid",
                    "Prepared Scene Image is unavailable",
                ))
            })?;
            next_ids.insert(handle.id());
            publish.insert(
                record.path().clone(),
                ImagePublication {
                    handle,
                    record,
                    image,
                    slot: images.slot_revision(handle),
                    state: states.state_revision(handle.id()),
                },
            );
        }
        let mut retire = Vec::new();
        for image in owned {
            if !next_ids.contains(&image.handle.id()) {
                states
                    .next_version(image.handle.id())
                    .map_err(|_| version_exhausted())?;
                retire.push((image.handle, image.slot.clone(), image.state.clone()));
            }
        }
        Ok(Self { publish, retire })
    }

    pub(super) fn image_for_path(
        &self,
        path: &nara_asset::AssetPath,
    ) -> Option<(Handle<ImageAsset>, ImageAsset)> {
        self.publish.get(path).and_then(|item| {
            item.image
                .share_retained()
                .map(|image| (item.handle, image))
        })
    }

    pub(super) fn validate(
        &self,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<(), HostFault> {
        for item in self.publish.values() {
            if images.slot_revision(item.handle) != item.slot
                || states.state_revision(item.handle.id()) != item.state
            {
                return Err(superseded());
            }
        }
        for (handle, slot, state) in &self.retire {
            if images.slot_revision(*handle) != *slot
                || states.state_revision(handle.id()) != *state
            {
                return Err(superseded());
            }
        }
        Ok(())
    }

    pub(super) fn commit(
        self,
        server: &AssetServer,
        images: &mut Assets<ImageAsset>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> Vec<OwnedImage> {
        // The Scene transaction cannot run lifecycle hooks or replace these engine resources.
        // Validate the complete captured baseline before the first publication.
        for item in self.publish.values() {
            assert_eq!(images.slot_revision(item.handle), item.slot);
            assert_eq!(states.state_revision(item.handle.id()), item.state);
            assert_eq!(
                server.stable_id(item.handle.id()),
                Some(item.record.stable_id())
            );
            assert_eq!(
                server.path(item.handle.id()),
                Some(item.record.path().as_str())
            );
        }
        for (handle, slot, state) in &self.retire {
            assert_eq!(images.slot_revision(*handle), *slot);
            assert_eq!(states.state_revision(handle.id()), *state);
        }
        let mut receipts = Vec::with_capacity(self.publish.len());
        for item in self.publish.into_values() {
            let source = item
                .image
                .source()
                .expect("source Image publication retains imported provenance");
            let source_hash = source.source_hash();
            let import_hash = source.artifact().key().digest();
            images
                .commit_loaded(
                    item.handle,
                    item.image,
                    states,
                    events,
                    Some(source_hash),
                    Some(import_hash),
                )
                .expect("Scene Image publication version was preflighted");
            receipts.push(OwnedImage::capture(
                item.handle,
                item.record,
                images,
                states,
            ));
        }
        for (handle, _, _) in self.retire {
            assert!(
                images
                    .remove_with_state(handle, states, events)
                    .expect("Scene Image retirement version was preflighted")
                    .is_some()
            );
        }
        receipts
    }
}

fn superseded() -> HostFault {
    HostFault::new(single_error(
        "project.scene.image-ownership-changed",
        "Scene Image publication is no longer owned by the current level",
    ))
}

fn identity_conflict() -> HostFault {
    HostFault::new(single_error(
        "project.scene.asset-identity-conflict",
        "Prepared Scene asset identity conflicts with the runtime",
    ))
}

fn version_exhausted() -> HostFault {
    HostFault::new(single_error(
        "project.scene.image-version-exhausted",
        "Scene Image publication versions are exhausted",
    ))
}
