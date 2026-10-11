use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use mcr_jvm::{Doorbell, EntryDesc, ExtensionTable, Flavour, Ring, RingError};

use crate::events::BridgeEvent;

/// Bump on any change to [`ENTRIES`] or the message layouts; Java refuses a mismatched table.
pub const BRIDGE_VERSION: u32 = 1;
pub const TABLE_PROPERTY: &str = "mcr.bridge.table";

pub const SHARED_CONTROL_RING: i32 = 0;
pub const SHARED_CONTROL_BELL: i32 = 1;

pub const MSG_FRAME: u16 = 1;
pub const MSG_SHUTDOWN: u16 = 2;

pub const FRAME_INDEX: usize = 0;
pub const FRAME_TICKS: usize = 8;
pub const FRAME_WIDTH: usize = 12;
pub const FRAME_HEIGHT: usize = 16;
pub const FRAME_PARTIAL_TICK: usize = 20;
pub const FRAME_SCALE: usize = 24;
pub const FRAME_SIZE: usize = 32;

pub const REPORT_MOD_LOADED: i32 = 1;
pub const REPORT_INIT: i32 = 2;
pub const REPORT_SCREEN: i32 = 3;
pub const REPORT_CRASH: i32 = 4;
pub const REPORT_INFO: i32 = 5;
pub const REPORT_CONNECT: i32 = 6;
pub const REPORT_STOPPING: i32 = 7;

/// Java caps reports at this size too; anything longer is truncated there.
pub const MAX_REPORT_BYTES: usize = 16 << 10;
const CONTROL_RING_CAPACITY: usize = 64 << 10;
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
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub index: u64,
    pub ticks_to_do: u32,
    pub width: u32,
    pub height: u32,
    pub partial_tick: f32,
    pub scale: f32,
}

impl Frame {
    pub fn encode(&self) -> [u8; FRAME_SIZE] {
        let mut out = [0u8; FRAME_SIZE];
        out[FRAME_INDEX..FRAME_INDEX + 8].copy_from_slice(&self.index.to_ne_bytes());
        out[FRAME_TICKS..FRAME_TICKS + 4].copy_from_slice(&self.ticks_to_do.to_ne_bytes());
        out[FRAME_WIDTH..FRAME_WIDTH + 4].copy_from_slice(&self.width.to_ne_bytes());
        out[FRAME_HEIGHT..FRAME_HEIGHT + 4].copy_from_slice(&self.height.to_ne_bytes());
        out[FRAME_PARTIAL_TICK..FRAME_PARTIAL_TICK + 4]
            .copy_from_slice(&self.partial_tick.to_ne_bytes());
        out[FRAME_SCALE..FRAME_SCALE + 4].copy_from_slice(&self.scale.to_ne_bytes());
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
}

static SHARED: OnceLock<Shared> = OnceLock::new();
static TABLE: OnceLock<&'static ExtensionTable> = OnceLock::new();

pub(crate) fn shared() -> Result<&'static Shared, RingError> {
    if let Some(s) = SHARED.get() {
        return Ok(s);
    }
    let control = Ring::new(CONTROL_RING_CAPACITY)?;
    Ok(SHARED.get_or_init(|| Shared {
        control,
        bell: Doorbell::new(),
        events: Mutex::new(None),
        crashed: AtomicBool::new(false),
        dropped_events: AtomicU64::new(0),
    }))
}

pub(crate) fn table() -> &'static ExtensionTable {
    TABLE.get_or_init(|| {
        let fns: [*const c_void; 2] = [shared_address as *const c_void, report as *const c_void];
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
        }
        emit(event);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
            ("MSG_FRAME", MSG_FRAME.into()),
            ("MSG_SHUTDOWN", MSG_SHUTDOWN.into()),
            ("FRAME_INDEX", FRAME_INDEX as i64),
            ("FRAME_TICKS", FRAME_TICKS as i64),
            ("FRAME_WIDTH", FRAME_WIDTH as i64),
            ("FRAME_HEIGHT", FRAME_HEIGHT as i64),
            ("FRAME_PARTIAL_TICK", FRAME_PARTIAL_TICK as i64),
            ("FRAME_SCALE", FRAME_SCALE as i64),
            ("FRAME_SIZE", FRAME_SIZE as i64),
            ("REPORT_MOD_LOADED", REPORT_MOD_LOADED.into()),
            ("REPORT_INIT", REPORT_INIT.into()),
            ("REPORT_SCREEN", REPORT_SCREEN.into()),
            ("REPORT_CRASH", REPORT_CRASH.into()),
            ("REPORT_INFO", REPORT_INFO.into()),
            ("REPORT_CONNECT", REPORT_CONNECT.into()),
            ("REPORT_STOPPING", REPORT_STOPPING.into()),
        ];
        for (name, value) in ints {
            assert_eq!(int_const(&src, name), *value, "{name}");
        }
        assert!(src.contains(&format!("TABLE_PROPERTY = \"{TABLE_PROPERTY}\"")));
    }

    #[test]
    fn frame_layout_round_trips() {
        let f = Frame {
            index: 0x0102_0304_0506_0708,
            ticks_to_do: 3,
            width: 854,
            height: 480,
            partial_tick: 0.25,
            scale: 2.0,
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
        *shared().unwrap().events.lock().unwrap() = None;
    }
}
