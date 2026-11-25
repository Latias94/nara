use nara_core::diagnostic::DiagnosticReport;
use serde::{Deserialize, Serialize};

use crate::validation::{error, with_field_path};
use crate::{EffectiveRenderSettings, ProjectRenderManifest, ProjectWorldCanvasManifest};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRenderPatch {
    #[serde(default)]
    pub world_canvas: Option<ProjectWorldCanvasManifest>,
    #[serde(default)]
    pub clear_world_canvas: bool,
}

impl ProjectRenderPatch {
    pub(crate) fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        ProjectRenderManifest {
            world_canvas: self.world_canvas,
        }
        .validate_into(diagnostics, prefix);
        if self.clear_world_canvas && self.world_canvas.is_some() {
            diagnostics.push(with_field_path(
                error(
                    "project.render.conflicting-world-canvas-patch",
                    "Render profile patch cannot both clear and replace the world canvas",
                ),
                &format!("{prefix}.world_canvas"),
            ));
        }
    }

    pub(crate) fn apply_to(&self, render: &mut EffectiveRenderSettings) {
        if self.clear_world_canvas {
            render.world_canvas = None;
        }
        if let Some(canvas) = self.world_canvas {
            render.world_canvas = Some(canvas.into());
        }
    }
}

#[cfg(test)]
mod tests;
