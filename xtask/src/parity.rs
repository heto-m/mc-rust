//! `parity`: runs the differential harnesses of `crates/mcr-parity` against `java/mcr-oracle`.
//!
//! Per harness, under `.cache/parity/<name>/`: `cases.jsonl` (generated, seeded by name),
//! `vanilla-<key>.jsonl` (the oracle's answers, reused while the key holds), `mcr.jsonl`,
//! `summary.json`. `.cache/parity/status.json` keeps each harness's last result.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use mcr_modmeta::resolve::GAME_VERSION;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};

use crate::paths::Layout;
use crate::{fabric, java, jdk};

const KEY_FORMAT: &str = "parity-oracle/1";
const ORACLE_MODULE: &str = "mcr-oracle";
const ORACLE_MAIN: &str = "dev.mcr.oracle.Oracle";
const SCOPE_MEMORY: &str = "MemoryMax=5G";
const MAX_HEAP: &str = "-Xmx2G";
const ORACLE_TIMEOUT: Duration = Duration::from_secs(900);
/// Probe results also depend on dependency versions.
const SHARED_SCOPE: &[&str] = &["Cargo.lock", "crates/mcr-parity/src/lib.rs"];

pub const EXIT_PASS: i32 = 0;
pub const EXIT_MISMATCH: i32 = 1;
pub const EXIT_ERROR: i32 = 2;

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub names: Vec<String>,
    pub all: bool,
    pub failed: bool,
    pub fresh: bool,
    pub max: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HarnessInfo {
    name: String,
    scope: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Pass,
    Mismatch,
    Error,
}

impl Outcome {
    fn label(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Mismatch => "FAIL",
            Self::Error => "ERROR",
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_bytes(hasher: &mut Sha1, label: &str, bytes: &[u8]) {
    hasher.update((label.len() as u64).to_le_bytes());
    hasher.update(label.as_bytes());
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn file_sha1(path: &Path) -> Result<String> {
    let mut hasher = Sha1::new();
    let mut file = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Class entries only, by name and content: rebuilding identical sources keeps the key.
fn jar_classes_hash(jar: &Path) -> Result<String> {
    let mut zip = zip::ZipArchive::new(fs::File::open(jar)?)
        .with_context(|| format!("reading {}", jar.display()))?;
    let mut names: Vec<String> = zip
        .file_names()
        .filter_map(Result::ok)
        .filter(|n| n.ends_with(".class"))
        .map(std::borrow::Cow::into_owned)
        .collect();
    names.sort();
    let mut hasher = Sha1::new();
    for name in names {
        let mut bytes = Vec::new();
        zip.by_name(&name)?.read_to_end(&mut bytes)?;
        hash_bytes(&mut hasher, &name, &bytes);
    }
    Ok(hex(&hasher.finalize()))
}

/// Every file under the scope paths, by repository path and content.
fn scope_hash(root: &Path, scope: &[String]) -> Result<String> {
    let mut hasher = Sha1::new();
    let mut entries: Vec<&str> = scope
        .iter()
        .map(String::as_str)
        .chain(SHARED_SCOPE.iter().copied())
        .collect();
    entries.sort_unstable();
    entries.dedup();
    for entry in entries {
        let path = root.join(entry);
        let files = if path.is_dir() {
            java::files_with_ext(&path, None)?
        } else if path.is_file() {
            vec![path]
        } else {
            hash_bytes(&mut hasher, entry, b"<missing>");
            continue;
        };
        for file in files {
            let rel = file.strip_prefix(root).unwrap_or(&file).to_string_lossy();
            hash_bytes(&mut hasher, &rel, &fs::read(&file)?);
        }
    }
    Ok(hex(&hasher.finalize()))
}

pub fn oracle_key(client_jar_sha: &str, oracle_classes: &str, cases_sha: &str) -> String {
    let mut hasher = Sha1::new();
    for part in [KEY_FORMAT, client_jar_sha, oracle_classes, cases_sha] {
        hash_bytes(&mut hasher, "", part.as_bytes());
    }
    hex(&hasher.finalize())
}

fn parse_list(text: &str) -> Result<Vec<HarnessInfo>> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let v: Value = serde_json::from_str(line).context("mcr-parity list output")?;
            Ok(HarnessInfo {
                name: v["name"]
                    .as_str()
                    .context("harness without name")?
                    .to_owned(),
                scope: v["scope"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        })
        .collect()
}

#[derive(Debug, Default, Clone, PartialEq)]
struct Status {
    harnesses: BTreeMap<String, Value>,
}

impl Status {
    fn load(path: &Path) -> Self {
        let harnesses = fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.get("harnesses").and_then(Value::as_object).cloned())
            .map(|m| m.into_iter().collect())
            .unwrap_or_default();
        Self { harnesses }
    }

    fn save(&self, path: &Path) -> Result<()> {
        let v = json!({"format": 1, "harnesses": self.harnesses});
        fs::write(path, serde_json::to_string_pretty(&v)?)?;
        Ok(())
    }

    fn passed(&self, name: &str) -> bool {
        self.harnesses
            .get(name)
            .and_then(|h| h["passed"].as_bool())
            .unwrap_or(false)
    }

    fn inputs(&self, name: &str) -> Option<&str> {
        self.harnesses.get(name).and_then(|h| h["inputs"].as_str())
    }
}

/// Which harnesses run: the named ones, all, the last failures, or (none of those) the ones whose
/// inputs changed or that never passed.
fn select<'a>(
    all: &'a [HarnessInfo],
    opts: &Options,
    status: &Status,
    inputs: &BTreeMap<String, String>,
) -> Result<Vec<&'a HarnessInfo>> {
    if !opts.names.is_empty() {
        return opts
            .names
            .iter()
            .map(|n| {
                all.iter().find(|h| &h.name == n).with_context(|| {
                    format!(
                        "unknown harness {n} (have: {})",
                        all.iter()
                            .map(|h| h.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
            })
            .collect();
    }
    Ok(all
        .iter()
        .filter(|h| {
            if opts.all {
                true
            } else if opts.failed {
                status.harnesses.contains_key(&h.name) && !status.passed(&h.name)
            } else {
                !status.passed(&h.name)
                    || status.inputs(&h.name) != inputs.get(&h.name).map(String::as_str)
            }
        })
        .collect())
}

struct Ctx {
    probe_bin: PathBuf,
    java: PathBuf,
    oracle_cp: String,
    natives: Option<PathBuf>,
    client_jar: PathBuf,
    assets: Option<(PathBuf, String)>,
}

fn capped(program: &Path) -> Command {
    let mut cmd = Command::new("systemd-run");
    cmd.args([
        "--user",
        "--scope",
        "-q",
        "-p",
        SCOPE_MEMORY,
        "-p",
        "MemorySwapMax=0",
    ])
    .arg(program);
    cmd.env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET");
    cmd
}

fn run_oracle(ctx: &Ctx, name: &str, cases: &Path, out: &Path) -> Result<()> {
    let tmp = out.with_extension("part");
    let mut cmd = capped(Path::new("timeout"));
    cmd.arg(ORACLE_TIMEOUT.as_secs().to_string())
        .args(["unshare", "-rn"])
        .arg(&ctx.java)
        .args([
            MAX_HEAP,
            "-XX:+UseSerialGC",
            "-Djava.awt.headless=true",
            "--enable-native-access=ALL-UNNAMED",
        ]);
    if let Some(natives) = &ctx.natives {
        cmd.arg(format!("-Dorg.lwjgl.librarypath={}", natives.display()));
    }
    cmd.arg("-cp")
        .arg(&ctx.oracle_cp)
        .arg(ORACLE_MAIN)
        .arg(name)
        .arg("--cases")
        .arg(cases)
        .arg("--out")
        .arg(&tmp);
    if let Some((dir, index)) = &ctx.assets {
        cmd.arg("--assets").arg(dir).arg("--asset-index").arg(index);
    }
    cmd.current_dir(out.parent().context("oracle output dir")?);
    let status = cmd.status().context("starting the oracle JVM")?;
    if !status.success() {
        let _ = fs::remove_file(&tmp);
        bail!("oracle for {name} failed ({status})");
    }
    fs::rename(&tmp, out)?;
    Ok(())
}

fn run_probe(ctx: &Ctx, args: &[&std::ffi::OsStr]) -> Result<i32> {
    let mut cmd = Command::new(&ctx.probe_bin);
    cmd.args(args);
    let status = cmd
        .status()
        .with_context(|| format!("starting {}", ctx.probe_bin.display()))?;
    status
        .code()
        .with_context(|| format!("mcr-parity killed ({status})"))
}

fn run_harness(
    ctx: &Ctx,
    dir: &Path,
    name: &str,
    key: &str,
    opts: &Options,
) -> Result<(Outcome, Value)> {
    let cases = dir.join("cases.jsonl");
    let vanilla = dir.join(format!("vanilla-{}.jsonl", &key[..16]));
    if opts.fresh || !vanilla.is_file() {
        println!("[parity] {name}: running the vanilla oracle");
        for stale in fs::read_dir(dir)? {
            let path = stale?.path();
            let file = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if file.starts_with("vanilla-") && path != vanilla {
                fs::remove_file(&path)?;
            }
        }
        run_oracle(ctx, name, &cases, &vanilla)?;
    } else {
        println!("[parity] {name}: vanilla oracle output cached");
    }

    let mcr = dir.join("mcr.jsonl");
    let mut probe: Vec<std::ffi::OsString> = vec![
        "probe".into(),
        name.into(),
        "--cases".into(),
        cases.clone().into(),
        "--out".into(),
        mcr.clone().into(),
        "--client-jar".into(),
        ctx.client_jar.clone().into(),
    ];
    if let Some((assets, index)) = &ctx.assets {
        probe.extend([
            "--assets".into(),
            assets.into(),
            "--asset-index".into(),
            index.into(),
        ]);
    }
    let refs: Vec<&std::ffi::OsStr> = probe.iter().map(|s| s.as_os_str()).collect();
    if run_probe(ctx, &refs)? != 0 {
        bail!("probe for {name} failed");
    }

    let summary_path = dir.join("summary.json");
    let max = opts.max.to_string();
    let code = run_probe(
        ctx,
        &[
            "diff".as_ref(),
            name.as_ref(),
            "--cases".as_ref(),
            cases.as_os_str(),
            "--vanilla".as_ref(),
            vanilla.as_os_str(),
            "--mcr".as_ref(),
            mcr.as_os_str(),
            "--max".as_ref(),
            max.as_ref(),
            "--summary".as_ref(),
            summary_path.as_os_str(),
        ],
    )?;
    let summary: Value = serde_json::from_str(&fs::read_to_string(&summary_path)?)?;
    match code {
        0 => Ok((Outcome::Pass, summary)),
        1 => Ok((Outcome::Mismatch, summary)),
        _ => bail!("diff for {name} failed"),
    }
}

fn build_probe(layout: &Layout) -> Result<PathBuf> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args(["build", "-q", "-p", "mcr-parity", "--bin", "mcr-parity"])
        .current_dir(layout.root())
        .status()
        .context("starting cargo")?;
    if !status.success() {
        bail!("building mcr-parity failed ({status})");
    }
    Ok(layout
        .target_dir()
        .join("debug")
        .join(format!("mcr-parity{}", std::env::consts::EXE_SUFFIX)))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn join_cp(paths: &[PathBuf]) -> Result<String> {
    std::env::join_paths(paths)?
        .into_string()
        .map_err(|_| anyhow::anyhow!("non UTF-8 class path"))
}

/// The exit code: 0 every selected harness passed, 1 some mismatched, 2 setup or a harness failed to run.
pub fn run(layout: &Layout, opts: &Options) -> Result<i32> {
    let probe_bin = build_probe(layout)?;
    let list = Command::new(&probe_bin)
        .arg("list")
        .output()
        .with_context(|| format!("starting {}", probe_bin.display()))?;
    if !list.status.success() {
        bail!("mcr-parity list failed");
    }
    let harnesses = parse_list(&String::from_utf8_lossy(&list.stdout))?;

    let cache = layout.cache_dir();
    let (java_dir, java_out) = (layout.java_src_dir(), layout.java_out_dir());
    let dirs = java::Dirs {
        java: &java_dir,
        out: &java_out,
        cache: &cache,
    };
    let oracle_jar = java::build_one(&dirs, &java::resolve_toolchain(layout)?, ORACLE_MODULE)?;
    let (mut cp, version_json) = java::vanilla_class_path(&cache, GAME_VERSION)?;
    cp.insert(0, oracle_jar.clone());
    let client_jar = layout.client_jar(GAME_VERSION);
    let index = fabric::parse_asset_index(&fs::read_to_string(&version_json)?)?;
    let assets = layout
        .assets_dir()
        .join("indexes")
        .join(format!("{index}.json"))
        .is_file()
        .then(|| (layout.assets_dir(), index));
    if assets.is_none() {
        println!(
            "[parity] no asset index: fonts beyond the jar (unifont) are missing on both sides; run `cargo xtask assets`"
        );
    }
    let natives = layout
        .fabric_dir()
        .join("natives")
        .join(fabric::Host::current().natives_dir_name());
    let ctx = Ctx {
        probe_bin,
        java: jdk::resolve_java(layout)?,
        oracle_cp: join_cp(&cp)?,
        natives: natives.is_dir().then_some(natives),
        client_jar: client_jar.clone(),
        assets,
    };

    let parity = layout.parity_dir();
    fs::create_dir_all(&parity)?;
    let jar_sha = file_sha1(&client_jar)?;
    let oracle_classes = jar_classes_hash(&oracle_jar)?;
    let mut keys = BTreeMap::new();
    let mut inputs = BTreeMap::new();
    for h in &harnesses {
        let dir = parity.join(&h.name);
        fs::create_dir_all(&dir)?;
        let cases = dir.join("cases.jsonl");
        let code = run_probe(
            &ctx,
            &["cases".as_ref(), h.name.as_ref(), cases.as_os_str()],
        )?;
        if code != 0 {
            bail!("generating cases for {} failed", h.name);
        }
        let key = oracle_key(&jar_sha, &oracle_classes, &file_sha1(&cases)?);
        let mut hasher = Sha1::new();
        hash_bytes(&mut hasher, "oracle", key.as_bytes());
        hash_bytes(
            &mut hasher,
            "scope",
            scope_hash(layout.root(), &h.scope)?.as_bytes(),
        );
        inputs.insert(h.name.clone(), hex(&hasher.finalize()));
        keys.insert(h.name.clone(), key);
    }

    let status_path = parity.join("status.json");
    let mut status = Status::load(&status_path);
    let selected = select(&harnesses, opts, &status, &inputs)?;
    if selected.is_empty() {
        println!("[parity] nothing to run: every harness passed with these inputs (use --all)");
        return Ok(EXIT_PASS);
    }

    let mut results = Vec::new();
    for h in selected {
        let dir = parity.join(&h.name);
        let (outcome, summary) = match run_harness(&ctx, &dir, &h.name, &keys[&h.name], opts) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[parity] {}: {e:#}", h.name);
                (Outcome::Error, json!({}))
            }
        };
        let (cases, matched) = (
            summary["cases"].as_u64().unwrap_or(0),
            summary["matched"].as_u64().unwrap_or(0),
        );
        status.harnesses.insert(
            h.name.clone(),
            json!({
                "passed": outcome == Outcome::Pass,
                "outcome": outcome.label(),
                "cases": cases,
                "matched": matched,
                "inputs": inputs[&h.name],
                "oracle": keys[&h.name],
                "time": now(),
            }),
        );
        status.save(&status_path)?;
        results.push((h.name.clone(), outcome, cases, matched));
    }

    println!("[parity] results:");
    for (name, outcome, cases, matched) in &results {
        let rate = if *cases == 0 {
            0.0
        } else {
            *matched as f64 * 100.0 / *cases as f64
        };
        println!(
            "  {:<5} {name:<24} {matched}/{cases} ({rate:.2}%)",
            outcome.label()
        );
    }
    let worst = results
        .iter()
        .map(|r| r.1)
        .fold(Outcome::Pass, |a, b| match (a, b) {
            (Outcome::Error, _) | (_, Outcome::Error) => Outcome::Error,
            (Outcome::Mismatch, _) | (_, Outcome::Mismatch) => Outcome::Mismatch,
            _ => Outcome::Pass,
        });
    Ok(match worst {
        Outcome::Pass => EXIT_PASS,
        Outcome::Mismatch => EXIT_MISMATCH,
        Outcome::Error => EXIT_ERROR,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::tempfile_dir;

    fn infos(names: &[&str]) -> Vec<HarnessInfo> {
        names
            .iter()
            .map(|n| HarnessInfo {
                name: (*n).into(),
                scope: vec![],
            })
            .collect()
    }

    fn names(list: Vec<&HarnessInfo>) -> Vec<&str> {
        list.into_iter().map(|h| h.name.as_str()).collect()
    }

    #[test]
    fn selection_by_name_all_failed_and_changed() {
        let all = infos(&["a", "b", "c", "d"]);
        let mut status = Status::default();
        status
            .harnesses
            .insert("a".into(), json!({"passed": true, "inputs": "1"}));
        status
            .harnesses
            .insert("b".into(), json!({"passed": false, "inputs": "1"}));
        status
            .harnesses
            .insert("c".into(), json!({"passed": true, "inputs": "1"}));
        let inputs: BTreeMap<String, String> = [("a", "1"), ("b", "1"), ("c", "2"), ("d", "1")]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        let opts = |f: fn(&mut Options)| {
            let mut o = Options::default();
            f(&mut o);
            o
        };
        let changed = select(&all, &Options::default(), &status, &inputs).unwrap();
        assert_eq!(names(changed), ["b", "c", "d"]);
        let failed = select(&all, &opts(|o| o.failed = true), &status, &inputs).unwrap();
        assert_eq!(names(failed), ["b"]);
        let every = select(&all, &opts(|o| o.all = true), &status, &inputs).unwrap();
        assert_eq!(every.len(), 4);
        let named = select(
            &all,
            &opts(|o| o.names = vec!["d".into(), "a".into()]),
            &status,
            &inputs,
        )
        .unwrap();
        assert_eq!(names(named), ["d", "a"]);
        assert!(
            select(
                &all,
                &opts(|o| o.names = vec!["zz".into()]),
                &status,
                &inputs
            )
            .is_err()
        );
    }

    #[test]
    fn status_round_trips_and_tolerates_garbage() {
        let dir = tempfile_dir("parity-status");
        let path = dir.join("status.json");
        assert_eq!(Status::load(&path), Status::default());
        fs::write(&path, "not json").unwrap();
        assert_eq!(Status::load(&path), Status::default());
        let mut s = Status::default();
        s.harnesses
            .insert("font".into(), json!({"passed": true, "inputs": "x"}));
        s.save(&path).unwrap();
        let back = Status::load(&path);
        assert!(back.passed("font"));
        assert_eq!(back.inputs("font"), Some("x"));
    }

    #[test]
    fn oracle_key_tracks_each_input() {
        let base = oracle_key("jar", "classes", "cases");
        assert_eq!(base, oracle_key("jar", "classes", "cases"));
        assert_ne!(base, oracle_key("jar2", "classes", "cases"));
        assert_ne!(base, oracle_key("jar", "classes2", "cases"));
        assert_ne!(base, oracle_key("jar", "classes", "cases2"));
        assert_ne!(oracle_key("ab", "c", "d"), oracle_key("a", "bc", "d"));
    }

    #[test]
    fn scope_hash_follows_contents_not_mtimes() {
        let dir = tempfile_dir("parity-scope");
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/a.rs"), "a").unwrap();
        let scope = vec!["src".to_owned()];
        let one = scope_hash(&dir, &scope).unwrap();
        fs::write(dir.join("src/a.rs"), "a").unwrap();
        assert_eq!(one, scope_hash(&dir, &scope).unwrap());
        fs::write(dir.join("src/b.rs"), "b").unwrap();
        assert_ne!(one, scope_hash(&dir, &scope).unwrap());
    }

    #[test]
    fn jar_hash_ignores_resources_and_order() {
        use std::io::Write;
        let dir = tempfile_dir("parity-jar");
        let write = |path: &Path, entries: &[(&str, &str)]| {
            let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
            for (name, body) in entries {
                zip.start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        };
        let (a, b, c) = (dir.join("a.jar"), dir.join("b.jar"), dir.join("c.jar"));
        write(
            &a,
            &[("x/A.class", "1"), ("x/B.class", "2"), ("README", "r")],
        );
        write(&b, &[("x/B.class", "2"), ("x/A.class", "1")]);
        write(&c, &[("x/A.class", "1"), ("x/B.class", "3")]);
        assert_eq!(jar_classes_hash(&a).unwrap(), jar_classes_hash(&b).unwrap());
        assert_ne!(jar_classes_hash(&a).unwrap(), jar_classes_hash(&c).unwrap());
    }

    #[test]
    fn list_output_parses() {
        let list = parse_list(
            "{\"name\":\"font\",\"scope\":[\"crates/mcr-ui/src/font.rs\"],\"cases\":3}\n\n",
        )
        .unwrap();
        assert_eq!(
            list,
            [HarnessInfo {
                name: "font".into(),
                scope: vec!["crates/mcr-ui/src/font.rs".into()]
            }]
        );
        assert!(parse_list("{}").is_err());
    }
}
