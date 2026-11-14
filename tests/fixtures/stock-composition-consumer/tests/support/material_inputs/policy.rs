use std::sync::{Arc, Mutex, atomic::AtomicUsize};

use nara::{
    material::{Material2dBinding, MaterialParameterValue},
    render::{Extent2d, RenderImage2d, RenderImagePacket},
};
use nara_render_wgpu::{
    AccessRole, GraphError, GraphLimits, WgpuDeviceRequirements, WgpuFeatureDeclaration,
    WgpuFeatureDefinition, WgpuFeatureInstance, WgpuMaterialTextureBinding,
    WgpuPolicyBindingContext, WgpuPolicyBuildContext, WgpuPolicyDefinition, WgpuPolicyProgram,
    WgpuProgramTexture, WgpuProgramTextureOutput, WgpuPublishedFrame, WgpuRenderHost,
    WgpuRenderHostError, WgpuRenderInput, WgpuRenderPolicy, WgpuRenderProducer,
    WgpuRenderProgramBuilder, WgpuRenderProgramError, WgpuStockBindings, WgpuStockCapture,
    WgpuStockStages, WgpuWorldColor,
};
use nara_stock_grade_effect::{Grade, GradeCapture};

use crate::scene;

use super::{
    ACCENT_ID, Branch, GRADE, PALETTE_ID, Palette, SCALE, SCALE_ID, descriptor,
    feature::{PaletteCapture, PaletteWriter},
};

#[derive(Clone)]
pub enum Source {
    Generated {
        colors: [[f32; 4]; 4],
        format: wgpu::TextureFormat,
    },
    Imported(WgpuProgramTextureOutput<Palette>),
}

#[derive(Clone, Copy, Debug)]
pub enum Wiring {
    Exact,
    Duplicate,
    ForeignGraph,
    MissingWriter,
    CpuOnly,
    RetryUnmatched,
    RetryOperation,
    RetryPlainWorld,
    SharedSlots,
}

#[derive(Clone)]
pub struct Ports {
    pub palette: WgpuProgramTexture<Palette>,
    pub branch: WgpuProgramTexture<Branch>,
    pub world: WgpuProgramTexture<WgpuWorldColor>,
}

pub struct Capture {
    stock: Arc<WgpuStockCapture>,
    material: Material2dBinding,
    source: Source,
    wiring: Wiring,
    limits: GraphLimits,
    inspection: Arc<Mutex<Option<Ports>>>,
}

struct Policy {
    stock: WgpuStockStages,
    writer: WgpuFeatureDeclaration<PaletteWriter>,
    grade: WgpuFeatureDeclaration<Grade<Palette, Branch>>,
}

pub struct Bindings {
    stock: WgpuStockBindings,
    writer: Arc<WgpuFeatureInstance<PaletteWriter>>,
    grade: Arc<WgpuFeatureInstance<Grade<Palette, Branch>>>,
}

pub struct Scenario {
    pub input: WgpuRenderInput,
    pub live: Arc<AtomicUsize>,
    inspection: Arc<Mutex<Option<Ports>>>,
}

impl Scenario {
    pub fn new(
        original: &WgpuRenderInput,
        stock: Arc<WgpuStockCapture>,
        material: Material2dBinding,
        source: Source,
        wiring: Wiring,
    ) -> Self {
        Self::with_limits(
            original,
            stock,
            material,
            source,
            wiring,
            GraphLimits::default(),
        )
    }

    pub fn with_limits(
        original: &WgpuRenderInput,
        stock: Arc<WgpuStockCapture>,
        material: Material2dBinding,
        source: Source,
        wiring: Wiring,
        limits: GraphLimits,
    ) -> Self {
        let live = Arc::new(AtomicUsize::new(0));
        let policy = Policy {
            stock: WgpuStockStages::new(),
            writer: WgpuFeatureDefinition::new(PaletteWriter {
                live: Arc::clone(&live),
            })
            .declare(),
            grade: WgpuFeatureDefinition::new(Grade::new()).declare(),
        };
        let installation = WgpuPolicyDefinition::new(policy).install().unwrap();
        let inspection = Arc::new(Mutex::new(None));
        let input = installation
            .capture(
                original.packet().clone(),
                Arc::new(Capture {
                    stock,
                    material,
                    source,
                    wiring,
                    limits,
                    inspection: Arc::clone(&inspection),
                }),
            )
            .unwrap();
        drop(installation);
        Self {
            input,
            live,
            inspection,
        }
    }

    pub fn ports(&self) -> Ports {
        self.inspection.lock().unwrap().as_ref().unwrap().clone()
    }
}

impl WgpuRenderPolicy for Policy {
    type Capture = Capture;
    type Bindings = Bindings;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        self.stock
            .device_requirements()
            .union(self.writer.device_requirements())
            .union(self.grade.device_requirements())
    }

    fn bind(
        &self,
        _: &Capture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Bindings, WgpuRenderProgramError> {
        Ok(Bindings {
            stock: self.stock.bind(context)?,
            writer: context.feature(&self.writer)?,
            grade: context.feature(&self.grade)?,
        })
    }

    fn validate_capture(
        &self,
        packet: &nara::render::RenderFramePacket,
        capture: &Capture,
    ) -> Result<(), WgpuRenderProgramError> {
        WgpuStockStages::validate_capture(packet, &capture.stock)
    }

    fn build(
        &self,
        bindings: &Bindings,
        capture: &Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        let mut stock =
            self.stock
                .begin(&bindings.stock, &capture.stock, context, capture.limits)?;
        let palette = match &capture.source {
            Source::Generated { format, .. } if matches!(capture.wiring, Wiring::ForeignGraph) => {
                let mut foreign = WgpuRenderProgramBuilder::new(
                    context.packet().source_generation(),
                    context.packet().frame_index(),
                    GraphLimits::default(),
                )?;
                let palette = foreign.texture::<Palette>(descriptor(*format))?;
                foreign.initialize(
                    80,
                    &[palette.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
                )?;
                let _foreign_program = foreign.finish(&[palette.output()])?;
                palette
            }
            Source::Generated { format, .. } => {
                stock.graph().texture::<Palette>(descriptor(*format))?
            }
            Source::Imported(output) => stock.graph().import_texture(output)?,
        };
        let selected = WgpuMaterialTextureBinding::new(&capture.material, PALETTE_ID, &palette)?;
        assert_eq!(selected.descriptor(), palette.descriptor());
        let mut selections = vec![selected];
        if matches!(capture.wiring, Wiring::Duplicate) {
            selections.push(selections[0].clone());
        }
        if matches!(capture.wiring, Wiring::SharedSlots) {
            selections.push(WgpuMaterialTextureBinding::new(
                &capture.material,
                ACCENT_ID,
                &palette,
            )?);
        }
        if matches!(capture.wiring, Wiring::RetryUnmatched) {
            let mut values = capture.material.parameters().clone();
            values.insert(
                SCALE_ID.to_owned(),
                MaterialParameterValue::Vec4([SCALE[0], SCALE[1], SCALE[2], -0.0]),
            );
            let unmatched = capture
                .material
                .generation()
                .bind_values(values, capture.material.textures().clone())
                .map_err(|error| WgpuRenderProgramError::feature(error.to_string()))?;
            let refused = WgpuMaterialTextureBinding::new(&unmatched, PALETTE_ID, &palette)?;
            let facts = *stock.facts_mut();
            match stock.world_with_material_textures(10, &[refused]) {
                Err(WgpuRenderProgramError::Feature { message }) => {
                    assert!(message.contains("no matching captured draw"), "{message}");
                }
                Err(error) => panic!("unrelated same-builder refusal: {error}"),
                Ok(_) => panic!("an unmatched Material selection consumed the world operation"),
            }
            assert_eq!(stock.facts_mut().frame, facts.frame);
            assert_eq!(stock.facts_mut().image_sprites, facts.image_sprites);
            assert_eq!(
                stock.facts_mut().instance_buffer_bytes,
                facts.instance_buffer_bytes
            );
        }
        if matches!(
            capture.wiring,
            Wiring::RetryOperation | Wiring::RetryPlainWorld
        ) {
            stock.graph().initialize(
                80,
                &[palette.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
            )?;
            let facts = *stock.facts_mut();
            let refused = if matches!(capture.wiring, Wiring::RetryPlainWorld) {
                stock.world(80)
            } else {
                stock.world_with_material_textures(80, &selections)
            };
            assert!(matches!(
                refused,
                Err(WgpuRenderProgramError::Graph(
                    GraphError::DuplicateOperationKey(80)
                ))
            ));
            assert_eq!(stock.facts_mut().frame, facts.frame);
            assert_eq!(stock.facts_mut().image_sprites, facts.image_sprites);
            assert_eq!(
                stock.facts_mut().instance_buffer_bytes,
                facts.instance_buffer_bytes
            );
        }
        let world = stock.world_with_material_textures(10, &selections)?;
        if matches!(
            capture.wiring,
            Wiring::CpuOnly | Wiring::RetryUnmatched | Wiring::SharedSlots
        ) {
            stock.graph().initialize(
                80,
                &[palette.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
            )?;
            return stock.finish(100, &world, &[world.output()]);
        }
        if matches!(
            capture.wiring,
            Wiring::RetryOperation | Wiring::RetryPlainWorld
        ) {
            return stock.finish(100, &world, &[world.output()]);
        }
        if matches!(capture.wiring, Wiring::MissingWriter) {
            return stock.finish(100, &world, &[world.output()]);
        }
        let branch = stock
            .graph()
            .texture::<Branch>(palette.descriptor().clone())?;
        stock.graph().operation(
            5,
            &bindings.grade,
            Arc::new(GradeCapture {
                source: palette.clone(),
                destination: branch.clone(),
                parameters: GRADE,
            }),
            &[palette.read(AccessRole::Sampled)],
            &[branch.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
        )?;
        if let Source::Generated { colors, .. } = &capture.source {
            stock.graph().operation(
                80,
                &bindings.writer,
                Arc::new(PaletteCapture {
                    destination: palette.clone(),
                    colors: *colors,
                }),
                &[],
                &[palette.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
            )?;
        }
        let result = stock.finish(
            100,
            &world,
            &[palette.output(), branch.output(), world.output()],
        )?;
        *capture
            .inspection
            .lock()
            .map_err(|_| WgpuRenderProgramError::feature("material inspection poisoned"))? =
            Some(Ports {
                palette,
                branch,
                world,
            });
        Ok(result)
    }
}

pub fn publish(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    original: &WgpuRenderInput,
) -> Result<WgpuPublishedFrame, WgpuRenderHostError> {
    let target = RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        Extent2d::new(scene::WIDTH, scene::HEIGHT).unwrap(),
    )
    .unwrap();
    let input = original.with_image_target(target)?;
    assert_eq!(
        input.packet().source_generation(),
        original.packet().source_generation()
    );
    assert_eq!(
        input.packet().frame_index(),
        original.packet().frame_index()
    );
    assert_eq!(input.packet().views(), original.packet().views());
    let reservation =
        host.reserve_image_target(producer, input.packet().source_generation(), target)?;
    host.activate_image_target(producer, reservation)?;
    let candidate = host.prepare(producer, &input)?;
    host.submit_candidate(producer, candidate)
        .map_err(|failure| failure.error)
}
