use std::time::Duration;

#[cfg(any(feature = "desktop", test))]
use nara::save::SaveError;
use nara::{
    core::UuidIdError, ecs::bevy_ecs::error::Result as SystemResult, prelude::*, save::SaveStore,
};

use crate::simulation::bind_arena;

#[cfg(any(feature = "desktop", test))]
pub(crate) const ARRIVAL_AUDIO: &str = "0193f100-2222-7000-8000-000000000003";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Sector {
    #[default]
    One,
    Two,
}

impl Sector {
    pub fn asset(self) -> Result<AssetId, UuidIdError> {
        match self {
            Self::One => "01a219c4-2965-7297-b503-da50fb749023",
            Self::Two => "0193f100-2222-7000-8000-000000000002",
        }
        .parse()
    }
}

#[derive(Clone, Copy, Component, Debug, Default, PartialEq, Eq, Schema)]
#[schema(id = "night-relay.SaveSnapshot", version = 1)]
pub(crate) struct SaveSnapshot {
    pub sector_one_best_ms: Option<u64>,
    pub sector_two_best_ms: Option<u64>,
}

#[derive(Default, Resource)]
pub(crate) struct Campaign {
    pub sector: Sector,
    pub pending: Option<SceneInstanceId>,
    pub progress: SaveSnapshot,
    save: Option<(SaveStore, &'static str)>,
}

impl Campaign {
    #[cfg(feature = "desktop")]
    pub fn persistent() -> Self {
        const SLOT: &str = "progress";

        match SaveStore::for_app("night-relay").and_then(|store| Self::load(store, SLOT)) {
            Ok(campaign) => campaign,
            Err(error) => {
                eprintln!("Night Relay could not open its progress save: {error}");
                Self::default()
            }
        }
    }

    #[cfg(any(feature = "desktop", test))]
    pub fn load(store: SaveStore, slot: &'static str) -> Result<Self, SaveError> {
        let progress = match store.load_with_backup::<SaveSnapshot>(slot) {
            Ok(loaded) => {
                if loaded.recovered {
                    eprintln!("Night Relay recovered progress from its backup save");
                }
                loaded.value
            }
            Err(SaveError::NotFound(_)) => SaveSnapshot::default(),
            Err(error) => return Err(error),
        };
        let mut campaign = Self::with_save(store, slot);
        if progress.sector_one_best_ms.is_some() {
            campaign.sector = Sector::Two;
        }
        campaign.progress = progress;
        Ok(campaign)
    }

    #[cfg(any(feature = "desktop", test))]
    pub fn with_save(store: SaveStore, slot: &'static str) -> Self {
        Self {
            save: Some((store, slot)),
            ..Self::default()
        }
    }

    pub fn clear(&mut self, elapsed: Duration) {
        let best = match self.sector {
            Sector::One => &mut self.progress.sector_one_best_ms,
            Sector::Two => &mut self.progress.sector_two_best_ms,
        };
        let millis = elapsed.as_millis() as u64;
        if best.is_none_or(|previous| millis < previous) {
            *best = Some(millis);
        }
        if let Some((store, slot)) = &self.save
            && let Err(error) = store.save(slot, &self.progress)
        {
            eprintln!("Night Relay could not save progress: {error}");
        }
    }
}

pub(crate) fn resolve_switch(world: &mut World) -> SystemResult {
    let Some(instance) = world
        .get_resource::<Campaign>()
        .and_then(|campaign| campaign.pending)
    else {
        return Ok(());
    };
    let manager = world.resource::<SceneManager>();
    if let Some(SceneLoadState::Failed(message)) = manager.state(instance) {
        eprintln!("Night Relay could not enter Sector 2: {message}");
        world.resource_mut::<SceneManager>().despawn_scene(instance);
        world.resource_mut::<Campaign>().pending = None;
    } else if manager.main_instance() == Some(instance) {
        bind_arena(world, instance, true)?;
        let mut campaign = world.resource_mut::<Campaign>();
        campaign.sector = Sector::Two;
        campaign.pending = None;
    }
    Ok(())
}
