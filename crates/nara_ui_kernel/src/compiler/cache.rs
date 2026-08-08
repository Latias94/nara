use std::{collections::BTreeSet, sync::Arc};

use crate::{
    UiEnvironment, UiKey, UiLayer, UiLayoutFact, UiPaintPrimitive, UiPhysicalRect, UiRect,
    UiRootKey,
};

use super::order::RootOrdered;

#[derive(Clone, Debug)]
pub(super) struct RootLayoutFact {
    pub(super) key: Option<UiKey>,
    pub(super) route_parent: Option<UiKey>,
    pub(super) root_key: UiRootKey,
    pub(super) structural_index: usize,
    pub(super) local_z: i32,
    pub(super) rect: UiRect,
    pub(super) physical_rect: UiPhysicalRect,
    pub(super) clip_rect: Option<UiRect>,
    pub(super) visible: bool,
}

impl RootLayoutFact {
    pub(super) fn bind(&self, layer: UiLayer) -> UiLayoutFact {
        UiLayoutFact {
            key: self.key.clone(),
            route_parent: self.route_parent.clone(),
            root_key: self.root_key.clone(),
            layer,
            structural_index: self.structural_index,
            local_z: self.local_z,
            rect: self.rect,
            physical_rect: self.physical_rect,
            clip_rect: self.clip_rect,
            visible: self.visible,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct RootVisual {
    pub(super) layouts: Vec<RootOrdered<RootLayoutFact>>,
    pub(super) paint: Vec<RootOrdered<UiPaintPrimitive>>,
    pub(super) shaped_glyphs: usize,
}

impl RootVisual {
    pub(super) fn node_count(&self) -> usize {
        self.layouts.len()
    }

    pub(super) fn paint_count(&self) -> usize {
        self.paint.len()
    }
}

#[derive(Debug, Default)]
pub(super) struct RootCache {
    entries: Vec<CachedRoot>,
}

impl RootCache {
    pub(super) fn compatible(
        &self,
        key: &UiRootKey,
        environment: UiEnvironment,
    ) -> Option<Arc<RootVisual>> {
        self.entries
            .iter()
            .find(|entry| &entry.key == key && entry.environment == environment)
            .map(|entry| Arc::clone(&entry.visual))
    }

    pub(super) fn commit(
        &mut self,
        clock: u64,
        maximum: usize,
        authored_keys: &BTreeSet<UiRootKey>,
        touched: &BTreeSet<UiRootKey>,
        published: impl IntoIterator<Item = (UiRootKey, UiEnvironment, Arc<RootVisual>)>,
    ) {
        self.entries
            .retain(|entry| authored_keys.contains(&entry.key));
        for entry in &mut self.entries {
            if touched.contains(&entry.key) {
                entry.last_used = clock;
            }
        }
        for (key, environment, visual) in published {
            if let Some(entry) = self.entries.iter_mut().find(|entry| entry.key == key) {
                entry.environment = environment;
                entry.visual = visual;
                entry.last_used = clock;
            } else {
                self.entries.push(CachedRoot {
                    key,
                    environment,
                    visual,
                    last_used: clock,
                });
            }
        }
        while self.entries.len() > maximum {
            let Some((oldest, _)) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| (entry.last_used, entry.key.clone()))
            else {
                break;
            };
            self.entries.remove(oldest);
        }
    }
}

#[derive(Clone, Debug)]
struct CachedRoot {
    key: UiRootKey,
    environment: UiEnvironment,
    visual: Arc<RootVisual>,
    last_used: u64,
}
