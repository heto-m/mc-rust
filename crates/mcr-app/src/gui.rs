use std::sync::Arc;

use anyhow::{Context, Result};
use mcr_assets::ResourceManager;
use mcr_render::{Overlay, OverlayCtx, wgpu};
use mcr_ui::{ItemIcons, MemoryClipboard, ScreenAssets, ToastManager, Ui, UiRenderer};

/// Everything that draws GUI: owned by the shell between sessions and lent to a session while it runs.
pub struct Gui {
    pub ui: Ui,
    pub ui_renderer: UiRenderer,
    pub icons: ItemIcons,
    pub assets: ScreenAssets,
    pub clipboard: MemoryClipboard,
    pub toasts: ToastManager,
    resources: Arc<ResourceManager>,
    language: String,
}

impl std::fmt::Debug for Gui {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gui").finish_non_exhaustive()
    }
}

impl Gui {
    pub fn load(
        resources: Arc<ResourceManager>,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> Result<Self> {
        let mut ui = Ui::load(&resources).context("loading ui")?;
        let icons = ItemIcons::load(&resources).context("loading item icons")?;
        let assets = ScreenAssets::load(&mut ui, &resources).context("loading screen textures")?;
        Ok(Self {
            ui,
            ui_renderer: UiRenderer::new(device, format),
            icons,
            assets,
            clipboard: MemoryClipboard::default(),
            toasts: ToastManager::default(),
            resources,
            language: mcr_ui::DEFAULT_LANGUAGE.to_owned(),
        })
    }
}

impl Gui {
    /// Rebuilds the fonts, language, sprites and item icons; on error nothing changes.
    pub fn reload(&mut self, resources: &Arc<ResourceManager>) -> Result<()> {
        let mut ui = Ui::load(resources).context("loading ui")?;
        let icons = ItemIcons::load(resources).context("loading item icons")?;
        let assets = ScreenAssets::load(&mut ui, resources).context("loading screen textures")?;
        self.ui = ui;
        self.icons = icons;
        self.assets = assets;
        self.ui_renderer.reset_pages();
        self.resources = Arc::clone(resources);
        if let Err(e) = self.ui.set_language(&self.resources, &self.language) {
            tracing::warn!("language {} failed to reload: {e}", self.language);
        }
        Ok(())
    }

    pub fn sync_language(&mut self, code: &str) {
        if self.language == code {
            return;
        }
        self.language = code.to_owned();
        if let Err(e) = self.ui.set_language(&self.resources, code) {
            tracing::warn!("language {code} failed to load: {e}");
        }
    }

    /// Advances the toasts and returns the sound events they want played.
    pub fn tick_toasts(&mut self, now_ms: u64, display_time: f64) -> Vec<&'static str> {
        self.toasts.update(now_ms, display_time)
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
    fn record(&mut self, _ctx: &mut OverlayCtx<'_>) {}

    fn record_ui(&mut self, ctx: &mut OverlayCtx<'_>) {
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
