use std::sync::Arc;

use nara_render::RenderFrameSourceGeneration;
use nara_render_wgpu::{
    AccessRole, WgpuFeatureDefinition, WgpuFeatureInstance, WgpuProgramBuffer,
    WgpuProgramBufferOutput, WgpuProgramOutput, WgpuProgramOutputs, WgpuProgramRead,
    WgpuProgramTexture, WgpuProgramTextureOutput, WgpuRenderFeature, WgpuRenderHost,
    WgpuRenderProducer, WgpuRenderProgram, WgpuRenderProgramBuilder,
};

use super::{features::*, model::*};

pub const HORIZONTAL: u64 = 80;
pub const VERTICAL: u64 = 70;

pub struct Installed {
    upload: WgpuFeatureInstance<Upload>,
    pub horizontal: WgpuFeatureInstance<Blur>,
    pub vertical: WgpuFeatureInstance<Blur>,
    complement: WgpuFeatureInstance<Complement>,
    luminance: WgpuFeatureInstance<Luminance>,
    normalize: WgpuFeatureInstance<Normalize>,
    histogram: WgpuFeatureInstance<Histogram>,
}

pub fn install<F: WgpuRenderFeature>(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    feature: F,
) -> WgpuFeatureInstance<F> {
    let definition = WgpuFeatureDefinition::new(feature);
    host.create_feature_instance(producer, &definition)
        .expect("install an exact immutable Feature definition")
}

impl Installed {
    pub fn new(host: &mut WgpuRenderHost, producer: &WgpuRenderProducer) -> Self {
        Self {
            upload: install(host, producer, Upload),
            horizontal: install(host, producer, Blur(Axis::Horizontal)),
            vertical: install(host, producer, Blur(Axis::Vertical)),
            complement: install(host, producer, Complement),
            luminance: install(host, producer, Luminance),
            normalize: install(host, producer, Normalize),
            histogram: install(host, producer, Histogram),
        }
    }
}

#[derive(Clone)]
pub struct Ports {
    pub scene: WgpuProgramTexture<SceneColor>,
    pub blurred: WgpuProgramTexture<SceneColor>,
    pub effect: WgpuProgramTexture<EffectColor>,
    pub histogram: WgpuProgramTexture<HistogramColor>,
    pub raw: WgpuProgramBuffer<RawCounts>,
    pub frequencies: WgpuProgramBuffer<Frequencies>,
}

impl Ports {
    pub fn outputs(&self) -> [WgpuProgramOutput; 6] {
        // Publication order deliberately differs from both reservation and declaration order.
        [
            self.frequencies.output(),
            self.histogram.output(),
            self.scene.output(),
            self.raw.output(),
            self.effect.output(),
            self.blurred.output(),
        ]
    }

    pub fn copy_reads(&self) -> [WgpuProgramRead; 6] {
        [
            self.scene.read(AccessRole::CopySource),
            self.blurred.read(AccessRole::CopySource),
            self.effect.read(AccessRole::CopySource),
            self.histogram.read(AccessRole::CopySource),
            self.raw.read(AccessRole::CopySource),
            self.frequencies.read(AccessRole::CopySource),
        ]
    }

    pub fn published(&self, outputs: &WgpuProgramOutputs) -> Published {
        Published {
            scene: outputs
                .texture(&self.scene)
                .expect("the exact scene port was published"),
            blurred: outputs
                .texture(&self.blurred)
                .expect("the exact blurred port was published"),
            effect: outputs
                .texture(&self.effect)
                .expect("the exact effect port was published"),
            histogram: outputs
                .texture(&self.histogram)
                .expect("the exact histogram port was published"),
            raw: outputs
                .buffer(&self.raw)
                .expect("the exact raw port was published"),
            frequencies: outputs
                .buffer(&self.frequencies)
                .expect("the exact normalized port was published"),
        }
    }
}

pub struct Published {
    pub scene: WgpuProgramTextureOutput<SceneColor>,
    pub blurred: WgpuProgramTextureOutput<SceneColor>,
    pub effect: WgpuProgramTextureOutput<EffectColor>,
    pub histogram: WgpuProgramTextureOutput<HistogramColor>,
    pub raw: WgpuProgramBufferOutput<RawCounts>,
    pub frequencies: WgpuProgramBufferOutput<Frequencies>,
}

impl Published {
    pub fn import(&self, builder: &mut WgpuRenderProgramBuilder) -> Ports {
        Ports {
            scene: builder
                .import_texture(&self.scene)
                .expect("import the exact retained scene lease"),
            blurred: builder
                .import_texture(&self.blurred)
                .expect("import the exact retained blur lease"),
            effect: builder
                .import_texture(&self.effect)
                .expect("import the exact retained effect lease"),
            histogram: builder
                .import_texture(&self.histogram)
                .expect("import the exact retained histogram lease"),
            raw: builder
                .import_buffer(&self.raw)
                .expect("import the exact retained raw lease"),
            frequencies: builder
                .import_buffer(&self.frequencies)
                .expect("import the exact retained normalized lease"),
        }
    }
}

pub struct Workload {
    pub program: WgpuRenderProgram,
    pub ports: Ports,
}

pub fn build(
    source: RenderFrameSourceGeneration,
    frame: u64,
    pixels: Arc<[[f32; 4]]>,
    installed: &Installed,
) -> Workload {
    let mut builder =
        WgpuRenderProgramBuilder::new(source, frame, limits()).expect("bounded workload");
    let scene = builder.texture::<SceneColor>(texture_descriptor()).unwrap();
    let horizontal = builder.texture::<SceneColor>(texture_descriptor()).unwrap();
    let blurred = builder.texture::<SceneColor>(texture_descriptor()).unwrap();
    let effect = builder
        .texture::<EffectColor>(texture_descriptor())
        .unwrap();
    let histogram = builder
        .texture::<HistogramColor>(texture_descriptor())
        .unwrap();
    let raw = builder.buffer::<RawCounts>(buffer_descriptor()).unwrap();
    let frequencies = builder.buffer::<Frequencies>(buffer_descriptor()).unwrap();

    // A serial executor following insertion order or ascending key order gets this wrong.
    builder
        .operation(
            30,
            &installed.histogram,
            Arc::new(HistogramCapture {
                frequencies: frequencies.clone(),
                destination: histogram.clone(),
            }),
            &[frequencies.read(AccessRole::Vertex)],
            &[histogram.preserve(&effect, AccessRole::Attachment)],
        )
        .unwrap();
    builder
        .operation(
            40,
            &installed.normalize,
            Arc::new(NormalizeCapture {
                frequencies: frequencies.clone(),
            }),
            &[],
            &[frequencies.preserve(&raw, AccessRole::StorageReadWrite)],
        )
        .unwrap();
    builder
        .operation(
            60,
            &installed.complement,
            Arc::new(ComplementCapture {
                source: blurred.clone(),
                destination: effect.clone(),
            }),
            &[blurred.read(AccessRole::Sampled)],
            &[effect.clear(wgpu::Color::TRANSPARENT, AccessRole::StorageWrite)],
        )
        .unwrap();
    builder
        .operation(
            VERTICAL,
            &installed.vertical,
            Arc::new(BlurCapture {
                source: horizontal.clone(),
                destination: blurred.clone(),
            }),
            &[horizontal.read(AccessRole::Sampled)],
            &[blurred.clear(wgpu::Color::TRANSPARENT, AccessRole::StorageWrite)],
        )
        .unwrap();
    builder
        .operation(
            50,
            &installed.luminance,
            Arc::new(LuminanceCapture {
                source: scene.clone(),
                counts: raw.clone(),
            }),
            &[scene.read(AccessRole::Sampled)],
            &[raw.zero(AccessRole::StorageReadWrite)],
        )
        .unwrap();
    builder
        .operation(
            HORIZONTAL,
            &installed.horizontal,
            Arc::new(BlurCapture {
                source: scene.clone(),
                destination: horizontal.clone(),
            }),
            &[scene.read(AccessRole::Sampled)],
            &[horizontal.clear(wgpu::Color::TRANSPARENT, AccessRole::StorageWrite)],
        )
        .unwrap();
    builder
        .operation(
            90,
            &installed.upload,
            Arc::new(UploadCapture {
                destination: scene.clone(),
                pixels,
            }),
            &[],
            &[scene.clear(wgpu::Color::TRANSPARENT, AccessRole::CopyDestination)],
        )
        .unwrap();

    let ports = Ports {
        scene,
        blurred,
        effect,
        histogram,
        raw,
        frequencies,
    };
    let program = builder
        .finish(&ports.outputs())
        .expect("complete independently authored workload");
    Workload { program, ports }
}
