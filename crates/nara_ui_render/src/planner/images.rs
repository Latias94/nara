use std::collections::{BTreeMap, BTreeSet};

use nara_image::{ImageAsset, PreparedImageSnapshot, PreparedImages};
use nara_render::RenderResourceKey;
use nara_ui_kernel::{UiFrame, UiImageId, UiPaintPrimitive, UiResolvedBrush};

#[derive(Debug, Default)]
pub struct UiPreparedImages {
    images: BTreeMap<UiImageId, PreparedImageSnapshot>,
    ambiguous: BTreeMap<UiImageId, (RenderResourceKey, RenderResourceKey)>,
}

impl UiPreparedImages {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: UiImageId, snapshot: PreparedImageSnapshot) {
        self.insert_snapshot(id, snapshot);
    }

    pub(crate) fn capture(
        referenced: &BTreeSet<UiImageId>,
        prepared: Option<&PreparedImages>,
    ) -> Self {
        let Some(prepared) = prepared else {
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
            let id = UiImageId::external::<ImageAsset>(key.asset_id().raw());
            if referenced.contains(&id) {
                index.insert_snapshot(id, snapshot.clone());
            }
        }
        index
    }

    fn insert_snapshot(&mut self, id: UiImageId, snapshot: PreparedImageSnapshot) {
        if self.ambiguous.contains_key(&id) {
            return;
        }
        match self.images.get(&id) {
            None => {
                self.images.insert(id, snapshot);
            }
            Some(existing) if existing.identity() == snapshot.identity() => {}
            Some(existing) => {
                let existing_key = existing.identity().key();
                let replacement_key = snapshot.identity().key();
                self.images.remove(&id);
                self.ambiguous.insert(id, (existing_key, replacement_key));
            }
        }
    }

    pub(super) fn resolve(
        &self,
        id: &UiImageId,
    ) -> Result<PreparedImageSnapshot, UiPreparedImageError> {
        if let Some((first, second)) = self.ambiguous.get(id).copied() {
            return Err(UiPreparedImageError::Ambiguous {
                id: id.to_string(),
                first,
                second,
            });
        }
        self.images
            .get(id)
            .cloned()
            .ok_or_else(|| UiPreparedImageError::Missing { id: id.to_string() })
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
