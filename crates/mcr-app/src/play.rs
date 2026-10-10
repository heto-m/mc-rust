use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use mcr_buf::BlockPos;
use mcr_client::{
    ChatKind, Client, ClientConfig, ClientError, ClientEvent, PlayerInput, sanitize_for_log,
};
use mcr_physics::Pose;
use mcr_protocol::play::serverbound::Facing;
use mcr_protocol::types::game::Hand;
use mcr_render::glam::DVec3;
use mcr_render::{Camera, Environment, FrameStats, Overlay, OverlayCtx, Renderer, wgpu};
use mcr_settings::bindings::mapping_index;
use mcr_settings::{HeldState, InputKey, KeyBindings, MAPPINGS, Options, OptionsFile};
use mcr_ui::{ItemIcons, Ui, UiRenderer, vanilla_resources};

use crate::assets::Assets;
use crate::audio::{Audio, BlockSound, wire_category};
use crate::autopilot::Autopilot;
use crate::debug::{self, DebugInput};
use crate::hud::{Hud, HudInput};
use crate::input::{KEY_BACKSPACE, KEY_ENTER, KEY_ESCAPE, KEY_LEFT_CTRL, LineEditor};
use crate::mesh_world::MeshWorld;
use crate::net_world::NetworkWorld;
use crate::outline::{OutlineRenderer, WorldBox};
use crate::render_system::{PipelineStats, RenderSystem};
use crate::target::{BlockHit, Target, pick, raycast_block, reach};

pub const TICK_SECONDS: f32 = 0.05;
const MAX_TICKS_PER_FRAME: u32 = 10;
const THIRD_PERSON_DISTANCE: f64 = 4.0;
const CLICK_DELAY: u32 = 5;
const USE_DELAY: u32 = 4;
const HIT_SOUND_INTERVAL: u32 = 4;
const STEP_STRIDE: f32 = 0.6;

#[derive(Debug, Clone)]
pub struct PlayConfig {
    pub addr: String,
    pub name: String,
    pub game_dir: PathBuf,
    pub jar: PathBuf,
    pub assets_dir: PathBuf,
    pub render_distance: Option<i32>,
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
    chat: usize,
    command: usize,
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
            chat: i("key.chat"),
            command: i("key.command"),
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
}

impl Overlay for FrameOverlay<'_> {
    fn record(&mut self, ctx: &mut OverlayCtx<'_>) {
        self.outline.record(ctx);
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
    ui: Ui,
    hud: Hud,
    ui_renderer: UiRenderer,
    outline: OutlineRenderer,
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
    chat: Option<LineEditor>,
    pub paused: bool,
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
    pub shot: Option<PathBuf>,
    autopilot: Option<Autopilot>,
    started: Instant,
    last_frame: Instant,
    fps: f32,
    frame_ms: f32,
    pub teleports: u32,
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

fn asset_index_id(jar: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(jar.parent()?.join("version.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value["assetIndex"]["id"].as_str().map(str::to_owned)
}

impl Play {
    pub fn new(renderer: Renderer, cfg: &PlayConfig) -> Result<Self> {
        let data = mcr_data::v26_3();
        let file = OptionsFile::load(&cfg.game_dir.join("options.txt")).unwrap_or_else(|e| {
            eprintln!("options.txt unreadable: {e}");
            OptionsFile::default()
        });
        let options = file.options.clone();
        let render_distance = cfg
            .render_distance
            .unwrap_or(options.render_distance)
            .clamp(2, 64);

        let assets = Arc::new(Assets::load(&cfg.jar)?);
        let index = asset_index_id(&cfg.jar);
        let resources = vanilla_resources(
            &cfg.jar,
            index.as_deref().map(|i| (cfg.assets_dir.as_path(), i)),
        )
        .context("opening resources")?;
        let ui = Ui::load(&resources).context("loading ui")?;
        let icons = ItemIcons::load(&resources).context("loading item icons")?;
        let hud = Hud::new(&ui, icons, data);

        let mut config = ClientConfig::default().with_addr(&cfg.addr);
        config.name = cfg.name.clone();
        config.view_distance = render_distance.clamp(2, 32) as i8;
        let client = Client::connect(config).context("connecting")?;
        println!("joined {} as {}", cfg.addr, cfg.name);

        let world = NetworkWorld::new(client, render_distance);
        let format = renderer.target_format();
        let ui_renderer = UiRenderer::new(renderer.device(), format);
        let outline = OutlineRenderer::new(renderer.device(), format);
        let render = RenderSystem::new(renderer, assets, &world);
        let audio = Audio::start(
            &cfg.assets_dir,
            index.as_deref().unwrap_or("34"),
            &options,
            data,
        );
        if audio.is_none() {
            println!("audio: disabled");
        }
        let now = Instant::now();
        Ok(Self {
            world,
            render,
            ui,
            hud,
            ui_renderer,
            outline,
            options,
            bindings: file.bindings,
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
            chat: None,
            paused: false,
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
            shot: None,
            autopilot: None,
            started: now,
            last_frame: now,
            fps: 0.0,
            frame_ms: 0.0,
            teleports: 0,
        })
    }

    pub fn set_autopilot(&mut self, autopilot: Autopilot) {
        self.autopilot = Some(autopilot);
    }

    pub fn closed(&self) -> Option<&str> {
        self.closed.as_deref()
    }

    pub fn wants_grab(&self) -> bool {
        self.closed.is_none() && !self.paused && self.chat.is_none()
    }

    pub fn chat_open(&self) -> bool {
        self.chat.is_some()
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

    pub fn advance(&mut self, dt: f32) {
        for _ in 0..self.due_ticks(dt) {
            self.tick();
        }
    }

    pub fn partial(&self) -> f32 {
        (self.accum / TICK_SECONDS).clamp(0.0, 1.0)
    }

    fn busy(&self) -> bool {
        self.chat.is_some() || self.paused
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

    pub fn script_finished(&self) -> bool {
        self.autopilot.as_ref().is_none_or(|a| a.finished)
    }

    pub fn tick_without_script(&mut self) {
        let pilot = self.autopilot.take();
        self.tick();
        self.autopilot = pilot;
    }

    pub fn tick(&mut self) {
        if self.closed.is_some() {
            return;
        }
        if let Some(mut pilot) = self.autopilot.take() {
            pilot.step(self);
            self.autopilot = Some(pilot);
        }
        self.ticks += 1;
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
        if self.world.client.state.player.loaded && self.closed.is_none() {
            self.target = self.pick_from(self.cur_eye);
            self.gameplay();
            self.step_sounds();
        }
        self.hud.tick();
        if let Some(audio) = &mut self.audio {
            audio.set_listener(self.cur_eye, self.yaw);
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
        let target = if f.is_finite() {
            1.0 + (f - 1.0) * scale
        } else {
            1.0
        };
        self.fov_mod += (target - self.fov_mod) * 0.5;
    }

    fn on_event(&mut self, event: ClientEvent) {
        match event {
            ClientEvent::Chat { kind, message, .. } => {
                println!("chat: {}", sanitize_for_log(&message.to_plain_string()));
                if kind == ChatKind::ActionBar {
                    self.hud.show_actionbar();
                } else {
                    self.hud
                        .add_chat(&mut self.ui, &message, self.ticks as u32, &self.options);
                }
            }
            ClientEvent::Title { .. } => self.hud.show_title(&self.world.client.state),
            ClientEvent::TitlesCleared => self.hud.clear_titles(),
            ClientEvent::Disconnected(reason) => {
                if self.closed.is_none() {
                    self.closed = Some(reason.to_log_string());
                }
            }
            ClientEvent::Died { .. } => println!("died"),
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
            ClientEvent::LevelEvent {
                event: 2001,
                pos,
                data,
                ..
            } => {
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
        if !active {
            self.abort_dig();
            return;
        }
        match self.target {
            Some(Target::Entity { id, .. }) => {
                self.abort_dig();
                if click {
                    self.net(|c| c.attack(id));
                    self.swing();
                }
            }
            Some(Target::Block(hit)) => self.mine(hit, click),
            None => self.abort_dig(),
        }
    }

    fn mine(&mut self, hit: BlockHit, click: bool) {
        let creative = self.world.client.state.player.instabuild;
        if creative {
            if click || self.attack_delay == 0 {
                self.net(|c| c.break_instantly(hit.pos, hit.face));
                self.swing();
                self.play_block(hit.state, BlockSound::Break, center(hit.pos));
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
                self.play_block(hit.state, BlockSound::Break, center(hit.pos));
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
        if !active {
            if std::mem::take(&mut self.using_item) {
                self.net(Client::release_use_item);
            }
            return;
        }
        if !click && self.use_delay > 0 {
            return;
        }
        match self.target {
            Some(Target::Entity { id, .. }) => {
                if click {
                    self.net(|c| c.interact(id, Hand::Main));
                    self.swing();
                }
            }
            Some(Target::Block(hit)) => {
                let predicted = self.predict_place(hit);
                self.net(|c| c.use_item_on(Hand::Main, hit.pos, hit.face, hit.cursor, hit.inside));
                self.swing();
                if let Some(state) = predicted {
                    let at = neighbour(hit.pos, hit.face);
                    self.play_block(state, BlockSound::Place, center(at));
                }
                self.use_delay = USE_DELAY;
            }
            None => {
                self.net(|c| c.use_item(Hand::Main));
                self.using_item = true;
                self.use_delay = USE_DELAY;
            }
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
        pick(
            &state.chunks,
            &state.entities,
            self.data,
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
        if self.busy() || !lines.is_finite() {
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

    pub fn resume(&mut self) {
        self.paused = false;
    }

    pub fn key_event(&mut self, key: InputKey, pressed: bool, text: Option<&str>) {
        if self.closed.is_some() {
            return;
        }
        if self.chat.is_some() {
            self.chat_key(key, pressed, text);
            return;
        }
        if key == InputKey::Keyboard(KEY_ESCAPE) {
            if pressed {
                self.paused = !self.paused;
                if self.paused {
                    self.release_all();
                }
            }
            return;
        }
        if self.paused {
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

    fn chat_key(&mut self, key: InputKey, pressed: bool, text: Option<&str>) {
        if !pressed {
            return;
        }
        match key {
            InputKey::Keyboard(KEY_ESCAPE) => self.chat = None,
            InputKey::Keyboard(KEY_ENTER) => {
                let line = self
                    .chat
                    .take()
                    .map(|e| e.text().to_owned())
                    .unwrap_or_default();
                self.submit_chat(&line);
            }
            InputKey::Keyboard(KEY_BACKSPACE) => {
                if let Some(e) = &mut self.chat {
                    e.backspace();
                }
            }
            _ => {
                if let (Some(e), Some(t)) = (&mut self.chat, text) {
                    e.insert(t);
                }
            }
        }
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
            self.chat = Some(LineEditor::default());
        } else if i == k.command {
            self.chat = Some(LineEditor::with_text("/"));
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
            self.net(|c| c.select_hotbar(n - 1));
        }
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

    fn environment(&self, render_distance: i32) -> Environment {
        let state = &self.world.client.state;
        let mut env = match state.dimension.name.as_str() {
            "minecraft:the_nether" => Environment::nether(),
            "minecraft:overworld" => {
                let w = &state.world;
                Environment::overworld(
                    w.day_time().unwrap_or(6000).max(0) as u64,
                    w.rain_level,
                    w.thunder_level,
                )
            }
            _ => Environment::default(),
        };
        env.set_render_distance_chunks(render_distance.max(2) as u32);
        env
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
            chat_input: self.chat.as_ref().map(LineEditor::text),
            debug: debug.as_ref(),
            millis: self.started.elapsed().as_millis() as u64,
            framebuffer: size,
        };
        self.hud.draw(&mut self.ui, &input);

        let env = self.environment(self.world.render_distance());
        self.render.renderer.set_environment(&env);
        self.render.renderer.set_animation_tick(self.ticks);
        let mut overlay = FrameOverlay {
            outline: &self.outline,
            ui_renderer: &mut self.ui_renderer,
            ui: &mut self.ui,
        };
        self.render.draw_with(&camera, 1.0, Some(&mut overlay))
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
