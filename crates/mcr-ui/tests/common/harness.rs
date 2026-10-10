use mcr_data::v26_3;
use mcr_protocol::types::item::ItemStack;
use mcr_ui::{
    Env, GameView, ItemIcons, MemoryClipboard, RenderCx, Screen, ScreenAction, ScreenAssets, Ui,
    UiInput, UiRenderer, gui_scale, vanilla_resources,
};

use super::{Gpu, asset_index, gpu, jar_path, render, save_png};

pub const SIZE: (u32, u32) = (854, 480);

pub fn stack(name: &str, count: i32) -> ItemStack {
    let id = v26_3()
        .item_by_name(&format!("minecraft:{name}"))
        .unwrap_or_else(|| panic!("unknown item {name}"));
    ItemStack::new(id as i32, count)
}

#[derive(Default)]
pub struct TestView {
    pub slots: Vec<Option<ItemStack>>,
    pub data: Vec<i32>,
    pub carried: Option<ItemStack>,
    pub xp: i32,
    pub creative: bool,
    pub score: i32,
    pub merchant: Option<mcr_ui::MerchantInfo>,
    pub options: Vec<ItemStack>,
    pub patterns: usize,
    pub pages: Vec<mcr_protocol::types::text::Component>,
}

impl TestView {
    pub fn with_slots(n: usize) -> Self {
        Self {
            slots: vec![None; n],
            data: vec![0; 16],
            ..Self::default()
        }
    }

    pub fn set(&mut self, slot: usize, name: &str, count: i32) {
        self.slots[slot] = Some(stack(name, count));
    }
}

impl GameView for TestView {
    fn slot(&self, index: usize) -> Option<&ItemStack> {
        self.slots.get(index)?.as_ref()
    }

    fn carried(&self) -> Option<&ItemStack> {
        self.carried.as_ref()
    }

    fn data(&self, index: usize) -> i32 {
        self.data.get(index).copied().unwrap_or(0)
    }

    fn xp_level(&self) -> i32 {
        self.xp
    }

    fn creative(&self) -> bool {
        self.creative
    }

    fn score(&self) -> i32 {
        self.score
    }

    fn merchant(&self) -> Option<&mcr_ui::MerchantInfo> {
        self.merchant.as_ref()
    }

    fn stonecutter_options(&self) -> &[ItemStack] {
        &self.options
    }

    fn loom_pattern_count(&self) -> usize {
        self.patterns
    }

    fn book_pages(&self) -> &[mcr_protocol::types::text::Component] {
        &self.pages
    }

    fn enchant_clue(&self, option: usize) -> Option<(String, i32)> {
        Some((["Sharpness", "Unbreaking", "Looting"][option].into(), 2))
    }
}

pub struct Harness {
    pub gpu: Gpu,
    pub ui: Ui,
    pub icons: ItemIcons,
    pub assets: ScreenAssets,
    pub renderer: UiRenderer,
}

impl Harness {
    pub fn new() -> Option<Self> {
        let jar = jar_path()?;
        let gpu = gpu()?;
        let index = asset_index();
        let resources =
            vanilla_resources(&jar, index.as_ref().map(|(d, i)| (d.as_path(), i.as_str())))
                .expect("resources");
        let mut ui = Ui::load(&resources).expect("ui");
        let icons = ItemIcons::load(&resources).expect("icons");
        let assets = ScreenAssets::load(&mut ui, &resources).expect("assets");
        let renderer = UiRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
        Some(Self {
            gpu,
            ui,
            icons,
            assets,
            renderer,
        })
    }

    pub fn shot(
        &mut self,
        name: &str,
        size: (u32, u32),
        screen: &mut dyn Screen,
        view: &dyn GameView,
        mouse: (f32, f32),
    ) -> Vec<u8> {
        let scale = gui_scale(size.0, size.1, 0);
        screen.init(size.0.div_ceil(scale) as i32, size.1.div_ceil(scale) as i32);
        self.shot_inited(name, size, screen, view, mouse)
    }

    pub fn shot_inited(
        &mut self,
        name: &str,
        size: (u32, u32),
        screen: &mut dyn Screen,
        view: &dyn GameView,
        mouse: (f32, f32),
    ) -> Vec<u8> {
        {
            let mut frame = self.ui.frame(size.0, size.1);
            let mut cx = RenderCx::new(&mut frame, &mut self.icons, &self.assets, view);
            cx.mouse = mouse;
            cx.now_ms = 0;
            screen.render(&mut cx);
            cx.finish();
        }
        let img = render(
            &self.gpu,
            &mut self.renderer,
            &mut self.ui,
            size,
            wgpu::Color {
                r: 0.36,
                g: 0.50,
                b: 0.32,
                a: 1.0,
            },
        );
        save_png(name, size.0, size.1, &img);
        img
    }
}

pub fn send(screen: &mut dyn Screen, view: &dyn GameView, inputs: &[UiInput]) -> Vec<ScreenAction> {
    let mut clipboard = MemoryClipboard::default();
    let mut env = Env {
        view,
        clipboard: &mut clipboard,
        now_ms: 0,
    };
    let mut out = Vec::new();
    for input in inputs {
        screen.input(&mut env, input, &mut out);
    }
    out
}

pub fn typed(text: &str) -> Vec<UiInput> {
    text.chars().map(UiInput::Char).collect()
}
