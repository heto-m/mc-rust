//! Java's world screens and mcr's local world sessions: requests from Java, progress back to it.

use std::collections::VecDeque;

use std::path::PathBuf;

use crate::abi::{WORLD_EULA, WORLD_FAILED, WORLD_LAN, WORLD_READY, WORLD_STATUS, WORLD_STOPPED};

/// Longest name, seed or folder accepted from Java.
pub const MAX_FIELD: usize = 256;
/// Longest superflat settings JSON accepted from Java.
pub const MAX_FLAT: usize = 8 * 1024;
/// Longest temporary data pack folder path accepted from Java.
pub const MAX_PACK_DIR: usize = 4 * 1024;
const MAX_RULES: usize = 128;
const MAX_PACKS: usize = 64;
/// Longest text sent to Java with one world message.
pub const MAX_TEXT: usize = 4096;
/// World messages waiting for Java's next frame.
const MAX_QUEUED: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewMode {
    Survival,
    Hardcore,
    Creative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewPreset {
    Normal,
    Flat,
    LargeBiomes,
    Amplified,
}

/// Java's Create New World options that mcr can create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWorldRequest {
    pub name: String,
    pub mode: NewMode,
    /// 0 peaceful to 3 hard.
    pub difficulty: u8,
    pub cheats: bool,
    pub preset: NewPreset,
    pub seed: String,
    pub structures: bool,
    pub bonus_chest: bool,
    /// Changed game rules as `(id, value)`, unvalidated.
    pub game_rules: Vec<(String, String)>,
    pub experiments: Vec<String>,
    /// Enabled data packs from Java's pack folder (the names after `file/`).
    pub packs: Vec<String>,
    /// Java's temporary pack folder, absolute.
    pub pack_dir: Option<PathBuf>,
    /// Superflat settings as the generator JSON.
    pub flat: Option<String>,
}

/// Open to LAN's choices from Java; `mode` is the game mode id (0 survival to 3 spectator).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanRequest {
    pub mode: u8,
    pub cheats: bool,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldRequest {
    /// A folder directly inside the saves directory.
    Play(String),
    Create(NewWorldRequest),
    Eula(bool),
    Lan(LanRequest),
    Cancel,
}

fn field(s: &str) -> Option<String> {
    (s.len() <= MAX_FIELD && !s.chars().any(char::is_control)).then(|| s.to_owned())
}

fn folder(s: &str) -> Option<String> {
    let ok = !s.is_empty()
        && s != "."
        && s != ".."
        && !s.contains(['/', '\\', '\0'])
        && !s.chars().any(char::is_control);
    if ok { field(s) } else { None }
}

fn flag(s: &str) -> Option<bool> {
    match s {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

fn parse_create(lines: std::str::Lines<'_>) -> Option<NewWorldRequest> {
    let mut w = NewWorldRequest {
        name: String::new(),
        mode: NewMode::Survival,
        difficulty: 2,
        cheats: false,
        preset: NewPreset::Normal,
        seed: String::new(),
        structures: true,
        bonus_chest: false,
        game_rules: Vec::new(),
        experiments: Vec::new(),
        packs: Vec::new(),
        pack_dir: None,
        flat: None,
    };
    let mut named = false;
    for line in lines.take(32 + MAX_RULES) {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "name" => {
                w.name = field(value.trim())?;
                named = !w.name.is_empty();
            }
            "mode" => {
                w.mode = match value {
                    "survival" => NewMode::Survival,
                    "hardcore" => NewMode::Hardcore,
                    "creative" => NewMode::Creative,
                    _ => return None,
                }
            }
            "difficulty" => w.difficulty = value.parse().ok().filter(|d| *d <= 3)?,
            "cheats" => w.cheats = flag(value)?,
            "preset" => {
                w.preset = match value {
                    "normal" => NewPreset::Normal,
                    "flat" => NewPreset::Flat,
                    "large_biomes" => NewPreset::LargeBiomes,
                    "amplified" => NewPreset::Amplified,
                    _ => return None,
                }
            }
            "seed" => w.seed = field(value)?,
            "structures" => w.structures = flag(value)?,
            "bonus" => w.bonus_chest = flag(value)?,
            "experiments" => w.experiments = list(value)?,
            "packs" => {
                w.packs = list(value)?
                    .into_iter()
                    .filter_map(|p| p.strip_prefix("file/").and_then(folder))
                    .collect();
            }
            "packdir" => {
                let path = PathBuf::from(value);
                let ok = value.len() <= MAX_PACK_DIR
                    && path.is_absolute()
                    && !value.chars().any(char::is_control);
                w.pack_dir = ok.then_some(path);
            }
            "flat" => {
                w.flat = (value.len() <= MAX_FLAT && !value.chars().any(char::is_control))
                    .then(|| value.to_owned());
            }
            _ => {
                if let Some(rule) = key.strip_prefix("rule.")
                    && w.game_rules.len() < MAX_RULES
                {
                    w.game_rules.push((field(rule)?, field(value)?));
                }
            }
        }
    }
    named.then_some(w)
}

fn list(value: &str) -> Option<Vec<String>> {
    value
        .split(',')
        .filter(|v| !v.is_empty())
        .take(MAX_PACKS)
        .map(field)
        .collect()
}

fn parse_lan(arg: &str) -> Option<LanRequest> {
    let mut lan = LanRequest {
        mode: 0,
        cheats: false,
        port: None,
    };
    for part in arg.split('\t').take(8) {
        let (key, value) = part.split_once('=')?;
        match key {
            "mode" => lan.mode = value.parse().ok().filter(|m| *m <= 3)?,
            "cheats" => lan.cheats = flag(value)?,
            "port" => {
                lan.port = match value.parse::<u16>().ok()? {
                    0 => None,
                    p if p >= 1024 => Some(p),
                    _ => return None,
                }
            }
            _ => {}
        }
    }
    Some(lan)
}

impl WorldRequest {
    /// `None` for anything malformed; Java's screen then just waits for a reply that never comes.
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        let head = lines.next()?;
        let (verb, arg) = head.split_once('\t').unwrap_or((head, ""));
        match verb {
            "play" => folder(arg).map(Self::Play),
            "create" => parse_create(lines).map(Self::Create),
            "eula" => flag(arg).map(Self::Eula),
            "lan" => parse_lan(arg).map(Self::Lan),
            "cancel" => Some(Self::Cancel),
            _ => None,
        }
    }
}

/// What Java's world screens hear about the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldEvent {
    /// A progress line, plain or `lang:key|arg`.
    Status(String),
    Eula,
    /// The server is up and Rust is connecting.
    Ready,
    Failed(String),
    /// The server exited; the reason is set when nobody asked it to stop.
    Stopped(Option<String>),
    /// Open to LAN's outcome: the port, or why it failed.
    Lan(Result<u16, String>),
}

fn clipped(text: &str) -> &str {
    let mut end = text.len().min(MAX_TEXT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

impl WorldEvent {
    pub fn encode(&self) -> Vec<u8> {
        let (kind, text) = match self {
            Self::Status(t) => (WORLD_STATUS, t.as_str()),
            Self::Eula => (WORLD_EULA, ""),
            Self::Ready => (WORLD_READY, ""),
            Self::Failed(t) => (WORLD_FAILED, t.as_str()),
            Self::Stopped(t) => (WORLD_STOPPED, t.as_deref().unwrap_or("")),
            Self::Lan(r) => {
                let text = match r {
                    Ok(port) => format!("ok\t{port}"),
                    Err(e) => format!("failed\t{e}"),
                };
                return encoded(WORLD_LAN, &text);
            }
        };
        encoded(kind, text)
    }
}

fn encoded(kind: u32, text: &str) -> Vec<u8> {
    let text = clipped(text);
    let mut out = Vec::with_capacity(4 + text.len());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    out
}

/// World messages in order; a status line replaces a status line still waiting.
#[derive(Debug, Default)]
pub(crate) struct WorldQueue(VecDeque<WorldEvent>);

impl WorldQueue {
    pub fn push(&mut self, event: WorldEvent) {
        if let (WorldEvent::Status(_), Some(WorldEvent::Status(_))) = (&event, self.0.back()) {
            self.0.pop_back();
        }
        if self.0.len() >= MAX_QUEUED {
            // Keep the newest messages: Java only needs the latest state of a stuck session.
            self.0.pop_front();
        }
        self.0.push_back(event);
    }

    pub fn front(&self) -> Option<&WorldEvent> {
        self.0.front()
    }

    pub fn pop(&mut self) {
        self.0.pop_front();
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_requests() {
        assert_eq!(
            WorldRequest::parse("play\tNew World (1)"),
            Some(WorldRequest::Play("New World (1)".into()))
        );
        assert_eq!(WorldRequest::parse("play\t../x"), None);
        assert_eq!(WorldRequest::parse("play\t.."), None);
        assert_eq!(WorldRequest::parse("play\t"), None);
        assert_eq!(WorldRequest::parse("play\ta\u{7}b"), None);
        assert_eq!(
            WorldRequest::parse(&format!("play\t{}", "a".repeat(300))),
            None
        );
        assert_eq!(
            WorldRequest::parse("eula\t1"),
            Some(WorldRequest::Eula(true))
        );
        assert_eq!(
            WorldRequest::parse("eula\t0"),
            Some(WorldRequest::Eula(false))
        );
        assert_eq!(WorldRequest::parse("eula\tyes"), None);
        assert_eq!(WorldRequest::parse("cancel"), Some(WorldRequest::Cancel));
        assert_eq!(WorldRequest::parse("bogus"), None);
        assert_eq!(WorldRequest::parse(""), None);
    }

    #[test]
    fn parses_create() {
        let text = "create\nname= Flat \nmode=creative\ndifficulty=1\ncheats=1\npreset=flat\nseed=-42\nstructures=0\nbonus=1\nfuture=x";
        assert_eq!(
            WorldRequest::parse(text),
            Some(WorldRequest::Create(NewWorldRequest {
                name: "Flat".into(),
                mode: NewMode::Creative,
                difficulty: 1,
                cheats: true,
                preset: NewPreset::Flat,
                seed: "-42".into(),
                structures: false,
                bonus_chest: true,
                game_rules: Vec::new(),
                experiments: Vec::new(),
                packs: Vec::new(),
                pack_dir: None,
                flat: None,
            }))
        );
        assert_eq!(
            WorldRequest::parse("create\nmode=survival"),
            None,
            "a name is required"
        );
        assert_eq!(WorldRequest::parse("create\nname=a\nmode=debug"), None);
        assert_eq!(
            WorldRequest::parse("create\nname=a\npreset=single_biome"),
            None
        );
        assert_eq!(WorldRequest::parse("create\nname=a\ndifficulty=9"), None);
        let Some(WorldRequest::Create(w)) = WorldRequest::parse("create\nname=a") else {
            panic!("defaults");
        };
        assert_eq!(
            (w.mode, w.difficulty, w.structures),
            (NewMode::Survival, 2, true)
        );
    }

    #[test]
    fn parses_create_extras() {
        let text = "create\nname=W\nrule.keep_inventory=true\nrule.minecraft:advance_time=false\nexperiments=trade_rebalance,x\npacks=vanilla,file/a.zip,file/../b,fabric,file/Folder\npackdir=/tmp/mcworld-1\nflat={\"layers\":[]}";
        let Some(WorldRequest::Create(w)) = WorldRequest::parse(text) else {
            panic!("create");
        };
        assert_eq!(
            w.game_rules,
            [
                ("keep_inventory".to_owned(), "true".to_owned()),
                ("minecraft:advance_time".to_owned(), "false".to_owned())
            ]
        );
        assert_eq!(w.experiments, ["trade_rebalance", "x"]);
        assert_eq!(w.packs, ["a.zip", "Folder"]);
        assert_eq!(
            w.pack_dir.as_deref(),
            Some(std::path::Path::new("/tmp/mcworld-1"))
        );
        assert_eq!(w.flat.as_deref(), Some("{\"layers\":[]}"));
        let Some(WorldRequest::Create(w)) =
            WorldRequest::parse("create\nname=W\npackdir=relative/dir")
        else {
            panic!("create");
        };
        assert_eq!(w.pack_dir, None, "only absolute folders");
        let many: String = (0..500).map(|i| format!("rule.r{i}=1\n")).collect();
        let Some(WorldRequest::Create(w)) = WorldRequest::parse(&format!("create\nname=W\n{many}"))
        else {
            panic!("create");
        };
        assert!(w.game_rules.len() <= MAX_RULES);
        let long_flat = format!("create\nname=W\nflat={}", "x".repeat(MAX_FLAT + 1));
        let Some(WorldRequest::Create(w)) = WorldRequest::parse(&long_flat) else {
            panic!("create");
        };
        assert_eq!(w.flat, None);
    }

    #[test]
    fn parses_lan() {
        assert_eq!(
            WorldRequest::parse("lan\tmode=1\tcheats=1\tport=25601"),
            Some(WorldRequest::Lan(LanRequest {
                mode: 1,
                cheats: true,
                port: Some(25601)
            }))
        );
        assert_eq!(
            WorldRequest::parse("lan\tmode=0\tcheats=0\tport=0"),
            Some(WorldRequest::Lan(LanRequest {
                mode: 0,
                cheats: false,
                port: None
            }))
        );
        for bad in [
            "lan\tmode=4",
            "lan\tport=80",
            "lan\tport=x",
            "lan\tcheats=2",
            "lan\tjunk",
        ] {
            assert_eq!(WorldRequest::parse(bad), None, "{bad}");
        }
        let ok = WorldEvent::Lan(Ok(25601)).encode();
        assert_eq!(&ok[..4], &WORLD_LAN.to_le_bytes());
        assert_eq!(&ok[4..], b"ok\t25601");
        let failed = WorldEvent::Lan(Err("busy".into())).encode();
        assert_eq!(&failed[4..], b"failed\tbusy");
    }

    #[test]
    fn events_encode_bounded() {
        let e = WorldEvent::Failed("x".into()).encode();
        assert_eq!(&e[..4], &WORLD_FAILED.to_le_bytes());
        assert_eq!(&e[4..], b"x");
        assert_eq!(WorldEvent::Stopped(None).encode().len(), 4);
        let long = "é".repeat(MAX_TEXT);
        let e = WorldEvent::Status(long).encode();
        assert!(e.len() <= 4 + MAX_TEXT);
        assert!(std::str::from_utf8(&e[4..]).is_ok());
    }

    #[test]
    fn queue_coalesces_status_and_stays_bounded() {
        let mut q = WorldQueue::default();
        q.push(WorldEvent::Status("a".into()));
        q.push(WorldEvent::Status("b".into()));
        q.push(WorldEvent::Ready);
        q.push(WorldEvent::Status("c".into()));
        assert_eq!(q.len(), 3);
        assert_eq!(q.front(), Some(&WorldEvent::Status("b".into())));
        for _ in 0..100 {
            q.push(WorldEvent::Eula);
        }
        assert_eq!(q.len(), MAX_QUEUED);
    }
}
