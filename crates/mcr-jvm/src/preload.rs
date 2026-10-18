//! The classes a session loaded, recorded per JVM configuration so the next session can load them early on
//! background threads. HotSpot's class-load log records the names; marker classes that Java loads at known
//! milestones split them into segments, and a class is only preloaded once its segment's milestone is reached.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// `MCR_JVM_PRELOAD=off` neither records nor preloads; `on` preloads where the default would not.
pub const ENV: &str = "MCR_JVM_PRELOAD";
pub const HEADER: &str = "mcr-jvm-preload 1";
pub const MAX_CLASSES: usize = 200_000;
pub const MAX_NAME: usize = 512;
pub const MAX_SEGMENTS: usize = 16;
/// HotSpot's log of a long session; anything past this is not read.
pub const MAX_LOG_BYTES: u64 = 64 << 20;
/// Preloading sessions that may end badly (crash, hang, kill) before the key stops preloading.
pub const MAX_STRIKES: u32 = 2;
/// Class lists kept for other keys (the least recently used go).
pub const MAX_LISTS: usize = 4;
/// Packages the JDK's own loaders define; preloading them gains nothing.
const JDK_PACKAGES: [&str; 6] = ["java.", "javax.", "jdk.", "sun.", "com.sun.", "org.w3c."];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreloadPlan {
    /// Record this session's class loads into `log`; preload from `list` when given.
    On {
        log: PathBuf,
        list: Option<PathBuf>,
    },
    Off(&'static str),
}

impl PreloadPlan {
    /// The HotSpot logging option for [`PreloadPlan::On`].
    pub fn options(&self) -> Vec<String> {
        match self {
            Self::On { log, .. } => vec![format!(
                "-Xlog:class+load=info:file=\"{}\":none:filecount=0",
                log.display()
            )],
            Self::Off(_) => Vec::new(),
        }
    }

    pub fn list(&self) -> Option<&Path> {
        match self {
            Self::On { list, .. } => list.as_deref(),
            Self::Off(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassList {
    dir: PathBuf,
    key: String,
    /// Binary name prefix of the marker classes; the marker for milestone `k` is `<prefix><k>`.
    marker: String,
}

impl ClassList {
    /// `key` names the JVM configuration, as [`crate::aot::AotCache::key`] does.
    pub fn new(dir: impl Into<PathBuf>, key: &str, marker: &str) -> Self {
        Self {
            dir: dir.into(),
            key: key.to_owned(),
            marker: marker.to_owned(),
        }
    }

    fn file(&self, ext: &str) -> PathBuf {
        self.dir.join(format!("{}.{ext}", self.key))
    }

    pub fn list_path(&self) -> PathBuf {
        self.file("classes")
    }

    pub fn log_path(&self) -> PathBuf {
        self.file(&format!("{}.classlog", std::process::id()))
    }

    /// `by_default` is what happens when [`ENV`] is not set; lists are recorded either way.
    pub fn plan(&self, by_default: bool) -> PreloadPlan {
        let preload = match std::env::var(ENV) {
            Ok(v) if v.eq_ignore_ascii_case("off") || v == "0" => {
                return PreloadPlan::Off("disabled by MCR_JVM_PRELOAD");
            }
            Ok(v) if v.eq_ignore_ascii_case("on") || v == "1" => true,
            _ => by_default,
        };
        if self.strikes() >= MAX_STRIKES {
            return PreloadPlan::Off("sessions that preloaded failed too often");
        }
        let list = self.list_path();
        PreloadPlan::On {
            log: self.log_path(),
            list: (preload && list.is_file()).then_some(list),
        }
    }

    /// Prepares the directory, drops logs that dead sessions left, and counts a preloading session as a
    /// strike until [`ClassList::finish`] clears it, so a session that hangs or is killed counts too.
    pub fn begin(&self, plan: &PreloadPlan) -> io::Result<()> {
        let PreloadPlan::On { log, list } = plan else {
            return Ok(());
        };
        fs::create_dir_all(&self.dir)?;
        let prefix = format!("{}.", self.key);
        for entry in fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".classlog"))
            {
                remove(&path)?;
            }
        }
        remove(log)?;
        if let Some(list) = list {
            let _ = File::options()
                .write(true)
                .open(list)
                .and_then(|f| f.set_modified(SystemTime::now()));
            fs::write(self.file("strikes"), (self.strikes() + 1).to_string())?;
        }
        Ok(())
    }

    /// Merges this session's log into the list; a `good` session clears its strike. Returns the number of
    /// classes in the list.
    pub fn finish(&self, plan: &PreloadPlan, good: bool) -> io::Result<usize> {
        let PreloadPlan::On { log, list } = plan else {
            return Ok(0);
        };
        if list.is_some() && good {
            remove(&self.file("strikes"))?;
        }
        let session = match File::open(log) {
            Ok(f) => parse_log(BufReader::new(f.take(MAX_LOG_BYTES)), &self.marker)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        remove(log)?;
        let old = match fs::read_to_string(self.list_path()) {
            Ok(text) => parse_list(&text).unwrap_or_default(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        if session.is_empty() {
            return Ok(old.iter().map(Vec::len).sum());
        }
        let merged = merge(&old, &session);
        let count = merged.iter().map(Vec::len).sum();
        let part = self.file("classes.part");
        {
            let mut out = io::BufWriter::new(File::create(&part)?);
            out.write_all(format_list(&merged).as_bytes())?;
            out.flush()?;
        }
        fs::rename(&part, self.list_path())?;
        self.evict()?;
        Ok(count)
    }

    fn strikes(&self) -> u32 {
        fs::read_to_string(self.file("strikes"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    /// Keeps this key's list and the [`MAX_LISTS`] most recently used others.
    fn evict(&self) -> io::Result<()> {
        let mut lists = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if let Some(key) = name.strip_suffix(".classes")
                && key != self.key
                && key.len() == 40
            {
                lists.push((entry.metadata()?.modified()?, entry.path()));
            }
        }
        lists.sort_by_key(|l| std::cmp::Reverse(l.0));
        for (_, path) in lists.into_iter().skip(MAX_LISTS) {
            remove(&path)?;
        }
        Ok(())
    }
}

/// Segments of class names from HotSpot's `class+load` log (decorations `none`): names before the first marker are
/// dropped, each marker starts its segment, and only the first load of a name counts.
pub fn parse_log(reader: impl BufRead, marker: &str) -> io::Result<Vec<Vec<String>>> {
    let mut segments: Vec<Vec<String>> = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0;
    for line in reader.split(b'\n') {
        let line = line?;
        let Ok(line) = std::str::from_utf8(&line) else {
            continue;
        };
        let Some((name, _)) = line.split_once(" source: ") else {
            continue;
        };
        if let Some(k) = name.strip_prefix(marker) {
            if let Ok(k) = k.parse::<usize>()
                && k < MAX_SEGMENTS
            {
                while segments.len() <= k {
                    segments.push(Vec::new());
                }
            }
            continue;
        }
        if segments.is_empty() || total >= MAX_CLASSES || !preloadable(name) {
            continue;
        }
        if seen.insert(name.to_owned()) {
            segments
                .last_mut()
                .expect("non-empty")
                .push(name.to_owned());
            total += 1;
        }
    }
    Ok(segments)
}

/// A binary class name worth loading by name: no hidden classes, lambdas, generated or JDK classes.
fn preloadable(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && !name.contains("$$")
        && !name.starts_with("org.spongepowered.asm.synthetic.")
        && !JDK_PACKAGES.iter().any(|p| name.starts_with(p))
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'$' | b'_'))
}

/// This session's order first, then classes of the old list it did not load, in their old segment and order.
pub fn merge(old: &[Vec<String>], session: &[Vec<String>]) -> Vec<Vec<String>> {
    let seen: HashSet<&str> = session.iter().flatten().map(String::as_str).collect();
    let mut out: Vec<Vec<String>> = vec![Vec::new(); old.len().max(session.len())];
    let mut total = 0;
    for (k, segment) in out.iter_mut().enumerate() {
        let fresh = session.get(k).into_iter().flatten();
        let kept = old
            .get(k)
            .into_iter()
            .flatten()
            .filter(|n| !seen.contains(n.as_str()));
        for name in fresh.chain(kept) {
            if total == MAX_CLASSES {
                return out;
            }
            segment.push(name.clone());
            total += 1;
        }
    }
    out
}

pub fn format_list(segments: &[Vec<String>]) -> String {
    let mut out = String::from(HEADER);
    out.push('\n');
    for (k, segment) in segments.iter().enumerate() {
        out.push_str(&format!("@{k}\n"));
        for name in segment {
            out.push_str(name);
            out.push('\n');
        }
    }
    out
}

/// `None` for a list of another format; bad lines are skipped and the bounds hold.
pub fn parse_list(text: &str) -> Option<Vec<Vec<String>>> {
    let mut lines = text.lines();
    if lines.next()? != HEADER {
        return None;
    }
    let mut segments: Vec<Vec<String>> = Vec::new();
    let mut total = 0;
    for line in lines {
        if let Some(k) = line.strip_prefix('@') {
            let k: usize = k.parse().ok()?;
            if k < segments.len() || k >= MAX_SEGMENTS {
                return None;
            }
            segments.resize_with(k + 1, Vec::new);
        } else if let Some(segment) = segments.last_mut()
            && total < MAX_CLASSES
            && preloadable(line)
        {
            segment.push(line.to_owned());
            total += 1;
        }
    }
    Some(segments)
}

fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARK: &str = "dev.mcr.Mark";

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mcr-preload-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn segs(s: &[&[&str]]) -> Vec<Vec<String>> {
        s.iter()
            .map(|seg| seg.iter().map(|n| (*n).to_owned()).collect())
            .collect()
    }

    const LOG: &str = "java.lang.Object source: shared objects file
net.fabricmc.Early source: file:/l.jar
dev.mcr.Mark0 source: file:/b.jar
net.minecraft.A source: shared objects file (top)
net.minecraft.A$$Lambda/0x0000000801234 source: net.minecraft.A
java.util.Foo source: jrt:/java.base
org.spongepowered.asm.synthetic.args.Args$1 source: __JVM_DefineClass__
net.minecraft.B source: file:/c.jar
dev.mcr.Mark2 source: file:/b.jar
net.minecraft.A source: file:/c.jar
mod.C$Inner source: file:/m.jar
garbage line
";

    #[test]
    fn log_lines_become_segments() {
        let s = parse_log(LOG.as_bytes(), MARK).unwrap();
        assert_eq!(
            s,
            segs(&[
                &["net.minecraft.A", "net.minecraft.B"],
                &[],
                &["mod.C$Inner"]
            ])
        );
        assert!(
            parse_log("a.B source: x\n".as_bytes(), MARK)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn merge_keeps_unseen_classes_in_their_segment() {
        let old = segs(&[&["a", "b"], &["c"], &["w1", "w2"]]);
        let session = segs(&[&["b", "x"], &["c", "a"]]);
        assert_eq!(
            merge(&old, &session),
            segs(&[&["b", "x"], &["c", "a"], &["w1", "w2"]])
        );
    }

    #[test]
    fn list_round_trips_and_rejects_other_formats() {
        let s = segs(&[&["a.B", "a.C$D"], &[], &["e.F"]]);
        let text = format_list(&s);
        assert!(text.starts_with("mcr-jvm-preload 1\n@0\na.B\n"));
        assert_eq!(parse_list(&text), Some(s));
        assert_eq!(parse_list("mcr-jvm-preload 0\n@0\na.B\n"), None);
        assert_eq!(parse_list(&format!("{HEADER}\n@1\n@0\n")), None);
        assert_eq!(parse_list(&format!("{HEADER}\n@99\n")), None);
        assert_eq!(
            parse_list(&format!("{HEADER}\nloose\n@0\nok.A\nbad name\n")),
            Some(segs(&[&["ok.A"]]))
        );
    }

    #[test]
    fn hostile_logs_stay_bounded() {
        let mut log = String::from("dev.mcr.Mark0 source: x\n");
        for i in 0..MAX_CLASSES + 10 {
            log.push_str(&format!("c.C{i} source: x\n"));
        }
        log.push_str(&format!("{} source: x\n", "a".repeat(MAX_NAME + 1)));
        log.push_str("dev.mcr.Mark999999999999 source: x\n");
        let s = parse_log(log.as_bytes(), MARK).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].len(), MAX_CLASSES);
    }

    #[test]
    fn session_cycle_records_merges_and_strikes() {
        let dir = temp("cycle");
        let list = ClassList::new(&dir, &"ab".repeat(20), MARK);
        let first = list.plan(true);
        let PreloadPlan::On { log, list: None } = &first else {
            panic!("{first:?}")
        };
        assert!(first.options()[0].contains(&format!("file=\"{}\"", log.display())));
        list.begin(&first).unwrap();
        fs::write(log, LOG).unwrap();
        assert_eq!(list.finish(&first, true).unwrap(), 3);
        assert!(!log.exists());

        let second = list.plan(true);
        assert_eq!(second.list(), Some(list.list_path().as_path()));
        assert!(list.plan(false).list().is_none());
        list.begin(&second).unwrap();
        list.finish(&second, true).unwrap();
        for _ in 0..MAX_STRIKES - 1 {
            list.begin(&second).unwrap();
            list.finish(&second, false).unwrap();
        }
        assert!(list.plan(true).list().is_some());
        list.begin(&second).unwrap();
        assert!(
            matches!(list.plan(true), PreloadPlan::Off(_)),
            "a killed session counts"
        );
        let text = fs::read_to_string(list.list_path()).unwrap();
        assert_eq!(
            parse_list(&text),
            Some(segs(&[
                &["net.minecraft.A", "net.minecraft.B"],
                &[],
                &["mod.C$Inner"]
            ]))
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn eviction_keeps_recent_lists() {
        let dir = temp("evict");
        let list = ClassList::new(&dir, &"ab".repeat(20), MARK);
        let now = SystemTime::now();
        let mut others = Vec::new();
        for i in 0..MAX_LISTS + 2 {
            let path = dir.join(format!("{}.classes", format!("{i:x}").repeat(40)));
            fs::write(&path, HEADER).unwrap();
            File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(now - std::time::Duration::from_secs(i as u64 * 10))
                .unwrap();
            others.push(path);
        }
        let plan = list.plan(true);
        list.begin(&plan).unwrap();
        fs::write(list.log_path(), LOG).unwrap();
        list.finish(&plan, true).unwrap();
        assert!(list.list_path().exists());
        for (i, path) in others.iter().enumerate() {
            assert_eq!(path.exists(), i < MAX_LISTS, "{i}");
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
