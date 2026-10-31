//! Disposable comparison, not qualification of a production render extension or GPU Host.

#![forbid(unsafe_code)]

#[path = "support/gpu.rs"]
mod gpu;

#[test]
#[ignore = "requires a real native GPU adapter; run explicitly and serially"]
fn window_free_gpu_harness_reads_back_linear_premultiplied_pixels() {
    let gpu = gpu::Gpu::new();
    let texture = gpu.rgba32_texture(
        "comparison-window-free-clear",
        64,
        64,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("comparison-window-free-clear"),
        });
    {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("comparison-window-free-clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.25,
                        g: 0.125,
                        b: 0.0,
                        a: 0.5,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    gpu.queue.submit([encoder.finish()]);
    let pixels = gpu.read_rgba32(&texture, 64, 64);
    assert_eq!(pixels.len(), 64 * 64);
    for pixel in pixels {
        for (actual, expected) in pixel.into_iter().zip([0.25, 0.125, 0.0, 0.5]) {
            assert!((actual - expected).abs() <= 1.0e-6);
        }
    }
}
