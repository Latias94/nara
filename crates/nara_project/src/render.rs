use nara_diagnostic::DiagnosticReport;
use serde::{Deserialize, Serialize};

use crate::validation::{error, with_field_path, with_public_u64};

/// Optional world presentation settings, independent of window and UI resolution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRenderManifest {
    #[serde(default)]
    pub world_canvas: Option<ProjectWorldCanvasManifest>,
}

impl ProjectRenderManifest {
    pub(crate) fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if let Some(canvas) = self.world_canvas {
            canvas.validate_into(diagnostics, &format!("{prefix}.world_canvas"));
        }
    }
}

/// A fixed world raster extent using IntegerFit and nearest final sampling.
///
/// Runtime UI continues to rasterize at the native window resolution. The selected render
/// backend checks this positive extent against its device and allocation limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectWorldCanvasManifest {
    pub width: u32,
    pub height: u32,
}

impl ProjectWorldCanvasManifest {
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        for (field, extent) in [("width", self.width), ("height", self.height)] {
            if extent == 0 {
                let diagnostic = with_field_path(
                    error(
                        "project.render.invalid-world-canvas-extent",
                        "World canvas width and height must be greater than zero",
                    ),
                    &format!("{prefix}.{field}"),
                );
                diagnostics.push(with_public_u64(diagnostic, "actual", u64::from(extent)));
            }
        }
    }
}

/// Replaces or clears the entire inherited world canvas in one profile.
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
        if let Some(canvas) = self.world_canvas {
            canvas.validate_into(diagnostics, &format!("{prefix}.world_canvas"));
        }
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

/// Resolved world presentation data; it does not request a rendering capability.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EffectiveRenderSettings {
    pub world_canvas: Option<EffectiveWorldCanvas>,
}

impl From<ProjectRenderManifest> for EffectiveRenderSettings {
    fn from(render: ProjectRenderManifest) -> Self {
        Self {
            world_canvas: render.world_canvas.map(EffectiveWorldCanvas::from),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveWorldCanvas {
    pub width: u32,
    pub height: u32,
}

impl From<ProjectWorldCanvasManifest> for EffectiveWorldCanvas {
    fn from(canvas: ProjectWorldCanvasManifest) -> Self {
        Self {
            width: canvas.width,
            height: canvas.height,
        }
    }
}

#[cfg(test)]
mod tests;
