use std::sync::{Arc, Mutex};

use nara::{ProductConfiguration, ProductRecipe, ProductRecipeError};
use nara_render_wgpu::{
    AccessRole, GraphLimits, WgpuDeviceRequirements, WgpuFeatureDeclaration, WgpuFeatureDefinition,
    WgpuFeatureInstance, WgpuPolicyBindingContext, WgpuPolicyBuildContext, WgpuPolicyDefinition,
    WgpuPolicyProgram, WgpuProgramTexture, WgpuRenderPolicy, WgpuRenderProgramError,
    WgpuSceneColor, WgpuStockBindings, WgpuStockCapture, WgpuStockCaptureAdapter, WgpuStockStages,
    WgpuWorldColor,
};
use nara_stock_blur_effect::{Blur, BlurCapture};
use nara_stock_grade_effect::{Grade, GradeCapture, GradeParameters};

use crate::CompositionPlugin;

/// Blur radius counts texels in the explicitly selected space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectSpace {
    World,
    Resolved,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompositionConfiguration {
    pub space: EffectSpace,
    pub radius: u32,
    pub grade: GradeParameters,
    pub enabled: bool,
}

impl Default for CompositionConfiguration {
    fn default() -> Self {
        Self {
            space: EffectSpace::Resolved,
            radius: 1,
            grade: GradeParameters {
                exposure_stops: -0.5,
                saturation: 0.65,
                contrast: 1.1,
            },
            enabled: true,
        }
    }
}

impl ProductConfiguration for CompositionConfiguration {
    fn write_canonical(&self, bytes: &mut Vec<u8>) {
        bytes.extend_from_slice(b"consumer.stock-composition.v1\0");
        bytes.push(match self.space {
            EffectSpace::World => 0,
            EffectSpace::Resolved => 1,
        });
        bytes.push(u8::from(self.enabled));
        bytes.extend_from_slice(&self.radius.to_le_bytes());
        for parameter in [
            self.grade.exposure_stops,
            self.grade.saturation,
            self.grade.contrast,
        ] {
            bytes.extend_from_slice(&parameter.to_bits().to_le_bytes());
        }
    }
}

/// CPU graph handles for inspecting a prepared candidate's exact published branches.
/// Recording a handle is not evidence of native submission or publication.
#[derive(Clone)]
pub struct CompositionPorts {
    pub original_world: WgpuProgramTexture<WgpuWorldColor>,
    pub original_scene: Option<WgpuProgramTexture<WgpuSceneColor>>,
    pub selected: SelectedPorts,
}

#[derive(Clone)]
pub enum SelectedPorts {
    World {
        blurred: WgpuProgramTexture<WgpuWorldColor>,
        graded: WgpuProgramTexture<WgpuWorldColor>,
        resolved: Option<WgpuProgramTexture<WgpuSceneColor>>,
    },
    Resolved {
        blurred: WgpuProgramTexture<WgpuSceneColor>,
        graded: WgpuProgramTexture<WgpuSceneColor>,
    },
    Disabled,
}

pub struct CompositionCapture {
    pub stock: Arc<WgpuStockCapture>,
    pub configuration: CompositionConfiguration,
    // Optional finite test observation. It stores CPU handles only; outputs.texture validates
    // actual publication. Each capture is built at most a few times by these finite consumers.
    inspection: Option<Arc<Mutex<Vec<CompositionPorts>>>>,
}

impl CompositionCapture {
    pub fn new(stock: Arc<WgpuStockCapture>, configuration: CompositionConfiguration) -> Self {
        Self {
            stock,
            configuration,
            inspection: None,
        }
    }

    pub fn inspected(
        stock: Arc<WgpuStockCapture>,
        configuration: CompositionConfiguration,
        inspection: Arc<Mutex<Vec<CompositionPorts>>>,
    ) -> Self {
        Self {
            stock,
            configuration,
            inspection: Some(inspection),
        }
    }

    fn observe(&self, ports: CompositionPorts) -> Result<(), WgpuRenderProgramError> {
        if let Some(inspection) = &self.inspection {
            let mut inspection = inspection.lock().map_err(|_| {
                WgpuRenderProgramError::feature("composition inspection mutex poisoned")
            })?;
            if inspection.len() >= 16 {
                return Err(WgpuRenderProgramError::feature(
                    "finite composition inspection full",
                ));
            }
            inspection.push(ports);
        }
        Ok(())
    }
}

pub struct CompositionPolicy {
    stock: WgpuStockStages,
    world_blur: WgpuFeatureDeclaration<Blur<WgpuWorldColor, WgpuWorldColor>>,
    world_grade: WgpuFeatureDeclaration<Grade<WgpuWorldColor, WgpuWorldColor>>,
    scene_blur: WgpuFeatureDeclaration<Blur<WgpuSceneColor, WgpuSceneColor>>,
    scene_grade: WgpuFeatureDeclaration<Grade<WgpuSceneColor, WgpuSceneColor>>,
}

impl Default for CompositionPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl CompositionPolicy {
    pub fn new() -> Self {
        Self {
            stock: WgpuStockStages::new(),
            world_blur: WgpuFeatureDefinition::new(Blur::new()).declare(),
            world_grade: WgpuFeatureDefinition::new(Grade::new()).declare(),
            scene_blur: WgpuFeatureDefinition::new(Blur::new()).declare(),
            scene_grade: WgpuFeatureDefinition::new(Grade::new()).declare(),
        }
    }
}

pub struct CompositionBindings {
    stock: WgpuStockBindings,
    world_blur: Arc<WgpuFeatureInstance<Blur<WgpuWorldColor, WgpuWorldColor>>>,
    world_grade: Arc<WgpuFeatureInstance<Grade<WgpuWorldColor, WgpuWorldColor>>>,
    scene_blur: Arc<WgpuFeatureInstance<Blur<WgpuSceneColor, WgpuSceneColor>>>,
    scene_grade: Arc<WgpuFeatureInstance<Grade<WgpuSceneColor, WgpuSceneColor>>>,
}

impl WgpuRenderPolicy for CompositionPolicy {
    type Capture = CompositionCapture;
    type Bindings = CompositionBindings;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        self.stock
            .device_requirements()
            .union(self.world_blur.device_requirements())
            .union(self.world_grade.device_requirements())
            .union(self.scene_blur.device_requirements())
            .union(self.scene_grade.device_requirements())
    }

    fn bind(
        &self,
        _: &CompositionCapture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<CompositionBindings, WgpuRenderProgramError> {
        Ok(CompositionBindings {
            stock: self.stock.bind(context)?,
            world_blur: context.feature(&self.world_blur)?,
            world_grade: context.feature(&self.world_grade)?,
            scene_blur: context.feature(&self.scene_blur)?,
            scene_grade: context.feature(&self.scene_grade)?,
        })
    }

    fn validate_capture(
        &self,
        packet: &nara::render::RenderFramePacket,
        capture: &CompositionCapture,
    ) -> Result<(), WgpuRenderProgramError> {
        WgpuStockStages::validate_capture(packet, &capture.stock)
    }

    fn build(
        &self,
        bindings: &CompositionBindings,
        capture: &CompositionCapture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        let mut program = self.stock.begin(
            &bindings.stock,
            &capture.stock,
            context,
            GraphLimits::default(),
        )?;
        let original_world = program.world(10)?;
        let has_canvas = context.packet().world_canvas().is_some();
        let original_scene = has_canvas
            .then(|| program.resolve_canvas(20, &original_world))
            .transpose()?;
        let mut outputs = vec![original_world.output()];
        if let Some(scene) = &original_scene {
            outputs.push(scene.output());
        }
        let configuration = capture.configuration;
        if !configuration.enabled {
            capture.observe(CompositionPorts {
                original_world: original_world.clone(),
                original_scene: original_scene.clone(),
                selected: SelectedPorts::Disabled,
            })?;
            return if let Some(scene) = &original_scene {
                program.finish(90, scene, &outputs)
            } else {
                program.finish(90, &original_world, &outputs)
            };
        }
        // Without Canvas the world already has target dimensions; there is no invented resolve.
        match (configuration.space, original_scene.as_ref()) {
            (EffectSpace::World, _) | (_, None) => {
                let (blurred, graded) = effects(
                    &mut program,
                    &original_world,
                    &bindings.world_blur,
                    &bindings.world_grade,
                    configuration,
                )?;
                outputs.extend([blurred.output(), graded.output()]);
                let resolved = has_canvas
                    .then(|| program.resolve_canvas(50, &graded))
                    .transpose()?;
                capture.observe(CompositionPorts {
                    original_world,
                    original_scene,
                    selected: SelectedPorts::World {
                        blurred,
                        graded: graded.clone(),
                        resolved: resolved.clone(),
                    },
                })?;
                if let Some(scene) = &resolved {
                    outputs.push(scene.output());
                    program.finish(90, scene, &outputs)
                } else {
                    program.finish(90, &graded, &outputs)
                }
            }
            (EffectSpace::Resolved, Some(scene)) => {
                let (blurred, graded) = effects(
                    &mut program,
                    scene,
                    &bindings.scene_blur,
                    &bindings.scene_grade,
                    configuration,
                )?;
                outputs.extend([blurred.output(), graded.output()]);
                capture.observe(CompositionPorts {
                    original_world,
                    original_scene,
                    selected: SelectedPorts::Resolved {
                        blurred,
                        graded: graded.clone(),
                    },
                })?;
                program.finish(90, &graded, &outputs)
            }
        }
    }
}

fn effects<S: 'static>(
    program: &mut nara_render_wgpu::WgpuStockProgramBuilder<'_>,
    source: &WgpuProgramTexture<S>,
    blur: &WgpuFeatureInstance<Blur<S, S>>,
    grade: &WgpuFeatureInstance<Grade<S, S>>,
    configuration: CompositionConfiguration,
) -> Result<(WgpuProgramTexture<S>, WgpuProgramTexture<S>), WgpuRenderProgramError> {
    let blurred = program.graph().texture::<S>(source.descriptor().clone())?;
    program.graph().operation(
        30,
        blur,
        Arc::new(BlurCapture {
            source: source.clone(),
            destination: blurred.clone(),
            radius: configuration.radius,
        }),
        &[source.read(AccessRole::Sampled)],
        &[blurred.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
    )?;
    let graded = program.graph().texture::<S>(source.descriptor().clone())?;
    program.graph().operation(
        40,
        grade,
        Arc::new(GradeCapture {
            source: blurred.clone(),
            destination: graded.clone(),
            parameters: configuration.grade,
        }),
        &[blurred.read(AccessRole::Sampled)],
        &[graded.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
    )?;
    Ok((blurred, graded))
}

/// Selects the same independent composition for product Runtime and Editor preview owners.
pub fn recipe(
    configuration: CompositionConfiguration,
) -> Result<ProductRecipe, ProductRecipeError> {
    configured_recipe(configuration, None)
}

/// Selects the same policy while observing bounded CPU graph ports from fresh Editor adapters.
/// The native test must join these handles to the actual publication before reading outputs.
/// Runtime captures retain their ordinary collector and original UI responsibility.
pub fn recipe_with_inspection(
    configuration: CompositionConfiguration,
    inspection: Arc<Mutex<Vec<CompositionPorts>>>,
) -> Result<ProductRecipe, ProductRecipeError> {
    configured_recipe(configuration, Some(inspection))
}

fn configured_recipe(
    configuration: CompositionConfiguration,
    inspection: Option<Arc<Mutex<Vec<CompositionPorts>>>>,
) -> Result<ProductRecipe, ProductRecipeError> {
    ProductRecipe::new().select_wgpu_policy::<CompositionPlugin, _, _, _>(
        configuration,
        |configuration| CompositionPlugin(*configuration),
        move |configuration| {
            let configuration = *configuration;
            let inspection = inspection.clone();
            Ok(WgpuStockCaptureAdapter::new(
                WgpuPolicyDefinition::new(CompositionPolicy::new()).install()?,
                move |stock| {
                    Ok(Arc::new(if let Some(inspection) = &inspection {
                        CompositionCapture::inspected(
                            Arc::new(stock),
                            configuration,
                            Arc::clone(inspection),
                        )
                    } else {
                        CompositionCapture::new(Arc::new(stock), configuration)
                    }))
                },
            ))
        },
    )
}
