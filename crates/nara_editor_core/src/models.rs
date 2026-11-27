use std::collections::{BTreeMap, BTreeSet, VecDeque};

use nara_core::{Diagnostic, DiagnosticSeverity, EntityUuid};
use nara_scene::authoring::SceneDocument;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    Replace,
    Toggle,
    Range,
}

#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    entities: Vec<EntityUuid>,
    anchor: Option<EntityUuid>,
}

impl SelectionState {
    pub fn entities(&self) -> &[EntityUuid] {
        &self.entities
    }

    pub fn active(&self) -> Option<EntityUuid> {
        self.entities.last().copied()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn replace(&mut self, entities: impl IntoIterator<Item = EntityUuid>) {
        let mut seen = BTreeSet::new();
        self.entities = entities
            .into_iter()
            .filter(|entity| seen.insert(*entity))
            .collect();
        self.anchor = self.active();
    }

    pub fn select(&mut self, entity: EntityUuid, mode: SelectionMode, visible: &[EntityUuid]) {
        match mode {
            SelectionMode::Replace => {
                self.entities.clear();
                self.entities.push(entity);
                self.anchor = Some(entity);
            }
            SelectionMode::Toggle => {
                if let Some(index) = self.entities.iter().position(|id| *id == entity) {
                    self.entities.remove(index);
                    if self.anchor == Some(entity) {
                        self.anchor = self.active();
                    }
                } else {
                    self.entities.push(entity);
                    self.anchor = Some(entity);
                }
            }
            SelectionMode::Range => {
                let from = self
                    .anchor
                    .and_then(|anchor| visible.iter().position(|id| *id == anchor));
                let to = visible.iter().position(|id| *id == entity);
                if let (Some(from), Some(to)) = (from, to) {
                    self.entities.clear();
                    self.entities.extend(
                        visible[from.min(to)..=from.max(to)]
                            .iter()
                            .copied()
                            .filter(|id| *id != entity),
                    );
                    self.entities.push(entity);
                } else {
                    self.select(entity, SelectionMode::Replace, visible);
                }
            }
        }
    }

    pub(crate) fn retain(&mut self, mut contains: impl FnMut(EntityUuid) -> bool) {
        self.entities.retain(|entity| contains(*entity));
        if self.anchor.is_some_and(|anchor| !contains(anchor)) {
            self.anchor = self.active();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HierarchyRow {
    pub entity: EntityUuid,
    pub name: String,
    pub depth: usize,
    pub has_children: bool,
}

#[derive(Debug, Clone, Default)]
pub struct HierarchyModel {
    collapsed: BTreeSet<EntityUuid>,
}

impl HierarchyModel {
    pub fn is_collapsed(&self, entity: EntityUuid) -> bool {
        self.collapsed.contains(&entity)
    }

    pub fn toggle(&mut self, entity: EntityUuid) {
        if !self.collapsed.remove(&entity) {
            self.collapsed.insert(entity);
        }
    }

    pub fn rows(&self, scene: &SceneDocument) -> Vec<HierarchyRow> {
        let mut groups = BTreeMap::<_, Vec<_>>::new();
        for entity in &scene.entities {
            groups.entry(entity.parent).or_default().push(entity);
        }
        let mut pending: Vec<_> = groups
            .remove(&None)
            .unwrap_or_default()
            .into_iter()
            .rev()
            .map(|row| (row, 0))
            .collect();
        let mut rows = Vec::new();
        while let Some((row, depth)) = pending.pop() {
            let children = groups.remove(&Some(row.id)).unwrap_or_default();
            rows.push(HierarchyRow {
                entity: row.id,
                name: row.name.clone(),
                depth,
                has_children: !children.is_empty(),
            });
            if !self.is_collapsed(row.id) {
                pending.extend(children.into_iter().rev().map(|row| (row, depth + 1)));
            }
        }
        rows
    }
}

/// A bounded Console shared by build, Schema, Play and native renderer diagnostics.
pub struct ConsoleModel {
    entries: VecDeque<Diagnostic>,
    capacity: usize,
}

impl Default for ConsoleModel {
    fn default() -> Self {
        Self::new(1_000)
    }
}

impl ConsoleModel {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity,
        }
    }

    pub fn entries(&self) -> &VecDeque<Diagnostic> {
        &self.entries
    }

    pub fn push(&mut self, entry: Diagnostic) {
        if self.capacity == 0 {
            return;
        }
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn line(&mut self, severity: DiagnosticSeverity, message: impl Into<String>) {
        self.push(Diagnostic {
            severity,
            message: message.into(),
            code: None,
            location: None,
        });
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
