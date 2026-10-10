use crate::camera::Camera;

#[derive(Debug)]
/// Everything an overlay needs to draw over the finished world pass.
pub struct OverlayCtx<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub color: &'a wgpu::TextureView,
    /// Reverse-Z `Depth32Float` holding the world depth.
    pub depth: &'a wgpu::TextureView,
    pub format: wgpu::TextureFormat,
    pub size: (u32, u32),
    pub camera: &'a Camera,
}

pub trait Overlay {
    /// Runs inside the world pass between opaque and translucent terrain; bind groups are restored afterwards.
    fn opaque(&mut self, _pass: &mut wgpu::RenderPass<'_>) {}

    fn record(&mut self, ctx: &mut OverlayCtx<'_>);
}
