use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use mcr_assets::ResourceManager;
use mcr_render::{Overlay, OverlayCtx, wgpu};
use mcr_ui::{ItemIcons, Lang, ScreenAssets, ToastManager, Ui, UiRenderer};

use crate::mod_input::UiClipboard;

/// Everything that draws GUI: owned by the shell between sessions and lent to a session while it runs.
pub struct Gui {
    pub ui: Ui,
    pub ui_renderer: UiRenderer,
    pub icons: ItemIcons,
    pub assets: ScreenAssets,
    pub clipboard: UiClipboard,
    pub toasts: ToastManager,
    resources: Arc<ResourceManager>,
    language: String,
    lang: Arc<RwLock<Arc<Lang>>>,
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
        let lang = Arc::new(RwLock::new(ui.shared_lang()));
        Ok(Self {
            lang,
            ui,
            ui_renderer: UiRenderer::new(device, format),
            icons,
            assets,
            clipboard: UiClipboard::default(),
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
        self.publish_lang();
        Ok(())
    }

    fn publish_lang(&self) {
        if let Ok(mut lang) = self.lang.write() {
            *lang = self.ui.shared_lang();
        }
    }

    /// Chat as this GUI draws it, in whatever language it uses when the line arrives.
    pub fn text_renderer(&self) -> mcr_client::TextRenderer {
        let lang = self.lang.clone();
        mcr_client::TextRenderer::new(move |component| {
            let lang = lang.read().map(|l| l.clone()).unwrap_or_default();
            mcr_ui::flatten(&lang, component)
                .iter()
                .map(|cell| cell.ch)
                .collect()
        })
    }

    pub fn sync_language(&mut self, code: &str) {
        if self.language == code {
            return;
        }
        self.language = code.to_owned();
        if let Err(e) = self.ui.set_language(&self.resources, code) {
            tracing::warn!("language {code} failed to load: {e}");
        }
        self.publish_lang();
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

/// Waits for an opener in the background so it does not stay a zombie until mcr exits.
pub fn reap(child: std::io::Result<std::process::Child>) {
    if let Ok(mut child) = child {
        std::thread::spawn(move || child.wait());
    }
}

pub fn open_url(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    let child = std::process::Command::new("xdg-open")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    reap(child);
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

#[cfg(all(test, target_os = "linux"))]
mod reap_tests {
    fn zombie_children() -> usize {
        let me = format!("PPid:\t{}", std::process::id());
        std::fs::read_dir("/proc")
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| std::fs::read_to_string(e.path().join("status")).ok())
            .filter(|s| s.lines().any(|l| l == me) && s.lines().any(|l| l.starts_with("State:\tZ")))
            .count()
    }

    #[test]
    fn openers_do_not_stay_zombies() {
        for _ in 0..64 {
            super::reap(std::process::Command::new("true").spawn());
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while zombie_children() > 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(zombie_children(), 0);
    }
}
