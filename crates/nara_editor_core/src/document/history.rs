use std::collections::{BTreeMap, BTreeSet};

use nara_core::EntityUuid;
use nara_scene::authoring::{
    SceneAsset, SceneDocument, SceneDocumentError, SceneEntity, SceneSchemaRebind,
};

const HISTORY_LIMIT: usize = 256;

#[derive(Clone, Default)]
pub(super) struct CommandHistory {
    pub(super) entries: Vec<HistoryEntry>,
    pub(super) cursor: usize,
    base_revision: u64,
    next_revision: u64,
}

#[derive(Clone)]
pub(super) struct HistoryEntry {
    pub(super) patch: ScenePatch,
    revision: u64,
}

impl CommandHistory {
    pub(super) fn revision(&self) -> u64 {
        self.cursor
            .checked_sub(1)
            .map_or(self.base_revision, |index| self.entries[index].revision)
    }

    pub(super) fn push(&mut self, patch: ScenePatch) -> bool {
        if patch.is_empty() {
            return false;
        }
        self.entries.truncate(self.cursor);
        self.insert_before_redo(patch);
        true
    }

    pub(super) fn insert_before_redo(&mut self, patch: ScenePatch) {
        if self.entries.len() == HISTORY_LIMIT {
            if self.cursor == 0 {
                self.entries.pop();
            } else {
                self.base_revision = self.entries.remove(0).revision;
                self.cursor -= 1;
            }
        }
        self.next_revision += 1;
        self.entries.insert(
            self.cursor,
            HistoryEntry {
                patch,
                revision: self.next_revision,
            },
        );
        self.cursor += 1;
    }

    #[cfg(any(unix, windows))]
    pub(super) fn rebase_redo(&mut self, scene: &SceneDocument) {
        if self.cursor == self.entries.len() {
            return;
        }
        let mut future = scene.clone();
        self.entries
            .extract_if(self.cursor.., |entry| {
                let before = future.clone();
                entry.patch.apply(&mut future, true);
                entry.patch = ScenePatch::between(&before, &future);
                entry.patch.is_empty()
            })
            .for_each(drop);
    }

    pub(super) fn rebind(
        &mut self,
        rebind: &SceneSchemaRebind<'_>,
        assets: &BTreeMap<String, SceneAsset>,
    ) -> Result<(), SceneDocumentError> {
        let (undo, redo) = self.entries.split_at_mut(self.cursor);
        rebind_history_rows(undo.iter_mut().rev(), rebind, assets, false)?;
        rebind_history_rows(redo.iter_mut(), rebind, assets, true)
    }
}

fn rebind_history_rows<'a>(
    entries: impl Iterator<Item = &'a mut HistoryEntry>,
    rebind: &SceneSchemaRebind<'_>,
    mut assets: &'a BTreeMap<String, SceneAsset>,
    forward: bool,
) -> Result<(), SceneDocumentError> {
    for entry in entries {
        let patch = &mut entry.patch;
        let (before, after) = patch
            .assets
            .as_ref()
            .map_or((assets, assets), |patch| (&patch.before, &patch.after));
        rebind.rebind_rows(&mut patch.before, before)?;
        rebind.rebind_rows(&mut patch.after, after)?;
        assets = if forward { after } else { before };
    }
    Ok(())
}

#[derive(Clone, Default)]
pub(super) struct ScenePatch {
    entities: BTreeSet<EntityUuid>,
    before: Vec<SceneEntity>,
    after: Vec<SceneEntity>,
    assets: Option<AssetPatch>,
}

#[derive(Clone)]
struct AssetPatch {
    before: BTreeMap<String, SceneAsset>,
    after: BTreeMap<String, SceneAsset>,
}

impl ScenePatch {
    fn is_empty(&self) -> bool {
        self.entities.is_empty() && self.assets.is_none()
    }

    pub(super) fn from_rows(rows: impl IntoIterator<Item = (SceneEntity, SceneEntity)>) -> Self {
        let mut patch = Self::default();
        for (before, after) in rows {
            patch.entities.insert(before.id);
            patch.before.push(before);
            patch.after.push(after);
        }
        patch
    }

    pub(super) fn between(before: &SceneDocument, after: &SceneDocument) -> Self {
        let after_rows: BTreeMap<_, _> = after.entities.iter().map(|row| (row.id, row)).collect();
        let before_ids: BTreeSet<_> = before.entities.iter().map(|row| row.id).collect();
        let mut patch = Self::default();
        for row in &before.entities {
            if after_rows.get(&row.id).copied() != Some(row) {
                patch.entities.insert(row.id);
                patch.before.push(row.clone());
            }
        }
        for row in &after.entities {
            if !before_ids.contains(&row.id) || patch.entities.contains(&row.id) {
                patch.entities.insert(row.id);
                patch.after.push(row.clone());
            }
        }
        if before.assets != after.assets {
            patch.assets = Some(AssetPatch {
                before: before.assets.clone(),
                after: after.assets.clone(),
            });
        }
        patch
    }

    pub(super) fn apply(&self, scene: &mut SceneDocument, forward: bool) {
        let rows = if forward { &self.after } else { &self.before };
        scene
            .entities
            .retain(|row| !self.entities.contains(&row.id));
        scene.entities.extend(rows.iter().cloned());
        scene.entities.sort_unstable_by_key(|row| row.id);
        if let Some(assets) = &self.assets {
            scene.assets = if forward {
                &assets.after
            } else {
                &assets.before
            }
            .clone();
        }
    }
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::*;
    use crate::{EditorCommand, EditorDocument, EditorRegistry, EditorSession};

    fn session() -> EditorSession {
        let mut scene = SceneDocument::new("97270f99-fd97-483f-a605-a1e7acaea745".parse().unwrap());
        scene.entities.push(SceneEntity::new(
            "fc46193e-2e80-4a44-934a-000000000001".parse().unwrap(),
            "Initial",
        ));
        EditorSession::new(
            EditorDocument::new(scene).unwrap(),
            EditorRegistry::default(),
        )
    }

    fn rename(session: &mut EditorSession, name: String) {
        let entity = session.document().scene().entities[0].id;
        assert!(
            session
                .apply(EditorCommand::Rename { entity, name })
                .unwrap()
        );
    }

    #[test]
    fn eviction_preserves_the_saved_revision_at_the_undo_boundary() {
        let mut session = session();
        let folder = tempfile::tempdir().unwrap();
        rename(&mut session, "Saved edit".into());
        session
            .save_as(folder.path().join("saved.scene.ron"))
            .unwrap();
        for step in 0..HISTORY_LIMIT {
            rename(&mut session, format!("Edit {step}"));
        }
        for _ in 0..HISTORY_LIMIT {
            assert!(session.undo().unwrap());
        }
        assert!(!session.undo().unwrap());
        assert!(!session.is_dirty());
        assert_eq!(session.document().scene().entities[0].name, "Saved edit");

        rename(&mut session, "New branch".into());
        assert!(session.is_dirty());
        assert!(!session.redo().unwrap());
        assert!(session.undo().unwrap());
        assert!(!session.is_dirty());
    }

    #[test]
    fn reloading_at_the_undo_boundary_keeps_the_nearest_redo_entries() {
        let mut session = session();
        let folder = tempfile::tempdir().unwrap();
        session
            .save_as(folder.path().join("saved.scene.ron"))
            .unwrap();
        for step in 0..HISTORY_LIMIT {
            rename(&mut session, format!("Edit {step}"));
        }
        for _ in 0..HISTORY_LIMIT {
            assert!(session.undo().unwrap());
        }
        let mut external = session.document().scene().clone();
        external.entities[0].name = "External".into();
        external
            .write(folder.path().join("saved.scene.ron"))
            .unwrap();
        let disk = EditorDocument::open(folder.path().join("saved.scene.ron")).unwrap();
        assert!(session.reload_sources(Some(disk), BTreeMap::new()).unwrap());
        assert_eq!(session.document().scene(), &external);
        assert!(!session.is_dirty());
        assert!(session.undo().unwrap());
        assert_eq!(session.document().scene().entities[0].name, "Initial");
        assert!(session.is_dirty());
        assert!(session.redo().unwrap());
        assert!(!session.is_dirty());
        for step in 0..HISTORY_LIMIT - 1 {
            assert!(session.redo().unwrap());
            assert_eq!(
                session.document().scene().entities[0].name,
                format!("Edit {step}")
            );
        }
        assert!(!session.redo().unwrap());
    }

    #[test]
    fn reloading_discards_noop_redo_without_evicting_valid_edits_or_moving_the_save_cursor() {
        let mut session = session();
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("saved.scene.ron");
        session.save_as(&path).unwrap();
        for step in 0..HISTORY_LIMIT {
            rename(&mut session, format!("Edit {step}"));
        }
        for _ in 0..HISTORY_LIMIT {
            assert!(session.undo().unwrap());
        }
        let mut external = session.document().scene().clone();
        external.entities[0].name = "Edit 0".into();
        external.write(&path).unwrap();
        assert!(
            session
                .reload_sources(Some(EditorDocument::open(path).unwrap()), BTreeMap::new())
                .unwrap()
        );
        assert!(!session.is_dirty());
        assert!(session.undo().unwrap());
        assert_eq!(session.document().scene().entities[0].name, "Initial");
        assert!(session.is_dirty());
        assert!(session.redo().unwrap());
        assert_eq!(session.document().scene(), &external);
        assert!(!session.is_dirty());
        for step in 1..HISTORY_LIMIT {
            assert!(session.redo().unwrap());
            assert_eq!(
                session.document().scene().entities[0].name,
                format!("Edit {step}")
            );
            assert!(session.is_dirty());
        }
        assert!(!session.can_redo());
        assert!(!session.redo().unwrap());
        for _ in 1..HISTORY_LIMIT {
            assert!(session.undo().unwrap());
        }
        assert_eq!(session.document().scene(), &external);
        assert!(!session.is_dirty());
    }
}
