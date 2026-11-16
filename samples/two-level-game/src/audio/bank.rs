use nara::{asset::AssetRef, project_host::ProjectDomainAssets};

use crate::sound_bank::{SoundBank, SoundBankDomain, bank_product};

/// Keeps the exact decoded bank and its original content reservation together.
#[derive(Clone, Debug)]
pub(crate) struct RetainedSoundBank {
    assets: ProjectDomainAssets,
    reference: AssetRef,
}

impl RetainedSoundBank {
    pub(super) fn acquire(
        assets: &ProjectDomainAssets,
        reference: &AssetRef,
    ) -> Result<Self, String> {
        let bank = assets
            .collection::<SoundBankDomain>()
            .and_then(|collection| collection.get(reference))
            .ok_or("the declared Sound Bank was not acquired")?;
        if bank.product() != bank_product() {
            return Err("the game requires its declared primary Sound Bank Product".into());
        }
        Ok(Self {
            assets: assets.clone(),
            reference: reference.clone(),
        })
    }

    pub(super) fn bank(&self) -> &SoundBank {
        self.assets
            .collection::<SoundBankDomain>()
            .and_then(|collection| collection.get(&self.reference))
            .expect("retained immutable Sound Bank collection")
    }

    pub(super) fn assets(&self) -> &ProjectDomainAssets {
        &self.assets
    }

    pub(crate) fn validate(
        &self,
        assets: &ProjectDomainAssets,
        reference: &AssetRef,
    ) -> Result<(), String> {
        let proposed = assets
            .collection::<SoundBankDomain>()
            .and_then(|collection| collection.get(reference))
            .ok_or("prepared Scene lacks its declared Sound Bank")?;
        if proposed != self.bank() {
            return Err("the audio Runtime's Sound Bank changed; start a fresh Runtime".into());
        }
        Ok(())
    }
}
