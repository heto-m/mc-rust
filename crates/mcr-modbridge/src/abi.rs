use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use mcr_jvm::{Doorbell, EntryDesc, ExtensionTable, Flavour, Ring, RingError};

use crate::events::BridgeEvent;
use crate::frame::FrameShared;
use crate::input::{self, InputShared};
use crate::net::{NetShared, SPILL_LIMIT};
use crate::sections::SectionShared;
use crate::sound::{self, SoundShared};

/// Bump on any change to [`ENTRIES`] or the message layouts; Java refuses a mismatched table.
pub const BRIDGE_VERSION: u32 = 17;
pub const TABLE_PROPERTY: &str = "mcr.bridge.table";
/// Set when Rust composes Java's frames: Java's main target and slots follow the lockstep.
pub const COMPOSE_PROPERTY: &str = "mcr.compose";

pub const SHARED_CONTROL_RING: i32 = 0;
pub const SHARED_CONTROL_BELL: i32 = 1;
pub const SHARED_NET_IN_RING: i32 = 2;
pub const SHARED_NET_OUT_RING: i32 = 3;
pub const SHARED_SOUND_RING: i32 = 4;
pub const SHARED_SOUND_STATES: i32 = 5;
pub const SHARED_SOUND_CONTROL: i32 = 6;
pub const SHARED_FRAME_STATE: i32 = 16;
pub const SHARED_FRAME_BELL: i32 = 17;
pub const SHARED_INPUT_STATE: i32 = 18;
/// mcr's reached-section bitset for Java's culling.
pub const SHARED_SECTIONS: i32 = 19;

pub const MSG_FRAME: u16 = 1;
pub const MSG_SHUTDOWN: u16 = 2;
pub const MSG_INPUT: u16 = 3;
/// `name\tkey` lines: Rust's vanilla key bindings, which Java's mappings follow.
pub const MSG_KEYS: u16 = 4;
/// One absolute pack file path per line, in server push order: the server packs Rust applied.
pub const MSG_PACKS: u16 = 5;
/// `[u32 kind][utf8 text]`: the local world session's progress for Java's world screens.
pub const MSG_WORLD: u16 = 6;
pub const WORLD_STATUS: u32 = 1;
pub const WORLD_EULA: u32 = 2;
pub const WORLD_READY: u32 = 3;
pub const WORLD_FAILED: u32 = 4;
pub const WORLD_STOPPED: u32 = 5;
/// Java caps the lines of `MSG_KEYS`, `MSG_PACKS` and `REPORT_KEYS` at this many.
pub const SYNC_MAX_LINES: usize = 256;

/// One `MSG_INPUT`: a fixed head (kind, four ints, four doubles), then UTF-8 for text kinds.
pub const IN_TYPE: usize = 0;
pub const IN_A: usize = 4;
pub const IN_B: usize = 8;
pub const IN_C: usize = 12;
pub const IN_D: usize = 16;
pub const IN_X: usize = 24;
pub const IN_Y: usize = 32;
pub const IN_DX: usize = 40;
pub const IN_DY: usize = 48;
pub const IN_TEXT_DATA: usize = 56;
pub const IN_KEY: u32 = 1;
pub const IN_TEXT: u32 = 2;
pub const IN_PREEDIT: u32 = 3;
pub const IN_MOTION: u32 = 4;
pub const IN_BUTTON: u32 = 5;
pub const IN_WHEEL: u32 = 6;
pub const IN_DROP: u32 = 7;

/// The input state block: Rust writes keys, modifiers and its flags; Java writes the rest.
pub const ISTATE_KEYS: usize = 0;
pub const ISTATE_MODS: usize = 64;
pub const ISTATE_RUST_FLAGS: usize = 68;
pub const ISTATE_JAVA_FLAGS: usize = 72;
pub const ISTATE_GUI_SCALE: usize = 76;
pub const ISTATE_CURSOR: usize = 80;
pub const ISTATE_NATIVE_SEQ: usize = 84;
pub const ISTATE_NATIVE_KIND: usize = 88;
/// Movement keys a mod let through while a Java screen is open, as `MOVE_*` bits.
pub const ISTATE_SCREEN_MOVES: usize = 92;
pub const ISTATE_TEXT_AREA: usize = 96;
pub const ISTATE_SIZE: usize = 128;
pub const RUST_FOCUSED: u32 = 1;
pub const JAVA_SCREEN: u32 = 1;
pub const JAVA_OVERLAY: u32 = 2;
pub const JAVA_GRABBED: u32 = 4;
pub const JAVA_TEXT_INPUT: u32 = 8;
pub const JAVA_QUIT: u32 = 16;
/// Vanilla's `Gui.isPausing()`: the screen or overlay would pause a singleplayer game.
pub const JAVA_PAUSING: u32 = 32;
pub const MOVE_FORWARD: u32 = 1;
pub const MOVE_BACKWARD: u32 = 2;
pub const MOVE_LEFT: u32 = 4;
pub const MOVE_RIGHT: u32 = 8;
pub const MOVE_JUMP: u32 = 16;
pub const MOVE_SNEAK: u32 = 32;
pub const MOVE_SPRINT: u32 = 64;

pub const FRAME_INDEX: usize = 0;
pub const FRAME_TICKS: usize = 8;
pub const FRAME_WIDTH: usize = 12;
pub const FRAME_HEIGHT: usize = 16;
pub const FRAME_PARTIAL_TICK: usize = 20;
pub const FRAME_SCALE: usize = 24;
pub const FRAME_FLAGS: usize = 28;
pub const FRAME_CAMERA: usize = 32;
pub const FRAME_YAW: usize = 56;
pub const FRAME_PITCH: usize = 60;
pub const FRAME_VIEW: usize = 64;
pub const FRAME_PROJ: usize = 128;
pub const FRAME_RENDER_DISTANCE: usize = 192;
pub const FRAME_ENTITY_SCALE: usize = 196;
/// The camera mcr renders this frame when it is not the player camera (`FRAME_CAMERA_HANDOFF`).
pub const FRAME_RENDER_CAMERA: usize = 200;
pub const FRAME_RENDER_YAW: usize = 224;
pub const FRAME_RENDER_PITCH: usize = 228;
/// How far mcr's third-person player camera sits from the eye (an `f32`); 0 in first person.
pub const FRAME_CAMERA_DISTANCE: usize = 232;
pub const FRAME_SIZE: usize = 236;
/// Java signals every frame it finishes and tags its GPU work with slots.
pub const FRAME_LOCKSTEP: u32 = 1;
/// mcr's player camera is in third person this frame, `FRAME_CAMERA_DISTANCE` from the eye.
pub const FRAME_DETACHED: u32 = 2;
/// mcr renders from the camera Java reported last frame: `view`, `proj` and the render camera
/// fields are that camera's, while the camera fields stay the player's.
pub const FRAME_CAMERA_HANDOFF: u32 = 4;
/// The shared section block holds the sections mcr's graph reached from this frame's camera.
pub const FRAME_SECTIONS: u32 = 8;

/// Java's side of the lockstep, written by Java and read by the render thread.
pub const FSTATE_DONE: usize = 0;
pub const FSTATE_TICKS: usize = 8;
pub const FSTATE_NANOS: usize = 16;
pub const FSTATE_FLAGS: usize = 24;
pub const FSTATE_WIDTH: usize = 28;
pub const FSTATE_HEIGHT: usize = 32;
/// Java's field of view over vanilla's for the same state (an `f32`): what FOV mods changed.
pub const FSTATE_FOV_SCALE: usize = 36;
/// `CAMERA_*` flags; the camera fields after it count only with `CAMERA_MOD`.
pub const FSTATE_CAMERA: usize = 40;
pub const FSTATE_CAMERA_POS: usize = 48;
/// Radians in mcr's convention, like `FrameBegin`'s yaw and pitch.
pub const FSTATE_CAMERA_YAW: usize = 72;
pub const FSTATE_CAMERA_PITCH: usize = 76;
/// `PROF_COUNT` microsecond counters (`u32`) of Java's last frame, indexed by `PROF_*`.
pub const FSTATE_PROF: usize = 80;
/// Radians about the view axis, applied in view space after yaw and pitch.
pub const FSTATE_CAMERA_ROLL: usize = 112;
/// Java's camera type while it leads the perspective: a `PERSPECTIVE_*` value.
pub const FSTATE_PERSPECTIVE: usize = 116;
/// Radians about the view axis that mods added to the level's bob pose (an `f32`), in mcr's view space.
pub const FSTATE_VIEW_ROLL: usize = 120;
pub const FSTATE_SIZE: usize = 128;
/// Java does not lead the perspective (outside the lockstep or without a level).
pub const PERSPECTIVE_NONE: u32 = 0;
pub const PERSPECTIVE_FIRST: u32 = 1;
pub const PERSPECTIVE_THIRD_BACK: u32 = 2;
pub const PERSPECTIVE_THIRD_FRONT: u32 = 3;
/// Leaving the frame wait to returning from it: packets, input and size.
pub const PROF_PRE: usize = 0;
/// Client ticks.
pub const PROF_TICK: usize = 1;
/// `GameRenderer.extract`: camera, level (entities) and GUI extraction, where HUD mods run.
pub const PROF_EXTRACT: usize = 2;
/// `GameRenderer.render`, which contains `PROF_WORLD` and `PROF_GUI`.
pub const PROF_RENDER: usize = 3;
pub const PROF_WORLD: usize = 4;
/// `GuiRenderer.render`.
pub const PROF_GUI: usize = 5;
/// `Minecraft.renderFrame`, which contains `PROF_EXTRACT` and `PROF_RENDER`.
pub const PROF_FRAME: usize = 6;
pub const PROF_COUNT: usize = 8;
pub const FSTATE_READY: u32 = 1;
pub const FSTATE_LOADED: u32 = 2;
/// A mod cancelled or redirected vanilla's weather extraction this frame.
pub const FSTATE_WEATHER: u32 = 4;
/// A mod took over the local player's footsteps, so mcr must not play its own.
pub const FSTATE_STEPS: u32 = 8;
/// A mod consumes the mouse's turning (Freelook), so mcr must not turn its player.
pub const FSTATE_TURN: u32 = 16;
/// A mod moved Java's camera or made another entity the camera entity.
pub const CAMERA_MOD: u32 = 1;
/// The camera shows the local player: third person, or a camera entity other than the player.
pub const CAMERA_DETACHED: u32 = 2;
/// The mod took the player's movement input.
pub const CAMERA_INPUT: u32 = 4;

/// The shared section block: a sequence number, the box in section coordinates, then one bit per
/// section at `x + z * dx + y * dx * dz`.
pub const SEC_SEQ: usize = 0;
pub const SEC_ORIGIN: usize = 8;
pub const SEC_DIMS: usize = 20;
pub const SEC_BITS: usize = 64;
pub const SEC_MAX_BITS: usize = 1 << 23;

pub const SLOT_IMMEDIATE: u32 = 0;
pub const SLOT_GUI: u32 = 1;
/// World slots draw into mcr's scene color and reverse-Z depth between its own passes.
pub const SLOT_WORLD_SOLID: u32 = 2;
pub const SLOT_WORLD_TRANSLUCENT: u32 = 3;
pub const SLOT_WORLD_AFTER: u32 = 4;
/// Hand, outline blit and post effects: scene color with a private depth.
pub const SLOT_POST_WORLD: u32 = 5;

pub const REPORT_MOD_LOADED: i32 = 1;
pub const REPORT_INIT: i32 = 2;
pub const REPORT_SCREEN: i32 = 3;
pub const REPORT_CRASH: i32 = 4;
pub const REPORT_INFO: i32 = 5;
pub const REPORT_CONNECT: i32 = 6;
pub const REPORT_STOPPING: i32 = 7;
/// `name\tkey` lines: vanilla mappings Java rebound since it last applied or reported them.
pub const REPORT_KEYS: i32 = 8;
/// Java's world screens: `play\t<folder>`, `create` with `key=value` lines, `eula\t0|1`, `cancel`.
pub const REPORT_WORLD: i32 = 9;

pub const PHASE_LOGIN: u8 = 1;
pub const PHASE_CONFIGURATION: u8 = 2;
pub const PHASE_PLAY: u8 = 3;

/// Every net message starts with `[u8 tag][u8 0][u16 0][u32 session]`.
pub const NET_HEADER: usize = 8;
pub const NET_TAG: usize = 0;
pub const NET_SESSION: usize = 4;

pub const NET_FRAME: u16 = 1;
pub const NET_OPEN: u16 = 2;
pub const NET_CLOSE: u16 = 3;
pub const NET_PLAYER: u16 = 4;
pub const NET_PROBE: u16 = 5;
pub const NET_ACTION: u16 = 6;

pub const OUT_FRAME: u16 = 1;
pub const OUT_CHAT: u16 = 2;
pub const OUT_DISCONNECT: u16 = 3;
pub const OUT_PROBE: u16 = 4;
/// `OUT_DISCONNECT` tags: Java left on purpose, or its connection failed on its own (any tag but
/// `DISCONNECT_LEFT` detaches Java).
pub const DISCONNECT_LEFT: u8 = 0;
#[cfg(test)]
pub const DISCONNECT_FAILED: u8 = 1;

pub const CHAT_MESSAGE: u8 = 0;
pub const CHAT_COMMAND: u8 = 1;

pub const OPEN_PORT: usize = 8;
pub const OPEN_HOST: usize = 12;

pub const PLAYER_TICK: usize = 8;
pub const PLAYER_POS: usize = 16;
pub const PLAYER_OLD_POS: usize = 40;
pub const PLAYER_VELOCITY: usize = 64;
pub const PLAYER_YAW: usize = 88;
pub const PLAYER_PITCH: usize = 92;
pub const PLAYER_FLAGS: usize = 96;
pub const PLAYER_POSE: usize = 100;
pub const PLAYER_SLOT: usize = 104;
pub const PLAYER_VEHICLE: usize = 108;
/// `INPUT_*` bits of the movement keys Rust's player moved with this tick.
pub const PLAYER_INPUT: usize = 112;
pub const PLAYER_SIZE: usize = 120;

pub const PLAYER_ON_GROUND: u32 = 1;
pub const PLAYER_SHIFT: u32 = 2;
pub const PLAYER_SPRINT: u32 = 4;
pub const PLAYER_SWIM: u32 = 8;
pub const PLAYER_FALL_FLYING: u32 = 16;

pub const INPUT_FORWARD: u32 = 1;
pub const INPUT_BACKWARD: u32 = 2;
pub const INPUT_LEFT: u32 = 4;
pub const INPUT_RIGHT: u32 = 8;
pub const INPUT_JUMP: u32 = 16;
pub const INPUT_SHIFT: u32 = 32;
pub const INPUT_SPRINT: u32 = 64;

/// `NET_ACTION`: something Rust's player did that the server never echoes back to it.
pub const ACTION_KIND: usize = 8;
pub const ACTION_ARG: usize = 12;
pub const ACTION_POS: usize = 16;
pub const ACTION_VALUE: usize = 28;
pub const ACTION_SIZE: usize = 32;
/// Arm swing; `ACTION_ARG` holds `SWING_*` bits.
pub const ACTION_SWING: u32 = 1;
/// The held stack lost one item, or all of it when `ACTION_ARG` is 1.
pub const ACTION_DROP: u32 = 2;
/// Mining at `ACTION_POS`; `ACTION_VALUE` is 0 at the start, the progress, 1 when broken, -1 when aborted.
pub const ACTION_DESTROY: u32 = 3;
pub const ACTION_RESET_ATTACK: u32 = 4;
pub const SWING_OFF_HAND: u32 = 1;
pub const SWING_ATTACK: u32 = 2;

pub const PROBE_REQUEST: usize = 8;
pub const PROBE_CENTER_X: usize = 12;
pub const PROBE_CENTER_Z: usize = 16;
pub const PROBE_RADIUS: usize = 20;
pub const PROBE_FLAGS: usize = 24;
pub const PROBE_COUNT: usize = 28;
pub const PROBE_POSITIONS: usize = 32;
pub const PROBE_GC: u32 = 1;
/// Test control: after answering, Java leaves the world like the pause menu's Disconnect button.
pub const PROBE_DISCONNECT: u32 = 2;

pub const ANSWER_REQUEST: usize = 8;
pub const ANSWER_FLAGS: usize = 12;
pub const ANSWER_PLAYER_TICK: usize = 16;
pub const ANSWER_POS: usize = 24;
pub const ANSWER_HEAP: usize = 48;
pub const ANSWER_CHUNK_PACKETS: usize = 56;
pub const ANSWER_CHUNK_NANOS: usize = 64;
pub const ANSWER_FRAMES: usize = 72;
pub const ANSWER_FEED_NANOS: usize = 80;
pub const ANSWER_CHUNKS: usize = 88;
pub const ANSWER_BLOCKS: usize = 92;
pub const ANSWER_ENTITIES: usize = 96;
/// Count of Java's selected hotbar stack.
pub const ANSWER_HELD_COUNT: usize = 100;
/// Swings Java's player started for Rust's `ACTION_SWING`/`ACTION_DROP`.
pub const ANSWER_SWINGS: usize = 104;
/// `TutorialSteps` ordinal in Java's options.
pub const ANSWER_TUTORIAL: usize = 108;
pub const ANSWER_DATA: usize = 112;
pub const ANSWER_LEVEL: u32 = 1;
pub const ANSWER_PLAYER: u32 = 2;

/// Java caps reports at this size too; anything longer is truncated there.
pub const MAX_REPORT_BYTES: usize = 16 << 10;
const CONTROL_RING_CAPACITY: usize = 64 << 10;
pub const NET_IN_RING_CAPACITY: usize = 32 << 20;
pub const NET_OUT_RING_CAPACITY: usize = 4 << 20;
const EVENT_QUEUE: usize = 1024;

pub const ENTRIES: &[EntryDesc] = &[
    EntryDesc {
        name: "shared_address",
        descriptor: "(I)J",
        flavour: Flavour::Critical,
    },
    EntryDesc {
        name: "report",
        descriptor: "(IJI)V",
        flavour: Flavour::Normal,
    },
    EntryDesc {
        name: "gpu_device",
        descriptor: "(IJI)J",
        flavour: Flavour::Normal,
    },
    EntryDesc {
        name: "gpu_encode",
        descriptor: "(JIJI)J",
        flavour: Flavour::Normal,
    },
    EntryDesc {
        name: "gpu_pass",
        descriptor: "(JJIJI)J",
        flavour: Flavour::Normal,
    },
    EntryDesc {
        name: "gpu_error",
        descriptor: "(JI)I",
        flavour: Flavour::Normal,
    },
    EntryDesc {
        name: "sound_upload",
        descriptor: "(JIIII)I",
        flavour: Flavour::Normal,
    },
    EntryDesc {
        name: "sound_free",
        descriptor: "(I)V",
        flavour: Flavour::Normal,
    },
    EntryDesc {
        name: "sound_stream_open",
        descriptor: "(IIIII)I",
        flavour: Flavour::Normal,
    },
    EntryDesc {
        name: "sound_stream_push",
        descriptor: "(IIJI)I",
        flavour: Flavour::Normal,
    },
    EntryDesc {
        name: "clipboard_get",
        descriptor: "(JI)I",
        flavour: Flavour::Normal,
    },
    EntryDesc {
        name: "clipboard_set",
        descriptor: "(JI)V",
        flavour: Flavour::Normal,
    },
];

/// One `MSG_FRAME`: what Java's next frame runs with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameBegin {
    pub index: u64,
    pub ticks_to_do: u32,
    pub width: u32,
    pub height: u32,
    pub partial_tick: f32,
    pub scale: f32,
    pub flags: u32,
    pub camera_pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// Column-major, camera-relative rotation.
    pub view: [f32; 16],
    /// Column-major reverse-Z projection.
    pub proj: [f32; 16],
    /// mcr's render distance in chunks (its fog distance); 0 leaves Java's own.
    pub render_distance: u32,
    /// mcr's entity distance scaling; 0 leaves Java's own.
    pub entity_scale: f32,
    /// With `FRAME_CAMERA_HANDOFF`, the camera mcr renders from (Java's last reported one).
    pub render_pos: [f64; 3],
    pub render_yaw: f32,
    pub render_pitch: f32,
    /// Distance from the eye to the player camera along its view axis; 0 in first person.
    pub camera_distance: f32,
}

impl Default for FrameBegin {
    fn default() -> Self {
        Self {
            index: 0,
            ticks_to_do: 0,
            width: 854,
            height: 480,
            partial_tick: 0.0,
            scale: 1.0,
            flags: 0,
            camera_pos: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            view: IDENTITY,
            proj: IDENTITY,
            render_distance: 0,
            entity_scale: 0.0,
            render_pos: [0.0; 3],
            render_yaw: 0.0,
            render_pitch: 0.0,
            camera_distance: 0.0,
        }
    }
}

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

impl FrameBegin {
    pub fn encode(&self) -> [u8; FRAME_SIZE] {
        fn put(out: &mut [u8], at: usize, bytes: &[u8]) {
            out[at..at + bytes.len()].copy_from_slice(bytes);
        }
        let mut out = [0u8; FRAME_SIZE];
        put(&mut out, FRAME_INDEX, &self.index.to_ne_bytes());
        put(&mut out, FRAME_TICKS, &self.ticks_to_do.to_ne_bytes());
        put(&mut out, FRAME_WIDTH, &self.width.to_ne_bytes());
        put(&mut out, FRAME_HEIGHT, &self.height.to_ne_bytes());
        put(
            &mut out,
            FRAME_PARTIAL_TICK,
            &self.partial_tick.to_ne_bytes(),
        );
        put(&mut out, FRAME_SCALE, &self.scale.to_ne_bytes());
        put(&mut out, FRAME_FLAGS, &self.flags.to_ne_bytes());
        for (i, v) in self.camera_pos.iter().enumerate() {
            put(&mut out, FRAME_CAMERA + i * 8, &v.to_ne_bytes());
        }
        put(&mut out, FRAME_YAW, &self.yaw.to_ne_bytes());
        put(&mut out, FRAME_PITCH, &self.pitch.to_ne_bytes());
        for (i, v) in self.view.iter().enumerate() {
            put(&mut out, FRAME_VIEW + i * 4, &v.to_ne_bytes());
        }
        for (i, v) in self.proj.iter().enumerate() {
            put(&mut out, FRAME_PROJ + i * 4, &v.to_ne_bytes());
        }
        put(
            &mut out,
            FRAME_RENDER_DISTANCE,
            &self.render_distance.to_ne_bytes(),
        );
        put(
            &mut out,
            FRAME_ENTITY_SCALE,
            &self.entity_scale.to_ne_bytes(),
        );
        for (i, v) in self.render_pos.iter().enumerate() {
            put(&mut out, FRAME_RENDER_CAMERA + i * 8, &v.to_ne_bytes());
        }
        put(&mut out, FRAME_RENDER_YAW, &self.render_yaw.to_ne_bytes());
        put(
            &mut out,
            FRAME_RENDER_PITCH,
            &self.render_pitch.to_ne_bytes(),
        );
        put(
            &mut out,
            FRAME_CAMERA_DISTANCE,
            &self.camera_distance.to_ne_bytes(),
        );
        out
    }
}

/// Process-wide because the JVM is: Java keeps these addresses for as long as it lives.
#[derive(Debug)]
pub(crate) struct Shared {
    pub control: Arc<Ring>,
    pub bell: Doorbell,
    pub events: Mutex<Option<SyncSender<BridgeEvent>>>,
    pub crashed: AtomicBool,
    pub dropped_events: AtomicU64,
    pub net: NetShared,
    pub frame: FrameShared,
    pub sound: Arc<SoundShared>,
    pub input: InputShared,
    pub sections: SectionShared,
}

static SHARED: OnceLock<Shared> = OnceLock::new();
static TABLE: OnceLock<&'static ExtensionTable> = OnceLock::new();

pub(crate) fn shared() -> Result<&'static Shared, RingError> {
    if let Some(s) = SHARED.get() {
        return Ok(s);
    }
    let control = Ring::new(CONTROL_RING_CAPACITY)?;
    let net = NetShared::new(NET_IN_RING_CAPACITY, NET_OUT_RING_CAPACITY, SPILL_LIMIT)?;
    let sound = Arc::new(SoundShared::new()?);
    Ok(SHARED.get_or_init(|| Shared {
        control,
        bell: Doorbell::new(),
        events: Mutex::new(None),
        crashed: AtomicBool::new(false),
        dropped_events: AtomicU64::new(0),
        net,
        frame: FrameShared::new(),
        sound,
        input: InputShared::new(),
        sections: SectionShared::new(),
    }))
}

pub(crate) fn shared_now() -> Option<&'static Shared> {
    SHARED.get()
}

pub(crate) fn table() -> &'static ExtensionTable {
    TABLE.get_or_init(|| {
        use crate::gpu::ffi;
        let fns: [*const c_void; 12] = [
            shared_address as *const c_void,
            report as *const c_void,
            ffi::gpu_device as *const c_void,
            ffi::gpu_encode as *const c_void,
            ffi::gpu_pass as *const c_void,
            ffi::gpu_error as *const c_void,
            sound::sound_upload as *const c_void,
            sound::sound_free as *const c_void,
            sound::sound_stream_open as *const c_void,
            sound::sound_stream_push as *const c_void,
            input::clipboard_get as *const c_void,
            input::clipboard_set as *const c_void,
        ];
        let entries: Vec<_> = ENTRIES.iter().copied().zip(fns).collect();
        ExtensionTable::leak(BRIDGE_VERSION, &entries)
    })
}

pub(crate) fn event_queue() -> (
    SyncSender<BridgeEvent>,
    std::sync::mpsc::Receiver<BridgeEvent>,
) {
    std::sync::mpsc::sync_channel(EVENT_QUEUE)
}

pub(crate) fn emit(event: BridgeEvent) {
    let Some(shared) = SHARED.get() else {
        return;
    };
    let sender = shared.events.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(tx) = sender.as_ref()
        && tx.try_send(event).is_err()
    {
        shared.dropped_events.fetch_add(1, Ordering::Relaxed);
    }
}

extern "C" fn shared_address(id: i32) -> i64 {
    let Some(shared) = SHARED.get() else {
        return 0;
    };
    match id {
        SHARED_CONTROL_RING => shared.control.addr(),
        SHARED_CONTROL_BELL => shared.bell.addr(),
        SHARED_NET_IN_RING => shared.net.inbound_addr(),
        SHARED_NET_OUT_RING => shared.net.outbound_addr(),
        SHARED_FRAME_STATE => shared.frame.state_addr(),
        SHARED_FRAME_BELL => shared.frame.bell.addr(),
        SHARED_SOUND_RING => shared.sound.ring_addr(),
        SHARED_SOUND_STATES => shared.sound.states_addr(),
        SHARED_SOUND_CONTROL => shared.sound.control_addr(),
        SHARED_INPUT_STATE => shared.input.state_addr(),
        SHARED_SECTIONS => shared.sections.addr(),
        _ => 0,
    }
}

extern "C" fn report(kind: i32, ptr: i64, len: i32) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let len = usize::try_from(len).unwrap_or(0).min(MAX_REPORT_BYTES);
        let text = if ptr == 0 || len == 0 {
            String::new()
        } else {
            // SAFETY: Java passes a live confined-arena segment of at least `len` bytes for the
            // duration of this downcall; `len` was clamped to Java's own cap.
            let bytes = unsafe { std::slice::from_raw_parts(ptr as usize as *const u8, len) };
            String::from_utf8_lossy(bytes).into_owned()
        };
        let event = BridgeEvent::from_report(kind, &text);
        event.log();
        if matches!(event, BridgeEvent::Crash { .. })
            && let Some(shared) = SHARED.get()
        {
            shared.crashed.store(true, Ordering::Release);
            shared.net.detach();
            shared.frame.java_gone();
        }
        emit(event);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::{self, ffi};
    use std::path::Path;

    #[test]
    fn java_gpu_name_tables_match_rust() {
        let src = bridge_abi_java();
        let tables: &[(&str, &[&str])] = &[
            ("GPU_FORMATS", gpu::enums::GPU_FORMATS),
            ("BLEND_FACTORS", gpu::enums::BLEND_FACTORS),
            ("BLEND_OPS", gpu::enums::BLEND_OPS),
            ("COMPARE_OPS", gpu::enums::COMPARE_OPS),
            ("POLYGON_MODES", gpu::enums::POLYGON_MODES),
            ("TOPOLOGIES", gpu::enums::TOPOLOGIES),
            ("UNIFORM_TYPES", gpu::enums::UNIFORM_TYPES),
            ("ADDRESS_MODES", gpu::enums::ADDRESS_MODES),
            ("FILTER_MODES", gpu::enums::FILTER_MODES),
        ];
        for (name, rust) in tables {
            let decl = format!("String[] {name} = {{");
            let start = src.find(&decl).unwrap_or_else(|| panic!("{name} missing")) + decl.len();
            let body = &src[start..start + src[start..].find('}').unwrap()];
            let java: Vec<&str> = body.split('"').skip(1).step_by(2).collect();
            assert_eq!(&java, rust, "{name}");
        }
    }

    fn bridge_abi_java() -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../java/mcr-bridge/src/dev/mcr/bridge/BridgeAbi.java");
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn int_const(src: &str, name: &str) -> i64 {
        let decl = format!("static final int {name} = ");
        let start = src
            .find(&decl)
            .unwrap_or_else(|| panic!("{name} missing in BridgeAbi.java"));
        let rest = &src[start + decl.len()..];
        rest[..rest.find(';').unwrap()].trim().parse().unwrap()
    }

    #[test]
    fn java_entries_match_rust() {
        let src = bridge_abi_java();
        let start = src.find("enum Entry {").unwrap();
        let body = &src[start..];
        let body = &body[..body.find(';').unwrap()];
        let java: Vec<(String, String, String)> = body
            .lines()
            .filter(|l| l.contains("(\""))
            .map(|l| {
                let q: Vec<&str> = l.split('"').skip(1).step_by(2).collect();
                let flavour = l.rsplit("Flavour.").next().unwrap();
                let flavour = flavour.trim_end_matches([')', ',', ' ', ';']).to_owned();
                (q[0].to_owned(), q[1].to_owned(), flavour)
            })
            .collect();
        let rust: Vec<(String, String, String)> = ENTRIES
            .iter()
            .map(|e| {
                let flavour = match e.flavour {
                    Flavour::Critical => "CRITICAL",
                    Flavour::Normal => "NORMAL",
                };
                (e.name.into(), e.descriptor.into(), flavour.into())
            })
            .collect();
        assert_eq!(java, rust);
    }

    #[test]
    fn java_constants_match_rust() {
        let src = bridge_abi_java();
        let ints: &[(&str, i64)] = &[
            ("VERSION", BRIDGE_VERSION.into()),
            ("SHARED_CONTROL_RING", SHARED_CONTROL_RING.into()),
            ("SHARED_CONTROL_BELL", SHARED_CONTROL_BELL.into()),
            ("SHARED_NET_IN_RING", SHARED_NET_IN_RING.into()),
            ("SHARED_NET_OUT_RING", SHARED_NET_OUT_RING.into()),
            ("SHARED_FRAME_STATE", SHARED_FRAME_STATE.into()),
            ("SHARED_FRAME_BELL", SHARED_FRAME_BELL.into()),
            ("SHARED_SOUND_RING", SHARED_SOUND_RING.into()),
            ("SHARED_SOUND_STATES", SHARED_SOUND_STATES.into()),
            ("SHARED_SOUND_CONTROL", SHARED_SOUND_CONTROL.into()),
            ("SHARED_INPUT_STATE", SHARED_INPUT_STATE.into()),
            ("SHARED_SECTIONS", SHARED_SECTIONS.into()),
            ("MSG_FRAME", MSG_FRAME.into()),
            ("MSG_SHUTDOWN", MSG_SHUTDOWN.into()),
            ("MSG_INPUT", MSG_INPUT.into()),
            ("MSG_KEYS", MSG_KEYS.into()),
            ("MSG_PACKS", MSG_PACKS.into()),
            ("MSG_WORLD", MSG_WORLD.into()),
            ("WORLD_STATUS", WORLD_STATUS.into()),
            ("WORLD_EULA", WORLD_EULA.into()),
            ("WORLD_READY", WORLD_READY.into()),
            ("WORLD_FAILED", WORLD_FAILED.into()),
            ("WORLD_STOPPED", WORLD_STOPPED.into()),
            ("REPORT_WORLD", REPORT_WORLD.into()),
            ("SYNC_MAX_LINES", SYNC_MAX_LINES as i64),
            ("REPORT_KEYS", REPORT_KEYS.into()),
            ("IN_TYPE", IN_TYPE as i64),
            ("IN_A", IN_A as i64),
            ("IN_B", IN_B as i64),
            ("IN_C", IN_C as i64),
            ("IN_D", IN_D as i64),
            ("IN_X", IN_X as i64),
            ("IN_Y", IN_Y as i64),
            ("IN_DX", IN_DX as i64),
            ("IN_DY", IN_DY as i64),
            ("IN_TEXT_DATA", IN_TEXT_DATA as i64),
            ("IN_KEY", IN_KEY.into()),
            ("IN_TEXT", IN_TEXT.into()),
            ("IN_PREEDIT", IN_PREEDIT.into()),
            ("IN_MOTION", IN_MOTION.into()),
            ("IN_BUTTON", IN_BUTTON.into()),
            ("IN_WHEEL", IN_WHEEL.into()),
            ("IN_DROP", IN_DROP.into()),
            ("ISTATE_KEYS", ISTATE_KEYS as i64),
            ("ISTATE_MODS", ISTATE_MODS as i64),
            ("ISTATE_RUST_FLAGS", ISTATE_RUST_FLAGS as i64),
            ("ISTATE_JAVA_FLAGS", ISTATE_JAVA_FLAGS as i64),
            ("ISTATE_GUI_SCALE", ISTATE_GUI_SCALE as i64),
            ("ISTATE_CURSOR", ISTATE_CURSOR as i64),
            ("ISTATE_NATIVE_SEQ", ISTATE_NATIVE_SEQ as i64),
            ("ISTATE_NATIVE_KIND", ISTATE_NATIVE_KIND as i64),
            ("ISTATE_SCREEN_MOVES", ISTATE_SCREEN_MOVES as i64),
            ("ISTATE_TEXT_AREA", ISTATE_TEXT_AREA as i64),
            ("ISTATE_SIZE", ISTATE_SIZE as i64),
            ("RUST_FOCUSED", RUST_FOCUSED.into()),
            ("JAVA_SCREEN", JAVA_SCREEN.into()),
            ("JAVA_OVERLAY", JAVA_OVERLAY.into()),
            ("JAVA_GRABBED", JAVA_GRABBED.into()),
            ("JAVA_TEXT_INPUT", JAVA_TEXT_INPUT.into()),
            ("JAVA_QUIT", JAVA_QUIT.into()),
            ("JAVA_PAUSING", JAVA_PAUSING.into()),
            ("MOVE_FORWARD", MOVE_FORWARD.into()),
            ("MOVE_BACKWARD", MOVE_BACKWARD.into()),
            ("MOVE_LEFT", MOVE_LEFT.into()),
            ("MOVE_RIGHT", MOVE_RIGHT.into()),
            ("MOVE_JUMP", MOVE_JUMP.into()),
            ("MOVE_SNEAK", MOVE_SNEAK.into()),
            ("MOVE_SPRINT", MOVE_SPRINT.into()),
            ("INPUT_MAX_TEXT", crate::input::MAX_EVENT_TEXT as i64),
            (
                "INPUT_MAX_CLIPBOARD",
                crate::input::MAX_CLIPBOARD_BYTES as i64,
            ),
            ("FRAME_INDEX", FRAME_INDEX as i64),
            ("FRAME_TICKS", FRAME_TICKS as i64),
            ("FRAME_WIDTH", FRAME_WIDTH as i64),
            ("FRAME_HEIGHT", FRAME_HEIGHT as i64),
            ("FRAME_PARTIAL_TICK", FRAME_PARTIAL_TICK as i64),
            ("FRAME_SCALE", FRAME_SCALE as i64),
            ("FRAME_FLAGS", FRAME_FLAGS as i64),
            ("FRAME_CAMERA", FRAME_CAMERA as i64),
            ("FRAME_YAW", FRAME_YAW as i64),
            ("FRAME_PITCH", FRAME_PITCH as i64),
            ("FRAME_VIEW", FRAME_VIEW as i64),
            ("FRAME_PROJ", FRAME_PROJ as i64),
            ("FRAME_RENDER_DISTANCE", FRAME_RENDER_DISTANCE as i64),
            ("FRAME_ENTITY_SCALE", FRAME_ENTITY_SCALE as i64),
            ("FRAME_RENDER_CAMERA", FRAME_RENDER_CAMERA as i64),
            ("FRAME_RENDER_YAW", FRAME_RENDER_YAW as i64),
            ("FRAME_RENDER_PITCH", FRAME_RENDER_PITCH as i64),
            ("FRAME_CAMERA_DISTANCE", FRAME_CAMERA_DISTANCE as i64),
            ("FRAME_SIZE", FRAME_SIZE as i64),
            ("FRAME_LOCKSTEP", FRAME_LOCKSTEP.into()),
            ("FRAME_DETACHED", FRAME_DETACHED.into()),
            ("FRAME_CAMERA_HANDOFF", FRAME_CAMERA_HANDOFF.into()),
            ("FRAME_SECTIONS", FRAME_SECTIONS.into()),
            ("FSTATE_DONE", FSTATE_DONE as i64),
            ("FSTATE_TICKS", FSTATE_TICKS as i64),
            ("FSTATE_NANOS", FSTATE_NANOS as i64),
            ("FSTATE_FLAGS", FSTATE_FLAGS as i64),
            ("FSTATE_WIDTH", FSTATE_WIDTH as i64),
            ("FSTATE_HEIGHT", FSTATE_HEIGHT as i64),
            ("FSTATE_FOV_SCALE", FSTATE_FOV_SCALE as i64),
            ("FSTATE_CAMERA", FSTATE_CAMERA as i64),
            ("FSTATE_CAMERA_POS", FSTATE_CAMERA_POS as i64),
            ("FSTATE_CAMERA_YAW", FSTATE_CAMERA_YAW as i64),
            ("FSTATE_CAMERA_PITCH", FSTATE_CAMERA_PITCH as i64),
            ("FSTATE_PROF", FSTATE_PROF as i64),
            ("FSTATE_CAMERA_ROLL", FSTATE_CAMERA_ROLL as i64),
            ("FSTATE_PERSPECTIVE", FSTATE_PERSPECTIVE as i64),
            ("FSTATE_VIEW_ROLL", FSTATE_VIEW_ROLL as i64),
            ("PERSPECTIVE_NONE", PERSPECTIVE_NONE.into()),
            ("PERSPECTIVE_FIRST", PERSPECTIVE_FIRST.into()),
            ("PERSPECTIVE_THIRD_BACK", PERSPECTIVE_THIRD_BACK.into()),
            ("PERSPECTIVE_THIRD_FRONT", PERSPECTIVE_THIRD_FRONT.into()),
            ("FSTATE_SIZE", FSTATE_SIZE as i64),
            ("PROF_PRE", PROF_PRE as i64),
            ("PROF_TICK", PROF_TICK as i64),
            ("PROF_EXTRACT", PROF_EXTRACT as i64),
            ("PROF_RENDER", PROF_RENDER as i64),
            ("PROF_WORLD", PROF_WORLD as i64),
            ("PROF_GUI", PROF_GUI as i64),
            ("PROF_FRAME", PROF_FRAME as i64),
            ("PROF_COUNT", PROF_COUNT as i64),
            ("FSTATE_READY", FSTATE_READY.into()),
            ("FSTATE_LOADED", FSTATE_LOADED.into()),
            ("FSTATE_WEATHER", FSTATE_WEATHER.into()),
            ("FSTATE_STEPS", FSTATE_STEPS.into()),
            ("FSTATE_TURN", FSTATE_TURN.into()),
            ("CAMERA_MOD", CAMERA_MOD.into()),
            ("CAMERA_DETACHED", CAMERA_DETACHED.into()),
            ("CAMERA_INPUT", CAMERA_INPUT.into()),
            ("SEC_SEQ", SEC_SEQ as i64),
            ("SEC_ORIGIN", SEC_ORIGIN as i64),
            ("SEC_DIMS", SEC_DIMS as i64),
            ("SEC_BITS", SEC_BITS as i64),
            ("SEC_MAX_BITS", SEC_MAX_BITS as i64),
            ("SLOT_IMMEDIATE", SLOT_IMMEDIATE.into()),
            ("SLOT_GUI", SLOT_GUI.into()),
            ("SLOT_WORLD_SOLID", SLOT_WORLD_SOLID.into()),
            ("SLOT_WORLD_TRANSLUCENT", SLOT_WORLD_TRANSLUCENT.into()),
            ("SLOT_WORLD_AFTER", SLOT_WORLD_AFTER.into()),
            ("SLOT_POST_WORLD", SLOT_POST_WORLD.into()),
            ("D_SLOT", ffi::D_SLOT.into()),
            ("D_LEVEL_VIEW", ffi::D_LEVEL_VIEW.into()),
            (
                "GPU_TEXTURE_VIRTUAL",
                gpu::resources::TEXTURE_VIRTUAL as i64,
            ),
            ("REPORT_MOD_LOADED", REPORT_MOD_LOADED.into()),
            ("REPORT_INIT", REPORT_INIT.into()),
            ("REPORT_SCREEN", REPORT_SCREEN.into()),
            ("REPORT_CRASH", REPORT_CRASH.into()),
            ("REPORT_INFO", REPORT_INFO.into()),
            ("REPORT_CONNECT", REPORT_CONNECT.into()),
            ("REPORT_STOPPING", REPORT_STOPPING.into()),
            ("PHASE_LOGIN", PHASE_LOGIN.into()),
            ("PHASE_CONFIGURATION", PHASE_CONFIGURATION.into()),
            ("PHASE_PLAY", PHASE_PLAY.into()),
            ("NET_HEADER", NET_HEADER as i64),
            ("NET_TAG", NET_TAG as i64),
            ("NET_SESSION", NET_SESSION as i64),
            ("NET_FRAME", NET_FRAME.into()),
            ("NET_OPEN", NET_OPEN.into()),
            ("NET_CLOSE", NET_CLOSE.into()),
            ("NET_PLAYER", NET_PLAYER.into()),
            ("NET_PROBE", NET_PROBE.into()),
            ("NET_ACTION", NET_ACTION.into()),
            ("OUT_FRAME", OUT_FRAME.into()),
            ("OUT_CHAT", OUT_CHAT.into()),
            ("OUT_DISCONNECT", OUT_DISCONNECT.into()),
            ("OUT_PROBE", OUT_PROBE.into()),
            ("DISCONNECT_LEFT", DISCONNECT_LEFT.into()),
            ("DISCONNECT_FAILED", DISCONNECT_FAILED.into()),
            ("CHAT_MESSAGE", CHAT_MESSAGE.into()),
            ("CHAT_COMMAND", CHAT_COMMAND.into()),
            ("OPEN_PORT", OPEN_PORT as i64),
            ("OPEN_HOST", OPEN_HOST as i64),
            ("PLAYER_TICK", PLAYER_TICK as i64),
            ("PLAYER_POS", PLAYER_POS as i64),
            ("PLAYER_OLD_POS", PLAYER_OLD_POS as i64),
            ("PLAYER_VELOCITY", PLAYER_VELOCITY as i64),
            ("PLAYER_YAW", PLAYER_YAW as i64),
            ("PLAYER_PITCH", PLAYER_PITCH as i64),
            ("PLAYER_FLAGS", PLAYER_FLAGS as i64),
            ("PLAYER_POSE", PLAYER_POSE as i64),
            ("PLAYER_SLOT", PLAYER_SLOT as i64),
            ("PLAYER_VEHICLE", PLAYER_VEHICLE as i64),
            ("PLAYER_INPUT", PLAYER_INPUT as i64),
            ("PLAYER_SIZE", PLAYER_SIZE as i64),
            ("PLAYER_ON_GROUND", PLAYER_ON_GROUND.into()),
            ("PLAYER_SHIFT", PLAYER_SHIFT.into()),
            ("PLAYER_SPRINT", PLAYER_SPRINT.into()),
            ("PLAYER_SWIM", PLAYER_SWIM.into()),
            ("PLAYER_FALL_FLYING", PLAYER_FALL_FLYING.into()),
            ("INPUT_FORWARD", INPUT_FORWARD.into()),
            ("INPUT_BACKWARD", INPUT_BACKWARD.into()),
            ("INPUT_LEFT", INPUT_LEFT.into()),
            ("INPUT_RIGHT", INPUT_RIGHT.into()),
            ("INPUT_JUMP", INPUT_JUMP.into()),
            ("INPUT_SHIFT", INPUT_SHIFT.into()),
            ("INPUT_SPRINT", INPUT_SPRINT.into()),
            ("ACTION_KIND", ACTION_KIND as i64),
            ("ACTION_ARG", ACTION_ARG as i64),
            ("ACTION_POS", ACTION_POS as i64),
            ("ACTION_VALUE", ACTION_VALUE as i64),
            ("ACTION_SIZE", ACTION_SIZE as i64),
            ("ACTION_SWING", ACTION_SWING.into()),
            ("ACTION_DROP", ACTION_DROP.into()),
            ("ACTION_DESTROY", ACTION_DESTROY.into()),
            ("ACTION_RESET_ATTACK", ACTION_RESET_ATTACK.into()),
            ("SWING_OFF_HAND", SWING_OFF_HAND.into()),
            ("SWING_ATTACK", SWING_ATTACK.into()),
            ("PROBE_REQUEST", PROBE_REQUEST as i64),
            ("PROBE_CENTER_X", PROBE_CENTER_X as i64),
            ("PROBE_CENTER_Z", PROBE_CENTER_Z as i64),
            ("PROBE_RADIUS", PROBE_RADIUS as i64),
            ("PROBE_FLAGS", PROBE_FLAGS as i64),
            ("PROBE_COUNT", PROBE_COUNT as i64),
            ("PROBE_POSITIONS", PROBE_POSITIONS as i64),
            ("PROBE_GC", PROBE_GC.into()),
            ("PROBE_DISCONNECT", PROBE_DISCONNECT.into()),
            ("ANSWER_REQUEST", ANSWER_REQUEST as i64),
            ("ANSWER_FLAGS", ANSWER_FLAGS as i64),
            ("ANSWER_PLAYER_TICK", ANSWER_PLAYER_TICK as i64),
            ("ANSWER_POS", ANSWER_POS as i64),
            ("ANSWER_HEAP", ANSWER_HEAP as i64),
            ("ANSWER_CHUNK_PACKETS", ANSWER_CHUNK_PACKETS as i64),
            ("ANSWER_CHUNK_NANOS", ANSWER_CHUNK_NANOS as i64),
            ("ANSWER_FRAMES", ANSWER_FRAMES as i64),
            ("ANSWER_FEED_NANOS", ANSWER_FEED_NANOS as i64),
            ("ANSWER_CHUNKS", ANSWER_CHUNKS as i64),
            ("ANSWER_BLOCKS", ANSWER_BLOCKS as i64),
            ("ANSWER_ENTITIES", ANSWER_ENTITIES as i64),
            ("ANSWER_HELD_COUNT", ANSWER_HELD_COUNT as i64),
            ("ANSWER_SWINGS", ANSWER_SWINGS as i64),
            ("ANSWER_TUTORIAL", ANSWER_TUTORIAL as i64),
            ("ANSWER_DATA", ANSWER_DATA as i64),
            ("ANSWER_LEVEL", ANSWER_LEVEL.into()),
            ("ANSWER_PLAYER", ANSWER_PLAYER.into()),
            ("PIPELINE_MAGIC", gpu::info::MAGIC.into()),
            ("PIPELINE_FORMAT", gpu::info::FORMAT.into()),
            ("GPU_MAX_ARGS", ffi::MAX_ARGS as i64),
            ("GPU_INFO_SLOTS", ffi::INFO_SLOTS as i64),
            ("GPU_FEATURE_POLYGON_LINE", ffi::FEATURE_POLYGON_LINE as i64),
            (
                "GPU_FEATURE_FIRST_INSTANCE",
                ffi::FEATURE_FIRST_INSTANCE as i64,
            ),
            ("GPU_MAX_UNIFORMS", gpu::pass::MAX_UNIFORMS as i64),
            ("D_INFO", ffi::D_INFO.into()),
            ("D_BUFFER", ffi::D_BUFFER.into()),
            ("D_TRANSIENT", ffi::D_TRANSIENT.into()),
            ("D_HOST", ffi::D_HOST.into()),
            ("D_TEXTURE", ffi::D_TEXTURE.into()),
            ("D_VIEW", ffi::D_VIEW.into()),
            ("D_SAMPLER", ffi::D_SAMPLER.into()),
            ("D_PIPELINE", ffi::D_PIPELINE.into()),
            ("D_DESTROY", ffi::D_DESTROY.into()),
            ("D_ENCODER", ffi::D_ENCODER.into()),
            ("D_SURFACE", ffi::D_SURFACE.into()),
            ("D_SURFACE_CONFIGURE", ffi::D_SURFACE_CONFIGURE.into()),
            ("D_SURFACE_PRESENT", ffi::D_SURFACE_PRESENT.into()),
            ("D_STATS", ffi::D_STATS.into()),
            ("E_CLEAR", ffi::E_CLEAR.into()),
            ("E_CLEAR_REGION", ffi::E_CLEAR_REGION.into()),
            ("E_WRITE_BUFFER", ffi::E_WRITE_BUFFER.into()),
            ("E_COPY_BUFFER", ffi::E_COPY_BUFFER.into()),
            ("E_WRITE_TEXTURE", ffi::E_WRITE_TEXTURE.into()),
            ("E_BUFFER_TO_TEXTURE", ffi::E_BUFFER_TO_TEXTURE.into()),
            ("E_TEXTURE_TO_BUFFER", ffi::E_TEXTURE_TO_BUFFER.into()),
            ("E_TEXTURE_TO_TEXTURE", ffi::E_TEXTURE_TO_TEXTURE.into()),
            ("E_FLUSH_MAPPED", ffi::E_FLUSH_MAPPED.into()),
            ("E_BLIT", ffi::E_BLIT.into()),
            ("E_DIRTY", ffi::E_DIRTY.into()),
            ("E_SUBMIT", ffi::E_SUBMIT.into()),
            ("E_POLL", ffi::E_POLL.into()),
            ("E_WAIT", ffi::E_WAIT.into()),
            ("E_SPLIT", ffi::E_SPLIT.into()),
            ("P_PIPELINE", gpu::pass::P_PIPELINE.into()),
            ("P_UNIFORM_BUFFER", gpu::pass::P_UNIFORM_BUFFER.into()),
            ("P_UNIFORM_TEXTURE", gpu::pass::P_UNIFORM_TEXTURE.into()),
            ("P_UNIFORM_NONE", gpu::pass::P_UNIFORM_NONE.into()),
            ("P_PUSH", gpu::pass::P_PUSH.into()),
            ("P_SCISSOR", gpu::pass::P_SCISSOR.into()),
            ("P_NO_SCISSOR", gpu::pass::P_NO_SCISSOR.into()),
            ("P_VERTEX", gpu::pass::P_VERTEX.into()),
            ("P_INDEX", gpu::pass::P_INDEX.into()),
            ("P_DRAW", gpu::pass::P_DRAW.into()),
            ("P_DRAW_INDEXED", gpu::pass::P_DRAW_INDEXED.into()),
            ("P_MULTI_DRAW", gpu::pass::P_MULTI_DRAW.into()),
            (
                "P_MULTI_DRAW_INDEXED",
                gpu::pass::P_MULTI_DRAW_INDEXED.into(),
            ),
            ("P_DRAW_INDIRECT", gpu::pass::P_DRAW_INDIRECT.into()),
            (
                "P_DRAW_INDEXED_INDIRECT",
                gpu::pass::P_DRAW_INDEXED_INDIRECT.into(),
            ),
            ("P_DEBUG_PUSH", gpu::pass::P_DEBUG_PUSH.into()),
            ("P_DEBUG_POP", gpu::pass::P_DEBUG_POP.into()),
            ("PASS_CONTINUES", gpu::pass::PASS_CONTINUES as i64),
            ("PASS_RESUMES", gpu::pass::PASS_RESUMES as i64),
        ];
        for (name, value) in ints {
            assert_eq!(int_const(&src, name), *value, "{name}");
        }
        assert!(src.contains(&format!("TABLE_PROPERTY = \"{TABLE_PROPERTY}\"")));
        assert!(src.contains(&format!("COMPOSE_PROPERTY = \"{COMPOSE_PROPERTY}\"")));
    }

    #[test]
    fn frame_layout_round_trips() {
        let mut view = [0.0f32; 16];
        view[15] = 7.5;
        let mut proj = [0.0f32; 16];
        proj[0] = -3.0;
        let f = FrameBegin {
            index: 0x0102_0304_0506_0708,
            ticks_to_do: 3,
            width: 854,
            height: 480,
            partial_tick: 0.25,
            scale: 2.0,
            flags: FRAME_LOCKSTEP,
            camera_pos: [1.5, -64.0, 1e7],
            yaw: 90.0,
            pitch: -12.5,
            view,
            proj,
            render_distance: 7,
            entity_scale: 1.5,
            render_pos: [-2.5, 70.25, 3e6],
            render_yaw: 1.25,
            render_pitch: -0.5,
            camera_distance: 3.25,
        };
        let b = f.encode();
        let u32_at = |o: usize| u32::from_ne_bytes(b[o..o + 4].try_into().unwrap());
        assert_eq!(
            u64::from_ne_bytes(b[..8].try_into().unwrap()),
            0x0102_0304_0506_0708
        );
        assert_eq!(u32_at(FRAME_TICKS), 3);
        assert_eq!((u32_at(FRAME_WIDTH), u32_at(FRAME_HEIGHT)), (854, 480));
        assert_eq!(f32::from_bits(u32_at(FRAME_PARTIAL_TICK)), 0.25);
        assert_eq!(f32::from_bits(u32_at(FRAME_SCALE)), 2.0);
        let f64_at = |o: usize| f64::from_ne_bytes(b[o..o + 8].try_into().unwrap());
        assert_eq!(u32_at(FRAME_FLAGS), FRAME_LOCKSTEP);
        assert_eq!(
            [
                f64_at(FRAME_CAMERA),
                f64_at(FRAME_CAMERA + 8),
                f64_at(FRAME_CAMERA + 16)
            ],
            [1.5, -64.0, 1e7]
        );
        assert_eq!(f32::from_bits(u32_at(FRAME_YAW)), 90.0);
        assert_eq!(f32::from_bits(u32_at(FRAME_PITCH)), -12.5);
        assert_eq!(f32::from_bits(u32_at(FRAME_VIEW + 60)), 7.5);
        assert_eq!(f32::from_bits(u32_at(FRAME_PROJ)), -3.0);
        assert_eq!(u32_at(FRAME_RENDER_DISTANCE), 7);
        assert_eq!(f32::from_bits(u32_at(FRAME_ENTITY_SCALE)), 1.5);
        assert_eq!(
            [
                f64_at(FRAME_RENDER_CAMERA),
                f64_at(FRAME_RENDER_CAMERA + 8),
                f64_at(FRAME_RENDER_CAMERA + 16)
            ],
            [-2.5, 70.25, 3e6]
        );
        assert_eq!(f32::from_bits(u32_at(FRAME_RENDER_YAW)), 1.25);
        assert_eq!(f32::from_bits(u32_at(FRAME_RENDER_PITCH)), -0.5);
        assert_eq!(f32::from_bits(u32_at(FRAME_CAMERA_DISTANCE)), 3.25);
        assert_eq!(FRAME_CAMERA_DISTANCE + 4, FRAME_SIZE);
    }

    #[test]
    fn report_bounds_and_tolerates_bad_pointers() {
        let (tx, rx) = event_queue();
        *shared().unwrap().events.lock().unwrap() = Some(tx);
        let text = "modmenu\t21.0.0";
        report(REPORT_MOD_LOADED, text.as_ptr() as i64, text.len() as i32);
        report(REPORT_INFO, 0, 100);
        report(REPORT_SCREEN, text.as_ptr() as i64, -5);
        let got: Vec<_> = rx.try_iter().collect();
        assert_eq!(
            got[0],
            BridgeEvent::ModLoaded {
                id: "modmenu".into(),
                version: "21.0.0".into()
            }
        );
        assert_eq!(got.len(), 3);
        assert_eq!(shared_address(99), 0);
        assert_ne!(shared_address(SHARED_CONTROL_RING), 0);
        assert_ne!(shared_address(SHARED_NET_IN_RING), 0);
        assert_ne!(shared_address(SHARED_NET_OUT_RING), 0);
        assert_ne!(shared_address(SHARED_FRAME_STATE), 0);
        assert_ne!(shared_address(SHARED_FRAME_BELL), 0);
        assert_ne!(shared_address(SHARED_SOUND_RING), 0);
        assert_ne!(shared_address(SHARED_SOUND_STATES), 0);
        assert_ne!(shared_address(SHARED_SOUND_CONTROL), 0);
        assert_ne!(shared_address(SHARED_INPUT_STATE), 0);
        assert_ne!(shared_address(SHARED_SECTIONS), 0);
        *shared().unwrap().events.lock().unwrap() = None;
    }
}
