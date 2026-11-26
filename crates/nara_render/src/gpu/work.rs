/// Direct WGPU resource creation and uploads recorded by Nara.
/// WGPU's internal staging and native work from another renderer are outside these counts.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuNativeWorkStats {
    pub created_buffer_calls: u64,
    pub created_texture_calls: u64,
    pub issued_buffer_write_calls: u64,
    pub issued_texture_write_calls: u64,
}
