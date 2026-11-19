use std::collections::{BTreeMap, BTreeSet};

use nara_asset::{AssetRecord, AssetSourcePath, ProjectAssetDatabase};

use super::super::{HostFault, single_error};

pub(super) struct CatalogChange {
    pub(super) database: ProjectAssetDatabase,
    pub(super) owned: Vec<AssetRecord>,
}

impl CatalogChange {
    pub(super) fn prepare<'a>(
        current: &ProjectAssetDatabase,
        owned: impl Iterator<Item = &'a AssetRecord>,
        target: impl IntoIterator<Item = AssetRecord>,
    ) -> Result<Self, HostFault> {
        let mut target_by_path = BTreeMap::<AssetSourcePath, AssetRecord>::new();
        for record in target {
            if let Some(prior) = target_by_path.get(record.path()) {
                if prior != &record {
                    return Err(conflict());
                }
            } else {
                target_by_path.insert(record.path().clone(), record);
            }
        }
        let mut database = current.clone();
        let mut prior_owned = BTreeSet::new();
        for record in owned {
            prior_owned.insert(record.path());
            match current.record_for_path(record.path()) {
                Some(existing) if existing == record => {
                    if !target_by_path.contains_key(record.path()) {
                        assert!(database.remove_exact(record));
                    }
                }
                None if current.record_for_stable_id(record.stable_id()).is_none() => {}
                _ => return Err(conflict()),
            }
        }
        let mut next_owned = Vec::new();
        for record in target_by_path.into_values() {
            match database.record_for_path(record.path()) {
                Some(existing) if existing != &record => return Err(conflict()),
                Some(_) if prior_owned.contains(record.path()) => next_owned.push(record),
                Some(_) => {} // An unchanged foreign registration remains borrowed.
                None => {
                    database.insert(record.clone()).map_err(|_| conflict())?;
                    next_owned.push(record);
                }
            }
        }
        Ok(Self {
            database,
            owned: next_owned,
        })
    }

    pub(super) fn owned_record(&self, path: &AssetSourcePath) -> Option<&AssetRecord> {
        let index = self
            .owned
            .binary_search_by(|record| record.path().cmp(path))
            .ok()?;
        self.owned.get(index)
    }
}

fn conflict() -> HostFault {
    HostFault::new(single_error(
        "project.scene.asset-catalog-conflict",
        "Scene asset registrations conflict with their current owners",
    ))
}
