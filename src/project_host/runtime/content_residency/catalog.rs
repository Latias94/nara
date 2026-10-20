use std::collections::{BTreeMap, BTreeSet};

use nara_asset::{AssetPath, AssetRecord, ProjectAssetDatabase};

use super::super::{HostFault, single_error};

pub(super) struct CatalogChange {
    pub(super) database: ProjectAssetDatabase,
    pub(super) owned: Vec<AssetRecord>,
}

impl CatalogChange {
    pub(super) fn prepare(
        current: &ProjectAssetDatabase,
        owned: &[AssetRecord],
        target: impl IntoIterator<Item = AssetRecord>,
    ) -> Result<Self, HostFault> {
        let mut target_by_path = BTreeMap::<AssetPath, AssetRecord>::new();
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
        let prior_owned = owned
            .iter()
            .map(|record| record.path())
            .collect::<BTreeSet<_>>();
        for record in owned {
            if current.record_for_path(record.path()) != Some(record) {
                return Err(conflict());
            }
            if !target_by_path.contains_key(record.path()) {
                assert!(database.remove_exact(record));
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
}

fn conflict() -> HostFault {
    HostFault::new(single_error(
        "project.scene.asset-catalog-conflict",
        "Scene asset registrations conflict with their current owners",
    ))
}
