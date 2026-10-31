//! Viable explicit order and slot controls, sharing exact kernels and owned physical versions.

use super::{effects::Effects, gpu::Gpu};

pub(super) const WIDTH: u32 = 64;
pub(super) const HEIGHT: u32 = 64;

#[derive(Clone, Copy, Debug)]
pub(super) enum Control {
    Ordered,
    Slots,
    Schedule,
}

#[derive(Clone)]
pub(super) struct Outputs {
    pub original: wgpu::Texture,
    pub horizontal: wgpu::Texture,
    pub blurred: wgpu::Texture,
    pub complement: wgpu::Texture,
    pub composite: wgpu::Texture,
    pub histogram: wgpu::Texture,
    pub raw: wgpu::Buffer,
    pub normalized: wgpu::Buffer,
}

pub(super) fn execute(
    gpu: &Gpu,
    effects: &Effects,
    pixels: &[[f32; 4]],
    control: Control,
) -> Outputs {
    let texture = |label| {
        gpu.rgba32_texture(
            label,
            WIDTH,
            HEIGHT,
            wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
    };
    // Each simultaneous output is physically separate. Keeping the original, blur and final
    // composite in this owner is the control's explicit lifetime proof; scratch is not recycled.
    let output = Outputs {
        original: texture("control-original"),
        horizontal: texture("control-horizontal"),
        blurred: texture("control-blurred"),
        complement: texture("control-complement"),
        composite: texture("control-composite"),
        histogram: texture("control-histogram"),
        raw: gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("control-raw-counts"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
        normalized: gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("control-normalized"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        }),
    };
    gpu.upload_rgba32(&output.original, WIDTH, HEIGHT, pixels);
    let view =
        |texture: &wgpu::Texture| texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("control-workload"),
        });
    for texture in [
        &output.horizontal,
        &output.blurred,
        &output.complement,
        &output.composite,
        &output.histogram,
    ] {
        clear(&mut encoder, texture);
    }
    encoder.clear_buffer(&output.raw, 0, None);
    if matches!(control, Control::Schedule) {
        let commands = schedule::encode(&gpu.device, effects, &output, encoder);
        gpu.queue.submit([commands]);
        return output;
    }
    match control {
        Control::Ordered => {
            effects.encode_blur_horizontal(
                &gpu.device,
                &mut encoder,
                &view(&output.original),
                &view(&output.horizontal),
            );
            effects.encode_blur_vertical(
                &gpu.device,
                &mut encoder,
                &view(&output.horizontal),
                &view(&output.blurred),
            );
        }
        Control::Slots => BlurSlot {
            effects,
            horizontal: &output.horizontal,
        }
        .encode(gpu, &mut encoder, &output.original, &output.blurred),
        Control::Schedule => unreachable!("schedule control encoded the complete workload above"),
    }
    effects.encode_complement(
        &gpu.device,
        &mut encoder,
        &view(&output.blurred),
        &view(&output.complement),
    );
    effects.encode_hud(
        &gpu.device,
        &mut encoder,
        &view(&output.complement),
        &view(&output.composite),
    );
    // A retained original feeds a separate branch. Normalization copies to a distinct successor
    // and cannot overwrite the raw statistics that another View/consumer still needs.
    effects.encode_luminance(
        &gpu.device,
        &mut encoder,
        &view(&output.original),
        &output.raw,
    );
    encoder.copy_buffer_to_buffer(&output.raw, 0, &output.normalized, 0, 16);
    effects.encode_normalize(&gpu.device, &mut encoder, &output.normalized);
    effects.encode_histogram(&mut encoder, &output.normalized, &view(&output.histogram));
    gpu.queue.submit([encoder.finish()]);
    output
}

mod schedule {
    use nara_ecs::{
        Res, ResMut, Resource, World,
        schedule::{IntoScheduleConfigs, Schedule},
    };

    use super::{Effects, Outputs};

    #[derive(Resource)]
    struct Capture {
        device: wgpu::Device,
        effects: Effects,
        output: Outputs,
    }

    #[derive(Resource)]
    struct Encoding(Option<wgpu::CommandEncoder>);

    pub(super) fn encode(
        device: &wgpu::Device,
        effects: &Effects,
        output: &Outputs,
        encoder: wgpu::CommandEncoder,
    ) -> wgpu::CommandBuffer {
        // This viable open-schedule control carries captured native resources, not gameplay
        // queries. Its author supplies exact ordering and predecessor retention explicitly;
        // the external owner alone submits the resulting command buffer.
        let mut world = World::new();
        world.insert_resource(Capture {
            device: device.clone(),
            effects: effects.clone(),
            output: output.clone(),
        });
        world.insert_resource(Encoding(Some(encoder)));
        let mut schedule = Schedule::default();
        schedule.set_executor(nara_ecs::bevy_ecs::schedule::SingleThreadedExecutor::new());
        schedule.add_systems(
            (
                horizontal, vertical, complement, hud, luminance, normalize, histogram,
            )
                .chain(),
        );
        schedule.run(&mut world);
        world
            .remove_resource::<Encoding>()
            .unwrap()
            .0
            .unwrap()
            .finish()
    }

    fn view(texture: &wgpu::Texture) -> wgpu::TextureView {
        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }
    fn horizontal(c: Res<Capture>, mut e: ResMut<Encoding>) {
        c.effects.encode_blur_horizontal(
            &c.device,
            e.0.as_mut().unwrap(),
            &view(&c.output.original),
            &view(&c.output.horizontal),
        );
    }
    fn vertical(c: Res<Capture>, mut e: ResMut<Encoding>) {
        c.effects.encode_blur_vertical(
            &c.device,
            e.0.as_mut().unwrap(),
            &view(&c.output.horizontal),
            &view(&c.output.blurred),
        );
    }
    fn complement(c: Res<Capture>, mut e: ResMut<Encoding>) {
        c.effects.encode_complement(
            &c.device,
            e.0.as_mut().unwrap(),
            &view(&c.output.blurred),
            &view(&c.output.complement),
        );
    }
    fn hud(c: Res<Capture>, mut e: ResMut<Encoding>) {
        c.effects.encode_hud(
            &c.device,
            e.0.as_mut().unwrap(),
            &view(&c.output.complement),
            &view(&c.output.composite),
        );
    }
    fn luminance(c: Res<Capture>, mut e: ResMut<Encoding>) {
        c.effects.encode_luminance(
            &c.device,
            e.0.as_mut().unwrap(),
            &view(&c.output.original),
            &c.output.raw,
        );
    }
    fn normalize(c: Res<Capture>, mut e: ResMut<Encoding>) {
        let encoder = e.0.as_mut().unwrap();
        encoder.copy_buffer_to_buffer(&c.output.raw, 0, &c.output.normalized, 0, 16);
        c.effects
            .encode_normalize(&c.device, encoder, &c.output.normalized);
    }
    fn histogram(c: Res<Capture>, mut e: ResMut<Encoding>) {
        c.effects.encode_histogram(
            e.0.as_mut().unwrap(),
            &c.output.normalized,
            &view(&c.output.histogram),
        );
    }
}

struct BlurSlot<'a> {
    effects: &'a Effects,
    horizontal: &'a wgpu::Texture,
}

impl BlurSlot<'_> {
    fn encode(
        &self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        destination: &wgpu::Texture,
    ) {
        let view =
            |texture: &wgpu::Texture| texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.effects.encode_blur_horizontal(
            &gpu.device,
            encoder,
            &view(source),
            &view(self.horizontal),
        );
        self.effects.encode_blur_vertical(
            &gpu.device,
            encoder,
            &view(self.horizontal),
            &view(destination),
        );
    }
}

pub(super) fn clear(encoder: &mut wgpu::CommandEncoder, texture: &wgpu::Texture) {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("comparison-explicit-clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
}
