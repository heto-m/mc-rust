//! Input for Java in mod mode: SDL3-equivalent events through the control ring, the key-state
//! bitset Java's `InputConstants.isKeyDown` reads, and Java's window wishes (grab, cursor, IME,
//! quit, native overlay) read back from a shared state block.

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use mcr_jvm::{PushError, RingWriter};

use crate::abi::{
    IN_A, IN_B, IN_BUTTON, IN_C, IN_D, IN_DROP, IN_DX, IN_DY, IN_KEY, IN_MOTION, IN_PREEDIT,
    IN_TEXT, IN_TEXT_DATA, IN_TYPE, IN_WHEEL, IN_X, IN_Y, ISTATE_CURSOR, ISTATE_GUI_SCALE,
    ISTATE_JAVA_FLAGS, ISTATE_KEYS, ISTATE_MODS, ISTATE_NATIVE_KIND, ISTATE_NATIVE_SEQ,
    ISTATE_RUST_FLAGS, ISTATE_SCREEN_MOVES, ISTATE_SIZE, ISTATE_TEXT_AREA, JAVA_GRABBED,
    JAVA_OVERLAY, JAVA_QUIT, JAVA_SCREEN, JAVA_TEXT_INPUT, MOVE_BACKWARD, MOVE_FORWARD, MOVE_JUMP,
    MOVE_LEFT, MOVE_RIGHT, MOVE_SNEAK, MOVE_SPRINT, MSG_INPUT, RUST_FOCUSED,
};

/// SDL keycodes of keys without a character are their scancode with this bit set.
pub const SDLK_SCANCODE_MASK: i32 = 0x4000_0000;
/// SDL scancodes, and so the key-state bitset, stop below this.
pub const SCANCODES: u32 = 512;
/// Longest text, pre-edit or drop list one event carries; longer input is cut on a char boundary.
pub const MAX_EVENT_TEXT: usize = 4096;
pub const MAX_DROP_PATHS: usize = 64;
/// Events waiting for Java's next frame; more are dropped and counted.
pub const MAX_QUEUED: usize = 1024;
/// Clipboard text in either direction is cut to this.
pub const MAX_CLIPBOARD_BYTES: usize = 1 << 20;
/// SDL mouse buttons Java accepts: left, middle, right, then extra buttons.
pub const MAX_BUTTON: u8 = 32;

pub mod keymod {
    pub const LSHIFT: u16 = 0x0001;
    pub const RSHIFT: u16 = 0x0002;
    pub const LCTRL: u16 = 0x0040;
    pub const RCTRL: u16 = 0x0080;
    pub const LALT: u16 = 0x0100;
    pub const RALT: u16 = 0x0200;
    pub const LGUI: u16 = 0x0400;
    pub const RGUI: u16 = 0x0800;
    pub const NUM: u16 = 0x1000;
    pub const CAPS: u16 = 0x2000;
}

const SCAN_CAPS_LOCK: u32 = 57;
const SCAN_NUM_LOCK: u32 = 83;

fn is_keypad(scancode: u32) -> bool {
    matches!(scancode, 83..=99 | 103 | 133 | 134 | 176..=221)
}

/// The SDL keycode of a key: control keys have fixed codes, printable keys the lowercase
/// character the layout gives without modifiers, keypad and other keys the masked scancode.
pub fn sdl_keycode(scancode: u32, printable: Option<char>) -> i32 {
    if scancode == 0 || scancode >= SCANCODES {
        return 0;
    }
    match scancode {
        40 => return 0x0D,
        41 => return 0x1B,
        42 => return 0x08,
        43 => return 0x09,
        44 => return 0x20,
        76 => return 0x7F,
        s if is_keypad(s) => return SDLK_SCANCODE_MASK | s as i32,
        _ => {}
    }
    match printable.and_then(|c| c.to_lowercase().next()) {
        Some(c) if !c.is_control() => c as i32,
        _ => SDLK_SCANCODE_MASK | scancode as i32,
    }
}

/// What a US layout types for a scancode without modifiers, for scripted input.
pub fn us_printable(scancode: u32) -> Option<char> {
    Some(match scancode {
        4..=29 => char::from(b'a' + (scancode - 4) as u8),
        30..=38 => char::from(b'1' + (scancode - 30) as u8),
        39 => '0',
        45 => '-',
        46 => '=',
        47 => '[',
        48 => ']',
        49 => '\\',
        51 => ';',
        52 => '\'',
        53 => '`',
        54 => ',',
        55 => '.',
        56 => '/',
        _ => return None,
    })
}

/// SDL modifier bit of a modifier key's scancode.
pub fn keymod_bit(scancode: u32) -> u16 {
    match scancode {
        224 => keymod::LCTRL,
        225 => keymod::LSHIFT,
        226 => keymod::LALT,
        227 => keymod::LGUI,
        228 => keymod::RCTRL,
        229 => keymod::RSHIFT,
        230 => keymod::RALT,
        231 => keymod::RGUI,
        _ => 0,
    }
}

/// `KeyEvent` actions as vanilla's `KeyboardHandler.keyPress` takes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Release = 0,
    Press = 1,
    Repeat = -1,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    Key {
        scancode: u32,
        keycode: i32,
        mods: u16,
        action: KeyAction,
    },
    Text(String),
    /// SDL text editing: `start` is the caret in code points (-1 for the end), `length` the selection.
    Preedit {
        text: String,
        start: i32,
        length: i32,
    },
    /// Window pixels; the deltas are raw device motion while the cursor is grabbed.
    Motion {
        x: f64,
        y: f64,
        dx: f64,
        dy: f64,
    },
    Button {
        button: u8,
        mods: u16,
        pressed: bool,
    },
    Wheel {
        dx: f64,
        dy: f64,
    },
    Drop(Vec<String>),
}

fn clip(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn finite(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(-1.0e7, 1.0e7)
    } else {
        0.0
    }
}

impl InputEvent {
    /// The message payload: a fixed head of ints and doubles, then UTF-8 text for text kinds.
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.clear();
        out.resize(IN_TEXT_DATA, 0);
        let put_i32 = |out: &mut Vec<u8>, at: usize, v: i32| {
            out[at..at + 4].copy_from_slice(&v.to_ne_bytes());
        };
        let put_f64 = |out: &mut Vec<u8>, at: usize, v: f64| {
            out[at..at + 8].copy_from_slice(&finite(v).to_ne_bytes());
        };
        let text = |out: &mut Vec<u8>, kind: u32, s: &str| {
            let s = clip(s, MAX_EVENT_TEXT);
            out[IN_TYPE..IN_TYPE + 4].copy_from_slice(&kind.to_ne_bytes());
            out[IN_D..IN_D + 4].copy_from_slice(&(s.len() as i32).to_ne_bytes());
            out.extend_from_slice(s.as_bytes());
        };
        match self {
            Self::Key {
                scancode,
                keycode,
                mods,
                action,
            } => {
                put_i32(out, IN_TYPE, IN_KEY as i32);
                put_i32(out, IN_A, *scancode as i32);
                put_i32(out, IN_B, *keycode);
                put_i32(out, IN_C, i32::from(*mods));
                put_i32(out, IN_D, *action as i32);
            }
            Self::Text(s) => text(out, IN_TEXT, s),
            Self::Preedit {
                text: s,
                start,
                length,
            } => {
                text(out, IN_PREEDIT, s);
                put_i32(out, IN_A, *start);
                put_i32(out, IN_B, *length);
            }
            Self::Motion { x, y, dx, dy } => {
                put_i32(out, IN_TYPE, IN_MOTION as i32);
                put_f64(out, IN_X, *x);
                put_f64(out, IN_Y, *y);
                put_f64(out, IN_DX, *dx);
                put_f64(out, IN_DY, *dy);
            }
            Self::Button {
                button,
                mods,
                pressed,
            } => {
                put_i32(out, IN_TYPE, IN_BUTTON as i32);
                put_i32(out, IN_A, i32::from(*button));
                put_i32(out, IN_B, i32::from(*mods));
                put_i32(out, IN_C, i32::from(*pressed));
            }
            Self::Wheel { dx, dy } => {
                put_i32(out, IN_TYPE, IN_WHEEL as i32);
                put_f64(out, IN_DX, *dx);
                put_f64(out, IN_DY, *dy);
            }
            Self::Drop(paths) => {
                let mut joined = String::new();
                for p in paths.iter().take(MAX_DROP_PATHS) {
                    if p.contains('\0') || joined.len() + p.len() + 1 > MAX_EVENT_TEXT {
                        continue;
                    }
                    joined.push_str(p);
                    joined.push('\0');
                }
                text(out, IN_DROP, &joined);
            }
        }
    }
}

#[repr(C, align(64))]
#[derive(Debug, Default)]
struct InputState {
    keys: [AtomicU64; 8],
    mods: AtomicU32,
    rust_flags: AtomicU32,
    java_flags: AtomicU32,
    gui_scale: AtomicU32,
    cursor: AtomicU32,
    native_seq: AtomicU32,
    native_kind: AtomicU32,
    screen_moves: AtomicU32,
    text_area: [AtomicI32; 4],
}

const _: () = {
    assert!(std::mem::offset_of!(InputState, keys) == ISTATE_KEYS);
    assert!(std::mem::offset_of!(InputState, mods) == ISTATE_MODS);
    assert!(std::mem::offset_of!(InputState, rust_flags) == ISTATE_RUST_FLAGS);
    assert!(std::mem::offset_of!(InputState, java_flags) == ISTATE_JAVA_FLAGS);
    assert!(std::mem::offset_of!(InputState, gui_scale) == ISTATE_GUI_SCALE);
    assert!(std::mem::offset_of!(InputState, cursor) == ISTATE_CURSOR);
    assert!(std::mem::offset_of!(InputState, native_seq) == ISTATE_NATIVE_SEQ);
    assert!(std::mem::offset_of!(InputState, native_kind) == ISTATE_NATIVE_KIND);
    assert!(std::mem::offset_of!(InputState, screen_moves) == ISTATE_SCREEN_MOVES);
    assert!(std::mem::offset_of!(InputState, text_area) == ISTATE_TEXT_AREA);
    assert!(std::mem::size_of::<InputState>() == ISTATE_SIZE);
};

/// The system clipboard as mcr sees it; Java's `ClipboardManager` reads and writes through it.
pub trait ClipboardHost: Send + Sync {
    fn get(&self) -> Option<String>;
    fn set(&self, text: &str);
}

/// Stands in until the app installs the system clipboard (and in tests).
#[derive(Debug, Default)]
pub struct MemoryClipboard(Mutex<String>);

impl ClipboardHost for MemoryClipboard {
    fn get(&self) -> Option<String> {
        Some(lock(&self.0).clone())
    }

    fn set(&self, text: &str) {
        clip(text, MAX_CLIPBOARD_BYTES).clone_into(&mut lock(&self.0));
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Process-wide like the VM: Java keeps the state block's address for as long as it lives.
pub(crate) struct InputShared {
    state: Box<InputState>,
    clipboard: Mutex<Arc<dyn ClipboardHost>>,
}

impl std::fmt::Debug for InputShared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputShared").finish_non_exhaustive()
    }
}

impl InputShared {
    pub fn new() -> Self {
        let state = Box::<InputState>::default();
        state.rust_flags.store(RUST_FOCUSED, Ordering::Release);
        Self {
            state,
            clipboard: Mutex::new(Arc::new(MemoryClipboard::default())),
        }
    }

    pub fn state_addr(&self) -> i64 {
        &raw const *self.state as i64
    }

    fn clipboard(&self) -> Arc<dyn ClipboardHost> {
        lock(&self.clipboard).clone()
    }

    #[cfg(test)]
    pub fn simulate_java(&self, flags: u32, cursor: u32, gui_scale: u32) {
        let s = &self.state;
        s.java_flags.store(flags, Ordering::Release);
        s.cursor.store(cursor, Ordering::Release);
        s.gui_scale.store(gui_scale, Ordering::Release);
    }

    #[cfg(test)]
    pub fn simulate_native(&self, seq: u32, kind: u32) {
        self.state.native_kind.store(kind, Ordering::Release);
        self.state.native_seq.store(seq, Ordering::Release);
    }
}

/// Cursor shapes Java selects (`CursorTypes`), in the order of the shared state's values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CursorKind {
    #[default]
    Default,
    Arrow,
    IBeam,
    Crosshair,
    PointingHand,
    ResizeNs,
    ResizeEw,
    ResizeAll,
    NotAllowed,
}

impl CursorKind {
    fn from_raw(v: u32) -> Self {
        match v {
            1 => Self::Arrow,
            2 => Self::IBeam,
            3 => Self::Crosshair,
            4 => Self::PointingHand,
            5 => Self::ResizeNs,
            6 => Self::ResizeEw,
            7 => Self::ResizeAll,
            8 => Self::NotAllowed,
            _ => Self::Default,
        }
    }
}

/// What Java's "mcr" button asks mcr to show above its GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeKind {
    Menu,
    Settings,
    Mods,
}

impl NativeKind {
    fn from_raw(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::Menu),
            1 => Some(Self::Settings),
            2 => Some(Self::Mods),
            _ => None,
        }
    }
}

/// Java's side of the input state, as last published.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JavaInput {
    /// A Java screen is open: every input event belongs to Java.
    pub screen: bool,
    /// Java's loading overlay is up.
    pub overlay: bool,
    /// Java grabbed the mouse (relative mode), as vanilla does with no screen open.
    pub grabbed: bool,
    /// A Java text field wants text input (SDL `StartTextInput`).
    pub text_input: bool,
    /// Java's Quit was pressed.
    pub quit: bool,
    pub cursor: CursorKind,
    /// Java's GUI scale, 0 before Java reported one.
    pub gui_scale: u32,
    /// The focused text field in window pixels: x, y, width, height.
    pub text_area: Option<[i32; 4]>,
    /// `MOVE_*` bits a mod let through while a screen is open (InvMove); 0 without a screen.
    pub screen_moves: u32,
}

impl JavaInput {
    /// Java owns all input: a screen or its loading overlay is up.
    pub fn captures_input(&self) -> bool {
        self.screen || self.overlay
    }
}

const MAX_GUI_SCALE: u32 = 16;
const MOVE_ALL: u32 =
    MOVE_FORWARD | MOVE_BACKWARD | MOVE_LEFT | MOVE_RIGHT | MOVE_JUMP | MOVE_SNEAK | MOVE_SPRINT;
const MAX_AREA: i32 = 1 << 16;

fn read_java(s: &InputState) -> JavaInput {
    let flags = s.java_flags.load(Ordering::Acquire);
    let area = std::array::from_fn(|i| s.text_area[i].load(Ordering::Acquire));
    let [x, y, w, h] = area;
    let valid = (0..MAX_AREA).contains(&x)
        && (0..MAX_AREA).contains(&y)
        && (1..MAX_AREA).contains(&w)
        && (1..MAX_AREA).contains(&h);
    let scale = s.gui_scale.load(Ordering::Acquire);
    JavaInput {
        screen: flags & JAVA_SCREEN != 0,
        overlay: flags & JAVA_OVERLAY != 0,
        grabbed: flags & JAVA_GRABBED != 0,
        text_input: flags & JAVA_TEXT_INPUT != 0,
        quit: flags & JAVA_QUIT != 0,
        cursor: CursorKind::from_raw(s.cursor.load(Ordering::Acquire)),
        gui_scale: if scale <= MAX_GUI_SCALE { scale } else { 0 },
        text_area: valid.then_some(area),
        screen_moves: if flags & JAVA_SCREEN != 0 {
            s.screen_moves.load(Ordering::Acquire) & MOVE_ALL
        } else {
            0
        },
    }
}

/// Events for Java's next frame and the state Java will see with them.
#[derive(Debug, Default)]
pub(crate) struct InputQueue {
    events: VecDeque<InputEvent>,
    keys: [u64; 8],
    buttons: u64,
    locks: u16,
    focused: bool,
    dropped: u64,
    native_seen: u32,
    scratch: Vec<u8>,
}

impl InputQueue {
    fn new() -> Self {
        Self {
            focused: true,
            ..Self::default()
        }
    }

    fn push(&mut self, event: InputEvent) {
        if let InputEvent::Motion { x, y, dx, dy } = event
            && let Some(InputEvent::Motion {
                x: lx,
                y: ly,
                dx: ldx,
                dy: ldy,
            }) = self.events.back_mut()
        {
            (*lx, *ly) = (x, y);
            *ldx += dx;
            *ldy += dy;
            return;
        }
        if self.events.len() >= MAX_QUEUED {
            self.dropped += 1;
            return;
        }
        self.events.push_back(event);
    }

    fn key_down(&self, scancode: u32) -> bool {
        scancode < SCANCODES && self.keys[(scancode / 64) as usize] & (1 << (scancode % 64)) != 0
    }

    fn set_key(&mut self, scancode: u32, down: bool) {
        let (word, bit) = ((scancode / 64) as usize, 1u64 << (scancode % 64));
        if down {
            self.keys[word] |= bit;
        } else {
            self.keys[word] &= !bit;
        }
    }

    fn mods(&self) -> u16 {
        (224..232)
            .filter(|&s| self.key_down(s))
            .map(keymod_bit)
            .fold(self.locks, |a, b| a | b)
    }

    fn key(&mut self, scancode: u32, printable: Option<char>, pressed: bool, repeat: bool) {
        if scancode == 0 || scancode >= SCANCODES {
            return;
        }
        let was = self.key_down(scancode);
        let action = match (pressed, was) {
            (false, false) => return,
            (false, true) => KeyAction::Release,
            (true, true) if repeat => KeyAction::Repeat,
            (true, true) => return,
            (true, false) => KeyAction::Press,
        };
        self.set_key(scancode, pressed);
        if action == KeyAction::Press {
            match scancode {
                SCAN_CAPS_LOCK => self.locks ^= keymod::CAPS,
                SCAN_NUM_LOCK => self.locks ^= keymod::NUM,
                _ => {}
            }
        }
        let mods = self.mods();
        self.push(InputEvent::Key {
            scancode,
            keycode: sdl_keycode(scancode, printable),
            mods,
            action,
        });
    }

    fn button(&mut self, button: u8, pressed: bool) {
        if button == 0 || button > MAX_BUTTON {
            return;
        }
        let bit = 1u64 << button;
        if (self.buttons & bit != 0) == pressed {
            return;
        }
        self.buttons ^= bit;
        let mods = self.mods();
        self.push(InputEvent::Button {
            button,
            mods,
            pressed,
        });
    }

    fn release_all(&mut self) {
        for s in 0..SCANCODES {
            if self.key_down(s) {
                self.key(s, None, false, false);
            }
        }
        for b in 1..=MAX_BUTTON {
            if self.buttons & (1 << b) != 0 {
                self.button(b, false);
            }
        }
    }

    fn clear(&mut self) {
        self.events.clear();
        self.keys = [0; 8];
        self.buttons = 0;
    }

    /// Publishes the key state and pushes the queued events; whatever does not fit waits.
    pub fn flush(&mut self, writer: &mut RingWriter, shared: &InputShared) -> usize {
        let s = &shared.state;
        for (slot, &bits) in s.keys.iter().zip(&self.keys) {
            slot.store(bits, Ordering::Release);
        }
        s.mods.store(u32::from(self.mods()), Ordering::Release);
        s.rust_flags.store(
            if self.focused { RUST_FOCUSED } else { 0 },
            Ordering::Release,
        );
        let mut pushed = 0;
        while let Some(event) = self.events.front() {
            event.encode(&mut self.scratch);
            match writer.try_push(MSG_INPUT, &self.scratch) {
                Ok(()) => {
                    self.events.pop_front();
                    pushed += 1;
                }
                Err(PushError::Full) => break,
                Err(e) => {
                    tracing::warn!("bridge input push failed: {e}");
                    self.events.pop_front();
                    self.dropped += 1;
                }
            }
        }
        pushed
    }

    /// Rust has authority again (bridge detached or booting): nothing queued reaches Java.
    pub fn discard(&mut self) {
        self.dropped += self.events.len() as u64;
        self.clear();
    }
}

/// The app's handle on Java's input: events go out with Java's next frame.
#[derive(Debug, Clone)]
pub struct ModInput {
    queue: Arc<Mutex<InputQueue>>,
    shared: &'static InputShared,
}

impl ModInput {
    pub(crate) fn new(shared: &'static InputShared) -> Self {
        Self {
            queue: Arc::new(Mutex::new(InputQueue::new())),
            shared,
        }
    }

    pub(crate) fn queue(&self) -> MutexGuard<'_, InputQueue> {
        lock(&self.queue)
    }

    /// A key transition by SDL scancode (USB HID usage); `printable` is the layout's character
    /// for the key without modifiers. Presses of a held key count only as repeats.
    pub fn key(&self, scancode: u32, printable: Option<char>, pressed: bool, repeat: bool) {
        self.queue().key(scancode, printable, pressed, repeat);
    }

    pub fn text(&self, text: &str) {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if !text.is_empty() {
            self.queue()
                .push(InputEvent::Text(clip(&text, MAX_EVENT_TEXT).to_owned()));
        }
    }

    pub fn preedit(&self, text: &str, start: i32, length: i32) {
        self.queue().push(InputEvent::Preedit {
            text: clip(text, MAX_EVENT_TEXT).to_owned(),
            start,
            length,
        });
    }

    pub fn motion(&self, x: f64, y: f64, dx: f64, dy: f64) {
        self.queue().push(InputEvent::Motion { x, y, dx, dy });
    }

    /// SDL button numbers: 1 left, 2 middle, 3 right, 4 and 5 the side buttons.
    pub fn button(&self, button: u8, pressed: bool) {
        self.queue().button(button, pressed);
    }

    pub fn wheel(&self, dx: f64, dy: f64) {
        if dx.is_finite() && dy.is_finite() && (dx != 0.0 || dy != 0.0) {
            self.queue().push(InputEvent::Wheel { dx, dy });
        }
    }

    pub fn drop_files(&self, paths: Vec<String>) {
        if !paths.is_empty() {
            self.queue().push(InputEvent::Drop(paths));
        }
    }

    pub fn set_focused(&self, focused: bool) {
        self.queue().focused = focused;
    }

    /// Lock key state from the keyboard; later presses toggle from here.
    pub fn set_locks(&self, caps: bool, num: bool) {
        let mut q = self.queue();
        q.locks = if caps { keymod::CAPS } else { 0 } | if num { keymod::NUM } else { 0 };
    }

    /// Key and button releases for everything Java still holds (input moves away from Java).
    pub fn release_all(&self) {
        self.queue().release_all();
    }

    pub fn is_held(&self, scancode: u32) -> bool {
        self.queue().key_down(scancode)
    }

    /// Events dropped because the queue was full or the bridge was not composing.
    pub fn dropped(&self) -> u64 {
        self.queue().dropped
    }

    pub fn queued(&self) -> usize {
        self.queue().events.len()
    }

    pub fn java(&self) -> JavaInput {
        read_java(&self.shared.state)
    }

    /// A new request from Java's "mcr" button since the last call.
    pub fn take_native_request(&self) -> Option<NativeKind> {
        let s = &self.shared.state;
        let seq = s.native_seq.load(Ordering::Acquire);
        let mut q = self.queue();
        if seq == q.native_seen {
            return None;
        }
        q.native_seen = seq;
        NativeKind::from_raw(s.native_kind.load(Ordering::Acquire))
    }

    /// Java's clipboard reads and writes go to `host` from now on.
    pub fn set_clipboard(&self, host: Arc<dyn ClipboardHost>) {
        *lock(&self.shared.clipboard) = host;
    }
}

/// Java asks for the clipboard: the text's byte length is returned, and as much as fits is copied.
pub(crate) extern "C" fn clipboard_get(ptr: i64, cap: i32) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(shared) = crate::abi::shared_now() else {
            return -1;
        };
        let Some(text) = shared.input.clipboard().get() else {
            return -1;
        };
        let text = clip(&text, MAX_CLIPBOARD_BYTES);
        let cap = usize::try_from(cap).unwrap_or(0);
        let n = text.len().min(cap);
        if ptr != 0 && n > 0 {
            // SAFETY: Java passes a live confined-arena segment of at least `cap` bytes for the
            // duration of this downcall, and `n <= cap`.
            let out = unsafe { std::slice::from_raw_parts_mut(ptr as usize as *mut u8, n) };
            out.copy_from_slice(&text.as_bytes()[..n]);
        }
        text.len() as i32
    }))
    .unwrap_or(-1)
}

pub(crate) extern "C" fn clipboard_set(ptr: i64, len: i32) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let Some(shared) = crate::abi::shared_now() else {
            return;
        };
        let len = usize::try_from(len).unwrap_or(0).min(MAX_CLIPBOARD_BYTES);
        if ptr == 0 || len == 0 {
            return;
        }
        // SAFETY: Java passes a live confined-arena segment of at least `len` bytes for the
        // duration of this downcall; `len` was clamped to the clipboard cap.
        let bytes = unsafe { std::slice::from_raw_parts(ptr as usize as *const u8, len) };
        let text = String::from_utf8_lossy(bytes);
        shared.input.clipboard().set(&text);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keycodes_follow_sdl() {
        assert_eq!(sdl_keycode(4, Some('a')), 'a' as i32);
        assert_eq!(sdl_keycode(4, Some('A')), 'a' as i32, "lowercase");
        assert_eq!(sdl_keycode(30, Some('1')), '1' as i32);
        assert_eq!(sdl_keycode(39, us_printable(39)), '0' as i32);
        assert_eq!(sdl_keycode(58, None), SDLK_SCANCODE_MASK | 58, "F1");
        assert_eq!(sdl_keycode(69, None), 0x4000_0045, "F12");
        assert_eq!(
            sdl_keycode(89, Some('1')),
            0x4000_0059,
            "keypad 1 is masked"
        );
        assert_eq!(sdl_keycode(88, None), 0x4000_0058, "keypad enter");
        assert_eq!(sdl_keycode(40, None), 0x0D);
        assert_eq!(sdl_keycode(41, None), 0x1B);
        assert_eq!(sdl_keycode(42, None), 0x08);
        assert_eq!(sdl_keycode(43, None), 0x09);
        assert_eq!(sdl_keycode(44, Some(' ')), 0x20);
        assert_eq!(sdl_keycode(76, None), 0x7F);
        assert_eq!(sdl_keycode(79, None), 1_073_741_903, "right arrow");
        assert_eq!(sdl_keycode(225, None), 0x4000_00E1, "left shift");
        assert_eq!(sdl_keycode(0, Some('a')), 0);
        assert_eq!(sdl_keycode(600, Some('a')), 0, "out of range");
    }

    #[test]
    fn non_us_layouts_use_their_characters() {
        assert_eq!(sdl_keycode(28, Some('z')), 'z' as i32, "QWERTZ Y key");
        assert_eq!(sdl_keycode(51, Some('ö')), 'ö' as i32);
        assert_eq!(sdl_keycode(20, Some('a')), 'a' as i32, "AZERTY Q key");
        assert_eq!(sdl_keycode(47, Some('\u{8}')), SDLK_SCANCODE_MASK | 47);
    }

    #[test]
    fn modifier_sides_become_keymod_bits() {
        let mut q = InputQueue::new();
        q.key(225, None, true, false);
        q.key(230, None, true, false);
        q.key(4, Some('a'), true, false);
        assert_eq!(q.mods(), keymod::LSHIFT | keymod::RALT);
        match q.events.back() {
            Some(InputEvent::Key { mods, action, .. }) => {
                assert_eq!(*mods, keymod::LSHIFT | keymod::RALT);
                assert_eq!(*action, KeyAction::Press);
            }
            other => panic!("{other:?}"),
        }
        q.key(229, None, true, false);
        q.key(225, None, false, false);
        q.key(224, None, true, false);
        q.key(228, None, true, false);
        q.key(227, None, true, false);
        q.key(231, None, true, false);
        q.key(226, None, true, false);
        assert_eq!(
            q.mods(),
            keymod::RSHIFT
                | keymod::LCTRL
                | keymod::RCTRL
                | keymod::LGUI
                | keymod::RGUI
                | keymod::LALT
                | keymod::RALT
        );
    }

    #[test]
    fn lock_keys_toggle_their_modifier_bits() {
        let mut q = InputQueue::new();
        q.key(57, None, true, false);
        q.key(57, None, true, true);
        q.key(57, None, false, false);
        assert_eq!(q.mods(), keymod::CAPS);
        q.key(83, None, true, false);
        q.key(83, None, false, false);
        assert_eq!(q.mods(), keymod::CAPS | keymod::NUM);
        q.key(57, None, true, false);
        q.key(57, None, false, false);
        assert_eq!(q.mods(), keymod::NUM);
        q.release_all();
        assert_eq!(q.mods(), keymod::NUM);
    }

    #[test]
    fn seeded_locks_toggle_from_the_keyboard_state() {
        let shared: &'static InputShared = Box::leak(Box::new(InputShared::new()));
        let input = ModInput::new(shared);
        input.set_locks(false, true);
        assert_eq!(input.queue().mods(), keymod::NUM);
        input.key(83, None, true, false);
        input.key(83, None, false, false);
        input.key(57, None, true, false);
        assert_eq!(input.queue().mods(), keymod::CAPS);
    }

    #[test]
    fn key_state_tracks_presses_repeats_and_releases() {
        let mut q = InputQueue::new();
        q.key(26, Some('w'), true, false);
        q.key(26, Some('w'), true, false);
        q.key(26, Some('w'), true, true);
        q.key(26, Some('w'), false, false);
        q.key(26, Some('w'), false, false);
        q.key(700, Some('w'), true, false);
        let actions: Vec<_> = q
            .events
            .iter()
            .map(|e| match e {
                InputEvent::Key { action, .. } => *action,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            actions,
            [KeyAction::Press, KeyAction::Repeat, KeyAction::Release]
        );
        assert!(!q.key_down(26));
    }

    #[test]
    fn release_all_lets_go_of_keys_and_buttons() {
        let mut q = InputQueue::new();
        q.key(26, Some('w'), true, false);
        q.key(511, None, true, false);
        q.button(1, true);
        q.button(0, true);
        q.button(40, true);
        q.events.clear();
        q.release_all();
        assert_eq!(q.events.len(), 3);
        assert_eq!(q.keys, [0; 8]);
        assert_eq!(q.buttons, 0);
        assert!(matches!(
            q.events.back(),
            Some(InputEvent::Button {
                button: 1,
                pressed: false,
                ..
            })
        ));
    }

    #[test]
    fn motion_coalesces_and_the_queue_is_bounded() {
        let mut q = InputQueue::new();
        q.push(InputEvent::Motion {
            x: 1.0,
            y: 1.0,
            dx: 1.0,
            dy: 0.0,
        });
        q.push(InputEvent::Motion {
            x: 3.0,
            y: 2.0,
            dx: 2.0,
            dy: 1.0,
        });
        assert_eq!(
            q.events.iter().collect::<Vec<_>>(),
            [&InputEvent::Motion {
                x: 3.0,
                y: 2.0,
                dx: 3.0,
                dy: 1.0
            }]
        );
        for _ in 0..MAX_QUEUED * 2 {
            q.push(InputEvent::Wheel { dx: 0.0, dy: 1.0 });
        }
        assert_eq!(q.events.len(), MAX_QUEUED);
        assert_eq!(q.dropped, MAX_QUEUED as u64 + 1);
    }

    fn i32_at(b: &[u8], at: usize) -> i32 {
        i32::from_ne_bytes(b[at..at + 4].try_into().unwrap())
    }

    fn f64_at(b: &[u8], at: usize) -> f64 {
        f64::from_ne_bytes(b[at..at + 8].try_into().unwrap())
    }

    #[test]
    fn events_encode_to_the_java_layout() {
        let mut out = Vec::new();
        InputEvent::Key {
            scancode: 8,
            keycode: 'e' as i32,
            mods: keymod::LCTRL,
            action: KeyAction::Repeat,
        }
        .encode(&mut out);
        assert_eq!(out.len(), IN_TEXT_DATA);
        assert_eq!(
            [
                i32_at(&out, IN_TYPE),
                i32_at(&out, IN_A),
                i32_at(&out, IN_B),
                i32_at(&out, IN_C),
                i32_at(&out, IN_D)
            ],
            [IN_KEY as i32, 8, 'e' as i32, 0x40, -1]
        );
        InputEvent::Motion {
            x: 10.5,
            y: f64::NAN,
            dx: f64::INFINITY,
            dy: -2.0,
        }
        .encode(&mut out);
        assert_eq!(f64_at(&out, IN_X), 10.5);
        assert_eq!(f64_at(&out, IN_Y), 0.0);
        assert_eq!(f64_at(&out, IN_DX), 0.0);
        assert_eq!(f64_at(&out, IN_DY), -2.0);
        InputEvent::Text("hé".into()).encode(&mut out);
        assert_eq!(i32_at(&out, IN_D), 3);
        assert_eq!(&out[IN_TEXT_DATA..], "hé".as_bytes());
        InputEvent::Preedit {
            text: "かな".into(),
            start: 1,
            length: 1,
        }
        .encode(&mut out);
        assert_eq!(
            [
                i32_at(&out, IN_TYPE),
                i32_at(&out, IN_A),
                i32_at(&out, IN_B)
            ],
            [IN_PREEDIT as i32, 1, 1]
        );
        let long = "é".repeat(MAX_EVENT_TEXT);
        InputEvent::Text(long).encode(&mut out);
        assert!(out.len() - IN_TEXT_DATA <= MAX_EVENT_TEXT);
        assert!(std::str::from_utf8(&out[IN_TEXT_DATA..]).is_ok());
        InputEvent::Drop(vec!["/a".into(), "b\0c".into(), "/d".into()]).encode(&mut out);
        assert_eq!(&out[IN_TEXT_DATA..], b"/a\0/d\0");
        let many: Vec<String> = (0..1000).map(|i| format!("/{i}")).collect();
        InputEvent::Drop(many).encode(&mut out);
        let count = out[IN_TEXT_DATA..].iter().filter(|&&b| b == 0).count();
        assert_eq!(count, MAX_DROP_PATHS);
    }

    #[test]
    fn flush_publishes_key_state_and_keeps_what_does_not_fit() {
        let shared: &'static InputShared = Box::leak(Box::new(InputShared::new()));
        let ring = mcr_jvm::Ring::new(4096).unwrap();
        let mut writer = ring.writer().unwrap();
        let mut reader = ring.reader().unwrap();
        let input = ModInput::new(shared);
        input.key(225, None, true, false);
        input.key(4, Some('a'), true, false);
        for i in 0..200 {
            input.text(&format!("chunk {i}"));
        }
        input.set_focused(false);
        let pushed = input.queue().flush(&mut writer, shared);
        assert!(pushed > 2 && pushed < 202, "{pushed}");
        assert_eq!(input.queued(), 202 - pushed);
        let s = &shared.state;
        assert_eq!(s.keys[0].load(Ordering::Acquire), 1 << 4);
        assert_eq!(s.keys[3].load(Ordering::Acquire), 1 << (225 - 192));
        assert_eq!(s.mods.load(Ordering::Acquire), u32::from(keymod::LSHIFT));
        assert_eq!(s.rust_flags.load(Ordering::Acquire), 0);
        let first = reader.try_pop().unwrap().unwrap();
        assert_eq!(first.kind(), MSG_INPUT);
        assert_eq!(i32_at(first.payload(), IN_A), 225);
    }

    #[test]
    fn java_state_is_validated() {
        let shared: &'static InputShared = Box::leak(Box::new(InputShared::new()));
        let input = ModInput::new(shared);
        assert_eq!(input.java(), JavaInput::default());
        shared.simulate_java(JAVA_SCREEN | JAVA_GRABBED | JAVA_QUIT, 2, 3);
        let j = input.java();
        assert!(j.screen && j.grabbed && j.quit && !j.overlay && !j.text_input);
        assert!(j.captures_input());
        assert_eq!((j.cursor, j.gui_scale), (CursorKind::IBeam, 3));
        shared.simulate_java(0, 99, 1000);
        let j = input.java();
        assert_eq!((j.cursor, j.gui_scale), (CursorKind::Default, 0));
        for (i, v) in [10, 20, 300, 18].into_iter().enumerate() {
            shared.state.text_area[i].store(v, Ordering::Release);
        }
        assert_eq!(input.java().text_area, Some([10, 20, 300, 18]));
        shared.state.text_area[2].store(-5, Ordering::Release);
        assert_eq!(input.java().text_area, None);
        shared.state.screen_moves.store(u32::MAX, Ordering::Release);
        assert_eq!(
            input.java().screen_moves,
            0,
            "no screen, no screen movement"
        );
        shared.simulate_java(JAVA_SCREEN, 0, 2);
        assert_eq!(
            input.java().screen_moves,
            MOVE_ALL,
            "unknown bits are dropped"
        );
    }

    #[test]
    fn native_requests_fire_once_per_press() {
        let shared: &'static InputShared = Box::leak(Box::new(InputShared::new()));
        let input = ModInput::new(shared);
        assert_eq!(input.take_native_request(), None);
        shared.simulate_native(1, 0);
        assert_eq!(input.take_native_request(), Some(NativeKind::Menu));
        assert_eq!(input.take_native_request(), None);
        shared.simulate_native(2, 2);
        assert_eq!(input.take_native_request(), Some(NativeKind::Mods));
        shared.simulate_native(3, 77);
        assert_eq!(
            input.take_native_request(),
            None,
            "unknown kinds are ignored"
        );
    }

    #[test]
    fn clipboard_downcalls_bound_and_tolerate_bad_pointers() {
        let shared = crate::abi::shared().unwrap();
        let input = ModInput::new(&shared.input);
        input.set_clipboard(Arc::new(MemoryClipboard::default()));
        let text = "hello";
        clipboard_set(text.as_ptr() as i64, text.len() as i32);
        let mut buf = [0u8; 3];
        assert_eq!(clipboard_get(buf.as_mut_ptr() as i64, 3), 5);
        assert_eq!(&buf, b"hel");
        assert_eq!(
            clipboard_get(0, 100),
            5,
            "a null buffer only asks the length"
        );
        assert_eq!(clipboard_get(buf.as_mut_ptr() as i64, -1), 5);
        clipboard_set(0, 10);
        clipboard_set(text.as_ptr() as i64, -4);
        let mut big = vec![0u8; 16];
        assert_eq!(clipboard_get(big.as_mut_ptr() as i64, 16), 5);
        assert_eq!(&big[..5], b"hello");
        let bad = [0xffu8, b'a'];
        clipboard_set(bad.as_ptr() as i64, 2);
        assert_eq!(shared.input.clipboard().get().unwrap(), "\u{fffd}a");
    }
}
