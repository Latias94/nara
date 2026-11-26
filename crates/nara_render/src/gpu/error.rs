#[derive(Debug, thiserror::Error)]
pub enum WgpuRenderError {
    #[error("WGPU device was lost")]
    DeviceLost,
    #[error("WGPU device error: {0}")]
    Device(String),
    #[error("invalid render target: {0}")]
    InvalidTarget(&'static str),
    #[error("render resource exceeds capacity: {0}")]
    Capacity(&'static str),
    #[error("all pooled render targets are still in use")]
    TargetsBusy,
    #[error("WGPU validation failed: {0}")]
    Validation(String),
}
