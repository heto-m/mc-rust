use anyhow::{Context, Result};
use mcr_assets::ResourceManager;
use mcr_render::{Overlay, OverlayCtx, wgpu};
use mcr_ui::{ItemIcons, MemoryClipboard, ScreenAssets, Ui, UiRenderer};

/// Everything that draws GUI: owned by the shell between sessions and lent to a session while it runs.
pub struct Gui {
    pub ui: Ui,
    pub ui_renderer: UiRenderer,
    pub icons: ItemIcons,
    pub assets: ScreenAssets,
    pub clipboard: MemoryClipboard,
}

impl std::fmt::Debug for Gui {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gui").finish_non_exhaustive()
    }
}

impl Gui {
    pub fn load(
        resources: &ResourceManager,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> Result<Self> {
        let mut ui = Ui::load(resources).context("loading ui")?;
        let icons = ItemIcons::load(resources).context("loading item icons")?;
        let assets = ScreenAssets::load(&mut ui, resources).context("loading screen textures")?;
        Ok(Self {
            ui,
            ui_renderer: UiRenderer::new(device, format),
            icons,
            assets,
            clipboard: MemoryClipboard::default(),
        })
    }
}

pub fn gui_size(framebuffer: (u32, u32), max_scale: i32) -> ((i32, i32), u32) {
    let scale = mcr_ui::gui_scale(
        framebuffer.0,
        framebuffer.1,
        u32::try_from(max_scale).unwrap_or(0),
    );
    (
        (
            framebuffer.0.div_ceil(scale) as i32,
            framebuffer.1.div_ceil(scale) as i32,
        ),
        scale,
    )
}

pub fn open_url(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    let _ = std::process::Command::new("xdg-open")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Draws only the GUI batch after the world pass.
pub struct UiOverlay<'a> {
    pub ui_renderer: &'a mut UiRenderer,
    pub ui: &'a mut Ui,
}

impl Overlay for UiOverlay<'_> {
    fn record(&mut self, ctx: &mut OverlayCtx<'_>) {
        self.ui_renderer.prepare(ctx.device, ctx.queue, self.ui);
        let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.ui_renderer.draw(&mut pass);
    }
}
