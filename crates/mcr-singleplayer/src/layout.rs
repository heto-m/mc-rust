use std::path::{Path, PathBuf};

/// The game version whose server jar singleplayer runs.
pub const VERSION: &str = "26.3";
/// Per-world directory holding everything the server writes besides the world itself.
pub const SERVER_DIR: &str = "mcr-server";
/// At most this many worlds are listed.
pub const MAX_FOLDER_LISTING: usize = 1000;
const MAX_FOLDER_CHARS: usize = 200;
const MAX_NAME_ATTEMPTS: u32 = 10_000;
const ILLEGAL: &[char] = &[
    '/', '\n', '\r', '\t', '\0', '\u{c}', '`', '?', '*', '\\', '<', '>', '|', '"', ':', '.',
];
const RESERVED: &[&str] = &["CON", "PRN", "AUX", "NUL", "CLOCK$"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    game_dir: PathBuf,
}

impl Layout {
    pub fn new(game_dir: impl Into<PathBuf>) -> Self {
        Self {
            game_dir: game_dir.into(),
        }
    }

    pub fn game_dir(&self) -> &Path {
        &self.game_dir
    }

    pub fn saves(&self) -> PathBuf {
        self.game_dir.join("saves")
    }

    /// `None` unless `folder` is a single plain path component.
    pub fn world(&self, folder: &str) -> Option<PathBuf> {
        valid_folder(folder).then(|| self.saves().join(folder))
    }

    pub fn server_dir(&self, folder: &str) -> Option<PathBuf> {
        Some(self.world(folder)?.join(SERVER_DIR))
    }

    pub fn version_dir(&self) -> PathBuf {
        self.game_dir.join("versions").join(VERSION)
    }

    pub fn server_jar(&self) -> PathBuf {
        self.version_dir().join("server.jar")
    }

    pub fn version_json(&self) -> PathBuf {
        self.version_dir().join("version.json")
    }

    /// Where the bundler jar unpacks its libraries (`-DbundlerRepoDir`), shared by every world.
    pub fn bundle_dir(&self) -> PathBuf {
        self.version_dir().join("bundle")
    }

    pub fn runtime_dir(&self, component: &str) -> Option<PathBuf> {
        valid_folder(component).then(|| self.game_dir.join("runtime").join(component))
    }

    /// The player's own EULA agreement, asked once before the first local server.
    pub fn eula_file(&self) -> PathBuf {
        self.game_dir.join("eula.txt")
    }

    pub fn eula_accepted(&self) -> bool {
        std::fs::read_to_string(self.eula_file())
            .is_ok_and(|t| t.lines().any(|l| l.trim() == "eula=true"))
    }

    pub fn accept_eula(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.game_dir)?;
        std::fs::write(
            self.eula_file(),
            "# Agreed in mcr to the Minecraft EULA (https://aka.ms/MinecraftEULA).\neula=true\n",
        )
    }
}

pub fn valid_folder(folder: &str) -> bool {
    !folder.is_empty()
        && folder.len() <= 255
        && folder != "."
        && folder != ".."
        && !folder.contains(['/', '\\', '\0'])
        && Path::new(folder).components().count() == 1
}

fn reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    RESERVED.contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

/// A folder name for a new world called `name`, like vanilla: unsafe characters become `_`, Windows
/// device names are wrapped in `_`, and ` (n)` is appended while `taken` says the name exists.
pub fn folder_for(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut base: String = name
        .trim()
        .chars()
        .map(|c| {
            if ILLEGAL.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .take(MAX_FOLDER_CHARS)
        .collect();
    if base.trim().is_empty() {
        base = "World".into();
    }
    if reserved(&base) {
        base = format!("_{base}_");
    }
    let mut count = 0u32;
    if let Some((stem, rest)) = base.rsplit_once(" (")
        && let Some(digits) = rest.strip_suffix(')')
        && !digits.is_empty()
        && digits.len() <= 4
        && digits.bytes().all(|b| b.is_ascii_digit())
    {
        count = digits.parse().unwrap_or(0);
        base = stem.to_owned();
    }
    for _ in 0..MAX_NAME_ATTEMPTS {
        let candidate = if count == 0 {
            base.clone()
        } else {
            format!("{base} ({count})")
        };
        if !taken(&candidate) {
            return candidate;
        }
        count += 1;
    }
    format!("{base} ({count})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_must_be_one_component() {
        for ok in ["New World", "a.b", "x (1)"] {
            assert!(valid_folder(ok), "{ok}");
        }
        for bad in ["", ".", "..", "a/b", "a\\b", "/abs", "a\0"] {
            assert!(!valid_folder(bad), "{bad:?}");
        }
        let layout = Layout::new("g");
        assert!(layout.world("../x").is_none());
        assert_eq!(
            layout.server_dir("W").unwrap(),
            Path::new("g/saves/W/mcr-server")
        );
    }

    #[test]
    fn folder_names_follow_vanilla() {
        let none = |_: &str| false;
        assert_eq!(folder_for("New World", none), "New World");
        assert_eq!(folder_for("a/b:c?.d", none), "a_b_c__d");
        assert_eq!(folder_for("  ", none), "World");
        assert_eq!(folder_for("con", none), "_con_");
        assert_eq!(folder_for("COM3", none), "_COM3_");
        assert_eq!(folder_for("COM0", none), "COM0");
        let taken = |n: &str| n == "New World" || n == "New World (1)";
        assert_eq!(folder_for("New World", taken), "New World (2)");
        assert_eq!(folder_for("New World (1)", taken), "New World (2)");
        assert_eq!(folder_for(&"x".repeat(300), none).chars().count(), 200);
        for hostile in [
            "..",
            ".",
            "../x",
            "a/../../b",
            "..\\..\\c",
            "/etc",
            "C:\\x",
            "a\0b",
        ] {
            let folder = folder_for(hostile, none);
            assert!(valid_folder(&folder), "{hostile:?} -> {folder:?}");
            assert!(
                !folder.contains(['/', '\\', '.']),
                "{hostile:?} -> {folder:?}"
            );
        }
    }

    #[test]
    fn eula_round_trip() {
        let dir = std::env::temp_dir().join(format!("mcr-sp-eula-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let layout = Layout::new(&dir);
        assert!(!layout.eula_accepted());
        layout.accept_eula().unwrap();
        assert!(layout.eula_accepted());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
