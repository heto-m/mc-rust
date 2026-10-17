//! A JDK 25 AOT cache (`-XX:AOTCache`) per JVM configuration. A normal session records it, a detached
//! helper builds it after that session ends, and later sessions start from it. HotSpot validates the
//! cache itself and ignores one that does not match, so a stale or damaged file only loses the speed-up.

use std::fs::{self, File, TryLockError};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use sha1::{Digest, Sha1};

/// The helper's first argument: `<program> --jvm-aot-create <dir> <key>` builds the cache.
pub const CREATE_ARG: &str = "--jvm-aot-create";
/// `MCR_JVM_AOT=off` runs without the cache and records nothing.
pub const ENV: &str = "MCR_JVM_AOT";
pub const MAX_CACHES: usize = 2;
pub const MAX_CACHE_BYTES: u64 = 1 << 30;
/// Recordings (or builds) that may fail for one key before it stops trying.
pub const MAX_ATTEMPTS: u32 = 3;
/// Other keys' leftovers younger than this may belong to a running mcr.
const STALE_AFTER: Duration = Duration::from_secs(3600);
const FORMAT: &str = "mcr-jvm-aot 1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AotPlan {
    /// Start from this cache.
    Use(PathBuf),
    /// Record this session's classes and profiles into this file at `DestroyJavaVM`.
    Record(PathBuf),
    /// A recording is waiting for, or in, the background build.
    Building,
    Off(&'static str),
}

impl AotPlan {
    pub fn options(&self) -> Vec<String> {
        match self {
            Self::Use(cache) => vec![format!("-XX:AOTCache={}", cache.display())],
            Self::Record(config) => vec![
                "-XX:AOTMode=record".into(),
                format!("-XX:AOTConfiguration={}", config.display()),
                // HotSpot lists every class it leaves out of the recording.
                "-Xlog:aot*=error".into(),
            ],
            Self::Building | Self::Off(_) => Vec::new(),
        }
    }

    pub fn records(&self) -> bool {
        matches!(self, Self::Record(_))
    }
}

/// Everything that makes a cache unusable when it changes.
#[derive(Debug, Clone, Copy)]
pub struct KeyInputs<'a> {
    pub java_home: &'a Path,
    /// Options that shape the cache (GC, object layout, modules); per-run `-D` properties stay out.
    pub options: &'a [String],
    /// Checked by size and modification time, as HotSpot does.
    pub class_path: &'a [PathBuf],
    /// Jars loaded outside the class path (mods); hashed by content.
    pub jars: &'a [PathBuf],
    /// Anything else, such as an ABI version.
    pub extra: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateOutcome {
    Created {
        bytes: u64,
    },
    TooLarge {
        bytes: u64,
    },
    Failed(String),
    /// Another helper holds the key's lock.
    Busy,
    /// Nothing recorded for the key.
    NothingToDo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AotCache {
    dir: PathBuf,
    key: String,
}

impl AotCache {
    pub fn new(dir: impl Into<PathBuf>, inputs: &KeyInputs<'_>) -> io::Result<Self> {
        Ok(Self {
            dir: dir.into(),
            key: key(inputs)?,
        })
    }

    /// For the helper; the key must be one [`AotCache::new`] made.
    pub fn open(dir: impl Into<PathBuf>, key: &str) -> io::Result<Self> {
        if key.len() != 40 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad AOT key"));
        }
        Ok(Self {
            dir: dir.into(),
            key: key.to_owned(),
        })
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self, ext: &str) -> PathBuf {
        self.dir.join(format!("{}.{ext}", self.key))
    }

    pub fn cache_path(&self) -> PathBuf {
        self.file("aot")
    }

    /// What this session does, from the files on disk and [`ENV`].
    pub fn plan(&self) -> AotPlan {
        if std::env::var(ENV).is_ok_and(|v| v.eq_ignore_ascii_case("off") || v == "0") {
            return AotPlan::Off("disabled by MCR_JVM_AOT");
        }
        let cache = self.cache_path();
        if cache.is_file() {
            return AotPlan::Use(cache);
        }
        if self.file("aotconf").is_file() || self.building() {
            return AotPlan::Building;
        }
        if self.attempts() >= MAX_ATTEMPTS {
            return AotPlan::Off("recording failed too often");
        }
        AotPlan::Record(self.file("aotconf.part"))
    }

    /// Side effects of starting `plan`: a recording counts as an attempt until its cache is built.
    pub fn begin(&self, plan: &AotPlan) -> io::Result<()> {
        match plan {
            AotPlan::Use(cache) => {
                // Only orders eviction, so a read-only cache still gets used.
                let _ = File::options()
                    .write(true)
                    .open(cache)
                    .and_then(|f| f.set_modified(SystemTime::now()));
                Ok(())
            }
            AotPlan::Record(part) => {
                fs::create_dir_all(&self.dir)?;
                remove(part)?;
                fs::write(self.file("attempts"), (self.attempts() + 1).to_string())
            }
            AotPlan::Building | AotPlan::Off(_) => Ok(()),
        }
    }

    /// Keeps a recording that a clean `DestroyJavaVM` finished, and drops any other.
    pub fn end_recording(&self, clean: bool) -> io::Result<bool> {
        let part = self.file("aotconf.part");
        let kept = clean && fs::metadata(&part).is_ok_and(|m| m.len() > 0);
        if kept {
            fs::rename(&part, self.file("aotconf"))?;
        } else {
            remove(&part)?;
        }
        Ok(kept)
    }

    /// HotSpot refused the cache it was given; the next session records a new one.
    pub fn reject(&self) -> io::Result<()> {
        remove(&self.cache_path())
    }

    /// Writes the helper's `java -XX:AOTMode=create` command line for this key.
    pub fn write_job(
        &self,
        java: &Path,
        options: &[String],
        class_path: &[PathBuf],
    ) -> io::Result<()> {
        let invalid = |what: &str| io::Error::new(io::ErrorKind::InvalidInput, what.to_owned());
        let class_path = std::env::join_paths(class_path).map_err(|e| invalid(&e.to_string()))?;
        let mut lines = vec![
            java.to_str()
                .ok_or_else(|| invalid("non UTF-8 java path"))?
                .to_owned(),
        ];
        lines.extend(options.iter().cloned());
        lines.extend([
            "-cp".to_owned(),
            class_path
                .into_string()
                .map_err(|_| invalid("non UTF-8 class path"))?,
            "-XX:AOTMode=create".into(),
            format!("-XX:AOTConfiguration={}", self.file("aotconf").display()),
            format!("-XX:AOTCache={}", self.file("aot.part").display()),
            "-Xlog:aot*=error".into(),
        ]);
        if lines.iter().any(|l| l.contains(['\n', '\r', '\0'])) {
            return Err(invalid("JVM option with a line break"));
        }
        fs::create_dir_all(&self.dir)?;
        fs::write(self.file("job"), lines.join("\n"))
    }

    /// A recording and its job wait for the helper, and no helper runs.
    pub fn needs_create(&self) -> bool {
        self.file("aotconf").is_file()
            && self.file("job").is_file()
            && !self.cache_path().is_file()
            && !self.building()
    }

    /// Starts `program CREATE_ARG dir key` detached from this process, so quitting mcr does not stop it.
    pub fn spawn_create(&self, program: &Path) -> io::Result<()> {
        let mut cmd = Command::new(program);
        cmd.arg(CREATE_ARG)
            .arg(&self.dir)
            .arg(&self.key)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        cmd.spawn().map(drop)
    }

    /// The helper: runs the job under the key's lock, then publishes the cache and evicts old ones.
    pub fn create(&self) -> io::Result<CreateOutcome> {
        let lock = File::create(self.file("lock"))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Ok(CreateOutcome::Busy),
            Err(TryLockError::Error(e)) => return Err(e),
        }
        let (Ok(job), true) = (
            fs::read_to_string(self.file("job")),
            self.file("aotconf").is_file(),
        ) else {
            return Ok(CreateOutcome::NothingToDo);
        };
        let mut lines = job.lines();
        let java = lines.next().unwrap_or_default();
        let log = File::create(self.file("log"))?;
        let status = Command::new(java)
            .args(lines)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .status();
        let part = self.file("aot.part");
        let bytes = fs::metadata(&part).map_or(0, |m| m.len());
        let outcome = match status {
            Ok(s) if s.success() && bytes > MAX_CACHE_BYTES => CreateOutcome::TooLarge { bytes },
            Ok(s) if s.success() && bytes > 0 => {
                fs::rename(&part, self.cache_path())?;
                remove(&self.file("attempts"))?;
                CreateOutcome::Created { bytes }
            }
            Ok(s) => CreateOutcome::Failed(format!("java exited with {s}")),
            Err(e) => CreateOutcome::Failed(e.to_string()),
        };
        remove(&part)?;
        remove(&self.file("aotconf"))?;
        remove(&self.file("job"))?;
        if let CreateOutcome::TooLarge { .. } = outcome {
            fs::write(self.file("attempts"), MAX_ATTEMPTS.to_string())?;
        }
        self.evict()?;
        Ok(outcome)
    }

    /// Keeps the [`MAX_CACHES`] most recently used caches, this key's first, and drops other keys'
    /// stale leftovers.
    pub fn evict(&self) -> io::Result<()> {
        let mut caches = Vec::new();
        let mut others = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some((key, ext)) = name.split_once('.') else {
                continue;
            };
            if key == self.key || key.len() != 40 {
                continue;
            }
            let modified = entry.metadata()?.modified()?;
            if ext == "aot" {
                caches.push((modified, entry.path()));
            } else {
                others.push((modified, entry.path()));
            }
        }
        caches.sort_by_key(|c| std::cmp::Reverse(c.0));
        let keep = MAX_CACHES.saturating_sub(usize::from(self.cache_path().is_file()));
        for (_, path) in caches.into_iter().skip(keep) {
            remove(&path)?;
        }
        let now = SystemTime::now();
        for (modified, path) in others {
            if now
                .duration_since(modified)
                .is_ok_and(|age| age > STALE_AFTER)
            {
                remove(&path)?;
            }
        }
        Ok(())
    }

    fn attempts(&self) -> u32 {
        fs::read_to_string(self.file("attempts"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    fn building(&self) -> bool {
        File::open(self.file("lock"))
            .is_ok_and(|f| matches!(f.try_lock(), Err(TryLockError::WouldBlock)))
    }
}

/// Whether this process mapped `cache`; `None` where that cannot be told.
pub fn mapped(cache: &Path) -> Option<bool> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let maps = fs::read_to_string("/proc/self/maps").ok()?;
    let cache = cache.to_str()?;
    Some(maps.lines().any(|l| l.ends_with(cache)))
}

/// Lowers the helper's CPU priority so a game started meanwhile keeps the cores.
pub fn lower_priority() {
    #[cfg(target_os = "linux")]
    // SAFETY: setpriority only reads its integer arguments.
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
}

fn key(inputs: &KeyInputs<'_>) -> io::Result<String> {
    let mut h = Sha1::new();
    let mut field = |bytes: &[u8]| {
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(bytes);
    };
    field(FORMAT.as_bytes());
    field(inputs.extra.as_bytes());
    field(inputs.java_home.as_os_str().as_encoded_bytes());
    field(&fs::read(inputs.java_home.join("release")).unwrap_or_default());
    for jvm in [
        "lib/server/libjvm.so",
        "bin/server/jvm.dll",
        "lib/server/libjvm.dylib",
    ] {
        field(stamp(&inputs.java_home.join(jvm)).as_bytes());
    }
    for option in inputs.options {
        field(option.as_bytes());
    }
    for path in inputs.class_path {
        field(path.as_os_str().as_encoded_bytes());
        field(stamp(path).as_bytes());
    }
    for path in inputs.jars {
        field(path.as_os_str().as_encoded_bytes());
        field(&content_hash(path)?);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn stamp(path: &Path) -> String {
    match fs::metadata(path) {
        Ok(m) => {
            let modified = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .unwrap_or_default();
            format!("{} {}", m.len(), modified.as_nanos())
        }
        Err(_) => "missing".into(),
    }
}

fn content_hash(path: &Path) -> io::Result<[u8; 20]> {
    let mut file = File::open(path)?;
    let mut h = Sha1::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(h.finalize().into());
        }
        h.update(&buf[..n]);
    }
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

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mcr-aot-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    struct Fixture {
        root: PathBuf,
        home: PathBuf,
        jar: PathBuf,
        lib: PathBuf,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let root = temp(tag);
            let home = root.join("jdk");
            fs::create_dir_all(home.join("lib/server")).unwrap();
            fs::write(home.join("release"), "JAVA_VERSION=\"25.0.4\"\n").unwrap();
            fs::write(home.join("lib/server/libjvm.so"), "jvm").unwrap();
            let jar = root.join("mod.jar");
            fs::write(&jar, "mod v1").unwrap();
            let lib = root.join("lib.jar");
            fs::write(&lib, "lib").unwrap();
            Self {
                root,
                home,
                jar,
                lib,
            }
        }

        fn cache(&self, options: &[&str], extra: &str) -> AotCache {
            let options: Vec<String> = options.iter().map(|s| (*s).to_owned()).collect();
            AotCache::new(
                self.root.join("aot"),
                &KeyInputs {
                    java_home: &self.home,
                    options: &options,
                    class_path: std::slice::from_ref(&self.lib),
                    jars: std::slice::from_ref(&self.jar),
                    extra,
                },
            )
            .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn key_follows_every_input() {
        let f = Fixture::new("key");
        let base = f.cache(&["-XX:+UseZGC"], "bridge 12").key().to_owned();
        assert_eq!(base.len(), 40);
        assert_eq!(f.cache(&["-XX:+UseZGC"], "bridge 12").key(), base);
        assert_ne!(f.cache(&["-XX:+UseG1GC"], "bridge 12").key(), base);
        assert_ne!(f.cache(&["-XX:+UseZGC"], "bridge 13").key(), base);
        fs::write(&f.jar, "mod v2").unwrap();
        let changed_mod = f.cache(&["-XX:+UseZGC"], "bridge 12").key().to_owned();
        assert_ne!(changed_mod, base);
        fs::write(f.home.join("release"), "JAVA_VERSION=\"25.0.5\"\n").unwrap();
        assert_ne!(f.cache(&["-XX:+UseZGC"], "bridge 12").key(), changed_mod);
        fs::remove_file(&f.jar).unwrap();
        assert!(
            AotCache::new(
                f.root.join("aot"),
                &KeyInputs {
                    java_home: &f.home,
                    options: &[],
                    class_path: &[],
                    jars: std::slice::from_ref(&f.jar),
                    extra: "",
                },
            )
            .is_err()
        );
    }

    #[test]
    fn record_then_build_then_use() {
        let f = Fixture::new("cycle");
        let c = f.cache(&[], "");
        let AotPlan::Record(part) = c.plan() else {
            panic!("{:?}", c.plan())
        };
        assert!(part.ends_with(format!("{}.aotconf.part", c.key())));
        c.begin(&c.plan()).unwrap();
        fs::write(&part, "recorded").unwrap();
        assert!(c.end_recording(true).unwrap());
        assert_eq!(c.plan(), AotPlan::Building);
        assert!(!c.needs_create(), "no job yet");
        c.write_job(Path::new("/bin/sh"), &["-c".into(), "exit 0".into()], &[])
            .unwrap();
        assert!(c.needs_create());
        fs::write(c.file("aot.part"), "cache").unwrap();
        assert_eq!(c.create().unwrap(), CreateOutcome::Created { bytes: 5 });
        assert_eq!(c.plan(), AotPlan::Use(c.cache_path()));
        assert!(!c.needs_create());
        for leftover in ["aotconf", "aotconf.part", "job", "attempts", "aot.part"] {
            assert!(!c.file(leftover).exists(), "{leftover}");
        }
        c.reject().unwrap();
        assert!(matches!(c.plan(), AotPlan::Record(_)));
    }

    #[test]
    fn options_per_plan() {
        let use_opts = AotPlan::Use("/c/k.aot".into()).options();
        assert_eq!(use_opts, ["-XX:AOTCache=/c/k.aot"]);
        let rec = AotPlan::Record("/c/k.aotconf.part".into()).options();
        assert_eq!(
            rec[..2],
            [
                "-XX:AOTMode=record",
                "-XX:AOTConfiguration=/c/k.aotconf.part"
            ]
        );
        assert!(AotPlan::Building.options().is_empty());
        assert!(AotPlan::Off("x").options().is_empty());
    }

    #[test]
    fn unclean_sessions_and_failed_builds_give_up_after_max_attempts() {
        let f = Fixture::new("attempts");
        let c = f.cache(&[], "");
        for _ in 0..MAX_ATTEMPTS {
            let plan = c.plan();
            assert!(plan.records(), "{plan:?}");
            c.begin(&plan).unwrap();
            fs::write(c.file("aotconf.part"), "half").unwrap();
            assert!(!c.end_recording(false).unwrap());
            assert!(!c.file("aotconf.part").exists());
        }
        assert!(matches!(c.plan(), AotPlan::Off(_)));

        let g = f.cache(&["-Xint"], "");
        g.begin(&g.plan()).unwrap();
        fs::write(g.file("aotconf.part"), "conf").unwrap();
        g.end_recording(true).unwrap();
        g.write_job(Path::new("/bin/sh"), &["-c".into(), "exit 3".into()], &[])
            .unwrap();
        assert!(matches!(g.create().unwrap(), CreateOutcome::Failed(_)));
        assert!(g.plan().records(), "a failed build records again");
    }

    #[test]
    fn oversized_caches_are_dropped_for_good() {
        let f = Fixture::new("large");
        let c = f.cache(&[], "");
        c.begin(&c.plan()).unwrap();
        fs::write(c.file("aotconf.part"), "conf").unwrap();
        c.end_recording(true).unwrap();
        c.write_job(Path::new("/bin/sh"), &["-c".into(), "exit 0".into()], &[])
            .unwrap();
        File::create(c.file("aot.part"))
            .unwrap()
            .set_len(MAX_CACHE_BYTES + 1)
            .unwrap();
        assert!(matches!(
            c.create().unwrap(),
            CreateOutcome::TooLarge { .. }
        ));
        assert!(!c.cache_path().exists() && !c.file("aot.part").exists());
        assert!(matches!(c.plan(), AotPlan::Off(_)));
    }

    #[test]
    fn a_held_lock_means_building() {
        let f = Fixture::new("lock");
        let c = f.cache(&[], "");
        fs::create_dir_all(c.dir()).unwrap();
        let lock = File::create(c.file("lock")).unwrap();
        lock.lock().unwrap();
        assert_eq!(c.plan(), AotPlan::Building);
        assert_eq!(c.create().unwrap(), CreateOutcome::Busy);
        lock.unlock().unwrap();
        assert!(c.plan().records());
        assert_eq!(c.create().unwrap(), CreateOutcome::NothingToDo);
    }

    #[test]
    fn eviction_keeps_the_newest_caches() {
        let f = Fixture::new("evict");
        let c = f.cache(&[], "");
        fs::create_dir_all(c.dir()).unwrap();
        fs::write(c.cache_path(), "mine").unwrap();
        let now = SystemTime::now();
        let other = |n: u8, ext: &str, age: u64| {
            let path = c
                .dir()
                .join(format!("{}.{ext}", format!("{n:x}").repeat(40)));
            fs::write(&path, "x").unwrap();
            File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(now - Duration::from_secs(age))
                .unwrap();
            path
        };
        let newest = other(1, "aot", 10);
        let older = other(2, "aot", 20);
        let old_conf = other(3, "aotconf", 2 * 3600);
        let fresh_part = other(4, "aotconf.part", 10);
        let unrelated = c.dir().join("notes.txt");
        fs::write(&unrelated, "keep").unwrap();
        c.evict().unwrap();
        assert!(c.cache_path().exists() && newest.exists());
        assert!(!older.exists() && !old_conf.exists());
        assert!(fresh_part.exists() && unrelated.exists());
    }

    #[test]
    fn open_rejects_paths() {
        assert!(AotCache::open("/c", "../../etc/passwd").is_err());
        assert!(AotCache::open("/c", &"a".repeat(40)).is_ok());
    }

    #[test]
    fn job_lines_round_trip() {
        let f = Fixture::new("job");
        let c = f.cache(&[], "");
        c.write_job(
            Path::new("/jdk/bin/java"),
            &["-XX:+UseZGC".into()],
            &[PathBuf::from("/a.jar"), PathBuf::from("/b.jar")],
        )
        .unwrap();
        let job = fs::read_to_string(c.file("job")).unwrap();
        let lines: Vec<&str> = job.lines().collect();
        let sep = if cfg!(windows) { ";" } else { ":" };
        assert_eq!(
            lines[..4],
            [
                "/jdk/bin/java",
                "-XX:+UseZGC",
                "-cp",
                &format!("/a.jar{sep}/b.jar")
            ]
        );
        assert_eq!(lines[4], "-XX:AOTMode=create");
        assert!(lines[6].ends_with(".aot.part"));
        assert!(
            c.write_job(Path::new("/j"), &["-Dx=a\nb".into()], &[])
                .is_err()
        );
    }
}
