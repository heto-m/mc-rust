use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use mcr_buf::BlockPos;
use mcr_client::{
    ChatKind, Client, ClientError, ClientEvent, PlayerInput, UseResult, UseTarget, sanitize_for_log,
};
use mcr_particles::ParticleEngine;
use mcr_physics::Pose;
use mcr_protocol::play::serverbound::Facing;
use mcr_protocol::types::ItemStack;
use mcr_render::glam::{DVec3, Mat4};
use mcr_render::{Camera, Environment, FrameStats, Overlay, OverlayCtx, Renderer, wgpu};
use mcr_settings::bindings::mapping_index;
use mcr_settings::{HeldState, InputKey, KeyBindings, MAPPINGS, Options};
use mcr_ui::{
    ChatScreen, ContainerScreen, CreativeScreen, DeathScreen, Env, HotbarStore, MenuKind,
    Modifiers, MouseButton, PauseScreen, RenderCx, Screen, ScreenAction, ScreenStack, Ui, UiInput,
    UiRenderer,
};

use crate::assets::Assets;
use crate::audio::{Audio, BlockSound, wire_category};
use crate::block_entities::BlockEntities;
use crate::debug::{self, DebugInput};
use crate::effects::{self, WorldEffects};
use crate::entities::{Cull, EntityLayer, FrameCtx, LocalSample};
use crate::gui::{self, Gui};
use crate::hand::{Hand as HeldHand, HandFrame, HandTick};
use crate::hud::{Hud, HudInput};
use crate::input::{KEY_ESCAPE, KEY_LEFT_CTRL, track_modifiers, ui_inputs};
use crate::mesh_world::MeshWorld;
use crate::nametags::{self, TagParams};
use crate::net_world::NetworkWorld;
use crate::outline::{OutlineRenderer, WorldBox};
use crate::overlays::Overlays;
use crate::preview::Preview;
use crate::render_system::{PipelineStats, RenderSystem};
use crate::settings::{OptionsScreen, SharedSettings};
use crate::target::{BlockHit, EntityPicker, Target, pick, raycast_block, reach};
use crate::view::{StateView, SuggestEngine};

pub const TICK_SECONDS: f32 = 0.05;
const MAX_TICKS_PER_FRAME: u32 = 10;
const THIRD_PERSON_DISTANCE: f64 = 4.0;
const CLICK_DELAY: u32 = 5;
const USE_DELAY: u32 = 4;
const HIT_SOUND_INTERVAL: u32 = 4;
const STEP_STRIDE: f32 = 0.6;

#[derive(Debug, Clone)]
pub struct PlayConfig {
    pub render_distance: Option<i32>,
    pub game_dir: std::path::PathBuf,
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
    playerlist: usize,
    pick: usize,
    gui: usize,
    perspective: usize,
    swap: usize,
    debug: usize,
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
            playerlist: i("key.playerlist"),
            pick: i("key.pickItem"),
            gui: i("key.toggleGui"),
            perspective: i("key.togglePerspective"),
            swap: i("key.swapOffhand"),
            debug: i("key.debug.overlay"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Dig {
    pos: BlockPos,
    progress: u32,
    total: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Perspective {
    First,
    ThirdBack,
    ThirdFront,
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
            self.ui_renderer.draw(&mut pass);
        }
        if let Some(preview) = self.preview {
            preview.record(ctx);
        }
    }
}

fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// Vanilla's cubic mouse sensitivity curve, in degrees per pixel.
pub fn degrees_per_pixel(sensitivity: f64) -> f32 {
    let s = sensitivity * 0.6 + 0.2;
    (s * s * s * 8.0 * 0.15) as f32
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
    water_ticks: u32,
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
    hotbar_path: std::path::PathBuf,
    death_shown: bool,
    death_message: Option<mcr_protocol::types::text::Component>,
    pub leave: bool,
    pub quit: bool,
    pack_answers: Vec<bool>,
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
    debug_on: bool,
    tab_held: bool,
    ctrl_held: bool,
    closed: Option<String>,
    attack_click: bool,
    use_click: bool,
    attack_delay: u32,
    use_delay: u32,
    dig: Option<Dig>,
    using_item: bool,
    target: Option<Target>,
    step_dist: f32,
    last_feet: [f64; 3],
    placed_seen: bool,
    started: Instant,
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
        let hud = Hud::new(&gui.ui, data);
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
            water_ticks: 0,
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
            hotbar_path,
            death_shown: false,
            death_message: None,
            leave: false,
            quit: false,
            pack_answers: Vec::new(),
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
            debug_on: false,
            tab_held: false,
            ctrl_held: false,
            closed: None,
            attack_click: false,
            use_click: false,
            attack_delay: 0,
            use_delay: 0,
            dig: None,
            using_item: false,
            target: None,
            step_dist: 0.0,
            last_feet: [0.0; 3],
            placed_seen: false,
            started: now,
            last_frame: now,
            fps: 0.0,
            frame_ms: 0.0,
            teleports: 0,
            container_updates: 0,
        }
    }

    /// Ends the session and returns what the shell lent out.
    pub fn finish(self) -> Parts {
        let (renderer, assets) = self.render.into_parts(&self.world);
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

    pub fn ready(&self) -> bool {
        let p = &self.world.client.state.player;
        p.loaded && self.render.settled(&self.world)
    }

    pub fn feet(&self) -> [f64; 3] {
        self.world.client.state.player.pos()
    }

    pub fn pipeline_stats(&self) -> PipelineStats {
        self.render.stats()
    }

    fn fail(&mut self, e: &ClientError) {
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

    pub fn due_ticks(&mut self, dt: f32) -> u32 {
        self.accum += dt.clamp(0.0, 0.25);
        let mut n = 0;
        while self.accum >= TICK_SECONDS && n < MAX_TICKS_PER_FRAME {
            self.accum -= TICK_SECONDS;
            n += 1;
        }
        if n == MAX_TICKS_PER_FRAME {
            self.accum = 0.0;
        }
        n
    }

    pub fn partial(&self) -> f32 {
        (self.accum / TICK_SECONDS).clamp(0.0, 1.0)
    }

    fn busy(&self) -> bool {
        !self.stack.is_empty()
    }

    fn player_input(&self) -> PlayerInput {
        let k = &self.keys;
        let on = |i: usize| !self.busy() && self.held.is_active(i);
        PlayerInput {
            forward: on(k.forward),
            backward: on(k.back),
            left: on(k.left),
            right: on(k.right),
            jump: on(k.jump),
            sneak: on(k.sneak),
            sprint: on(k.sprint),
            yaw: self.yaw,
            pitch: self.pitch,
        }
    }

    pub fn tick(&mut self) {
        if self.closed.is_some() {
            return;
        }
        self.ticks += 1;
        self.sync_settings();
        let input = self.player_input();
        self.prev_eye = self.cur_eye;
        self.prev_fov_mod = self.fov_mod;
        if let Err(e) = self.world.tick(&input) {
            self.fail(&e);
        }
        self.update_eye();
        self.update_fov();
        for event in self.world.take_events() {
            self.on_event(event);
        }
        self.tick_visuals();
        if self.world.client.state.player.loaded && self.closed.is_none() {
            self.target = self.pick_from(self.cur_eye);
            self.gameplay();
            self.step_sounds();
        }
        self.hud.tick();
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
        };
        if let Some(entities) = &mut self.entities {
            entities.tick(state, sample);
        }
        if let Some(block_entities) = &mut self.block_entities {
            block_entities.tick(state, self.ticks);
        }
        let scoping = self.scoping();
        self.overlays.tick(scoping);
        self.water_ticks = if self.eye_in_water(self.cur_eye) {
            (self.water_ticks + 1).min(600)
        } else {
            0
        };
        if let Some(hand) = &mut self.hand {
            let server_using = matches!(
                state.player.data.get(&8),
                Some(mcr_protocol::types::entity_data::EntityDataValue::Byte(f)) if f & 1 != 0
            );
            hand.tick(&HandTick {
                state,
                yaw: self.yaw,
                pitch: self.pitch,
                local_using: self.using_item,
                server_using,
            });
        }
        self.effects
            .tick(state, DVec3::from(self.cur_eye), &self.options);
        if let (Some(dig), Some(Target::Block(hit))) = (self.dig, self.target)
            && dig.pos == hit.pos
        {
            let face = match hit.face {
                Facing::Down => 0,
                Facing::Up => 1,
                Facing::North => 2,
                Facing::South => 3,
                Facing::West => 4,
                Facing::East => 5,
            };
            self.effects
                .block_crack(&self.world.client.state, hit.pos, face, hit.state as i32);
        }
    }

    fn update_eye(&mut self) {
        let player = &self.world.client.state.player;
        let target = player.physics.pose.eye_height();
        self.eye_height += (target - self.eye_height) * 0.5;
        let [x, y, z] = player.pos();
        self.cur_eye = [x, y + f64::from(self.eye_height), z];
        let snapped = player.placed && !self.placed_seen;
        if snapped {
            self.placed_seen = true;
        }
        if snapped || !player.placed {
            self.prev_eye = self.cur_eye;
        }
    }

    fn eye_in_water(&self, eye: [f64; 3]) -> bool {
        effects::eye_in_water(&self.world.client.state.chunks, eye)
    }

    fn scoping(&self) -> bool {
        self.perspective == Perspective::First && self.hand.as_ref().is_some_and(HeldHand::scoping)
    }

    fn update_fov(&mut self) {
        let player = &self.world.client.state.player;
        let p = &player.physics;
        let mut f = 1.0f32;
        if p.abilities.flying {
            f *= 1.1;
        }
        let speed = p.attributes.movement_speed as f32 * if p.sprinting { 1.3 } else { 1.0 };
        f *= (speed / player.walking_speed.max(1.0e-4) + 1.0) / 2.0;
        let scale = self.options.fov_effect_scale as f32;
        let target = if self.scoping() {
            0.1
        } else if f.is_finite() {
            1.0 + (f - 1.0) * scale
        } else {
            1.0
        };
        self.fov_mod += (target - self.fov_mod) * 0.5;
    }

    fn sync_map(&mut self, id: i32) {
        let Some(map) = self.world.client.state.gameplay.maps.get(&id) else {
            return;
        };
        let (image, deco) = (crate::map::image(&map.colors), crate::map::decorations(map));
        if let Some(entities) = &mut self.entities {
            entities.scene.set_map(id, image.clone(), deco.clone());
        }
        if let Some(hand) = &mut self.hand {
            hand.set_map(id, image, deco);
        }
    }

    fn on_event(&mut self, event: ClientEvent) {
        let local_id = self.world.client.state.player.entity_id;
        if let Some(entities) = &mut self.entities {
            entities.on_event(&event, local_id);
        }
        if let Some(block_entities) = &mut self.block_entities {
            block_entities.on_event(&event, &self.world.client.state);
        }
        match event {
            ClientEvent::EntityEvent {
                entity_id,
                event: 35,
            } => {
                if entity_id == local_id {
                    self.overlays.start_totem();
                }
                self.effects.totem(entity_id);
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
                self.effects.on_entity_added(&self.world.client.state, id);
            }
            ClientEvent::Particle {
                particle,
                pos,
                count,
                delta,
                speed,
                always_show,
            } => self
                .effects
                .emit(&particle, pos, delta, speed, count, always_show),
            ClientEvent::BlockDestruction {
                entity_id,
                pos,
                progress,
            } => self.effects.destruction(entity_id, pos, progress),
            ClientEvent::Explosion {
                center, particle, ..
            } => self.effects.explosion(center, &particle),
            ClientEvent::MapUpdated(id) => self.sync_map(id),
            ClientEvent::Respawned => {
                self.effects.clear();
                if let Some(entities) = &mut self.entities {
                    entities.clear();
                }
            }
            ClientEvent::Chat { kind, message, .. } => {
                let text: String = self.gui.ui.flatten(&message).iter().map(|c| c.ch).collect();
                println!("chat: {}", sanitize_for_log(&text));
                if kind == ChatKind::ActionBar {
                    self.hud.show_actionbar();
                } else {
                    self.hud
                        .add_chat(&mut self.gui.ui, &message, self.ticks as u32, &self.options);
                }
            }
            ClientEvent::Title { .. } => self.hud.show_title(&self.world.client.state),
            ClientEvent::TitlesCleared => self.hud.clear_titles(),
            ClientEvent::Toast(toast) => self.add_toast(toast),
            ClientEvent::Disconnected(reason) => {
                if self.closed.is_none() {
                    self.closed = Some(reason.to_log_string());
                }
            }
            ClientEvent::Died { message } => {
                println!("died");
                self.death_message = message;
            }
            ClientEvent::ContainerOpened {
                menu_type, title, ..
            } => match MenuKind::from_registry_id(menu_type) {
                Some(kind) => {
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
            } => {
                let name = match &sound {
                    mcr_buf::IdOr::Id(id) => self
                        .data
                        .registry("minecraft:sound_event")
                        .and_then(|r| u32::try_from(*id).ok().and_then(|i| r.name_of(i)))
                        .map(str::to_owned),
                    mcr_buf::IdOr::Inline(ev) => Some(ev.location.as_str().to_owned()),
                };
                let at = pos.or_else(|| {
                    let e = self.world.client.state.entities.get(entity_id?)?;
                    Some(e.pos)
                });
                if let (Some(name), Some(audio)) = (name, &mut self.audio) {
                    audio.play(&name, at, volume, pitch, wire_category(source));
                }
            }
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
                event: 2001,
                pos,
                data,
                ..
            } => {
                self.effects
                    .block_break(&self.world.client.state, pos, data);
                if let (Ok(state), Some(audio)) = (u32::try_from(data), &mut self.audio) {
                    audio.play_block(state, BlockSound::Break, center(pos));
                }
            }
            _ => {}
        }
    }

    fn held_block_state(&self) -> Option<u32> {
        let player = &self.world.client.state.player;
        let stack = self.world.client.state.inventory.hotbar(player.held_slot);
        if stack.is_empty() {
            return None;
        }
        let item = u32::try_from(stack.item).ok()?;
        let block = self.data.item_info(item)?.block?;
        Some(self.data.block_info(block)?.default_state)
    }

    fn swing(&mut self) {
        if let Some(hand) = &mut self.hand {
            hand.swing();
        }
        self.net(Client::swing);
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
        if self.busy() {
            self.attack_click = false;
            self.use_click = false;
            self.abort_dig();
            return;
        }
        self.attack_delay = self.attack_delay.saturating_sub(1);
        self.use_delay = self.use_delay.saturating_sub(1);
        self.do_attack();
        self.do_use();
    }

    fn abort_dig(&mut self) {
        if self.dig.take().is_some() {
            self.net(Client::abort_digging);
        }
    }

    fn do_attack(&mut self) {
        let click = std::mem::take(&mut self.attack_click);
        let active = self.held.is_active(self.keys.attack);
        if !active && !click {
            self.abort_dig();
            return;
        }
        self.attack_target(click);
        if !active {
            self.abort_dig();
        }
    }

    fn attack_target(&mut self, click: bool) {
        match self.target {
            Some(Target::Entity { id, .. }) => {
                self.abort_dig();
                if click {
                    self.net(|c| c.attack(id));
                    self.swing();
                    if let Some(hand) = &mut self.hand {
                        hand.reset_attack_strength();
                    }
                }
            }
            Some(Target::Block(hit)) => self.mine(hit, click),
            None => {
                self.abort_dig();
                if click {
                    self.swing();
                    if let Some(hand) = &mut self.hand {
                        hand.reset_attack_strength();
                    }
                }
            }
        }
    }

    fn mine(&mut self, hit: BlockHit, click: bool) {
        let creative = self.world.client.state.player.instabuild;
        if creative {
            if click || self.attack_delay == 0 {
                self.net(|c| c.break_instantly(hit.pos, hit.face));
                self.swing();
                self.attack_delay = CLICK_DELAY;
            }
            return;
        }
        if let Some(mut dig) = self.dig.filter(|d| d.pos == hit.pos) {
            dig.progress += 1;
            self.swing();
            if dig.progress % HIT_SOUND_INTERVAL == 0 {
                self.play_block(hit.state, BlockSound::Hit, center(hit.pos));
            }
            if dig.progress >= dig.total {
                self.net(Client::finish_digging);
                self.dig = None;
                self.attack_delay = CLICK_DELAY;
            } else {
                self.dig = Some(dig);
            }
            return;
        }
        self.abort_dig();
        if self.attack_delay > 0 {
            return;
        }
        let Some(total) = self.world.client.estimate_dig_ticks(hit.pos) else {
            return;
        };
        self.net(|c| c.start_digging(hit.pos, hit.face));
        self.swing();
        self.dig = Some(Dig {
            pos: hit.pos,
            progress: 0,
            total: total.max(1),
        });
    }

    fn do_use(&mut self) {
        let click = std::mem::take(&mut self.use_click);
        let active = self.held.is_active(self.keys.use_item);
        if click || (active && self.use_delay == 0) {
            self.use_target(click);
        }
        if !active && std::mem::take(&mut self.using_item) {
            self.net(Client::release_use_item);
        }
    }

    fn use_target(&mut self, click: bool) {
        let target = match self.target {
            Some(Target::Entity { id, .. }) if click => UseTarget::Entity {
                id,
                location: [0.0; 3],
            },
            Some(Target::Entity { .. }) => return,
            Some(Target::Block(hit)) => UseTarget::Block {
                pos: hit.pos,
                face: hit.face,
                cursor: hit.cursor,
                inside: hit.inside,
            },
            None => UseTarget::Air,
        };
        let predicted = match self.target {
            Some(Target::Block(hit)) => self.predict_place(hit),
            _ => None,
        };
        let Some(result) = self.net(|c| c.start_use(target)) else {
            return;
        };
        if result == UseResult::Success {
            if let Some(hand) = &mut self.hand {
                hand.swing();
            }
            self.net(Client::swing);
        }
        if self.world.client.using_item().is_some() {
            self.using_item = true;
        }
        if let (Some(state), Some(Target::Block(hit))) = (predicted, self.target)
            && result == UseResult::Success
        {
            let at = neighbour(hit.pos, hit.face);
            self.play_block(state, BlockSound::Place, center(at));
        }
        if !matches!(target, UseTarget::Entity { .. }) {
            self.use_delay = USE_DELAY;
        }
    }

    fn predict_place(&self, hit: BlockHit) -> Option<u32> {
        let state = self.held_block_state()?;
        let sneaking = self.world.client.state.player.physics.crouching;
        let target_interactive = self
            .data
            .block_state(hit.state)
            .is_some_and(|s| s.has_block_entity());
        if target_interactive && !sneaking {
            return None;
        }
        let at = neighbour(hit.pos, hit.face);
        let current = self.world.client.state.chunks.block(at.x, at.y, at.z)?;
        let replaceable = self
            .data
            .block_state(u32::from(current.0))
            .is_some_and(|s| s.is_air() || s.is_replaceable());
        replaceable.then_some(state)
    }

    fn play_block(&mut self, state: u32, kind: BlockSound, at: [f64; 3]) {
        if let Some(audio) = &mut self.audio {
            audio.play_block(state, kind, at);
        }
    }

    fn step_sounds(&mut self) {
        let player = &self.world.client.state.player;
        let p = &player.physics;
        let feet = p.pos;
        let moved = ((feet[0] - self.last_feet[0]).powi(2) + (feet[2] - self.last_feet[2]).powi(2))
            .sqrt() as f32;
        self.last_feet = feet;
        if !p.on_ground || p.crouching || p.abilities.flying || moved > 2.0 {
            return;
        }
        self.step_dist += moved * STEP_STRIDE;
        if self.step_dist < 1.0 {
            return;
        }
        self.step_dist = 0.0;
        let below = [
            feet[0].floor() as i32,
            (feet[1] - 0.2).floor() as i32,
            feet[2].floor() as i32,
        ];
        let state = self
            .world
            .client
            .state
            .chunks
            .block(below[0], below[1], below[2])
            .map(|s| u32::from(s.0));
        if let Some(state) = state {
            self.play_block(state, BlockSound::Step, feet);
        }
    }

    fn pick_from(&self, eye: [f64; 3]) -> Option<Target> {
        let state = &self.world.client.state;
        let dir = view_direction(self.yaw, self.pitch);
        let picker = EntityPicker {
            entities: &state.entities,
            players: &state.players,
            data: self.data,
            own_id: state.player.entity_id,
            own_vehicle: state.player.vehicle,
        };
        pick(
            &state.chunks,
            &picker,
            eye,
            dir,
            reach(state.player.instabuild),
        )
    }

    pub fn release_all(&mut self) {
        for i in 0..MAPPINGS.len() {
            self.held.update(i, false, &self.options);
        }
        self.tab_held = false;
    }

    pub fn mouse_motion(&mut self, dx: f64, dy: f64) {
        if self.busy() || self.closed.is_some() {
            return;
        }
        let k = degrees_per_pixel(self.options.mouse_sensitivity);
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

    pub fn look(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw = (self.yaw + dyaw).rem_euclid(360.0);
        self.pitch = (self.pitch + dpitch).clamp(-90.0, 90.0);
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
            });
            return;
        }
        self.scroll += lines * self.options.mouse_wheel_sensitivity as f32;
        let steps = self.scroll.trunc();
        self.scroll -= steps;
        if steps != 0.0 {
            let held = i32::from(self.world.client.state.player.held_slot);
            let slot = (held - steps.signum() as i32 * steps.abs().min(9.0) as i32).rem_euclid(9);
            self.net(|c| c.select_hotbar(slot as u8));
        }
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
            if pressed {
                self.open_pause();
            }
            return;
        }
        if key == InputKey::Keyboard(KEY_LEFT_CTRL) {
            self.ctrl_held = pressed;
        }
        let indices: Vec<usize> = self.bindings.lookup(key).to_vec();
        for i in indices {
            self.binding(i, pressed);
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
            let view = StateView {
                state: &self.world.client.state,
                operator_items: self.options.operator_items_tab,
            };
            let mut env = Env {
                view: &view,
                clipboard: &mut self.gui.clipboard,
                now_ms: self.started.elapsed().as_millis() as u64,
            };
            self.stack.input(&mut env, &input, &mut actions);
        }
        self.handle_actions(actions);
    }

    pub fn cursor_moved(&mut self, px: f64, py: f64) {
        let (_, scale) = gui::gui_size(self.render.renderer.size(), self.options.gui_scale);
        let scale = f64::from(scale);
        let (x, y) = ((px / scale) as f32, (py / scale) as f32);
        self.mouse = (x, y);
        if self.busy() {
            self.ui_input(UiInput::MouseMove { x, y });
        }
    }

    fn update_gui_size(&mut self) {
        let (size, _) = gui::gui_size(self.render.renderer.size(), self.options.gui_scale);
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

    pub fn take_pack_answers(&mut self) -> Vec<bool> {
        std::mem::take(&mut self.pack_answers)
    }

    pub fn force_close(&mut self, reason: String) {
        self.closed.get_or_insert(reason);
    }

    /// Swaps in assets rebuilt from a new resource stack without restarting the session.
    pub fn reload_resources(&mut self, assets: Arc<Assets>) -> anyhow::Result<()> {
        self.gui.reload(&assets.resources)?;
        self.hud.rebind(&self.gui.ui, self.data);
        self.overlays = Overlays::new();
        if let Some(audio) = &mut self.audio {
            audio.reload(&assets.resources);
        }
        self.effects = WorldEffects::new(assets.clone());
        self.effects.set_options(&self.options);
        let renderer = &mut self.render.renderer;
        renderer.set_effect_assets(&effects::effect_assets(&assets));
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
        for id in maps {
            self.sync_map(id);
        }
        self.render.reload_assets(assets.clone(), &self.world);
        self.assets = assets;
        Ok(())
    }

    fn push_screen(&mut self, screen: Box<dyn Screen>) {
        self.release_all();
        self.update_gui_size();
        self.stack.push(screen);
    }

    pub fn open_pause(&mut self) {
        if !self.busy() && self.closed.is_none() {
            self.push_screen(Box::new(PauseScreen::new(false, false)));
        }
    }

    pub fn lose_focus(&mut self) {
        self.release_all();
        if self.options.pause_on_lost_focus {
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

    pub fn open_inventory(&mut self) {
        if self.busy() {
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
        self.gui.sync_language(&self.options.language);
    }

    fn languages(&self) -> Vec<mcr_ui::LanguageInfo> {
        self.gui.ui.lang().languages().to_vec()
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
        self.death_check();
        if self.stack.is_empty() {
            return;
        }
        let mut actions = Vec::new();
        {
            let view = StateView {
                state: &self.world.client.state,
                operator_items: self.options.operator_items_tab,
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

    fn death_check(&mut self) {
        let player = &self.world.client.state.player;
        let (dead, show) = (player.dead, player.show_death_screen);
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
                ScreenAction::SetBeacon { primary, secondary } => {
                    let effects = self.data.registry("minecraft:mob_effect");
                    let id = |n: Option<String>| {
                        n.and_then(|n| effects.as_ref()?.id_of(&n))
                            .and_then(|i| i32::try_from(i).ok())
                    };
                    let (p, q) = (id(primary), id(secondary));
                    self.net(|c| c.set_beacon(p, q));
                }
                ScreenAction::Respawn => {
                    self.net(Client::respawn);
                    self.death_shown = false;
                    self.stack.clear();
                }
                ScreenAction::Disconnect => self.leave = true,
                ScreenAction::OpenOptions => {
                    let screen = OptionsScreen::new(self.settings.clone(), self.languages());
                    self.update_gui_size();
                    self.stack.push(Box::new(screen));
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
                ScreenAction::Quit => self.quit = true,
                ScreenAction::ResourcePackAnswer(accepted) => self.pack_answers.push(accepted),
                _ => {}
            }
        }
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
        } else if i == k.drop {
            let whole = self.ctrl_held;
            self.net(|c| c.drop_item(whole));
        } else if i == k.swap {
            self.net(Client::swap_offhand);
        } else if i == k.pick {
            if let Some(Target::Block(hit)) = self.target {
                self.net(|c| c.pick_block(hit.pos, false));
            }
        } else if i == k.gui {
            self.hud_hidden = !self.hud_hidden;
        } else if i == k.debug {
            self.debug_on = !self.debug_on;
        } else if i == k.perspective {
            self.perspective = match self.perspective {
                Perspective::First => Perspective::ThirdBack,
                Perspective::ThirdBack => Perspective::ThirdFront,
                Perspective::ThirdFront => Perspective::First,
            };
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

    fn view_roll(&self, partial: f32) -> Mat4 {
        let player = &self.world.client.state.player;
        let mut roll = Mat4::IDENTITY;
        if player.dead {
            let death = self.entities.as_ref().map_or(0, |e| e.local_death_time()) as f32;
            let duration = (death + partial).min(20.0);
            roll *= Mat4::from_rotation_z((40.0 - 8000.0 / (duration + 200.0)).to_radians());
        }
        let hurt = player.hurt_time as f32 - partial;
        if hurt >= 0.0 && player.hurt_duration > 0 {
            let t = hurt / player.hurt_duration as f32;
            let s = (t * t * t * t * std::f32::consts::PI).sin();
            let tilt = -s * 14.0 * self.options.damage_tilt_strength as f32;
            let dir = player.hurt_dir.to_radians();
            roll *= Mat4::from_rotation_y(-dir)
                * Mat4::from_rotation_z(tilt.to_radians())
                * Mat4::from_rotation_y(dir);
        }
        roll
    }

    pub fn camera(&self, partial: f32) -> Camera {
        let t = f64::from(partial);
        let eye = lerp3(self.prev_eye, self.cur_eye, t);
        let fov_mod = self.prev_fov_mod + (self.fov_mod - self.prev_fov_mod) * partial;
        let fov = (self.options.fov.0 as f32 * fov_mod).clamp(1.0, 170.0);
        let mut camera = Camera {
            position: DVec3::from(eye),
            yaw: std::f32::consts::PI - self.yaw.to_radians(),
            pitch: -self.pitch.to_radians(),
            fov_y: fov.to_radians(),
            near: 0.05,
            roll: self.view_roll(partial),
        };
        if self.perspective != Perspective::First {
            let forward = view_direction(self.yaw, self.pitch);
            let sign = if self.perspective == Perspective::ThirdBack {
                -1.0
            } else {
                1.0
            };
            let dir = forward.map(|v| v * sign);
            let distance = raycast_block(
                &self.world.client.state.chunks,
                self.data,
                eye,
                dir,
                THIRD_PERSON_DISTANCE,
            )
            .map_or(THIRD_PERSON_DISTANCE, |h| (h.distance - 0.2).max(0.0));
            camera.position = DVec3::from(eye) + DVec3::from(dir) * distance;
            if self.perspective == Perspective::ThirdFront {
                camera.yaw += std::f32::consts::PI;
                camera.pitch = -camera.pitch;
            }
        }
        camera
    }

    pub fn frame(&mut self) -> Option<FrameStats> {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().max(1.0e-6);
        self.last_frame = now;
        self.fps += (1.0 / dt - self.fps) * 0.05;
        self.frame_ms += (dt * 1000.0 - self.frame_ms) * 0.05;

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
        let mut env = effects::environment(
            &self.world.client.state,
            self.world.render_distance(),
            self.effects.flash(),
            self.options.gamma as f32,
        );
        if self.water_ticks > 0 || self.eye_in_water(camera.position.to_array()) {
            let sky = crate::entities::light_at(
                &self.world.client.state.chunks,
                camera.position.to_array(),
                0.0,
            )[1];
            effects::apply_underwater(&mut env, self.water_ticks, f32::from(sky));
        }
        self.prepare_visuals(&camera, partial, aspect, &env);

        self.update_gui_size();
        self.tick_toasts();
        let chat_open = self.chat_open();
        let debug = self.debug_on.then(|| {
            debug::build(&DebugInput {
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
            })
        });
        let input = HudInput {
            state: &self.world.client.state,
            options: &self.options,
            tick: self.ticks as u32,
            partial: self.partial(),
            hidden: self.hud_hidden,
            tab_held: self.tab_held,
            chat_open,
            debug: debug.as_ref(),
            millis: self.started.elapsed().as_millis() as u64,
        };
        {
            let view = StateView {
                state: &self.world.client.state,
                operator_items: self.options.operator_items_tab,
            };
            let max_scale = u32::try_from(self.options.gui_scale).unwrap_or(0);
            let mut frame = self.gui.ui.frame_scaled(size.0, size.1, max_scale);
            if let Some(layer) = &self.entities {
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
                );
                nametags::draw(
                    &mut frame,
                    &tags,
                    &TagParams {
                        camera: &camera,
                        framebuffer: size,
                        opacity: self.options.text_background_opacity,
                        state: &self.world.client.state,
                    },
                );
            }
            self.hud.draw(&mut frame, &mut self.gui.icons, &input);
            if !self.hud_hidden {
                let scoping = self.perspective == Perspective::First
                    && self.hand.as_ref().is_some_and(HeldHand::scoping);
                self.overlays.draw(
                    &mut frame,
                    &mut self.gui.icons,
                    &self.assets,
                    scoping,
                    partial,
                );
            }
            if !self.stack.is_empty() {
                let mut cx =
                    RenderCx::new(&mut frame, &mut self.gui.icons, &self.gui.assets, &view);
                cx.mouse = self.mouse;
                cx.now_ms = input.millis;
                self.stack.render(&mut cx);
            }
            if !self.hud_hidden && !self.gui.toasts.is_empty() {
                let mut cx =
                    RenderCx::new(&mut frame, &mut self.gui.icons, &self.gui.assets, &view);
                cx.now_ms = input.millis;
                self.gui.toasts.draw(&mut cx);
            }
        }

        self.render.renderer.set_environment(&env);
        self.render.renderer.set_animation_tick(self.ticks);
        let mut overlay = FrameOverlay {
            outline: &self.outline,
            ui_renderer: &mut self.gui.ui_renderer,
            ui: &mut self.gui.ui,
            entities: self.entities.as_ref(),
            block_entities: self.block_entities.as_ref(),
            hand: self.hand.as_ref(),
            preview: self.preview.as_ref(),
            particles: &mut self.effects.particles,
            env: &env,
            partial,
        };
        self.render.draw_with(&camera, 1.0, Some(&mut overlay))
    }

    fn prepare_visuals(&mut self, camera: &Camera, partial: f32, aspect: f32, env: &Environment) {
        let state = &self.world.client.state;
        let size = self.render.renderer.size();
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
            layer.update(&FrameCtx {
                state,
                partial,
                cull: &cull,
                show_local: self.perspective != Perspective::First,
                local_swing: swing,
                local_using: self.using_item,
            });
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
        if let Some(hand) = &mut self.hand {
            let eye = lerp3(self.prev_eye, self.cur_eye, f64::from(partial));
            let frame = HandFrame {
                state,
                options: &self.options,
                partial,
                aspect,
                pitch: self.pitch,
                yaw: self.yaw,
                lightmap: &env.lightmap(1.0),
                light: crate::entities::light_at(&state.chunks, eye, 0.0),
                invisible: false,
            };
            let visible = self.perspective == Perspective::First
                && !self.hud_hidden
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
        let (_, scale) = gui::gui_size(size, self.options.gui_scale);
        match (&mut self.preview, &mut self.entities, rect) {
            (Some(preview), Some(layer), Some(rect)) => {
                let view = layer.local_view(state);
                preview.prepare(view, &state.registries, rect, scale, self.mouse, size);
            }
            (Some(preview), _, _) => preview.hide(),
            _ => {}
        }
        self.render
            .renderer
            .set_effect_time(state.world.game_time.max(0) as u64, partial);
        self.render
            .renderer
            .set_cloud_settings(effects::cloud_settings(&self.options));
        if env.rain_level > 0.0 {
            let radius = self.options.weather_radius.clamp(3, 10) as u32;
            let source = self.effects.weather(state);
            self.render.renderer.update_weather(camera, radius, &source);
        }
        let own = self
            .dig
            .map(|d| (d.pos, (d.progress * 10 / d.total.max(1)).min(9) as u8));
        let decals = self.effects.decals(state, own);
        self.render.renderer.set_destroy_decals(&decals);
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

fn center(pos: BlockPos) -> [f64; 3] {
    [
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    ]
}

fn neighbour(pos: BlockPos, face: Facing) -> BlockPos {
    let (dx, dy, dz) = match face {
        Facing::Down => (0, -1, 0),
        Facing::Up => (0, 1, 0),
        Facing::North => (0, 0, -1),
        Facing::South => (0, 0, 1),
        Facing::West => (-1, 0, 0),
        Facing::East => (1, 0, 0),
    };
    BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitivity_matches_vanilla_default() {
        let k = degrees_per_pixel(0.5);
        assert!((k - 0.15).abs() < 1e-4);
        assert!(degrees_per_pixel(1.0) > degrees_per_pixel(0.0));
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
