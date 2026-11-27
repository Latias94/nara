use std::{collections::BTreeMap, error::Error};

use nara::{
    reflect::ComponentValue,
    scene::authoring::{SceneComponent, SceneDocument, SceneEntity},
};
use nara_asset::ProjectAssetDatabase;

use super::SourceProject;

pub(crate) enum SceneLayers {
    Saved,
    Snapshot {
        initial: SceneDocument,
        baseline: Option<SceneDocument>,
    },
}

impl SceneLayers {
    pub fn capture(
        project: &SourceProject,
        assets: &ProjectAssetDatabase,
        initial: &SceneDocument,
        snapshot: bool,
    ) -> Self {
        if snapshot {
            let baseline = project
                .read_entry(assets)
                .ok()
                .filter(|scene| scene.id == initial.id);
            Self::Snapshot {
                initial: initial.clone(),
                baseline,
            }
        } else {
            Self::Saved
        }
    }

    pub fn read(
        &self,
        project: &SourceProject,
        assets: &ProjectAssetDatabase,
    ) -> Result<SceneDocument, Box<dyn Error>> {
        match self {
            Self::Saved => project.read_entry(assets),
            Self::Snapshot {
                initial,
                baseline: None,
            } => Ok(initial.clone()),
            Self::Snapshot {
                initial,
                baseline: Some(baseline),
            } => {
                let disk = project.read_entry(assets)?;
                if disk.id != baseline.id {
                    return Err(
                        "the saved Night Relay scene changed document UUID during Play".into(),
                    );
                }
                Ok(overlay(baseline, initial, disk))
            }
        }
    }
}

fn overlay(
    baseline: &SceneDocument,
    initial: &SceneDocument,
    mut disk: SceneDocument,
) -> SceneDocument {
    let index = |document: &SceneDocument| {
        document
            .entities
            .iter()
            .cloned()
            .map(|row| (row.id, row))
            .collect::<BTreeMap<_, _>>()
    };
    let disk_rows = std::mem::take(&mut disk.entities)
        .into_iter()
        .map(|row| (row.id, row))
        .collect();
    disk.entities = overlay_map(&index(baseline), &index(initial), disk_rows, overlay_row)
        .into_values()
        .collect();
    disk
}

fn overlay_row(
    baseline: &SceneEntity,
    snapshot: &SceneEntity,
    mut disk: SceneEntity,
) -> SceneEntity {
    if baseline.name != snapshot.name {
        disk.name.clone_from(&snapshot.name);
    }
    if baseline.parent != snapshot.parent {
        disk.parent = snapshot.parent;
    }
    disk.components = overlay_map(
        &baseline.components,
        &snapshot.components,
        disk.components,
        overlay_component,
    );
    disk
}

fn overlay_component(
    baseline: &SceneComponent,
    snapshot: &SceneComponent,
    disk: SceneComponent,
) -> SceneComponent {
    match (baseline, snapshot, disk) {
        (
            SceneComponent::Known {
                version: baseline_version,
                value: ComponentValue::Map(baseline),
            },
            SceneComponent::Known {
                version: snapshot_version,
                value: ComponentValue::Map(snapshot),
            },
            SceneComponent::Known {
                version,
                value: ComponentValue::Map(disk),
            },
        ) if *baseline_version == version && *snapshot_version == version => SceneComponent::known(
            version,
            ComponentValue::Map(overlay_map(baseline, snapshot, disk, |_, snapshot, _| {
                snapshot.clone()
            })),
        ),
        (_, snapshot, _) => snapshot.clone(),
    }
}

fn overlay_map<K: Ord + Clone, V: Clone + PartialEq>(
    baseline: &BTreeMap<K, V>,
    snapshot: &BTreeMap<K, V>,
    mut disk: BTreeMap<K, V>,
    merge: impl Fn(&V, &V, V) -> V,
) -> BTreeMap<K, V> {
    for (key, old) in baseline {
        match snapshot.get(key) {
            None => {
                disk.remove(key);
            }
            Some(edited) if edited != old => {
                let value = disk
                    .remove(key)
                    .map_or_else(|| edited.clone(), |current| merge(old, edited, current));
                disk.insert(key.clone(), value);
            }
            Some(_) => {}
        }
    }
    for (key, value) in snapshot {
        if !baseline.contains_key(key) {
            disk.insert(key.clone(), value.clone());
        }
    }
    disk
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RunRules;
    use nara::prelude::Transform2d;

    #[test]
    fn snapshot_pins_edited_fields_while_saved_fields_follow_disk() -> Result<(), Box<dyn Error>> {
        let mut baseline = SceneDocument::new("221d4c8e-7992-41da-871b-030db8db544b".parse()?);
        let mut row = SceneEntity::new("0fe50e38-d5e6-4f62-888e-b7be19e091cf".parse()?, "Rules");
        row.set(&RunRules::default())?;
        row.set(&Transform2d::default())?;
        baseline.entities.push(row);
        let mut snapshot = baseline.clone();
        snapshot.entities[0].set(&RunRules {
            shield_charges: 4,
            ..RunRules::default()
        })?;
        let mut disk = baseline.clone();
        disk.entities[0].name = "Renamed".into();
        disk.entities[0].set(&RunRules {
            shield_charges: 2,
            move_speed: 210.0,
            ..RunRules::default()
        })?;
        disk.entities[0].set(&Transform2d::from_xy(17.0, -20.0))?;

        let merged = overlay(&baseline, &snapshot, disk);
        let row = &merged.entities[0];
        let rules = row.typed::<RunRules>()?.ok_or("missing rules")?;
        assert_eq!(rules.shield_charges, 4);
        assert_eq!(rules.move_speed, 210.0);
        assert_eq!(row.name, "Renamed");
        assert_eq!(
            row.typed::<Transform2d>()?,
            Some(Transform2d::from_xy(17.0, -20.0))
        );
        Ok(())
    }
}
