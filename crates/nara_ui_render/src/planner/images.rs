use std::collections::{BTreeMap, BTreeSet};

use nara_asset::AssetServer;
use nara_image::{PreparedImageSnapshot, PreparedImages};
use nara_render::RenderResourceKey;
use nara_ui_kernel::{UiFrame, UiImageId, UiPaintPrimitive, UiResolvedBrush};

#[derive(Debug, Default)]
pub struct UiPreparedImages {
    images: BTreeMap<String, PreparedImageSnapshot>,
    ambiguous: BTreeMap<String, (RenderResourceKey, RenderResourceKey)>,
}

impl UiPreparedImages {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: UiImageId, snapshot: PreparedImageSnapshot) {
        self.insert_alias(id.as_str().to_owned(), snapshot);
    }

    pub(crate) fn capture(
        referenced: &BTreeSet<UiImageId>,
        asset_server: Option<&AssetServer>,
        prepared: Option<&PreparedImages>,
    ) -> Self {
        let (Some(asset_server), Some(prepared)) = (asset_server, prepared) else {
            return Self::default();
        };
        let mut index = Self::default();
        if referenced.is_empty() {
            return index;
        }
        for key in prepared.keys() {
            let Some(snapshot) = prepared.get(key) else {
                continue;
            };
            let path = asset_server.path(key.asset_id());
            let stable_id = asset_server
                .stable_id(key.asset_id())
                .map(|id| id.to_string());
            for id in referenced {
                if !alias_matches(id.as_str(), path, stable_id.as_deref()) {
                    continue;
                }
                index.insert_alias(id.as_str().to_owned(), snapshot.clone());
            }
        }
        index
    }

    fn insert_alias(&mut self, alias: String, snapshot: PreparedImageSnapshot) {
        if self.ambiguous.contains_key(&alias) {
            return;
        }
        match self.images.get(&alias) {
            None => {
                self.images.insert(alias, snapshot);
            }
            Some(existing) if existing.identity() == snapshot.identity() => {}
            Some(existing) => {
                let existing_key = existing.identity().key();
                let replacement_key = snapshot.identity().key();
                self.images.remove(&alias);
                self.ambiguous
                    .insert(alias, (existing_key, replacement_key));
            }
        }
    }

    pub(super) fn resolve(
        &self,
        id: &UiImageId,
    ) -> Result<PreparedImageSnapshot, UiPreparedImageError> {
        if let Some((first, second)) = self.ambiguous.get(id.as_str()).copied() {
            return Err(UiPreparedImageError::Ambiguous {
                id: id.as_str().to_owned(),
                first,
                second,
            });
        }
        self.images
            .get(id.as_str())
            .cloned()
            .ok_or_else(|| UiPreparedImageError::Missing {
                id: id.as_str().to_owned(),
            })
    }
}

pub(super) enum UiPreparedImageError {
    Missing {
        id: String,
    },
    Ambiguous {
        id: String,
        first: RenderResourceKey,
        second: RenderResourceKey,
    },
}

pub(crate) fn collect_referenced_image_ids(
    frames: &[std::sync::Arc<UiFrame>],
) -> BTreeSet<UiImageId> {
    frames
        .iter()
        .flat_map(|frame| frame.paint())
        .filter_map(|primitive| match primitive {
            UiPaintPrimitive::Rect {
                brush: Some(UiResolvedBrush::Image { image, .. }),
                ..
            } => Some(image.clone()),
            _ => None,
        })
        .collect()
}

fn alias_matches(candidate: &str, path: Option<&str>, stable_id: Option<&str>) -> bool {
    path.is_some_and(|path| {
        candidate == path
            || candidate.strip_prefix("path:") == Some(path)
            || candidate.strip_prefix("project://") == Some(path)
    }) || stable_id.is_some_and(|stable_id| {
        candidate == stable_id || candidate.strip_prefix("stable:") == Some(stable_id)
    })
}

#[cfg(test)]
mod tests {
    use super::alias_matches;

    #[test]
    fn referenced_alias_matching_is_exact_and_domain_scoped() {
        assert!(alias_matches(
            "project://textures/hud.png",
            Some("textures/hud.png"),
            None
        ));
        assert!(alias_matches(
            "stable:6a24b57a-122a-4a33-a443-c05d0d24f478",
            None,
            Some("6a24b57a-122a-4a33-a443-c05d0d24f478")
        ));
        assert!(!alias_matches(
            "project://textures/hud.png.bak",
            Some("textures/hud.png"),
            None
        ));
    }
}
