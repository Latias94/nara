use nara_ecs::{Entity, Query, Res, ResMut};
use nara_ui::{ComputedUiLayouts, UiPanel};

use crate::{ExtractedUiItem, ExtractedUiItems, ExtractedUiMaterial, UiRenderStats};

pub fn extract_ui(
    mut extracted: ResMut<ExtractedUiItems>,
    mut stats: ResMut<UiRenderStats>,
    layouts: Res<ComputedUiLayouts>,
    panels: Query<(Entity, &UiPanel)>,
) {
    extracted.clear();
    stats.extracted_panels = 0;

    for layout in layouts.as_slice() {
        if !layout.visible || !layout.rect.is_non_empty() {
            continue;
        }
        let Ok((entity, panel)) = panels.get(layout.entity) else {
            continue;
        };
        let source_order = extracted.len() as u64;
        extracted.push(ExtractedUiItem {
            entity,
            source_order,
            root: layout.root,
            view_index: layout.view_index,
            target: layout.target,
            order: layout.order,
            z_index: layout.z_index,
            rect: layout.rect,
            clip_rect: layout.clip_rect,
            material: ExtractedUiMaterial {
                image: panel.material.image,
                sampler: panel.material.sampler,
                alpha_mode: panel.material.alpha_mode,
                tint: panel.material.tint,
            },
        });
        stats.extracted_panels = stats.extracted_panels.saturating_add(1);
    }
}
