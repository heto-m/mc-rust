use crate::camera::Camera;
use crate::oit::{OitInfo, OitStage};

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
    /// Set while `improvedTransparency` is on; translucent work drawn by `draw_oit` skips `record`.
    pub oit: Option<OitInfo<'a>>,
}

pub trait Overlay {
    /// Runs inside the world pass between opaque and translucent terrain; bind groups are restored afterwards.
    fn opaque(&mut self, _pass: &mut wgpu::RenderPass<'_>) {}

    /// Improved transparency: uploads translucent work for [`Overlay::draw_oit`]; false keeps it classic.
    fn prepare_oit(&mut self, _ctx: &mut OverlayCtx<'_>) -> bool {
        false
    }

    /// Draws translucent work into one improved transparency stage; `oit` binds the stage's inputs
    /// with the layout from [`OitInfo`].
    fn draw_oit(
        &mut self,
        _pass: &mut wgpu::RenderPass<'_>,
        _stage: OitStage,
        _oit: &wgpu::BindGroup,
    ) {
    }

    /// World-space work after the world pass (entities, particles, the hand).
    fn record(&mut self, ctx: &mut OverlayCtx<'_>);

    /// Screen-space UI; with an external submitter it lands after the external GUI.
    fn record_ui(&mut self, _ctx: &mut OverlayCtx<'_>) {}
}
