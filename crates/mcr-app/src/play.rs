use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use mcr_buf::BlockPos;
use mcr_client::{
    ChatKind, Client, ClientError, ClientEvent, CombatKeys, PlayerInput, UseTarget,
    sanitize_for_log,
};
use mcr_particles::ParticleEngine;
use mcr_physics::Pose;
use mcr_protocol::types::ItemStack;
use mcr_render::glam::{DVec3, Mat4};
use mcr_render::{Camera, Environment, FrameStats, Overlay, OverlayCtx, Renderer, wgpu};
use mcr_settings::bindings::mapping_index;
use mcr_settings::value::MusicToastState;
use mcr_settings::{HeldState, InputKey, KeyBindings, MAPPINGS, Options, SOUND_CATEGORIES};
use mcr_ui::{
    CREDITS_PATH, ChatScreen, ContainerScreen, CreativeScreen, CreditsScreen, CreditsSource,
    DeathScreen, Env, HotbarStore, MenuKind, Modifiers, MouseButton, POEM_PATH, POSTCREDITS_PATH,
    PauseScreen, RenderCx, Screen, ScreenAction, ScreenPreview, ScreenStack, Ui, UiInput,
    UiRenderer,
};

use crate::assets::Assets;
use mcr_audio::SoundSource;

use crate::ambience::Ambience;
use crate::animate::BlockAnimation;
use crate::audio::{Audio, BlockSound, MusicEvent, wire_category, world_audio};
use crate::block_entities::BlockEntities;
use crate::debug::{self, DebugInput};
use crate::effects::{self, WorldEffects};
use crate::entities::{Cull, EntityLayer, FrameCtx, LocalSample};
use crate::entity_loops::EntityLoops;
use crate::gui::{self, Gui};
use crate::hand::{Hand as HeldHand, HandFrame, HandTick};
use crate::hud::{Hud, HudInput};
use crate::input::{KEY_ESCAPE, KEY_LEFT_CTRL, KEY_RIGHT_CTRL, track_modifiers, ui_inputs};
use crate::mesh_world::MeshWorld;
use crate::movement_sounds::MovementSounds;
use crate::nametags::{self, TagParams};
use crate::net_world::NetworkWorld;
use crate::outline::{OutlineRenderer, WorldBox};
use crate::overlays::Overlays;
use crate::preview::Preview;
use crate::render_system::{PipelineStats, RenderSystem};
use crate::settings::{OptionsScreen, SharedSettings};
use crate::target::{EntityPicker, Target, pick, third_person_distance};
use crate::view::{StateView, SuggestEngine, ViewCache};
use mcr_modbridge::frame::JavaCamera;
use mcr_modbridge::sound::{SoundOwner, event_owner};

const MAX_MAP_LABEL_KEYS: usize = 512;
pub const TICK_SECONDS: f32 = 0.05;
const LOW_DISK_SPACE_TOAST: u32 = 0x4c44_5321;
const INSECURE_SERVER_TOAST: u32 = 0x5553_4357;

fn container_menu(
    state: &mcr_client::ClientState,
    menu_type: i32,
    title: mcr_protocol::types::text::Component,
) -> Option<(MenuKind, mcr_protocol::types::text::Component)> {
    if menu_type != -1 {
        return MenuKind::from_registry_id(menu_type).map(|k| (k, title));
    }
    let screen = state.mount_screen()?;
    let name = state
        .inventory
        .open
        .as_ref()?
        .mount
        .and_then(|m| state.entity_type_short_name(m.entity_type))?;
    let title = mcr_protocol::types::text::Component::translatable(
        format!("entity.minecraft.{name}"),
        Vec::new(),
    );
    let kind = match screen.family {
        mcr_client::MountFamily::Nautilus => MenuKind::Nautilus {
            saddle: screen.saddle,
            armor: screen.armor,
        },
        family => MenuKind::Mount {
            chest_columns: u8::try_from(screen.columns).unwrap_or(0),
            saddle: screen.saddle,
            armor: screen.armor,
            llama: family == mcr_client::MountFamily::Llama,
        },
    };
    Some((kind, title))
}

fn line_tag(tag: mcr_client::ChatTag) -> mcr_ui::ChatLineTag {
    match tag {
        mcr_client::ChatTag::System => mcr_ui::ChatLineTag::System,
        mcr_client::ChatTag::SystemSinglePlayer => mcr_ui::ChatLineTag::SystemSinglePlayer,
        mcr_client::ChatTag::NotSecure => mcr_ui::ChatLineTag::NotSecure,
        mcr_client::ChatTag::Modified { original } => mcr_ui::ChatLineTag::Modified { original },
        mcr_client::ChatTag::Error => mcr_ui::ChatLineTag::Error,
    }
}

/// Vanilla's chat log tags.
const COMMAND_HISTORY_FILE: &str = "command_history.txt";

fn tag_label(tag: &mcr_client::ChatTag) -> &'static str {
    match tag {
        mcr_client::ChatTag::System | mcr_client::ChatTag::SystemSinglePlayer => "System",
        mcr_client::ChatTag::NotSecure => "Not Secure",
        mcr_client::ChatTag::Modified { .. } => "Modified",
        mcr_client::ChatTag::Error => "Chat Error",
    }
}

/// Uniform in [0, 1) for cosmetic jitter such as sound pitch.
pub(crate) fn rand_unit() -> f32 {
    thread_local!(static SEED: std::cell::Cell<u32> = const { std::cell::Cell::new(0x2545_f491) });
    SEED.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        s.set(x);
        (x >> 8) as f32 / (1u32 << 24) as f32
    })
}
const MAX_TICKS_PER_FRAME: u32 = 10;

/// `LivingEntity#DATA_HEALTH_ID`, which only living entities carry.
const LIVING_HEALTH: u8 = 9;

fn music_volume(options: &Options) -> f32 {
    let volume = |name: &str| {
        SOUND_CATEGORIES
            .iter()
            .position(|c| *c == name)
            .map_or(1.0, |i| options.sound_volumes[i] as f32)
    };
    volume("master") * volume("music")
}

#[derive(Debug, Clone)]
pub struct PlayConfig {
    pub render_distance: Option<i32>,
    pub game_dir: std::path::PathBuf,
    /// Singleplayer: the pause screen pauses and offers Save and Quit.
    pub local_server: bool,
}

/// What the shell lends to a session.
pub struct Parts {
    pub renderer: Renderer,
    pub gui: Gui,
    pub assets: Arc<Assets>,
    pub audio: Option<Audio>,
    pub settings: SharedSettings,
}

#[derive(Debug, Clone, Copy)]
struct Keys {
    attack: usize,
    use_item: usize,
    forward: usize,
    left: usize,
    back: usize,
    right: usize,
    jump: usize,
    sneak: usize,
    sprint: usize,
    drop: usize,
    save_hotbar: usize,
    load_hotbar: usize,
    chat: usize,
    command: usize,
    inventory: usize,
    advancements: usize,
    playerlist: usize,
    pick: usize,
    gui: usize,
    perspective: usize,
    swap: usize,
    debug: usize,
    spectator_effect: usize,
    spectator_outlines: usize,
    spectator_hotbar: usize,
    hotbar: [usize; 9],
}

impl Keys {
    fn resolve() -> Self {
        let i = |name: &str| mapping_index(name).expect("vanilla key mapping");
        Self {
            attack: i("key.attack"),
            use_item: i("key.use"),
            forward: i("key.forward"),
            left: i("key.left"),
            back: i("key.back"),
            right: i("key.right"),
            jump: i("key.jump"),
            sneak: i("key.sneak"),
            sprint: i("key.sprint"),
            drop: i("key.drop"),
            save_hotbar: i("key.saveToolbarActivator"),
            load_hotbar: i("key.loadToolbarActivator"),
            chat: i("key.chat"),
            command: i("key.command"),
            inventory: i("key.inventory"),
            advancements: i("key.advancements"),
            playerlist: i("key.playerlist"),
            pick: i("key.pickItem"),
            gui: i("key.toggleGui"),
            perspective: i("key.togglePerspective"),
            swap: i("key.swapOffhand"),
            debug: i("key.debug.overlay"),
            spectator_effect: i("key.toggleSpectatorShaderEffects"),
            spectator_outlines: i("key.spectatorOutlines"),
            spectator_hotbar: i("key.spectatorHotbar"),
            hotbar: std::array::from_fn(|n| i(&format!("key.hotbar.{}", n + 1))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Perspective {
    First,
    ThirdBack,
    ThirdFront,
}

impl Perspective {
    /// Vanilla's F5 order.
    fn next(self) -> Self {
        match self {
            Self::First => Self::ThirdBack,
            Self::ThirdBack => Self::ThirdFront,
            Self::ThirdFront => Self::First,
        }
    }

    fn from_java(java: mcr_modbridge::Perspective) -> Self {
        match java {
            mcr_modbridge::Perspective::First => Self::First,
            mcr_modbridge::Perspective::ThirdBack => Self::ThirdBack,
            mcr_modbridge::Perspective::ThirdFront => Self::ThirdFront,
        }
    }
}

struct FrameOverlay<'a> {
    outline: &'a OutlineRenderer,
    ui_renderer: &'a mut UiRenderer,
    ui: &'a mut Ui,
    entities: Option<&'a EntityLayer>,
    block_entities: Option<&'a BlockEntities>,
    hand: Option<&'a HeldHand>,
    preview: Option<&'a Preview>,
    particles: &'a mut ParticleEngine,
    env: &'a Environment,
    partial: f32,
    glow: &'a mut Option<(wgpu::TextureFormat, mcr_render::GlowOutline)>,
}

impl Overlay for FrameOverlay<'_> {
    fn opaque(&mut self, pass: &mut wgpu::RenderPass<'_>) {
        if let Some(entities) = self.entities {
            entities.draw_opaque(pass);
        }
        if let Some(block_entities) = self.block_entities {
            block_entities.draw_opaque(pass);
        }
    }

    fn record(&mut self, ctx: &mut OverlayCtx<'_>) {
        self.outline.record(ctx);
        if self.entities.is_some() || self.block_entities.is_some() {
            let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("entities"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: ctx.color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: ctx.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(entities) = self.entities {
                entities.draw_blended(&mut pass);
            }
            if let Some(block_entities) = self.block_entities {
                block_entities.draw_blended(&mut pass);
            }
        }
        self.particles.record(ctx, self.partial, self.env);
        if let Some(hand) = self.hand {
            hand.record(ctx);
        }
        if let Some(entities) = self.entities.filter(|e| e.has_outlines()) {
            if self.glow.as_ref().is_none_or(|(f, _)| *f != ctx.format) {
                *self.glow = Some((
                    ctx.format,
                    mcr_render::GlowOutline::new(ctx.device, ctx.format),
                ));
            }
            let (_, glow) = self.glow.as_mut().expect("set above");
            {
                let mut pass = glow.begin(ctx.device, ctx.encoder, ctx.size);
                entities.draw_outlines(&mut pass);
            }
            glow.finish(ctx.device, ctx.queue, ctx.encoder, ctx.color);
        }
    }

    fn record_ui(&mut self, ctx: &mut OverlayCtx<'_>) {
        self.ui_renderer.prepare(ctx.device, ctx.queue, self.ui);
        {
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
            self.ui_renderer.draw_below_preview(&mut pass);
        }
        if let Some(preview) = self.preview {
            preview.record(ctx);
        }
        if self.ui_renderer.has_preview_split() {
            let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui-above-preview"),
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
            self.ui_renderer.draw_above_preview(&mut pass);
        }
    }
}

fn push_sample(history: &mut Vec<f32>, value: f32) {
    if history.len() >= mcr_ui::CHART_SAMPLES {
        history.remove(0);
    }
    history.push(value);
}

const GHOST_TICKS: u32 = 30;

fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// Vanilla's cubic mouse sensitivity curve, in degrees per pixel.
pub fn degrees_per_pixel(sensitivity: f64) -> f32 {
    let s = sensitivity * 0.6 + 0.2;
    (s * s * s * 8.0 * 0.15) as f32
}

/// `ScrollWheelHandler`: the accumulator restarts on a direction change and a whole notch moves
/// one slot, whatever its size.
fn scroll_wheel(acc: &mut f32, delta: f32) -> Option<i32> {
    if *acc != 0.0 && delta.signum() != acc.signum() {
        *acc = 0.0;
    }
    *acc += delta;
    let whole = acc.trunc();
    if whole == 0.0 {
        return None;
    }
    *acc -= whole;
    Some(whole.signum() as i32)
}

pub fn view_direction(yaw: f32, pitch: f32) -> [f64; 3] {
    let (yaw, pitch) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    [
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    ]
}

pub struct Play {
    pub world: NetworkWorld,
    pub render: RenderSystem,
    gui: Gui,
    hud: Hud,
    outline: OutlineRenderer,
    entities: Option<EntityLayer>,
    block_entities: Option<BlockEntities>,
    overlays: Overlays,
    camera_fx: crate::camera_fx::CameraFx,
    nausea_id: Option<i32>,
    map_label_keys: rustc_hash::FxHashMap<i32, Vec<crate::map::ProtoDecoration>>,
    vision: crate::vision::Vision,
    end_flash_sound: Option<u32>,
    assets: Arc<Assets>,
    hand: Option<HeldHand>,
    preview: Option<Preview>,
    effects: WorldEffects,
    player_inventory: bool,
    settings: SharedSettings,
    settings_seen: u64,
    stack: ScreenStack,
    gui_size: (i32, i32),
    suggest: SuggestEngine,
    mouse: (f32, f32),
    mods: Modifiers,
    container_open: bool,
    creative_open: bool,
    hotbars: Rc<RefCell<HotbarStore>>,
    view_cache: ViewCache,
    hotbar_path: std::path::PathBuf,
    command_history_path: std::path::PathBuf,
    death_shown: bool,
    death_message: Option<mcr_protocol::types::text::Component>,
    pub leave: bool,
    pub quit: bool,
    local_server: bool,
    /// The local world is open to LAN: no pausing, no second Open to LAN.
    pub lan_open: bool,
    lan_request: Option<mcr_ui::LanSettings>,
    capture_icon: bool,
    world_icon: Option<Vec<u8>>,
    pause_opened: bool,
    /// Set when the server sent us elsewhere; the shell reconnects there.
    pub transfer: Option<mcr_client::TransferTarget>,
    sky_factor: f32,
    needles: crate::needles::Needles,
    sun_turn: f32,
    moon_phase: u8,
    pack_answers: Vec<bool>,
    conduct_answers: Vec<(bool, bool, i32)>,
    dialog_replies: Vec<mcr_client::DialogReply>,
    test_status: mcr_ui::SharedTestStatus,
    chat_depth: Option<usize>,
    pub options: Options,
    bindings: KeyBindings,
    held: HeldState,
    keys: Keys,
    data: &'static mcr_data::Version,
    audio: Option<Audio>,
    pub yaw: f32,
    pub pitch: f32,
    accum: f32,
    pub ticks: u64,
    prev_eye: [f64; 3],
    cur_eye: [f64; 3],
    eye_height: f32,
    fov_mod: f32,
    prev_fov_mod: f32,
    scroll: f32,
    perspective: Perspective,
    hud_hidden: bool,
    /// Java's HUD is composed over the world in mod mode, replacing the native one.
    pub java_gui: bool,
    /// Java draws entities, block entities, nametags and the hand into the world.
    pub java_world: bool,
    /// What Java's FOV mods (zoom) do to the field of view; 1 without them.
    pub java_fov_scale: f32,
    /// The camera a Java mod (Freecam, camera tweaks) set last frame; mcr renders from it.
    pub java_camera: Option<JavaCamera>,
    /// A Java mod replaced vanilla's rain and snow, so mcr draws none.
    pub java_weather: bool,
    /// A mod plays the local player's footsteps in Java (Presence Footsteps).
    pub java_steps: bool,
    /// A mod in Java consumes the mouse's turning (Freelook), so the player does not turn.
    pub java_turn: bool,
    /// Java's camera type decides the perspective (mod mode, Java draws the world); F5 is Java's.
    java_perspective: bool,
    /// Radians about the view axis that Java mods added to the level's pose (Camera Overhaul).
    pub java_view_roll: f32,
    /// A Java screen owns all input; gameplay input stops.
    pub java_screen: bool,
    /// `MOVE_*` bits a mod (InvMove) lets through while a Java screen is open.
    pub java_screen_moves: u32,
    /// Java's GUI scale in mod mode, which native overlays follow.
    pub gui_scale_override: Option<i32>,
    /// A native overlay asked for the Mods screen.
    pub open_mods: bool,
    debug_on: bool,
    tab_held: bool,
    /// Left and right Ctrl: vanilla `hasControlDown` accepts either.
    ctrl_held: [bool; 2],
    closed: Option<String>,
    attack_click: bool,
    use_click: bool,
    pub(crate) target: Option<Target>,
    ambience: Ambience,
    block_animation: BlockAnimation,
    loops: EntityLoops,
    entity_events: crate::entity_events::EntityEvents,
    level_events: crate::level_events::LevelEvents,
    subtitles: crate::subtitles::Subtitles,
    skins: crate::skins::SkinStore,
    boss_darkening: (f32, f32),
    spectate_view: Option<(f32, f32)>,
    /// First-person yaw of a sleeper, along the bed: vanilla `Camera#alignWithEntity`.
    bed_view: Option<f32>,
    pub turn_latency: crate::latency::TurnLatency,
    seen_camera: Option<i32>,
    glow: Option<(wgpu::TextureFormat, mcr_render::GlowOutline)>,
    spectator_effect: crate::post_effects::SpectatorEffect,
    post_chains: Vec<mcr_render::PostChain>,
    ghost_age: Option<u32>,
    frame_history: Vec<f32>,
    tick_history: Vec<f32>,
    ping_history: Vec<f32>,
    movement: MovementSounds,
    placed_seen: bool,
    started: Instant,
    /// When a spectator menu key was last used, for `SpectatorGui`'s fade.
    spectator_menu_at: u64,
    last_frame: Instant,
    fps: f32,
    frame_ms: f32,
    pub teleports: u32,
    pub container_updates: u32,
}

impl std::fmt::Debug for Play {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Play")
            .field("ticks", &self.ticks)
            .field("yaw", &self.yaw)
            .field("pitch", &self.pitch)
            .finish_non_exhaustive()
    }
}

impl Play {
    pub fn new(parts: Parts, cfg: &PlayConfig, client: Client) -> Self {
        let data = mcr_data::v26_3();
        let Parts {
            renderer,
            gui,
            assets,
            audio,
            settings,
        } = parts;
        let (options, bindings, seen) = {
            let s = settings.borrow();
            (s.file.options.clone(), s.file.bindings.clone(), s.version)
        };
        let render_distance = cfg
            .render_distance
            .unwrap_or(options.render_distance)
            .clamp(2, 64);
        let hud = Hud::new(&gui.ui, data).with_style_reader(Box::new({
            let assets = assets.clone();
            move |path| assets.read(path)
        }));
        let command_history_path = cfg.game_dir.join(COMMAND_HISTORY_FILE);
        if let Ok(text) = std::fs::read_to_string(&command_history_path) {
            hud.chat.borrow_mut().load_command_history(&text);
        }
        let hotbar_path = cfg.game_dir.join("hotbar.txt");
        let hotbars = Rc::new(RefCell::new(
            std::fs::read_to_string(&hotbar_path)
                .map(|t| HotbarStore::from_text(&t))
                .unwrap_or_default(),
        ));
        let world = NetworkWorld::new(client, render_distance);
        let format = renderer.target_format();
        let outline = OutlineRenderer::new(renderer.device(), format);
        let scene_assets = assets.scene.clone();
        let mut effects = WorldEffects::new(assets.clone());
        effects.set_options(&options);
        let mut renderer = renderer;
        renderer.set_effect_assets(&effects::effect_assets(&assets));
        renderer.set_post_overrides(crate::post_effects::pack_overrides(&assets.resources));
        let (entities, block_entities, hand, preview) = match scene_assets {
            Some(sa) => (
                EntityLayer::new(renderer.device(), renderer.queue(), format, sa.clone()),
                BlockEntities::new(renderer.device(), renderer.queue(), format, sa.clone()),
                HeldHand::new(renderer.device(), renderer.queue(), format, sa.clone()),
                Preview::new(renderer.device(), renderer.queue(), format, sa),
            ),
            None => (None, None, None, None),
        };
        let overlay_assets = assets.clone();
        let animate_assets = assets.clone();
        let skin_assets = assets.clone();
        let render = RenderSystem::new(renderer, assets, &world);
        let now = Instant::now();
        Self {
            world,
            render,
            gui,
            hud,
            outline,
            entities,
            block_entities,
            overlays: Overlays::new(),
            camera_fx: Default::default(),
            nausea_id: data
                .registry("minecraft:mob_effect")
                .and_then(|r| r.id_of("minecraft:nausea"))
                .map(|id| id as i32),
            map_label_keys: rustc_hash::FxHashMap::default(),
            vision: crate::vision::Vision::default(),
            end_flash_sound: None,
            assets: overlay_assets,
            hand,
            preview,
            effects,
            player_inventory: false,
            settings,
            settings_seen: seen,
            stack: ScreenStack::new(0, 0),
            gui_size: (0, 0),
            suggest: SuggestEngine::default(),
            mouse: (0.0, 0.0),
            mods: Modifiers::NONE,
            container_open: false,
            creative_open: false,
            hotbars,
            view_cache: ViewCache::default(),
            hotbar_path,
            command_history_path,
            death_shown: false,
            death_message: None,
            leave: false,
            quit: false,
            local_server: cfg.local_server,
            lan_open: false,
            lan_request: None,
            capture_icon: false,
            world_icon: None,
            pause_opened: false,
            pack_answers: Vec::new(),
            conduct_answers: Vec::new(),
            dialog_replies: Vec::new(),
            test_status: mcr_ui::SharedTestStatus::default(),
            chat_depth: None,
            options,
            bindings,
            held: HeldState::default(),
            keys: Keys::resolve(),
            data,
            audio,
            yaw: 0.0,
            pitch: 0.0,
            accum: 0.0,
            ticks: 0,
            prev_eye: [0.0; 3],
            cur_eye: [0.0; 3],
            eye_height: Pose::Standing.eye_height(),
            fov_mod: 1.0,
            prev_fov_mod: 1.0,
            scroll: 0.0,
            perspective: Perspective::First,
            hud_hidden: false,
            java_gui: false,
            java_world: false,
            java_fov_scale: 1.0,
            java_camera: None,
            java_weather: false,
            java_steps: false,
            java_turn: false,
            java_perspective: false,
            java_view_roll: 0.0,
            java_screen: false,
            java_screen_moves: 0,
            gui_scale_override: None,
            open_mods: false,
            debug_on: false,
            tab_held: false,
            ctrl_held: [false; 2],
            closed: None,
            transfer: None,
            sky_factor: 1.0,
            needles: crate::needles::Needles::default(),
            sun_turn: 0.0,
            moon_phase: 0,
            attack_click: false,
            use_click: false,
            target: None,
            ambience: Ambience::new(data),
            block_animation: BlockAnimation::new(data, animate_assets),
            loops: EntityLoops::new(data),
            entity_events: Default::default(),
            level_events: Default::default(),
            subtitles: Default::default(),
            boss_darkening: (0.0, 0.0),
            spectate_view: None,
            bed_view: None,
            turn_latency: crate::latency::TurnLatency::default(),
            seen_camera: None,
            glow: None,
            spectator_effect: Default::default(),
            post_chains: Vec::new(),
            ghost_age: None,
            frame_history: Vec::new(),
            tick_history: Vec::new(),
            ping_history: Vec::new(),
            skins: crate::skins::SkinStore::new(Box::new(move |path| skin_assets.read(path))),
            movement: MovementSounds::new(data),
            placed_seen: false,
            started: now,
            spectator_menu_at: 0,
            last_frame: now,
            fps: 0.0,
            frame_ms: 0.0,
            teleports: 0,
            container_updates: 0,
        }
    }

    /// Ends the session and returns what the shell lent out.
    pub fn finish(mut self) -> Parts {
        if let Some(audio) = &mut self.audio {
            self.ambience.reset(audio);
            self.loops.reset(audio);
            audio.stop_world_sounds();
        }
        let (renderer, assets) = self.render.into_parts();
        Parts {
            renderer,
            gui: self.gui,
            assets,
            audio: self.audio,
            settings: self.settings,
        }
    }

    pub fn closed(&self) -> Option<&str> {
        self.closed.as_deref()
    }

    pub fn wants_grab(&self) -> bool {
        self.closed.is_none() && self.stack.is_empty()
    }

    pub fn screen_open(&self) -> bool {
        !self.stack.is_empty()
    }

    /// A pausing screen is open (only the singleplayer pause menu pauses).
    pub fn pause_open(&self) -> bool {
        self.stack.any_pauses_game()
    }

    /// The local world froze or resumed: world sounds pause with it, music and UI keep playing.
    pub fn set_world_paused(&mut self, paused: bool) {
        if let Some(audio) = &mut self.audio {
            audio.pause_world(paused);
        }
    }

    pub fn ready(&self) -> bool {
        let p = &self.world.client.state.player;
        p.loaded && self.render.settled(&self.world)
    }

    pub fn feet(&self) -> [f64; 3] {
        self.world.client.state.player.pos()
    }

    pub fn soak_counts(&self) -> (u32, usize, usize) {
        let state = &self.world.client.state;
        (
            self.entities.as_ref().map_or(0, |e| e.scene.live_layers()),
            state.entities.len(),
            state.chunks.len(),
        )
    }

    pub fn pipeline_stats(&self) -> PipelineStats {
        self.render.stats()
    }

    fn fail(&mut self, e: &ClientError) {
        if let ClientError::Transferred(target) = e {
            self.transfer = Some((**target).clone());
            return;
        }
        if self.closed.is_none() {
            self.closed = Some(sanitize_for_log(&e.to_string()));
        }
    }

    fn net<T>(&mut self, f: impl FnOnce(&mut Client) -> mcr_client::Result<T>) -> Option<T> {
        match f(&mut self.world.client) {
            Ok(v) => Some(v),
            Err(e) => {
                self.fail(&e);
                None
            }
        }
    }

    fn tick_seconds(&self) -> f32 {
        self.world.client.state.world.tick_seconds()
    }

    pub fn due_ticks(&mut self, dt: f32) -> u32 {
        let step = self.tick_seconds();
        self.accum += dt.clamp(0.0, 0.25);
        let mut n = 0;
        while self.accum >= step && n < MAX_TICKS_PER_FRAME {
            self.accum -= step;
            n += 1;
        }
        if n == MAX_TICKS_PER_FRAME {
            self.accum = 0.0;
        }
        n
    }

    pub fn partial(&self) -> f32 {
        (self.accum / self.tick_seconds()).clamp(0.0, 1.0)
    }

    fn busy(&self) -> bool {
        !self.stack.is_empty() || self.java_screen
    }

    fn gui_scale(&self) -> i32 {
        self.gui_scale_override.unwrap_or(self.options.gui_scale)
    }

    /// In mod mode Java opens these screens and toggles its own debug overlay.
    fn java_owns(&self, i: usize) -> bool {
        let k = &self.keys;
        self.java_gui
            && [
                k.chat,
                k.command,
                k.inventory,
                k.advancements,
                k.playerlist,
                k.debug,
            ]
            .contains(&i)
    }

    /// The mod camera moves with the movement keys, so the player stands still.
    fn input_captured(&self) -> bool {
        self.java_camera.is_some_and(|c| c.input)
    }

    fn player_input(&self) -> PlayerInput {
        let k = &self.keys;
        let moves = !self.busy() && !self.input_captured();
        let screen_moves = if self.stack.is_empty() && self.java_screen && !self.input_captured() {
            self.java_screen_moves
        } else {
            0
        };
        let on = |i: usize, bit: u32| moves && self.held.is_active(i) || screen_moves & bit != 0;
        use mcr_modbridge::{
            MOVE_BACKWARD, MOVE_FORWARD, MOVE_JUMP, MOVE_LEFT, MOVE_RIGHT, MOVE_SNEAK, MOVE_SPRINT,
        };
        PlayerInput {
            forward: on(k.forward, MOVE_FORWARD),
            backward: on(k.back, MOVE_BACKWARD),
            left: on(k.left, MOVE_LEFT),
            right: on(k.right, MOVE_RIGHT),
            jump: on(k.jump, MOVE_JUMP),
            sneak: on(k.sneak, MOVE_SNEAK),
            sprint: on(k.sprint, MOVE_SPRINT),
            yaw: self.yaw,
            pitch: self.pitch,
        }
    }

    pub fn tick(&mut self) {
        if self.closed.is_some() || self.transfer.is_some() {
            return;
        }
        let started = Instant::now();
        self.tick_inner();
        let ping = self
            .world
            .client
            .state
            .players
            .get(self.world.client.state.uuid)
            .map_or(0, |p| p.latency);
        push_sample(
            &mut self.tick_history,
            started.elapsed().as_secs_f32() * 1000.0,
        );
        push_sample(&mut self.ping_history, ping.max(0) as f32);
    }

    fn tick_inner(&mut self) {
        self.ticks += 1;
        self.sync_settings();
        self.world.client.state.player.physics.auto_jump = self.options.auto_jump;
        let input = self.player_input();
        self.prev_eye = self.cur_eye;
        self.prev_fov_mod = self.fov_mod;
        if let Err(e) = self.world.tick(&input) {
            self.fail(&e);
        }
        self.update_eye();
        if self.spectate_view.is_some() && input.sneak {
            self.world.client.state.camera_entity = None;
        }
        self.update_fov();
        for event in self.world.take_events() {
            self.on_event(event);
        }
        self.tick_visuals();
        self.skins.update(&self.world.client.state.players);
        self.ghost_age = self.ghost_age.map(|a| a + 1).filter(|a| *a < GHOST_TICKS);
        let darken = self
            .world
            .client
            .state
            .boss_bars
            .values()
            .any(|b| b.flags.darken_screen);
        self.boss_darkening = (
            self.boss_darkening.1,
            effects::tick_boss_darkening(self.boss_darkening.1, darken),
        );
        if self.world.client.state.player.loaded && self.closed.is_none() {
            self.target = self.pick_from(self.cur_eye);
            self.gameplay();
            self.sounds_tick();
        }
        self.screens_tick();
        if let Some(audio) = &mut self.audio {
            audio.set_listener(self.cur_eye, self.yaw);
        }
    }

    fn tick_visuals(&mut self) {
        let state = &self.world.client.state;
        let sample = LocalSample {
            feet: state.player.pos(),
            yaw: self.yaw,
            pitch: self.pitch,
            dead: state.player.dead,
            gliding: state.player.physics.fall_flying,
            crouching: state.player.physics.crouching,
            vel: state.player.physics.vel,
            on_ground: state.player.physics.on_ground,
            swimming: state.player.physics.swimming,
        };
        if let Some(entities) = &mut self.entities {
            entities.tick(state, sample);
        }
        let body_yaw = self
            .entities
            .as_ref()
            .map_or(self.yaw, |l| l.local_body_yaw(1.0));
        self.needles
            .tick(self.sun_turn, body_yaw, state.world.runs_normally);
        let running = state.world.runs_normally;
        if let Some(block_entities) = &mut self.block_entities
            && running
        {
            block_entities.tick(state, self.ticks);
        }
        let scoping = self.scoping();
        self.overlays.tick(scoping);
        if self.camera_fx.tick(state, self.nausea_id)
            && let Some(audio) = world_audio(&mut self.audio)
        {
            let pitch = rand_unit() * 0.4 + 0.8;
            audio.play(
                "minecraft:block.portal.trigger",
                None,
                0.25,
                pitch,
                SoundSource::Ambient,
            );
        }
        self.hud.tick(state, &self.options);
        let [block, sky] = crate::entities::light_at(
            &state.chunks,
            state.player.pos(),
            state.player.physics.eye_height(),
        );
        let darken = ((1.0 - self.sky_factor.clamp(0.0, 1.0)) * 15.0).round() as u8;
        let raw = block.max(sky.saturating_sub(darken));
        self.overlays
            .tick_vignette(crate::overlays::level_brightness(raw));
        self.vision.tick(state, self.cur_eye);
        if self.vision.end_flash_started() {
            self.end_flash_sound = Some(30);
        }
        if let Some(left) = self.end_flash_sound.as_mut() {
            *left = left.saturating_sub(1);
            if *left == 0 {
                self.end_flash_sound = None;
                if let Some(audio) = &mut self.audio {
                    audio.play_voice("weather.end_flash");
                }
            }
        }
        if let Some(hand) = &mut self.hand {
            use mcr_protocol::types::game::Hand as GameHand;
            let server_using = match state.player.data.get(&8) {
                Some(mcr_protocol::types::entity_data::EntityDataValue::Byte(f)) if f & 1 != 0 => {
                    Some(if f & 2 != 0 {
                        GameHand::Off
                    } else {
                        GameHand::Main
                    })
                }
                _ => None,
            };
            hand.tick(&HandTick {
                state,
                yaw: self.yaw,
                pitch: self.pitch,
                using: self
                    .world
                    .client
                    .using_item()
                    .map(|u| u.hand)
                    .or(server_using),
                swap_scale: self.world.client.item_swap_scale(1.0),
                hands_busy: self.world.client.hands_busy(),
            });
        }
        let rain = if running {
            let mut cx = crate::entity_events::EventCx {
                state,
                effects: &mut self.effects,
                audio: world_audio(&mut self.audio),
            };
            self.entity_events.tick(&mut cx);
            self.effects
                .tick(state, DVec3::from(self.cur_eye), &self.options)
        } else {
            None
        };
        if running {
            let sky_darken = ((1.0 - self.sky_factor.clamp(0.0, 1.0)) * 15.0).round() as u8;
            let mut tickers = self.block_entities.iter().flat_map(|b| b.tickers());
            self.block_animation.tick(
                state,
                &mut self.effects,
                world_audio(&mut self.audio),
                &mut tickers,
                sky_darken,
            );
        }
        for s in std::mem::take(&mut self.effects.sounds) {
            if let Some(audio) = world_audio(&mut self.audio) {
                audio.play(s.name, Some(s.pos), s.volume, s.pitch, SoundSource::Ambient);
            }
        }
        if let (Some(rain), Some(audio)) = (rain, world_audio(&mut self.audio)) {
            let at = rain.pos.map(|v| f64::from(v) + 0.5);
            if rain.above {
                audio.play(
                    "weather.rain.above",
                    Some(at),
                    0.1,
                    0.5,
                    SoundSource::Weather,
                );
            } else {
                audio.play("weather.rain", Some(at), 0.2, 1.0, SoundSource::Weather);
            }
        }
    }

    /// The type id of the entity the camera follows, `None` for the player's own eyes.
    fn camera_type(&self) -> Option<&'static str> {
        let state = &self.world.client.state;
        let e = state.entities.get(state.camera_entity?)?;
        self.data.entity_type_name(u32::try_from(e.kind).ok()?)
    }

    fn update_eye(&mut self) {
        let state = &self.world.client.state;
        let player = &state.player;
        let target = player.physics.eye_height();
        self.eye_height += (target - self.eye_height) * 0.5;
        let [x, y, z] = player.pos();
        self.cur_eye = [x, y + f64::from(self.eye_height), z];
        self.spectate_view = None;
        self.bed_view = player
            .sleeping_pos()
            .and_then(|bed| state.bed_orientation(bed))
            .map(|facing| mcr_client::facing_yaw(facing) - 180.0);
        if let Some(e) = state.camera_entity.and_then(|id| state.entities.get(id)) {
            self.cur_eye = crate::entities::eye_position(self.data, e);
            let yaw = if e.sim.is_some() { e.yaw } else { e.head_yaw };
            self.spectate_view = Some((yaw, e.pitch));
        }
        if state.camera_entity != self.seen_camera {
            self.seen_camera = state.camera_entity;
            self.spectator_effect.check(self.camera_type());
        }
        let snapped = player.placed && !self.placed_seen;
        if snapped {
            self.placed_seen = true;
        }
        if snapped || !player.placed {
            self.prev_eye = self.cur_eye;
        }
    }

    /// The render distance mcr's terrain and fog use, in chunks.
    pub fn render_distance(&self) -> i32 {
        self.world.render_distance()
    }

    pub fn third_person(&self) -> bool {
        self.perspective != Perspective::First
    }

    /// Follows Java's camera type while Java leads the perspective (`None`: mcr's F5 decides).
    pub fn follow_perspective(&mut self, java: Option<mcr_modbridge::Perspective>) {
        self.java_perspective = java.is_some();
        if let Some(java) = java {
            self.set_perspective(Perspective::from_java(java));
        }
    }

    pub fn perspective_name(&self) -> &'static str {
        match self.perspective {
            Perspective::First => "first",
            Perspective::ThirdBack => "back",
            Perspective::ThirdFront => "front",
        }
    }

    fn set_perspective(&mut self, next: Perspective) {
        let was_first = self.perspective == Perspective::First;
        self.perspective = next;
        if was_first != (next == Perspective::First) {
            let ty = self.camera_type().filter(|_| !was_first);
            self.spectator_effect.check(ty);
        }
    }

    /// How far the third-person player camera sits from the eye; `None` in first person.
    pub fn third_person_distance(&self, partial: f32) -> Option<f32> {
        self.third_person().then(|| {
            let eye = DVec3::from(lerp3(self.prev_eye, self.cur_eye, f64::from(partial)));
            self.player_camera(partial).position.distance(eye) as f32
        })
    }

    fn scoping(&self) -> bool {
        self.perspective == Perspective::First && self.hand.as_ref().is_some_and(HeldHand::scoping)
    }

    /// `Hud#extractCrosshair`'s visibility and indicator, and the hotbar attack indicator.
    fn crosshair_hud(&self) -> (Option<mcr_ui::CrosshairIndicator>, Option<f32>) {
        use mcr_protocol::types::game::GameType;
        use mcr_settings::value::AttackIndicator;
        use mcr_ui::CrosshairIndicator;
        let client = &self.world.client;
        let state = &client.state;
        let spectator = state.player.game_mode == GameType::Spectator;
        let scale = client.attack_strength_scale(0.0);
        let mode = self.options.attack_indicator;
        let hotbar = (mode == AttackIndicator::Hotbar).then_some(scale);
        if self.perspective != Perspective::First || (spectator && !self.spectator_crosshair()) {
            return (None, hotbar);
        }
        if mode != AttackIndicator::Crosshair {
            return (Some(CrosshairIndicator::None), hotbar);
        }
        let full = scale >= 1.0
            && match self.target {
                Some(Target::Entity { id, hit, .. }) => {
                    let alive = state.entities.get(id).is_some_and(|e| {
                        matches!(
                            e.data.get(&crate::hud::HEALTH_DATA_INDEX),
                            Some(mcr_protocol::types::entity_data::EntityDataValue::Float(h)) if *h > 0.0
                        )
                    });
                    alive && state.attack_strength_delay() > 5.0 && self.attack_in_range(hit)
                }
                _ => false,
            };
        let indicator = if full {
            CrosshairIndicator::Full
        } else if scale < 1.0 {
            CrosshairIndicator::Progress(scale)
        } else {
            CrosshairIndicator::None
        };
        (Some(indicator), hotbar)
    }

    /// `AttackRange#isInRange` of the held item, from the eye to where the ray met the target.
    fn attack_in_range(&self, hit: [f64; 3]) -> bool {
        use mcr_protocol::types::component::DataComponent;
        let state = &self.world.client.state;
        let stack = state.held_stack();
        let patched = stack.components.added.iter().find_map(|c| match c {
            DataComponent::AttackRange(r) => Some(r.clone()),
            _ => None,
        });
        let default = || {
            let component = self
                .data
                .registry("minecraft:data_component_type")?
                .id_of("minecraft:attack_range")?;
            let item = u32::try_from(stack.item).ok()?;
            // Every vanilla item with a default attack_range is a spear, and they share it.
            self.data
                .item_has_default_component(item, component)
                .then_some(mcr_protocol::types::component::AttackRange {
                    min_reach: 2.0,
                    max_reach: 4.5,
                    min_creative_reach: 2.0,
                    max_creative_reach: 6.5,
                    hitbox_margin: 0.125,
                    mob_factor: 0.5,
                })
        };
        let Some(range) = patched.or_else(default) else {
            return true;
        };
        let creative = state.player.game_mode == mcr_protocol::types::game::GameType::Creative;
        let (min, max) = if creative {
            (range.min_creative_reach, range.max_creative_reach)
        } else {
            (range.min_reach, range.max_reach)
        };
        let margin = f64::from(range.hitbox_margin);
        let d = DVec3::from_array(self.cur_eye).distance(DVec3::from_array(hit));
        d >= f64::from(min) - margin && d <= f64::from(max) + margin
    }

    /// `Hud#canRenderCrosshairForSpectator`.
    fn spectator_crosshair(&self) -> bool {
        let state = &self.world.client.state;
        match self.target {
            Some(Target::Entity { id, .. }) => state
                .entities
                .get(id)
                .and_then(|e| u32::try_from(e.kind).ok())
                .and_then(|k| self.data.entity_type_name(k))
                .is_some_and(crate::hud::spectator_entity),
            Some(Target::Block(hit)) => {
                let has_book = self
                    .data
                    .state_properties(hit.state)
                    .any(|(k, v)| k == "has_book" && v == "true");
                self.data
                    .state_block_name(hit.state)
                    .is_some_and(|n| crate::hud::spectator_block(n, has_book))
            }
            None => false,
        }
    }

    fn update_fov(&mut self) {
        let client = &self.world.client;
        let player = &client.state.player;
        let p = &player.physics;
        let speed = p.attributes.movement_speed as f32 * if p.sprinting { 1.3 } else { 1.0 };
        let walking = player.walking_speed;
        let bow_ticks = client
            .using_item()
            .filter(|u| {
                client
                    .state
                    .is_item(&client.state.hand_stack(u.hand), "bow")
            })
            .map(|u| client.ticks.saturating_sub(u.started_tick) as f32);
        let target = crate::camera_fx::fov_target(&crate::camera_fx::FovInput {
            flying: p.abilities.flying,
            speed_factor: (walking != 0.0).then(|| speed / walking),
            bow_ticks,
            scoping_first_person: self.scoping(),
            effect_scale: self.options.fov_effect_scale as f32,
        });
        self.fov_mod = crate::camera_fx::step_fov(self.fov_mod, target);
    }

    /// Ticks since the local player died, with the partial tick; `None` while alive.
    fn death_ticks(&self, partial: f32) -> Option<f32> {
        self.world
            .client
            .state
            .player
            .dead
            .then(|| self.entities.as_ref().map_or(0, |e| e.local_death_time()) as f32 + partial)
    }

    /// `Camera#getFluidInCamera` is water or lava, which narrows the FOV.
    fn camera_in_fluid(&self, camera: &Camera) -> bool {
        matches!(
            crate::vision::Vision::fog_kind(
                &self.world.client.state,
                camera.position.to_array(),
                camera.forward().to_array(),
            ),
            mcr_render::FogKind::Water | mcr_render::FogKind::Lava
        )
    }

    /// The hand's FOV: vanilla's fixed 70 degrees, narrowed by death and fluid.
    fn hud_fov(&self, camera: &Camera, partial: f32) -> f32 {
        crate::camera_fx::death_or_fluid_fov(
            70.0,
            self.death_ticks(partial),
            self.camera_in_fluid(camera),
            self.options.fov_effect_scale as f32,
        )
    }

    /// `GameRenderer#bobHurt` then `bobView` (when the option is on) for the local player.
    fn shake(&self, partial: f32) -> Mat4 {
        if self.spectate_view.is_some() {
            return Mat4::IDENTITY;
        }
        let player = &self.world.client.state.player;
        let hurt = (player.hurt_duration > 0).then_some(crate::camera_fx::Hurt {
            time: player.hurt_time as f32 - partial,
            duration: player.hurt_duration as f32,
            dir: player.hurt_dir,
        });
        let mut m = crate::camera_fx::hurt_matrix(
            self.death_ticks(partial),
            hurt,
            self.options.damage_tilt_strength as f32,
        );
        if self.options.bob_view {
            let (walked, amount) = self.camera_fx.walk.sample(partial);
            m *= crate::camera_fx::bob_matrix(walked, amount);
        }
        m
    }

    /// The `camera_distance` attribute times the scale, or the mount's when that is further.
    fn camera_distance(&self) -> f64 {
        let state = &self.world.client.state;
        let attr = |attrs: &rustc_hash::FxHashMap<i32, mcr_client::AttributeState>,
                    name: &str,
                    default: f64| {
            self.data
                .registry("minecraft:attribute")
                .and_then(|r| r.id_of(name))
                .and_then(|id| attrs.get(&(id as i32)))
                .map_or(default, |a| a.value(None))
        };
        let own = attr(&state.player.attributes, "minecraft:camera_distance", 4.0)
            * attr(&state.player.attributes, "minecraft:scale", 1.0);
        let mount = state
            .player
            .vehicle
            .and_then(|id| state.entities.get(id))
            .filter(|e| {
                matches!(
                    e.data.get(&LIVING_HEALTH),
                    Some(mcr_protocol::types::entity_data::EntityDataValue::Float(_))
                )
            })
            .map_or(0.0, |e| {
                attr(&e.attributes, "minecraft:camera_distance", 4.0)
                    * attr(&e.attributes, "minecraft:scale", 1.0)
            });
        let d = own.max(mount);
        if d.is_finite() {
            d.clamp(0.0, 512.0)
        } else {
            4.0
        }
    }

    fn sync_map(&mut self, id: i32) {
        let Some(map) = self.world.client.state.gameplay.maps.get(&id) else {
            return;
        };
        let (image, deco) = (crate::map::image(&map.colors), crate::map::decorations(map));
        let named = crate::map::named_decorations(map);
        let present = self.entities.as_ref().is_none_or(|e| e.scene.has_map(id))
            && self.hand.as_ref().is_none_or(|h| h.has_map(id));
        let labels = if present && self.map_label_keys.get(&id) == Some(&named) {
            None
        } else {
            if self.map_label_keys.len() >= MAX_MAP_LABEL_KEYS {
                self.map_label_keys.clear();
            }
            let labels = crate::map::labels(&mut self.gui.ui, map);
            self.map_label_keys.insert(id, named);
            Some(labels)
        };
        if let Some(entities) = &mut self.entities {
            entities
                .scene
                .set_map(id, image.clone(), deco.clone(), labels.clone());
        }
        if let Some(hand) = &mut self.hand {
            hand.set_map(id, image, deco, labels);
        }
    }

    /// In mod mode Java plays the sounds of the events it owns, so audio is lent out of reach meanwhile.
    fn on_event(&mut self, event: ClientEvent) {
        let java_owned = self.audio.as_ref().is_some_and(Audio::java_owned)
            && event_owner(&event) == Some(SoundOwner::Java);
        let parked = if java_owned { self.audio.take() } else { None };
        self.handle_event(event);
        if parked.is_some() {
            self.audio = parked;
        }
    }

    fn handle_event(&mut self, event: ClientEvent) {
        let local_id = self.world.client.state.player.entity_id;
        if let Some(entities) = &mut self.entities {
            entities.on_event(&event, local_id);
        }
        if let ClientEvent::EntitySwing {
            entity_id,
            offhand,
            animation,
        } = &event
            && *entity_id == local_id
            && let Some(hand) = &mut self.hand
        {
            use mcr_protocol::types::game::Hand as GameHand;
            let h = if *offhand {
                GameHand::Off
            } else {
                GameHand::Main
            };
            hand.swing_with(
                &self.world.client.state,
                h,
                (animation.kind, animation.duration),
            );
        }
        if let Some(block_entities) = &mut self.block_entities {
            block_entities.on_event(&event, &self.world.client.state);
        }
        if let ClientEvent::EntityEvent { entity_id, event } = &event {
            self.loops.on_entity_event(*entity_id, *event);
            let mut cx = crate::entity_events::EventCx {
                state: &self.world.client.state,
                effects: &mut self.effects,
                audio: self.audio.as_mut(),
            };
            self.entity_events.on_event(&mut cx, *entity_id, *event);
        }
        match event {
            ClientEvent::EntityEvent {
                entity_id,
                event: 35,
            } => {
                let state = &self.world.client.state;
                if entity_id == local_id {
                    self.overlays
                        .start_totem(crate::overlays::totem_stack(state));
                }
                self.effects.totem(entity_id);
                let at = if entity_id == local_id {
                    Some(state.player.pos())
                } else {
                    state.entities.get(entity_id).map(|e| e.pos)
                };
                let source = crate::overlays::entity_sound_source(state, entity_id);
                if let (Some(at), Some(audio)) = (at, &mut self.audio) {
                    audio.play("item.totem.use", Some(at), 1.0, 1.0, source);
                }
            }
            ClientEvent::EntityDamaged {
                entity_id, effects, ..
            } if entity_id == local_id => {
                if let Some(audio) = &mut self.audio {
                    audio.play_voice(effects.player_hurt_sound());
                }
            }
            ClientEvent::EntityEvent {
                entity_id,
                event: 3,
            } if entity_id == local_id => {
                if let Some(audio) = &mut self.audio {
                    audio.play_voice("entity.player.death");
                }
            }
            ClientEvent::EntityAdded(id) => {
                let state = &self.world.client.state;
                if self.effects.on_entity_added(state, id)
                    && let Some(e) = state.entities.get(id)
                    && let Some(audio) = world_audio(&mut self.audio)
                {
                    crate::effects::lightning_sounds(audio, e);
                }
            }
            ClientEvent::Particle(packet) => {
                self.effects.emit(&packet);
                if self.effects.particles.take_elder_guardian() {
                    self.ghost_age = Some(0);
                }
            }
            ClientEvent::BlockDestruction {
                entity_id,
                pos,
                progress,
            } => self.effects.destruction(entity_id, pos, progress),
            ClientEvent::Explosion {
                center,
                radius,
                particle,
                sound,
                block_count,
                block_particles,
            } => {
                self.effects
                    .explosion(center, radius, &particle, block_count, block_particles);
                let name = sound.and_then(|s| self.sound_name(&s));
                if let (Some(name), Some(audio)) = (name, &mut self.audio) {
                    let pitch = (1.0 + (rand_unit() - rand_unit()) * 0.2) * 0.7;
                    audio.play(
                        &name,
                        Some(center),
                        4.0,
                        pitch,
                        mcr_audio::SoundSource::Blocks,
                    );
                }
            }
            ClientEvent::MapUpdated(id) => self.sync_map(id),
            event if crate::net_world::resets_world(&event) => {
                self.effects.clear();
                self.entity_events.clear();
                self.subtitles.clear();
                if let Some(entities) = &mut self.entities {
                    entities.clear();
                }
            }
            ClientEvent::Chat {
                kind,
                message,
                tag,
                signature,
                ..
            } => {
                let text: String = self.gui.ui.flatten(&message).iter().map(|c| c.ch).collect();
                match &tag {
                    Some(t) if kind == ChatKind::Player => {
                        println!("chat [{}]: {}", tag_label(t), sanitize_for_log(&text))
                    }
                    _ => println!("chat: {}", sanitize_for_log(&text)),
                }
                if matches!(kind, ChatKind::Player | ChatKind::Disguised) {
                    let now = self.started.elapsed().as_millis() as u64;
                    self.hud.add_player_chat(
                        &mut self.gui.ui,
                        &message,
                        self.ticks as u32,
                        &self.options,
                        tag.map(line_tag),
                        signature.map(|s| s.0),
                        now,
                    );
                } else if kind != ChatKind::ActionBar {
                    self.hud.add_chat(
                        &mut self.gui.ui,
                        &message,
                        self.ticks as u32,
                        &self.options,
                        tag.map(line_tag),
                        signature.map(|s| s.0),
                    );
                }
            }
            ClientEvent::ChatDeleted(signature) => {
                self.hud.delete_chat(
                    &mut self.gui.ui,
                    &signature.0,
                    self.ticks as u32,
                    &self.options,
                );
            }
            ClientEvent::ChatCleared => self.hud.clear_chat(),
            ClientEvent::LocalSound {
                sound,
                pos,
                volume,
                pitch,
                source,
            } => {
                if let Some(audio) = &mut self.audio {
                    audio.play(sound, Some(pos), volume, pitch, wire_category(source));
                }
            }
            ClientEvent::EntityAnimation { entity_id, action } if action == 1 || action == 2 => {
                self.effects.crit(entity_id, action == 2);
            }
            ClientEvent::Toast(toast) => self.add_toast(toast),
            ClientEvent::Disconnected(reason) => {
                if self.closed.is_none() {
                    self.closed = Some(reason.to_log_string());
                }
            }
            ClientEvent::ElderGuardianGhost => self.ghost_age = Some(0),
            ClientEvent::Died { message } => {
                println!("died");
                self.death_message = message;
            }
            ClientEvent::ContainerOpened { .. } if self.java_gui => self.container_open = false,
            ClientEvent::ContainerOpened {
                menu_type, title, ..
            } => match container_menu(&self.world.client.state, menu_type, title) {
                Some((kind, title)) => {
                    if self.container_open {
                        self.stack.pop();
                    }
                    self.push_screen(Box::new(ContainerScreen::new(kind, title)));
                    self.container_open = true;
                }
                None => {
                    self.net(Client::close_container);
                }
            },
            ClientEvent::SignEditorOpened { .. }
            | ClientEvent::BookOpened { .. }
            | ClientEvent::BlockEditorOpened { .. }
            | ClientEvent::MinecartEditorOpened { .. }
            | ClientEvent::ShowCredits
                if self.java_gui => {}
            ClientEvent::SignEditorOpened { pos, side } => {
                let screen = crate::editors::sign_screen(&self.world.client.state, pos, side);
                self.push_screen(Box::new(screen));
            }
            ClientEvent::BookOpened { hand } => self.open_book(hand),
            ClientEvent::BlockEditorOpened { pos, kind } => {
                let state = &self.world.client.state;
                if let Some(screen) =
                    crate::editors::block_editor_screen(state, pos, kind, &self.test_status)
                {
                    self.push_screen(screen);
                }
            }
            ClientEvent::ShowCredits => self.show_credits(),
            ClientEvent::MinecartEditorOpened { entity_id } => {
                if let Some(screen) =
                    crate::editors::minecart_screen(&self.world.client.state, entity_id)
                {
                    self.push_screen(Box::new(screen));
                }
            }
            ClientEvent::TestInstanceStatus { status, size } => {
                *self.test_status.borrow_mut() = Some((status, size));
            }
            ClientEvent::ContainerUpdated(_) => self.container_updates += 1,
            ClientEvent::ContainerClosed(_) => {
                if self.container_open {
                    self.stack.pop();
                    self.container_open = false;
                }
            }
            ClientEvent::PlayerRotated => {
                let p = &self.world.client.state.player;
                self.yaw = p.physics.yaw;
                self.pitch = p.physics.pitch;
            }
            ClientEvent::ViewTurned { yaw } => {
                self.yaw = (self.yaw + yaw).rem_euclid(360.0);
            }
            ClientEvent::PlayerTeleported { .. } => {
                let p = &self.world.client.state.player;
                self.yaw = p.physics.yaw;
                self.pitch = p.physics.pitch;
                self.prev_eye = self.cur_eye;
                self.teleports += 1;
            }
            ClientEvent::Sound {
                sound,
                source,
                pos,
                entity_id,
                volume,
                pitch,
                seed,
            } => {
                let name = self.sound_name(&sound);
                let category = wire_category(source);
                let state = &self.world.client.state;
                match (name, &mut self.audio, entity_id) {
                    (Some(name), Some(audio), Some(id)) => {
                        if let Some(at) = sound_entity_pos(state, id) {
                            audio.play_bound(&name, id, at, volume, pitch, category, seed);
                        }
                    }
                    (Some(name), Some(audio), None) => {
                        audio.play_seeded(&name, pos, volume, pitch, category, seed, false);
                    }
                    _ => {}
                }
            }
            ClientEvent::StopSound { source, name } => {
                if let Some(audio) = &mut self.audio {
                    audio.stop_sound(name.as_ref().map(|n| n.as_str()), source.map(wire_category));
                }
            }
            ClientEvent::ConsumeEffects { hand, particles } => {
                self.consume_effects(hand, particles)
            }
            ClientEvent::BlockEvent { pos, b1, block, .. } => self.note_particle(pos, b1, block),
            ClientEvent::ItemTaken {
                pos,
                experience_orb,
                silent: false,
                ..
            } => {
                if let Some(audio) = &mut self.audio {
                    audio.play_pickup(pos, experience_orb);
                }
            }
            ClientEvent::LevelEvent {
                event: 1010,
                pos,
                data,
                ..
            } => {
                let sound = self.ambience.jukebox_sound(&self.world.client.state, data);
                if let (Some(sound), Some(audio)) = (sound, &mut self.audio) {
                    audio.play_record(&sound, [pos.x, pos.y, pos.z]);
                }
                if let Some(song) =
                    crate::ambience::jukebox_description(&self.world.client.state, data)
                {
                    self.world.client.state.titles.set_now_playing(song);
                }
            }
            ClientEvent::LevelEvent {
                event: 1011, pos, ..
            } => {
                if let Some(audio) = &mut self.audio {
                    audio.stop_record([pos.x, pos.y, pos.z]);
                }
            }
            ClientEvent::LevelEvent {
                event,
                pos,
                data,
                global,
                ..
            } => {
                let mut cx = crate::level_events::Cx {
                    state: &self.world.client.state,
                    effects: &mut self.effects,
                    audio: self.audio.as_mut(),
                    camera: self.cur_eye,
                };
                self.level_events.handle(&mut cx, event, pos, data, global);
            }
            _ => {}
        }
    }

    /// `ItemStack#onUseTick` and `completeUsingItem` of the local player's food or drink.
    fn consume_effects(&mut self, hand: mcr_protocol::types::game::Hand, particles: u8) {
        let state = &self.world.client.state;
        let stack = state.hand_stack(hand);
        let Some(c) = crate::entity_events::consumable(&stack) else {
            return;
        };
        let mut cx = crate::entity_events::EventCx {
            state,
            effects: &mut self.effects,
            audio: world_audio(&mut self.audio),
        };
        self.entity_events
            .consume(&mut cx, state.player.entity_id, &stack, &c, particles);
    }

    /// `NoteBlock#triggerEvent`: tunable instruments show a note coloured by its pitch.
    fn note_particle(&mut self, pos: BlockPos, b1: u8, block: i32) {
        let _ = b1;
        let data = self.data;
        if u32::try_from(block).ok() != data.block_by_name("minecraft:note_block") {
            return;
        }
        let Some(state) = self.world.client.state.chunks.block(pos.x, pos.y, pos.z) else {
            return;
        };
        let mut note = None;
        let mut tunable = true;
        for (k, v) in data.state_properties(state.0) {
            match k {
                "note" => note = v.parse::<u8>().ok(),
                "instrument" => {
                    tunable = !matches!(
                        v,
                        "zombie"
                            | "skeleton"
                            | "creeper"
                            | "dragon"
                            | "wither_skeleton"
                            | "piglin"
                            | "custom_head"
                    )
                }
                _ => {}
            }
        }
        if let (true, Some(note)) = (tunable, note) {
            let at = [
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 1.2,
                f64::from(pos.z) + 0.5,
            ];
            let particle = effects::particle_id("minecraft:note");
            self.effects
                .add_particle(&particle, at, [f64::from(note) / 24.0, 0.0, 0.0]);
        }
    }

    fn sound_name(&self, sound: &mcr_buf::IdOr<mcr_protocol::types::SoundEvent>) -> Option<String> {
        match sound {
            mcr_buf::IdOr::Id(id) => self
                .data
                .registry("minecraft:sound_event")
                .and_then(|r| u32::try_from(*id).ok().and_then(|i| r.name_of(i)))
                .map(str::to_owned),
            mcr_buf::IdOr::Inline(ev) => Some(ev.location.as_str().to_owned()),
        }
    }

    fn gameplay(&mut self) {
        if self.world.client.state.player.dead {
            if self.attack_click || self.use_click {
                self.net(Client::respawn);
            }
            self.attack_click = false;
            self.use_click = false;
            return;
        }
        let keys = CombatKeys {
            attack_clicks: u8::from(std::mem::take(&mut self.attack_click)),
            use_clicks: u8::from(std::mem::take(&mut self.use_click)),
            attack_down: self.held.is_active(self.keys.attack),
            use_down: self.held.is_active(self.keys.use_item),
        };
        let hit = match self.target {
            Some(Target::Entity { id, hit, .. }) => {
                let at = self
                    .world
                    .client
                    .state
                    .entities
                    .get(id)
                    .map_or(hit, |e| e.pos);
                UseTarget::Entity {
                    id,
                    location: std::array::from_fn(|a| hit[a] - at[a]),
                }
            }
            Some(Target::Block(hit)) => UseTarget::Block {
                pos: hit.pos,
                face: hit.face,
                cursor: hit.cursor,
                inside: hit.inside,
            },
            None => UseTarget::Air,
        };
        let busy = self.busy();
        let Some(out) = self.net(|c| c.handle_keybinds(keys, hit, busy)) else {
            return;
        };
        if let Some(hand) = &mut self.hand {
            let state = &self.world.client.state;
            if let Some((h, attack)) = out.swing_hand {
                hand.swing(state, h, attack);
            } else if out.swing {
                hand.swing(state, mcr_protocol::types::game::Hand::Main, true);
            }
            if let Some(h) = out.item_used {
                hand.item_used(h);
            }
        }
        if let Some((at, state)) = out.placed {
            self.play_block(state, BlockSound::Place, center(at));
        }
        if out.stab {
            self.play_stab();
        }
    }

    /// `PiercingWeapon#makeSound`: the server leaves the stabbing player out of its broadcast.
    fn play_stab(&mut self) {
        let state = &self.world.client.state;
        let stack = state.held_stack();
        let patched = stack.components.added.iter().find_map(|c| match c {
            mcr_protocol::types::component::DataComponent::PiercingWeapon(p) => {
                Some(p.sound.clone())
            }
            _ => None,
        });
        let name = match patched {
            Some(Some(sound)) => self.sound_name(&sound),
            Some(None) => None,
            None => {
                let item = u32::try_from(stack.item)
                    .ok()
                    .and_then(|i| self.data.item_name(i));
                Some(if item == Some("minecraft:wooden_spear") {
                    "minecraft:item.spear_wood.attack".to_owned()
                } else {
                    "minecraft:item.spear.attack".to_owned()
                })
            }
        };
        let at = self.world.client.state.player.pos();
        if let (Some(name), Some(audio)) = (name, &mut self.audio) {
            audio.play(&name, Some(at), 1.0, 1.0, SoundSource::Players);
        }
    }

    fn play_block(&mut self, state: u32, kind: BlockSound, at: [f64; 3]) {
        if let Some(audio) = &mut self.audio {
            audio.play_block(state, kind, at);
        }
    }

    fn sounds_tick(&mut self) {
        let Some(audio) = &mut self.audio else { return };
        audio.poll(self.ticks);
        let state = &self.world.client.state;
        audio.tick_bound(|id| sound_entity_pos(state, id));
        let now = self.started.elapsed().as_millis() as u64;
        for cue in audio.take_subtitle_cues() {
            self.subtitles.push(cue, now);
        }
        self.movement.steps_replaced = self.java_steps;
        self.movement.tick(state, self.ticks, audio);
        if audio.java_owned() {
            return;
        }
        self.ambience.set_credits(self.stack.credits_music(), audio);
        self.ambience.tick(state, self.cur_eye, audio);
        self.loops.tick(state, audio);
        let events = audio.take_music_events();
        self.apply_music_events(events);
    }

    fn apply_music_events(&mut self, events: Vec<MusicEvent>) {
        for event in events {
            match event {
                MusicEvent::Started(key) => {
                    let title = self
                        .gui
                        .ui
                        .lang()
                        .get(&key)
                        .map_or_else(|| key.clone(), str::to_owned);
                    self.gui.toasts.set_song(&mut self.gui.ui, Some(&title));
                    if self.options.music_toast == MusicToastState::PauseAndToast
                        && music_volume(&self.options) > 0.0
                    {
                        self.gui.toasts.show_now_playing();
                    }
                }
                MusicEvent::Stopped => self.gui.toasts.set_song(&mut self.gui.ui, None),
            }
        }
    }

    fn pick_from(&self, eye: [f64; 3]) -> Option<Target> {
        let state = &self.world.client.state;
        let dir = view_direction(self.yaw, self.pitch);
        let picker = EntityPicker {
            state,
            own_id: state.player.entity_id,
            own_vehicle: state.player.vehicle,
        };
        pick(
            &state.chunks,
            &picker,
            eye,
            dir,
            state.block_interaction_range(),
            state.entity_interaction_range(),
        )
    }

    pub fn release_all(&mut self) {
        for i in 0..MAPPINGS.len() {
            self.held.update(i, false, &self.options);
        }
        self.tab_held = false;
    }

    /// Keys still physically down when a Java screen closed, as if freshly pressed (vanilla
    /// resumes movement on close); only movement mappings, so a click on a closing button
    /// does not attack.
    pub fn resume_held(&mut self, keys: &[InputKey]) {
        let k = self.keys;
        let movement = [
            k.forward, k.back, k.left, k.right, k.jump, k.sneak, k.sprint,
        ];
        for &key in keys {
            let indices: Vec<usize> = self.bindings.lookup(key).to_vec();
            for i in indices {
                if movement.contains(&i) && !self.held.is_active(i) && !self.java_owns(i) {
                    self.binding(i, true);
                }
            }
        }
    }

    pub fn mouse_motion(&mut self, dx: f64, dy: f64) {
        if self.busy() || self.closed.is_some() || self.input_captured() || self.java_turn {
            return;
        }
        let k = degrees_per_pixel(self.options.mouse_sensitivity) * self.java_fov_scale.min(1.0);
        let sx = if self.options.invert_x_mouse {
            -1.0
        } else {
            1.0
        };
        let sy = if self.options.invert_y_mouse {
            -1.0
        } else {
            1.0
        };
        self.look(dx as f32 * k * sx, dy as f32 * k * sy);
    }

    fn is_spectator(&self) -> bool {
        self.world.client.state.player.game_mode == mcr_protocol::types::game::GameType::Spectator
    }

    fn millis(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// `SpectatorGui#getHotbarAlpha`: full for five seconds after a key, then a two second fade.
    fn spectator_menu_alpha(&mut self) -> f32 {
        let alpha = (self.spectator_menu_at as f32 + 5000.0 - self.millis() as f32) / 2000.0;
        let alpha = alpha.clamp(0.0, 1.0);
        if self.world.client.spectator_menu().is_some() && (alpha <= 0.0 || !self.is_spectator()) {
            self.world.client.close_spectator_menu();
        }
        alpha
    }

    pub fn look(&mut self, dyaw: f32, dpitch: f32) {
        let mut yaw = self.yaw + dyaw;
        if let Some((bow, limit)) = self.world.client.boat_view_clamp() {
            let delta = (yaw - bow + 180.0).rem_euclid(360.0) - 180.0;
            yaw += delta.clamp(-limit, limit) - delta;
        }
        self.yaw = yaw.rem_euclid(360.0);
        self.pitch = (self.pitch + dpitch).clamp(-90.0, 90.0);
        self.turn_latency.look(Instant::now(), self.yaw, self.pitch);
    }

    /// The look (yaw, pitch in degrees) a rendered camera shows, undoing [`Self::player_camera`].
    fn shown_look(&self, camera: &Camera) -> (f32, f32) {
        let (mut yaw, mut pitch) = (camera.yaw, camera.pitch);
        if self.perspective == Perspective::ThirdFront {
            yaw -= std::f32::consts::PI;
            pitch = -pitch;
        }
        (
            (std::f32::consts::PI - yaw).to_degrees().rem_euclid(360.0),
            -pitch.to_degrees(),
        )
    }

    pub fn scroll(&mut self, lines: f32) {
        if !lines.is_finite() {
            return;
        }
        if self.busy() {
            let (x, y) = self.mouse;
            self.ui_input(UiInput::Scroll {
                x,
                y,
                dx: 0.0,
                dy: lines,
                mods: self.mods,
            });
            return;
        }
        let Some(step) = scroll_wheel(
            &mut self.scroll,
            lines * self.options.mouse_wheel_sensitivity as f32,
        ) else {
            return;
        };
        if self.is_spectator() {
            if self.world.client.spectator_scroll(step) {
                self.spectator_menu_at = self.millis();
            }
            return;
        }
        let held = i32::from(self.world.client.state.player.held_slot);
        let slot = (held - step).rem_euclid(9);
        self.net(|c| c.select_hotbar(slot as u8));
    }

    pub fn key_event(&mut self, key: InputKey, pressed: bool, text: Option<&str>) {
        if self.closed.is_some() {
            return;
        }
        track_modifiers(&mut self.mods, key, pressed);
        if self.busy() {
            self.screen_key(key, pressed, text);
            return;
        }
        if key == InputKey::Keyboard(KEY_ESCAPE) {
            if pressed && !self.java_gui {
                self.open_pause();
            }
            return;
        }
        if key == InputKey::Keyboard(KEY_LEFT_CTRL) {
            self.ctrl_held[0] = pressed;
        } else if key == InputKey::Keyboard(KEY_RIGHT_CTRL) {
            self.ctrl_held[1] = pressed;
        }
        let indices: Vec<usize> = self.bindings.lookup(key).to_vec();
        for i in indices {
            if !self.java_owns(i) {
                self.binding(i, pressed);
            }
        }
    }

    fn screen_keys(&self) -> mcr_ui::ScreenKeys {
        let key = |index: usize| crate::input::screen_key(self.bindings.input(index));
        mcr_ui::ScreenKeys {
            hotbar: self.keys.hotbar.map(key),
            swap_offhand: key(self.keys.swap),
            drop: key(self.keys.drop),
            inventory: key(self.keys.inventory),
            chat: key(self.keys.chat),
            save_hotbar: key(self.keys.save_hotbar),
        }
    }

    fn screen_key(&mut self, key: InputKey, pressed: bool, text: Option<&str>) {
        for input in ui_inputs(key, pressed, text, self.mods, self.mouse) {
            self.ui_input(input);
        }
    }

    pub fn ui_input(&mut self, input: UiInput) {
        if let UiInput::MouseMove { x, y } = input {
            self.mouse = (x, y);
        }
        let mut actions = Vec::new();
        {
            self.view_cache.refresh(&self.world.client.state);
            let view = StateView {
                state: &self.world.client.state,
                operator_items: self.options.operator_items_tab,
                advanced_tooltips: self.options.advanced_item_tooltips,
                keys: self.screen_keys(),
                cache: &self.view_cache,
            };
            let mut env = Env {
                view: &view,
                clipboard: &mut self.gui.clipboard,
                now_ms: self.started.elapsed().as_millis() as u64,
            };
            self.stack.input(&mut env, &input, &mut actions);
        }
        self.handle_actions(actions);
        self.suggest.pump(&mut self.world.client);
    }

    pub fn cursor_moved(&mut self, px: f64, py: f64) {
        let (_, scale) = gui::gui_size(self.render.renderer.size(), self.gui_scale());
        let scale = f64::from(scale);
        let (x, y) = ((px / scale) as f32, (py / scale) as f32);
        self.mouse = (x, y);
        if self.busy() {
            self.ui_input(UiInput::MouseMove { x, y });
        }
    }

    fn update_gui_size(&mut self) {
        let (size, _) = gui::gui_size(self.render.renderer.size(), self.gui_scale());
        if size != self.gui_size {
            self.gui_size = size;
            self.stack.resize(size.0, size.1);
        }
    }

    /// Opens a modal screen from outside the play loop (the server resource pack prompt).
    pub fn show_screen(&mut self, screen: Box<dyn Screen>) {
        self.push_screen(screen);
    }

    pub fn close_top_screen(&mut self) {
        self.stack.pop();
    }

    /// Start LAN World was pressed with these settings.
    pub fn take_lan_request(&mut self) -> Option<mcr_ui::LanSettings> {
        self.lan_request.take()
    }

    /// The pause menu opened since the last call.
    pub fn take_pause_opened(&mut self) -> bool {
        std::mem::take(&mut self.pause_opened)
    }

    /// Captures the next frame's world image as the world icon.
    pub fn request_icon_capture(&mut self) {
        self.capture_icon = true;
    }

    /// The latest captured 64×64 RGBA world icon.
    pub fn take_world_icon(&mut self) -> Option<Vec<u8>> {
        self.world_icon.take()
    }

    /// A client-side chat line, like vanilla's local system messages.
    pub fn local_chat(&mut self, message: &mcr_protocol::types::text::Component) {
        let text: String = self.gui.ui.flatten(message).iter().map(|c| c.ch).collect();
        println!("chat: {}", sanitize_for_log(&text));
        self.hud.add_chat(
            &mut self.gui.ui,
            message,
            self.ticks as u32,
            &self.options,
            Some(mcr_ui::ChatLineTag::SystemSinglePlayer),
            None,
        );
    }

    fn open_lan_screen(&mut self) {
        let state = &self.world.client.state;
        let mode = state.player.game_mode as u8;
        let cheats = state.player.permission_level >= 2;
        let screen = mcr_ui::ShareToLanScreen::new(mode, cheats, state.world.hardcore)
            .with_port_check(Box::new(mcr_singleplayer::lan::port_available));
        self.push_screen(Box::new(screen));
    }

    pub fn take_server_icon(&mut self) -> Option<Vec<u8>> {
        self.world.client.state.server.icon.take()
    }

    pub fn take_conduct_answers(&mut self) -> Vec<(bool, bool, i32)> {
        std::mem::take(&mut self.conduct_answers)
    }

    pub fn take_pack_answers(&mut self) -> Vec<bool> {
        std::mem::take(&mut self.pack_answers)
    }

    pub fn take_dialog_replies(&mut self) -> Vec<mcr_client::DialogReply> {
        std::mem::take(&mut self.dialog_replies)
    }

    /// Shows a server dialog, replacing the one on top so the previous screen stays underneath.
    pub fn show_dialog(&mut self, dialog: mcr_ui::Dialog) {
        let links = crate::editors::server_links(&self.world.client.state);
        if self.stack.top_is_dialog() {
            self.stack.pop();
        }
        self.push_screen(Box::new(
            mcr_ui::DialogScreen::new(dialog, links).local(self.local_server),
        ));
    }

    pub fn clear_dialog(&mut self) {
        if self.stack.top_is_dialog() {
            self.stack.pop();
        }
    }

    fn open_book(&mut self, hand: mcr_protocol::types::game::Hand) {
        let state = &self.world.client.state;
        let Some((writable, _)) = crate::editors::book_pages(state, hand) else {
            return;
        };
        if writable {
            let name = self.world.client.config.name.clone();
            let screen = crate::editors::book_edit_screen(state, hand, &name);
            self.push_screen(Box::new(screen));
        } else if let Some(screen) = crate::editors::book_view_screen(state, hand) {
            self.push_screen(Box::new(screen));
        }
    }

    pub fn force_close(&mut self, reason: String) {
        self.closed.get_or_insert(reason);
    }

    /// Swaps in assets rebuilt from a new resource stack without restarting the session.
    pub fn assets(&self) -> &Arc<Assets> {
        &self.assets
    }

    pub fn reload_resources(&mut self, assets: Arc<Assets>) -> anyhow::Result<()> {
        self.gui.reload(&assets.resources)?;
        self.hud.rebind(&self.gui.ui, self.data);
        self.overlays = Overlays::new();
        if let Some(audio) = &mut self.audio {
            audio.reload(&assets.resources);
        }
        self.effects = WorldEffects::new(assets.clone());
        self.effects.set_options(&self.options);
        self.block_animation = BlockAnimation::new(self.data, assets.clone());
        let renderer = &mut self.render.renderer;
        renderer.set_effect_assets(&effects::effect_assets(&assets));
        renderer.set_post_overrides(crate::post_effects::pack_overrides(&assets.resources));
        let format = renderer.target_format();
        (self.entities, self.block_entities, self.hand, self.preview) = match assets.scene.clone() {
            Some(sa) => (
                EntityLayer::new(renderer.device(), renderer.queue(), format, sa.clone()),
                BlockEntities::new(renderer.device(), renderer.queue(), format, sa.clone()),
                HeldHand::new(renderer.device(), renderer.queue(), format, sa.clone()),
                Preview::new(renderer.device(), renderer.queue(), format, sa),
            ),
            None => (None, None, None, None),
        };
        if let Some(block_entities) = &mut self.block_entities {
            for pos in self.world.loaded_columns() {
                block_entities.load_chunk(&self.world.client.state, pos);
            }
        }
        let maps: Vec<i32> = self
            .world
            .client
            .state
            .gameplay
            .maps
            .keys()
            .copied()
            .collect();
        self.map_label_keys.clear();
        for id in maps {
            self.sync_map(id);
        }
        self.render.reload_assets(assets.clone(), &self.world);
        self.assets = assets;
        Ok(())
    }

    fn show_credits(&mut self) {
        if self.stack.credits_music() {
            return;
        }
        let text = |path: &str| {
            self.assets
                .read(path)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
        };
        let source = CreditsSource {
            poem: text(POEM_PATH),
            credits: text(CREDITS_PATH),
            postcredits: text(POSTCREDITS_PATH),
        };
        let player = self.world.client.config.name.clone();
        self.push_screen(Box::new(CreditsScreen::new(&source, true, &player)));
    }

    fn push_screen(&mut self, screen: Box<dyn Screen>) {
        self.release_all();
        self.update_gui_size();
        self.stack.push(screen);
    }

    pub fn open_advancements(&mut self) {
        if !self.busy() && self.closed.is_none() {
            self.push_screen(Box::new(mcr_ui::AdvancementsScreen::new()));
        }
    }

    pub fn open_pause(&mut self) {
        if !self.busy() && self.closed.is_none() {
            let local = self.local_server;
            let lan = local && !self.lan_open;
            self.push_screen(Box::new(PauseScreen::new(lan, local).with_lan(lan)));
            self.pause_opened = true;
        }
    }

    pub fn lose_focus(&mut self) {
        self.release_all();
        if self.options.pause_on_lost_focus && !self.java_gui {
            self.open_pause();
        }
    }

    pub fn open_chat(&mut self, initial: &str) {
        if self.busy() {
            return;
        }
        let screen = ChatScreen::new(
            self.hud.chat.clone(),
            self.suggest.suggester(),
            crate::hud::chat_options(&self.options, true),
        )
        .with_initial(initial);
        self.push_screen(Box::new(screen));
        self.chat_depth = Some(self.stack.len());
    }

    /// `ClientPacketListener#sendUnattendedCommand`: a clicked command runs at once when it parses
    /// with no permissions and sends no signed chat, otherwise the player confirms it.
    fn run_clicked_command(&mut self, command: String) {
        use mcr_client::CommandCheck;
        use mcr_ui::CommandPrompt;
        let state = &self.world.client.state;
        let check = state
            .commands
            .check(&command, state.player.permission_level >= 2);
        let back = !self.stack.is_empty();
        let prompt = match check {
            CommandCheck::NoIssues => {
                self.net(|c| c.send_command(&command));
                return;
            }
            CommandCheck::ParseErrors => CommandPrompt::ParseErrors,
            CommandCheck::PermissionsRequired => CommandPrompt::PermissionsRequired,
            CommandCheck::SignatureRequired => CommandPrompt::SignatureRequired { suggest: !back },
        };
        let screen = mcr_ui::ConfirmScreen::command_prompt(&command, prompt, back);
        self.push_screen(Box::new(screen));
    }

    pub fn open_inventory(&mut self) {
        if self.busy() {
            return;
        }
        if self.world.client.state.riding_custom_inventory() {
            self.net(Client::open_inventory);
            return;
        }
        let creative = self.world.client.state.player.instabuild;
        if creative {
            self.push_screen(Box::new(CreativeScreen::new(self.hotbars.clone())));
        } else {
            self.push_screen(Box::new(ContainerScreen::player_inventory()));
        }
        self.creative_open = creative;
        self.container_open = true;
        self.player_inventory = true;
    }

    fn hotbar_load_or_save(&mut self, index: usize, load: bool) {
        if load {
            let saved = self.hotbars.borrow().get(index).cloned();
            for (i, stack) in saved.unwrap_or_default().into_iter().enumerate() {
                self.net(|c| c.set_creative_slot(36 + i as i16, stack));
            }
        } else {
            let slots = &self.world.client.state.inventory.player.slots;
            let hotbar: [ItemStack; 9] = std::array::from_fn(|i| slots[36 + i].clone());
            let mut store = self.hotbars.borrow_mut();
            store.save(index, hotbar);
            if let Err(e) = std::fs::write(&self.hotbar_path, store.to_text()) {
                tracing::warn!("saving hotbars: {e}");
            }
        }
    }

    fn chat_open(&mut self) -> bool {
        let open = self.chat_depth.is_some_and(|d| self.stack.len() >= d);
        if !open {
            self.chat_depth = None;
        }
        open
    }

    fn sync_settings(&mut self) {
        let version = self.settings.borrow().version;
        if version == self.settings_seen {
            return;
        }
        self.settings_seen = version;
        let (options, bindings) = {
            let s = self.settings.borrow();
            (s.file.options.clone(), s.file.bindings.clone())
        };
        let old = std::mem::replace(&mut self.options, options);
        self.bindings = bindings;
        let rd = self.options.render_distance;
        if rd != old.render_distance {
            self.world.set_render_distance(rd);
            self.net(|c| c.set_view_distance(rd.clamp(2, 32) as i8));
        }
        if self.options.enable_vsync != old.enable_vsync {
            self.render.renderer.set_vsync(self.options.enable_vsync);
        }
        if let Some(audio) = &mut self.audio {
            audio.apply_volumes(&self.options);
        }
        if self.options.music_toast != old.music_toast
            && self.options.music_toast == MusicToastState::PauseAndToast
            && music_volume(&self.options) > 0.0
        {
            self.gui.toasts.show_now_playing();
        }
        self.gui.sync_language(&self.options.language);
    }

    fn languages(&self) -> Vec<mcr_ui::LanguageInfo> {
        self.gui.ui.lang().languages().to_vec()
    }

    pub fn gui_mut(&mut self) -> &mut Gui {
        &mut self.gui
    }

    fn add_toast(&mut self, toast: mcr_client::ToastEvent) {
        use mcr_client::ToastEvent;
        use mcr_protocol::play::clientbound::ui::AdvancementFrame;
        use mcr_ui::ToastFrame;
        match toast {
            ToastEvent::Advancement { title, frame, icon } => {
                let frame = match frame {
                    AdvancementFrame::Task => ToastFrame::Task,
                    AdvancementFrame::Goal => ToastFrame::Goal,
                    AdvancementFrame::Challenge => ToastFrame::Challenge,
                };
                self.gui
                    .toasts
                    .add_advancement(&mut self.gui.ui, frame, &title, icon);
            }
            ToastEvent::Recipe { station, result } => {
                self.gui.toasts.add_recipe(station, result);
            }
            ToastEvent::InsecureServer => {
                let lang = self.gui.ui.lang();
                let text = |key: &str| lang.get(key).unwrap_or(key).to_owned();
                let (title, message) = (
                    text("multiplayer.unsecureserver.toast.title"),
                    text("multiplayer.unsecureserver.toast"),
                );
                self.gui.toasts.add_system(
                    &mut self.gui.ui,
                    mcr_ui::SystemToastId::new(INSECURE_SERVER_TOAST),
                    &title,
                    Some(&message),
                );
            }
            ToastEvent::LowDiskSpace => {
                let lang = self.gui.ui.lang();
                let text = |key: &str| lang.get(key).unwrap_or(key).to_owned();
                let (title, message) = (
                    text("chunk.toast.lowDiskSpace"),
                    text("chunk.toast.lowDiskSpace.description"),
                );
                self.gui.toasts.add_system(
                    &mut self.gui.ui,
                    mcr_ui::SystemToastId::new(LOW_DISK_SPACE_TOAST),
                    &title,
                    Some(&message),
                );
            }
        }
    }

    fn tick_toasts(&mut self) {
        let now = self.started.elapsed().as_millis() as u64;
        let sounds = self
            .gui
            .tick_toasts(now, self.options.notification_display_time);
        if let Some(audio) = &mut self.audio {
            for sound in sounds {
                audio.play(sound, None, 1.0, 1.0, mcr_audio::SoundSource::Ui);
            }
        }
    }

    fn screens_tick(&mut self) {
        self.suggest.pump(&mut self.world.client);
        let history = self.hud.chat.borrow_mut().take_command_history();
        if let Some(text) = history
            && let Err(e) = std::fs::write(&self.command_history_path, text)
        {
            tracing::warn!("saving {COMMAND_HISTORY_FILE}: {e}");
        }
        self.death_check();
        self.bed_check();
        if self.stack.is_empty() {
            return;
        }
        let mut actions = Vec::new();
        {
            self.view_cache.refresh(&self.world.client.state);
            let view = StateView {
                state: &self.world.client.state,
                operator_items: self.options.operator_items_tab,
                advanced_tooltips: self.options.advanced_item_tooltips,
                keys: self.screen_keys(),
                cache: &self.view_cache,
            };
            let mut env = Env {
                view: &view,
                clipboard: &mut self.gui.clipboard,
                now_ms: self.started.elapsed().as_millis() as u64,
            };
            self.stack.tick(&mut env, &mut actions);
        }
        self.handle_actions(actions);
    }

    /// Respawns a dead player as the death screen's button would; the soak has nobody to press it.
    pub fn respawn_if_dead(&mut self) {
        if self.world.client.state.player.dead {
            self.handle_actions(vec![ScreenAction::Respawn]);
        }
    }

    /// Vanilla `Gui#tick` opens the in-bed chat whenever a sleeper has no screen.
    fn bed_check(&mut self) {
        let player = &self.world.client.state.player;
        if self.java_gui || self.busy() || player.dead || !player.is_sleeping() {
            return;
        }
        let screen = ChatScreen::new(
            self.hud.chat.clone(),
            self.suggest.suggester(),
            crate::hud::chat_options(&self.options, true),
        )
        .in_bed();
        self.push_screen(Box::new(screen));
        self.chat_depth = Some(self.stack.len());
    }

    fn death_check(&mut self) {
        let player = &self.world.client.state.player;
        let (dead, show) = (player.dead, player.show_death_screen);
        if self.java_gui {
            return;
        }
        if dead && show && !self.death_shown {
            self.death_shown = true;
            self.stack.clear();
            self.container_open = false;
            let hardcore = self.world.client.state.world.hardcore;
            let cause = self.death_message.take();
            self.push_screen(Box::new(DeathScreen::new(cause, hardcore)));
        } else if !dead && self.death_shown {
            self.death_shown = false;
            self.stack.clear();
        }
    }

    fn handle_actions(&mut self, actions: Vec<ScreenAction>) {
        use mcr_protocol::play::serverbound::ContainerInput as In;
        use mcr_ui::ClickType as Ct;
        for action in actions {
            match action {
                ScreenAction::SendChat(m) => {
                    self.net(|c| c.send_chat(&m));
                }
                ScreenAction::SendCommand(m) => {
                    self.net(|c| c.send_command(&m));
                }
                ScreenAction::ContainerClick(c) => {
                    let mode = match c.mode {
                        Ct::Pickup => In::Pickup,
                        Ct::QuickMove => In::QuickMove,
                        Ct::Swap => In::Swap,
                        Ct::Clone => In::Clone,
                        Ct::Throw => In::Throw,
                        Ct::QuickCraft => In::QuickCraft,
                        Ct::PickupAll => In::PickupAll,
                    };
                    let (slot, button) = (c.slot as i16, c.button as i8);
                    self.net(|cl| cl.click_container(slot, button, mode));
                }
                ScreenAction::ContainerButton(b) => {
                    self.net(|c| c.click_container_button(b));
                }
                ScreenAction::CloseContainer => {
                    self.net(Client::close_container);
                    self.stack.pop();
                    self.container_open = false;
                    if std::mem::take(&mut self.creative_open) {
                        self.world.client.set_cursor(ItemStack::default());
                    }
                }
                ScreenAction::CreativeSetCarried(stack) => self.world.client.set_cursor(stack),
                ScreenAction::CreativeClick(c) => {
                    let mode = match c.mode {
                        Ct::Pickup => In::Pickup,
                        Ct::QuickMove => In::QuickMove,
                        Ct::Swap => In::Swap,
                        Ct::Clone => In::Clone,
                        Ct::Throw => In::Throw,
                        Ct::QuickCraft => In::QuickCraft,
                        Ct::PickupAll => In::PickupAll,
                    };
                    let (slot, button) = (c.slot as i16, c.button as i8);
                    self.net(|cl| cl.creative_menu_click(slot, button, mode));
                }
                ScreenAction::CreativeSetSlot { slot, stack } => {
                    self.net(|c| c.set_creative_slot(slot as i16, stack));
                }
                ScreenAction::CreativeDrop(stack) => {
                    self.net(|c| c.creative_drop(stack));
                }
                ScreenAction::CreativeClearInventory => {
                    self.net(Client::creative_clear_inventory);
                }
                ScreenAction::CrafterSlotState { slot, enabled } => {
                    self.net(|c| c.set_crafter_slot_state(slot as i32, enabled));
                }
                ScreenAction::SelectTrade(i) => {
                    self.net(|c| c.select_trade(i as i32));
                }
                ScreenAction::RenameItem(name) => {
                    self.net(|c| c.rename_item(&name));
                }
                ScreenAction::SelectBundleItem { slot, index } => {
                    self.net(|c| c.select_bundle_item(slot, index));
                }
                ScreenAction::SetBeacon { primary, secondary } => {
                    let effects = self.data.registry("minecraft:mob_effect");
                    let id = |n: Option<String>| {
                        n.and_then(|n| effects.as_ref()?.id_of(&n))
                            .and_then(|i| i32::try_from(i).ok())
                    };
                    let (p, q) = (id(primary), id(secondary));
                    self.net(|c| c.set_beacon(p, q));
                }
                ScreenAction::StopSleeping => {
                    self.net(Client::stop_sleeping);
                }
                ScreenAction::Respawn => {
                    self.net(Client::respawn);
                    self.death_shown = false;
                    self.stack.clear();
                }
                ScreenAction::Disconnect => self.leave = true,
                ScreenAction::OpenLan if self.local_server && !self.lan_open => {
                    self.open_lan_screen();
                }
                ScreenAction::StartLan(settings) => {
                    self.lan_request = Some(settings);
                    self.stack.clear();
                }
                ScreenAction::OpenOptions => {
                    let screen = OptionsScreen::new(self.settings.clone(), self.languages());
                    self.update_gui_size();
                    self.stack.push(Box::new(screen));
                }
                ScreenAction::SubmitSign(lines) => {
                    let [a, b, c, d] = &lines;
                    self.net(|cl| cl.submit_sign([a, b, c, d]));
                }
                ScreenAction::SubmitBook { pages, title } => {
                    self.net(|c| c.submit_book(&pages, title.as_deref()));
                }
                ScreenAction::Editor(packet) => self.send_editor(packet),
                ScreenAction::ShowDialog(reference) => {
                    self.dialog_replies
                        .push(mcr_client::DialogReply::Open(reference));
                }
                ScreenAction::CustomClick { id, payload } => {
                    self.dialog_replies
                        .push(mcr_client::DialogReply::Custom { id, payload });
                }
                ScreenAction::RunCommand(command) => self.run_clicked_command(command),
                ScreenAction::OpenLink(url) => {
                    if self.options.chat_links {
                        if self.options.chat_links_prompt {
                            if let Some(confirm) = mcr_ui::ConfirmScreen::link(url) {
                                self.push_screen(Box::new(confirm));
                            }
                        } else {
                            gui::open_url(&url);
                        }
                    }
                }
                ScreenAction::OpenChat(text) => {
                    self.stack.clear();
                    self.chat_depth = None;
                    self.open_chat(&text);
                }
                ScreenAction::ChatQueueNext => {
                    let now = self.started.elapsed().as_millis() as u64;
                    self.hud.accept_next_chat(
                        &mut self.gui.ui,
                        self.ticks as u32,
                        &self.options,
                        now,
                    );
                }
                ScreenAction::DialogDisconnect => self.leave = true,
                ScreenAction::PlaySound(name) => {
                    if let Some(audio) = &mut self.audio {
                        audio.play(name, None, 1.0, 1.0, mcr_audio::SoundSource::Ui);
                    }
                }
                ScreenAction::OpenUrl(url) => gui::open_url(&url),
                ScreenAction::CopyToClipboard(text) => {
                    use mcr_ui::Clipboard;
                    self.gui.clipboard.set(&text);
                }
                ScreenAction::ClickSound => {
                    if let Some(audio) = &mut self.audio {
                        audio.click();
                    }
                }
                ScreenAction::OpenAdvancements => {
                    self.push_screen(Box::new(mcr_ui::AdvancementsScreen::new()));
                }
                ScreenAction::OpenStats => {
                    self.push_screen(Box::new(mcr_ui::StatsScreen::new()));
                }
                ScreenAction::RequestStats => {
                    self.net(Client::request_stats);
                }
                ScreenAction::SeenAdvancements(tab) => {
                    self.net(|c| c.seen_advancements(tab.as_deref()));
                }
                ScreenAction::RecipeBookSettings {
                    book_type,
                    open,
                    filtering,
                } => {
                    self.net(|c| c.set_recipe_book_settings(book_type, open, filtering));
                }
                ScreenAction::PlaceRecipe { recipe, max_items } => {
                    self.net(|c| c.place_recipe(recipe, max_items));
                }
                ScreenAction::RecipeSeen(recipe) => {
                    self.net(|c| c.mark_recipe_seen(recipe));
                }
                ScreenAction::Quit => self.quit = true,
                ScreenAction::OpenMods => self.open_mods = true,
                ScreenAction::ResourcePackAnswer(accepted) => self.pack_answers.push(accepted),
                ScreenAction::ConductAnswer {
                    accept,
                    remember,
                    hash,
                } => self.conduct_answers.push((accept, remember, hash)),
                _ => {}
            }
        }
    }

    fn send_editor(&mut self, packet: mcr_ui::EditorPacket) {
        use mcr_client::Client;
        use mcr_protocol::dispatch::PlayServerbound as S;
        use mcr_ui::EditorPacket as E;
        let send = |c: &mut Client, p: S<'_>| c.send_play(&p);
        match packet {
            E::CommandBlock(p) => self.net(|c| send(c, S::SetCommandBlock(p))),
            E::Minecart(p) => self.net(|c| send(c, S::SetCommandMinecart(p))),
            E::Structure(p) => self.net(|c| send(c, S::SetStructureBlock(*p))),
            E::Jigsaw(p) => self.net(|c| send(c, S::SetJigsawBlock(p))),
            E::JigsawGenerate(p) => self.net(|c| send(c, S::JigsawGenerate(p))),
            E::TestBlock(p) => self.net(|c| send(c, S::SetTestBlock(p))),
            E::TestInstance(p) => self.net(|c| send(c, S::TestInstanceBlockAction(*p))),
        };
    }

    pub fn ui_click(&mut self, x: f32, y: f32, button: MouseButton) {
        self.ui_input(UiInput::MouseMove { x, y });
        let mods = self.mods;
        self.ui_input(UiInput::MousePress {
            x,
            y,
            button,
            mods,
            double: false,
        });
        self.ui_input(UiInput::MouseRelease { x, y, button, mods });
    }

    /// Scripted click on container slot `index` of the open screen.
    pub fn click_slot(&mut self, index: usize, button: MouseButton) -> bool {
        let Some((x, y)) = self.stack.top().and_then(|s| s.slot_center(index)) else {
            return false;
        };
        self.ui_click(x as f32, y as f32, button);
        true
    }

    pub fn modifiers(&self) -> Modifiers {
        self.mods
    }

    pub fn submit_chat(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        println!("sending: {}", sanitize_for_log(line));
        match line.strip_prefix('/') {
            Some(command) => self.net(|c| c.send_command(command)),
            None => self.net(|c| c.send_chat(line)),
        };
    }

    fn binding(&mut self, i: usize, pressed: bool) {
        let logical = self.held.update(i, pressed, &self.options);
        let k = self.keys;
        let name = MAPPINGS[i].name;
        if !pressed {
            if i == k.playerlist {
                self.tab_held = false;
            }
            let _ = logical;
            return;
        }
        if i == k.attack {
            self.attack_click = true;
        } else if i == k.use_item {
            self.use_click = true;
        } else if i == k.playerlist {
            self.tab_held = true;
        } else if i == k.chat {
            self.open_chat("");
        } else if i == k.command {
            self.open_chat("/");
        } else if i == k.inventory {
            self.open_inventory();
        } else if i == k.advancements {
            self.open_advancements();
        } else if i == k.drop {
            let whole = self.ctrl_held.iter().any(|&d| d);
            if self.net(|c| c.drop_item(whole)) == Some(true)
                && let Some(hand) = &mut self.hand
            {
                hand.swing_with(
                    &self.world.client.state,
                    mcr_protocol::types::game::Hand::Main,
                    crate::hand::DEFAULT_SWING,
                );
            }
        } else if i == k.swap {
            self.net(Client::swap_offhand);
        } else if i == k.pick {
            let include_data = self.ctrl_held.iter().any(|&d| d);
            if !self.world.client.is_using_item() {
                match self.target {
                    Some(Target::Block(hit)) => {
                        self.net(|c| c.pick_block(hit.pos, include_data));
                    }
                    Some(Target::Entity { id, .. }) => {
                        self.net(|c| c.pick_entity(id, include_data));
                    }
                    None => {}
                }
            }
        } else if i == k.gui {
            self.hud_hidden = !self.hud_hidden;
        } else if i == k.debug {
            self.debug_on = !self.debug_on;
        } else if i == k.perspective {
            if !self.java_perspective {
                self.set_perspective(self.perspective.next());
            }
        } else if i == k.spectator_effect {
            self.spectator_effect.toggle();
        } else if i == k.spectator_hotbar {
            if self.is_spectator() && !self.world.client.is_using_item() {
                self.spectator_menu_at = self.millis();
                self.net(Client::spectator_hotbar_action);
            }
        } else if let Some(n) = name.strip_prefix("key.hotbar.")
            && let Ok(n) = n.parse::<u8>()
            && (1..=9).contains(&n)
            && self.is_spectator()
        {
            self.spectator_menu_at = self.millis();
            self.net(|c| c.spectator_hotbar_key(n - 1));
        } else if let Some(n) = name.strip_prefix("key.hotbar.")
            && let Ok(n) = n.parse::<u8>()
        {
            let (save, load) = (
                self.held.is_active(k.save_hotbar),
                self.held.is_active(k.load_hotbar),
            );
            if self.world.client.state.player.instabuild && self.stack.is_empty() && (save || load)
            {
                self.hotbar_load_or_save(usize::from(n - 1), load);
            } else {
                self.net(|c| c.select_hotbar(n - 1));
            }
        }
    }

    /// The camera-space effects on the world: Java's roll, hurt tilt, view bobbing and the warp.
    fn view_roll(&self, partial: f32) -> Mat4 {
        let warp = crate::camera_fx::warp_matrix(
            self.camera_fx
                .spinning_intensity(partial, self.options.screen_effect_scale as f32),
            self.camera_fx.spin.angle(partial),
        );
        Mat4::from_rotation_z(self.java_view_roll) * self.shake(partial) * warp
    }

    /// The camera mcr renders: a Java mod's while one set it, else the player's.
    pub fn camera(&self, partial: f32) -> Camera {
        let player = self.player_camera(partial);
        let Some(java) = self.java_camera else {
            return player;
        };
        let chunks = self.world.client.state.chunks.config();
        let min_y = f64::from(chunks.min_y());
        let reach = f64::from((self.render_distance().max(2) + 2) * 16);
        let java = java.bounded(
            lerp3(self.prev_eye, self.cur_eye, f64::from(partial)),
            reach,
            min_y - 64.0..=min_y + f64::from(chunks.height()) + 64.0,
        );
        Camera {
            position: DVec3::from(java.pos),
            yaw: java.yaw,
            pitch: java.pitch,
            roll: player.roll * Mat4::from_rotation_z(java.roll),
            ..player
        }
    }

    /// The camera vanilla derives from the player (eye, perspective, spectated entity).
    pub fn player_camera(&self, partial: f32) -> Camera {
        let t = f64::from(partial);
        let eye = lerp3(self.prev_eye, self.cur_eye, t);
        let fov_mod = self.prev_fov_mod + (self.fov_mod - self.prev_fov_mod) * partial;
        let fov = self.options.fov.0 as f32 * fov_mod * self.java_fov_scale;
        let (look_yaw, look_pitch) = self.spectate_view.unwrap_or((self.yaw, self.pitch));
        let bed = self
            .bed_view
            .filter(|_| self.perspective == Perspective::First && self.spectate_view.is_none());
        let (look_yaw, look_pitch) = bed.map_or((look_yaw, look_pitch), |yaw| (yaw, 0.0));
        let eye = if bed.is_some() {
            [eye[0], eye[1] + 0.15, eye[2]]
        } else {
            eye
        };
        let mut camera = Camera {
            position: DVec3::from(eye),
            yaw: std::f32::consts::PI - look_yaw.to_radians(),
            pitch: -look_pitch.to_radians(),
            fov_y: fov.to_radians(),
            near: 0.05,
            roll: self.view_roll(partial),
        };
        if self.perspective != Perspective::First {
            let forward = view_direction(look_yaw, look_pitch);
            let sign = if self.perspective == Perspective::ThirdBack {
                -1.0
            } else {
                1.0
            };
            let dir = forward.map(|v| v * sign);
            let distance = third_person_distance(
                &self.world.client.state.chunks,
                self.data,
                eye,
                dir,
                self.camera_distance(),
            );
            camera.position = DVec3::from(eye) + DVec3::from(dir) * distance;
            if self.perspective == Perspective::ThirdFront {
                camera.yaw += std::f32::consts::PI;
                camera.pitch = -camera.pitch;
            }
        }
        let fov = crate::camera_fx::death_or_fluid_fov(
            fov,
            self.death_ticks(partial),
            self.camera_in_fluid(&camera),
            self.options.fov_effect_scale as f32,
        );
        camera.fov_y = fov.clamp(1.0, 170.0).to_radians();
        camera
    }

    fn debug_charts(&self) -> Vec<mcr_ui::DebugChart> {
        let chart = |title: &str, unit, samples: &[f32], scale, warn, bad| mcr_ui::DebugChart {
            title: title.to_owned(),
            unit,
            samples: samples.to_vec(),
            scale,
            warn,
            bad,
        };
        vec![
            chart("Frame time", "ms", &self.frame_history, 50.0, 16.7, 33.4),
            chart("Tick time", "ms", &self.tick_history, 50.0, 25.0, 50.0),
            chart("Ping", "ms", &self.ping_history, 500.0, 150.0, 300.0),
        ]
    }

    pub fn frame(&mut self) -> Option<FrameStats> {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().max(1.0e-6);
        self.last_frame = now;
        self.fps += (1.0 / dt - self.fps) * 0.05;
        self.frame_ms += (dt * 1000.0 - self.frame_ms) * 0.05;
        push_sample(&mut self.frame_history, dt * 1000.0);

        self.world.camera = self.camera(self.partial());
        self.world.camera.fov_y = self.world.camera.fov_y.max(0.1);
        if let Err(e) = self.render.update(&mut self.world) {
            eprintln!("render update failed: {e:#}");
        }
        let camera = self.camera(self.partial());
        self.target = self.pick_from(lerp3(
            self.prev_eye,
            self.cur_eye,
            f64::from(self.partial()),
        ));
        let size = self.render.renderer.size();
        let boxes = self.outline_boxes();
        self.outline.build(&boxes, &camera, size);

        let partial = self.partial();
        let aspect = size.0 as f32 / size.1.max(1) as f32;
        let boss_fog = self
            .world
            .client
            .state
            .boss_bars
            .values()
            .any(|b| b.flags.create_world_fog);
        let forward = camera.forward();
        let env = self.vision.environment(&crate::vision::VisionFrame {
            state: &self.world.client.state,
            options: &self.options,
            camera: camera.position.to_array(),
            forward: forward.to_array(),
            partial,
            dt_ticks: dt / self.tick_seconds(),
            render_distance: self.world.render_distance(),
            lightning: self.effects.flash(),
            boss_darkening: self.boss_darkening.0
                + (self.boss_darkening.1 - self.boss_darkening.0) * partial,
            boss_fog,
        });
        self.sky_factor = env.sky_factor;
        self.sun_turn = (env.sun_angle / std::f32::consts::TAU).rem_euclid(1.0);
        self.moon_phase = env.moon_phase;
        self.prepare_visuals(&camera, partial, aspect, &env);
        let screen_fx = self.screen_effects(&camera, partial, aspect);

        self.update_gui_size();
        self.tick_toasts();
        self.hud.tick_chat(
            &mut self.gui.ui,
            self.ticks as u32,
            &self.options,
            self.started.elapsed().as_millis() as u64,
        );
        let hidden = self.hud_hidden || self.java_gui;
        let spectator_alpha = self.spectator_menu_alpha();
        let hotbar_keys = self.keys.hotbar.map(|i| self.bindings.input(i));
        let chat_open = self.chat_open();
        let (crosshair, hotbar_attack) = self.crosshair_hud();
        self.view_cache.refresh(&self.world.client.state);
        let debug = self.debug_on.then(|| {
            let mut overlay = debug::build(&DebugInput {
                state: &self.world.client.state,
                data: self.data,
                feet: self.feet(),
                yaw: self.yaw,
                pitch: self.pitch,
                target: self.target,
                fps: self.fps,
                frame_ms: self.frame_ms,
                pipeline: self.render.stats(),
                tick: self.ticks,
                adapter: &self.render.renderer.adapter_info().name,
                size,
                memory_mb: debug::resident_mb(),
                meshed_columns: self.world.loaded_count(),
            });
            overlay.charts = self.debug_charts();
            overlay
        });
        let input =
            HudInput {
                state: &self.world.client.state,
                options: &self.options,
                tick: self.ticks as u32,
                partial: self.partial(),
                hidden,
                sleep_overlay: !self.java_gui,
                tab_held: self.tab_held,
                chat_open,
                debug: debug.as_ref(),
                skins: &self.skins,
                camera: mcr_client::locator::Camera {
                    pos: camera.position.to_array(),
                    yaw: self.yaw,
                    pitch: self.pitch,
                    fov_y_degrees: camera.fov_y.to_degrees(),
                },
                eye_of: &|e| crate::entities::eye_position(self.data, e),
                show_heads: self.world.client.state.server.online_mode,
                millis: self.started.elapsed().as_millis() as u64,
                spectator: self.world.client.spectator_menu().map(|menu| {
                    crate::hud::SpectatorHud {
                        menu,
                        alpha: spectator_alpha,
                        keys: hotbar_keys,
                    }
                }),
                crosshair,
                hotbar_attack,
                jump_bar: self.world.client.jump_bar(),
                screen_shows_effects: self.stack.top_shows_active_effects(),
                screen_assets: &self.gui.assets,
                trim_materials: self.view_cache.trim_materials(),
            };
        {
            let view = StateView {
                state: &self.world.client.state,
                operator_items: self.options.operator_items_tab,
                advanced_tooltips: self.options.advanced_item_tooltips,
                keys: self.screen_keys(),
                cache: &self.view_cache,
            };
            let max_scale = u32::try_from(self.gui_scale()).unwrap_or(0);
            let mut frame = self.gui.ui.frame_scaled(size.0, size.1, max_scale);
            if let Some(layer) = self.entities.as_ref().filter(|_| !self.java_world) {
                let crosshair = match self.target {
                    Some(Target::Entity { id, .. }) => Some(id),
                    _ => None,
                };
                let tags = nametags::collect(
                    layer,
                    &self.world.client.state,
                    camera.position.to_array(),
                    partial,
                    crosshair,
                    self.hud_hidden,
                );
                nametags::draw(
                    &mut frame,
                    &tags,
                    &TagParams {
                        camera: &camera,
                        framebuffer: size,
                        opacity: nametags::background_opacity(
                            self.options.background_for_chat_only,
                            self.options.text_background_opacity,
                        ),
                        state: &self.world.client.state,
                    },
                );
            }
            if !self.java_gui {
                let scoping = self.perspective == Perspective::First
                    && self.hand.as_ref().is_some_and(HeldHand::scoping);
                let state = &self.world.client.state;
                let [x, _, z] = state.player.pos();
                let warning = self
                    .options
                    .vignette
                    .then(|| state.world.border.warning_strength(x, z));
                let overlay = crate::overlays::OverlayFrame {
                    state,
                    screen: screen_fx,
                    hidden: self.hud_hidden,
                    first_person: self.perspective == Perspective::First,
                    scoping,
                    border_warning: warning,
                    portal: self.camera_fx.portal.at(partial),
                    nausea: self.camera_fx.nausea.at(partial),
                    screen_effect_scale: self.options.screen_effect_scale as f32,
                    partial,
                    ticks: self.ticks,
                };
                self.overlays
                    .draw(&mut frame, &mut self.gui.icons, &self.assets, &overlay);
            }
            self.hud.draw(&mut frame, &mut self.gui.icons, &input);
            if !hidden && self.options.show_subtitles {
                let dir = view_direction(self.yaw, self.pitch);
                let yaw = f64::from(self.yaw).to_radians();
                let lines = self.subtitles.lines(
                    self.cur_eye,
                    dir,
                    [-yaw.cos(), 0.0, -yaw.sin()],
                    input.millis,
                    self.options.notification_display_time,
                );
                crate::subtitles::draw(&mut frame, &lines, &self.options);
            }
            if !self.stack.is_empty() {
                let mut cx =
                    RenderCx::new(&mut frame, &mut self.gui.icons, &self.gui.assets, &view);
                cx.mouse = self.mouse;
                cx.now_ms = input.millis;
                self.stack.render(&mut cx);
            }
            if !hidden && !self.gui.toasts.is_empty() {
                let mut cx =
                    RenderCx::new(&mut frame, &mut self.gui.icons, &self.gui.assets, &view);
                cx.now_ms = input.millis;
                self.gui.toasts.draw(&mut cx);
            }
            if !hidden {
                let toast = self.options.music_toast;
                let audible = music_volume(&self.options) > 0.0;
                let mut cx =
                    RenderCx::new(&mut frame, &mut self.gui.icons, &self.gui.assets, &view);
                cx.now_ms = input.millis;
                if self.stack.top_shows_music_toast() {
                    if toast != MusicToastState::Never && audible {
                        self.gui.toasts.draw_now_playing_static(&mut cx);
                    }
                } else if toast == MusicToastState::PauseAndToast {
                    self.gui.toasts.draw_now_playing(&mut cx);
                }
            }
        }

        let feet = self.feet();
        if let (Some((volume, pitch)), Some(audio)) = (self.hud.bubble_pop.take(), &mut self.audio)
        {
            audio.play(
                "minecraft:ui.hud.bubble_pop",
                Some(feet),
                volume,
                pitch,
                SoundSource::Players,
            );
        }
        self.render.renderer.set_environment(&env);
        self.render.renderer.set_animation_tick(self.ticks);
        crate::post_effects::requested(
            &self.world.client.state.post_effects,
            self.spectator_effect.chain(),
            &mut self.post_chains,
        );
        self.render.renderer.set_post_effects(&self.post_chains);
        self.render
            .renderer
            .set_menu_blur(self.options.menu_background_blurriness.max(0) as u32);
        let mut overlay = FrameOverlay {
            outline: &self.outline,
            ui_renderer: &mut self.gui.ui_renderer,
            ui: &mut self.gui.ui,
            entities: self.entities.as_ref().filter(|_| !self.java_world),
            block_entities: self.block_entities.as_ref().filter(|_| !self.java_world),
            hand: self.hand.as_ref().filter(|_| !self.java_world),
            preview: self.preview.as_ref(),
            particles: &mut self.effects.particles,
            env: &env,
            partial,
            glow: &mut self.glow,
        };
        if std::mem::take(&mut self.capture_icon) {
            self.render.renderer.request_capture();
        }
        let stats = self.render.draw_with(&camera, 1.0, Some(&mut overlay));
        if self.spectate_view.is_none() {
            let (yaw, pitch) = self.shown_look(&camera);
            self.turn_latency.shown(yaw, pitch, Instant::now());
        }
        if let Some(c) = self.render.renderer.take_capture() {
            self.world_icon =
                mcr_singleplayer::icon::thumbnail(c.width, c.height, &c.pixels, c.bgra);
        }
        stats
    }

    /// `LevelExtractor#extractPlayerState`'s in-block, water and fire overlays.
    fn screen_effects(
        &self,
        camera: &Camera,
        partial: f32,
        aspect: f32,
    ) -> crate::overlays::ScreenEffects {
        use mcr_protocol::types::entity_data::EntityDataValue;
        let state = &self.world.client.state;
        let player = &state.player;
        let mut fx = crate::overlays::ScreenEffects {
            hud_fov: self.hud_fov(camera, partial),
            yaw: self.yaw,
            pitch: self.pitch,
            ..Default::default()
        };
        if self.java_world || !player.loaded || player.is_sleeping() {
            return fx;
        }
        let spectator = player.game_mode == mcr_protocol::types::game::GameType::Spectator;
        let p = &player.physics;
        let feet = player.pos();
        let eye_y = feet[1] + f64::from(p.eye_height());
        if !spectator {
            let b = p.bounding_box();
            let near = crate::camera_fx::near_plane_box(
                [feet[0], eye_y, feet[2]],
                camera.forward(),
                camera.up(),
                camera.fov_y,
                aspect,
            );
            fx.block = crate::camera_fx::view_blocking_state(
                &state.chunks,
                feet,
                eye_y,
                b.max[0] - b.min[0],
                p.attributes.scale.clamp(0.0625, 16.0),
                &near,
            );
        }
        if self.perspective == Perspective::First && !spectator {
            if p.eye_in_water() {
                let [block, sky] = crate::entities::light_at(&state.chunks, feet, p.eye_height());
                let darken = ((1.0 - self.sky_factor.clamp(0.0, 1.0)) * 15.0).round() as u8;
                let raw = block.max(sky.saturating_sub(darken));
                fx.water = Some(crate::overlays::level_brightness(raw));
            }
            fx.fire = matches!(player.data.get(&0), Some(EntityDataValue::Byte(f)) if f & 1 != 0);
        }
        fx
    }

    fn prepare_visuals(&mut self, camera: &Camera, partial: f32, aspect: f32, env: &Environment) {
        let state = &self.world.client.state;
        let size = self.render.renderer.size();
        let item_world = self.needles.world(state, self.moon_phase);
        let body_yaw = self
            .entities
            .as_ref()
            .map_or(self.yaw, |l| l.local_body_yaw(partial));
        let owner = crate::needles::local_owner(state, body_yaw);
        crate::needles::apply_to_icons(self.gui.icons.context_mut(), &item_world, owner);
        if let Some(hand) = &mut self.hand {
            hand.set_item_world(item_world.clone(), owner);
        }
        if let Some(layer) = &mut self.entities {
            layer.scene.set_item_world(item_world);
        }
        if let Some(layer) = &mut self.entities {
            let cull = Cull {
                camera_pos: camera.position.to_array(),
                frustum: camera.frustum(aspect),
                distance_scale: self.options.entity_distance_scaling,
            };
            let swing = self
                .hand
                .as_ref()
                .map_or(0.0, |h| h.swing_progress(partial));
            layer.scene.set_camera(
                camera.position.to_array(),
                180.0 - camera.yaw.to_degrees(),
                -camera.pitch.to_degrees(),
            );
            layer.update(
                &FrameCtx {
                    state,
                    partial,
                    cull: &cull,
                    show_local: self.perspective != Perspective::First
                        && self.spectate_view.is_none(),
                    hide_entity: self
                        .world
                        .client
                        .state
                        .camera_entity
                        .filter(|_| self.perspective == Perspective::First),
                    eye: camera.position.to_array(),
                    ghost: self
                        .ghost_age
                        .map(|a| (a as f32 + partial) / GHOST_TICKS as f32),
                    local_swing: swing,
                    local_using: self.world.client.is_using_item(),
                    events: &self.entity_events,
                    skins: &self.skins,
                    spectator_outlines: state.player.game_mode
                        == mcr_protocol::types::game::GameType::Spectator
                        && self.held.is_active(self.keys.spectator_outlines),
                    text_background_opacity: if self.options.background_for_chat_only {
                        0.25
                    } else {
                        self.options.text_background_opacity as f32
                    },
                    local_use: self.world.client.using_item().map(|u| {
                        let held = self.world.client.ticks.saturating_sub(u.started_tick);
                        (
                            u.hand == mcr_protocol::types::game::Hand::Off,
                            held as f32 + partial,
                        )
                    }),
                    entity_shadows: self.options.entity_shadows,
                    sky_darken: ((1.0 - self.sky_factor.clamp(0.0, 1.0)) * 15.0).round() as u8,
                    ambient_light: self.vision.ambient_light(),
                    piglins_zombify: self.vision.piglins_zombify(),
                },
                &mut self.gui.ui,
            );
            if let Some(block_entities) = &mut self.block_entities {
                block_entities.scene.queue_text(layer.take_text_draws());
            }
            let params = layer.frame_params(camera, aspect, env);
            layer.scene.prepare(&params);
            if let Some(block_entities) = &mut self.block_entities {
                block_entities.update(
                    state,
                    params,
                    camera,
                    partial,
                    f64::from(self.world.render_distance()) * 16.0,
                    &mut self.gui.ui,
                );
            }
        }
        let hand_fov = self.hud_fov(camera, partial);
        let shake = self.shake(partial);
        let state = &self.world.client.state;
        if let Some(hand) = &mut self.hand {
            let eye = lerp3(self.prev_eye, self.cur_eye, f64::from(partial));
            let frame = HandFrame {
                state,
                options: &self.options,
                partial,
                aspect,
                fov: hand_fov,
                shake,
                pitch: self.pitch,
                yaw: self.yaw,
                lightmap: &env.lightmap(1.0),
                light: crate::entities::light_at(&state.chunks, eye, 0.0),
                invisible: false,
                skin: self.skins.get(state.uuid),
            };
            let visible = self.perspective == Perspective::First
                && !self.hud_hidden
                && !state.player.is_sleeping()
                && state.player.game_mode != mcr_protocol::types::game::GameType::Spectator;
            if visible {
                hand.prepare(&frame);
                hand.upload(&frame);
            } else {
                hand.hide();
            }
        }
        if self.stack.is_empty() {
            self.player_inventory = false;
        }
        let rect = self
            .stack
            .top()
            .and_then(|s| s.entity_rect())
            .filter(|_| self.player_inventory);
        let extra = self.stack.top().and_then(|s| s.screen_preview());
        let (_, scale) = gui::gui_size(size, self.gui_scale());
        match (&mut self.preview, &mut self.entities, rect, extra) {
            (Some(preview), Some(layer), Some(rect), _) => {
                let view = layer.local_view(state);
                preview.prepare(view, &state.registries, rect, scale, self.mouse, size);
            }
            (Some(preview), _, _, Some(ScreenPreview::ArmorStand { rect })) => {
                let stand = mcr_data::v26_3()
                    .entity_type_by_name("minecraft:armor_stand")
                    .map_or(-1, |id| id as i32);
                let result = state
                    .inventory
                    .open
                    .as_ref()
                    .and_then(|c| c.slots.get(3))
                    .filter(|s| !s.is_empty())
                    .map(|s| (s, crate::preview::stand_slot(s)));
                preview.prepare_armor_stand(stand, result, &state.registries, rect, scale, size);
            }
            (Some(preview), _, _, Some(ScreenPreview::Book { rect, open, flip })) => {
                let lerp = |v: [f32; 2]| v[0] + (v[1] - v[0]) * partial;
                preview.prepare_book(rect, scale, size, lerp(open), lerp(flip));
            }
            (Some(preview), ..) => preview.hide(),
            _ => {}
        }
        self.render
            .renderer
            .set_effect_time(state.world.game_time.max(0) as u64, partial);
        self.render
            .renderer
            .set_cloud_settings(effects::cloud_settings(&self.options));
        self.render.renderer.set_weather_hidden(self.java_weather);
        if env.rain_level > 0.0 && !self.java_weather {
            let radius = self.options.weather_radius.clamp(3, 10) as u32;
            let source = self.effects.weather(state);
            self.render.renderer.update_weather(camera, radius, &source);
        }
        let own = self
            .world
            .client
            .digging()
            .filter(|d| d.progress > 0.0)
            .map(|d| (d.pos, ((d.progress * 10.0) as u8).min(9)));
        let decals = self.effects.decals(state, own);
        self.render.renderer.set_destroy_decals(&decals);
        let distance = f64::from(self.world.render_distance().max(2) * 16);
        let millis = self.started.elapsed().as_millis();
        let border =
            effects::border_visual(state, camera.position.to_array(), distance, partial, millis);
        self.render.renderer.set_world_border(border, distance);
        self.effects.particles.set_camera(camera.position);
    }

    fn outline_boxes(&self) -> Vec<WorldBox> {
        let Some(Target::Block(hit)) = self.target else {
            return Vec::new();
        };
        let base = DVec3::new(
            f64::from(hit.pos.x),
            f64::from(hit.pos.y),
            f64::from(hit.pos.z),
        );
        self.data
            .outline_shape(hit.state)
            .iter()
            .map(|b| WorldBox {
                min: base + DVec3::from(b.min.map(f64::from)),
                max: base + DVec3::from(b.max.map(f64::from)),
            })
            .collect()
    }
}

/// Where an entity-bound sound plays; `None` once the entity is gone or silent, which stops it.
fn sound_entity_pos(state: &mcr_client::ClientState, id: i32) -> Option<[f64; 3]> {
    if id == state.player.entity_id {
        return Some(state.player.pos());
    }
    let e = state.entities.get(id)?;
    (!crate::effects::is_silent(e)).then_some(e.pos)
}

fn center(pos: BlockPos) -> [f64; 3] {
    [
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_moves_one_slot_and_resets_on_reversal() {
        let mut acc = 0.0;
        assert_eq!(scroll_wheel(&mut acc, 5.0), Some(1));
        assert_eq!(acc, 0.0);
        assert_eq!(scroll_wheel(&mut acc, 0.6), None);
        assert_eq!(scroll_wheel(&mut acc, -0.5), None, "reversal drops the 0.6");
        assert_eq!(scroll_wheel(&mut acc, -0.6), Some(-1));
        assert!(acc.abs() < 0.2);
    }

    fn mount_state(entity: &str, columns: usize) -> mcr_client::ClientState {
        let mut s =
            mcr_client::ClientState::new(&mcr_client::ClientConfig::default(), uuid::Uuid::nil());
        let entity_type = s.data.entity_type_by_name(entity).unwrap() as i32;
        s.inventory
            .open_container(4, -1, mcr_protocol::types::text::Component::text(""));
        s.inventory.open.as_mut().unwrap().mount = Some(mcr_client::MountInfo {
            columns,
            entity_type,
        });
        s
    }

    #[test]
    fn mount_open_event_builds_the_mount_menu() {
        let title = mcr_protocol::types::text::Component::text("");
        let (kind, _) =
            container_menu(&mount_state("minecraft:llama", 3), -1, title.clone()).unwrap();
        assert_eq!(
            kind,
            MenuKind::Mount {
                chest_columns: 3,
                saddle: false,
                armor: true,
                llama: true
            }
        );
        let (kind, _) =
            container_menu(&mount_state("minecraft:nautilus", 0), -1, title.clone()).unwrap();
        assert_eq!(
            kind,
            MenuKind::Nautilus {
                saddle: true,
                armor: true
            }
        );
        assert!(container_menu(&mount_state("minecraft:cow", 0), -1, title).is_none());
    }

    #[test]
    fn sensitivity_matches_vanilla_default() {
        let k = degrees_per_pixel(0.5);
        assert!((k - 0.15).abs() < 1e-4);
        assert!(degrees_per_pixel(1.0) > degrees_per_pixel(0.0));
    }

    #[test]
    fn perspectives_cycle_like_vanilla_and_map_from_java() {
        let mut p = Perspective::First;
        let mut seen = Vec::new();
        for _ in 0..3 {
            p = p.next();
            seen.push(p);
        }
        assert_eq!(
            seen,
            [
                Perspective::ThirdBack,
                Perspective::ThirdFront,
                Perspective::First
            ]
        );
        assert_eq!(
            Perspective::from_java(mcr_modbridge::Perspective::ThirdFront),
            Perspective::ThirdFront
        );
        assert_eq!(
            Perspective::from_java(mcr_modbridge::Perspective::First),
            Perspective::First
        );
    }

    #[test]
    fn view_direction_matches_vanilla_axes() {
        let south = view_direction(0.0, 0.0);
        assert!((south[2] - 1.0).abs() < 1e-9);
        let west = view_direction(90.0, 0.0);
        assert!((west[0] + 1.0).abs() < 1e-9);
        let down = view_direction(0.0, 90.0);
        assert!((down[1] + 1.0).abs() < 1e-9);
    }
}
