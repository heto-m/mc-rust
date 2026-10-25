use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use mcr_modmeta::resolve::GAME_VERSION;
use serde_json::Value;
use sha1::{Digest, Sha1};

use crate::jdk::REQUIRED_MAJOR;
use crate::paths::Layout;

const HASH_FORMAT: &str = "java-build/2";
const DEPS_FILE: &str = "deps.txt";
const MANIFEST_FILE: &str = "MANIFEST.MF";
const RESOURCES_DIR: &str = "resources";
const FABRIC_LAUNCH: &str = "fabric/launch.json";
const NEOFORGE_LAUNCH: &str = "neoforge/launch.json";
/// Sources and resources compiled only when the module's NeoForge dependency is present.
const NEOFORGE_SRC: &str = "src-neoforge";
const NEOFORGE_RESOURCES: &str = "resources-neoforge";

#[derive(Debug, Clone)]
pub struct Tool {
    pub program: PathBuf,
    pub prefix: Vec<OsString>,
}

impl Tool {
    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix);
        cmd
    }
}

#[derive(Debug, Clone)]
pub struct Toolchain {
    pub javac: Tool,
    pub jar: Tool,
}

/// One line of `java/<module>/deps.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dep {
    /// `module <name>`: another module's jar, built first.
    Module(String),
    /// `fabric-launch`: the class path and game jar from `.cache/fabric/launch.json`.
    FabricLaunch,
    /// `fabric-api-module <name>`: one module jar nested in the pinned Fabric API test mod.
    FabricApiModule(String),
    /// `neoforge-launch [optional]`: FML's class path from `.cache/neoforge/launch.json`, plus the module's
    /// `src-neoforge` and `resources-neoforge`; an optional one is skipped when NeoForge was never fetched.
    NeoForgeLaunch { optional: bool },
    /// `vanilla-client`: the client jar and the libraries its version.json lists, without a loader.
    VanillaClient,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub dir: PathBuf,
    pub src: PathBuf,
    pub deps: Vec<Dep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Built,
    UpToDate,
}

#[derive(Debug, Clone)]
pub struct Dirs<'a> {
    pub java: &'a Path,
    pub out: &'a Path,
    pub cache: &'a Path,
}

pub fn run(layout: &Layout, out: Option<&Path>) -> Result<()> {
    let toolchain = resolve_toolchain(layout)?;
    let (java, out, cache) = (
        layout.java_src_dir(),
        out.map_or_else(|| layout.java_out_dir(), Path::to_path_buf),
        layout.cache_dir(),
    );
    let dirs = Dirs {
        java: &java,
        out: &out,
        cache: &cache,
    };
    for (module, outcome) in build_all(&dirs, &toolchain)? {
        let state = match outcome {
            Outcome::Built => "built",
            Outcome::UpToDate => "up to date",
        };
        println!("[java] {}: {state}", module.name);
    }
    Ok(())
}

pub fn resolve_toolchain(layout: &Layout) -> Result<Toolchain> {
    let bin = layout.jdk_dir().join("bin");
    let exe = |name: &str| bin.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    let (javac, jar) = if exe("javac").is_file() && exe("jar").is_file() {
        (exe("javac"), exe("jar"))
    } else if path_javac_is_suitable() {
        (PathBuf::from("javac"), PathBuf::from("jar"))
    } else {
        bail!("no JDK {REQUIRED_MAJOR} with javac found; run `cargo xtask jdk --force` first");
    };
    let tool = |program| Tool {
        program,
        prefix: Vec::new(),
    };
    Ok(Toolchain {
        javac: tool(javac),
        jar: tool(jar),
    })
}

fn path_javac_is_suitable() -> bool {
    Command::new("javac")
        .arg("-version")
        .output()
        .ok()
        .and_then(|out| {
            let text = String::from_utf8_lossy(&out.stdout);
            let major = text.split_whitespace().nth(1)?.split('.').next()?;
            major.parse::<u32>().ok()
        })
        .is_some_and(|major| major >= REQUIRED_MAJOR)
}

pub fn parse_deps(text: &str) -> Result<Vec<Dep>> {
    let mut deps = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line.split_whitespace().collect::<Vec<_>>()[..] {
            ["module", name] => deps.push(Dep::Module(name.to_owned())),
            ["fabric-launch"] => deps.push(Dep::FabricLaunch),
            ["fabric-api-module", name] => deps.push(Dep::FabricApiModule(name.to_owned())),
            ["neoforge-launch"] => deps.push(Dep::NeoForgeLaunch { optional: false }),
            ["neoforge-launch", "optional"] => deps.push(Dep::NeoForgeLaunch { optional: true }),
            ["vanilla-client"] => deps.push(Dep::VanillaClient),
            _ => bail!("bad {DEPS_FILE} line `{line}`"),
        }
    }
    Ok(deps)
}

pub fn discover_modules(java_dir: &Path) -> Result<Vec<Module>> {
    let mut modules = Vec::new();
    if !java_dir.is_dir() {
        return Ok(modules);
    }
    for entry in fs::read_dir(java_dir)? {
        let path = entry?.path();
        let src = path.join("src");
        if !src.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .with_context(|| format!("non UTF-8 module directory {}", path.display()))?
            .to_owned();
        let deps = match fs::read_to_string(path.join(DEPS_FILE)) {
            Ok(text) => parse_deps(&text).with_context(|| format!("module {name}"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        modules.push(Module {
            name,
            dir: path,
            src,
            deps,
        });
    }
    modules.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(modules)
}

/// Dependencies first; fails on unknown modules and cycles.
pub fn build_order(modules: Vec<Module>) -> Result<Vec<Module>> {
    let names: Vec<String> = modules.iter().map(|m| m.name.clone()).collect();
    for m in &modules {
        for d in &m.deps {
            if let Dep::Module(dep) = d
                && !names.contains(dep)
            {
                bail!("module {} depends on unknown module {dep}", m.name);
            }
        }
    }
    let mut pending = modules;
    let mut ordered: Vec<Module> = Vec::with_capacity(pending.len());
    while !pending.is_empty() {
        let ready = pending.iter().position(|m| {
            m.deps.iter().all(|d| match d {
                Dep::Module(dep) => ordered.iter().any(|o| &o.name == dep),
                Dep::FabricLaunch
                | Dep::FabricApiModule(_)
                | Dep::NeoForgeLaunch { .. }
                | Dep::VanillaClient => true,
            })
        });
        match ready {
            Some(i) => ordered.push(pending.remove(i)),
            None => bail!(
                "module dependency cycle among {}",
                pending
                    .iter()
                    .map(|m| m.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
    Ok(ordered)
}

pub fn files_with_ext(dir: &Path, ext: Option<&str>) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if ext.is_none_or(|ext| path.extension().is_some_and(|e| e == ext)) {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

pub fn java_sources(src: &Path) -> Result<Vec<PathBuf>> {
    files_with_ext(src, Some("java"))
}

fn hash_file(hasher: &mut Sha1, base: &Path, path: &Path) -> Result<()> {
    let rel = path.strip_prefix(base).unwrap_or(path);
    let rel = rel.to_string_lossy();
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    hasher.update((rel.len() as u64).to_le_bytes());
    hasher.update(rel.as_bytes());
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(&bytes);
    Ok(())
}

/// The source and resource roots this build compiles: NeoForge's only when its dependency resolved.
fn roots(module: &Module, neoforge: bool) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut src = vec![module.src.clone()];
    let mut resources = vec![module.dir.join(RESOURCES_DIR)];
    if neoforge {
        src.push(module.dir.join(NEOFORGE_SRC));
        resources.push(module.dir.join(NEOFORGE_RESOURCES));
    }
    (src, resources)
}

/// Covers sources, resources, the manifest, `deps.txt` and every input the class path came from.
pub fn module_hash(module: &Module, inputs: &[PathBuf], neoforge: bool) -> Result<String> {
    let mut hasher = Sha1::new();
    hasher.update(HASH_FORMAT.as_bytes());
    hasher.update(REQUIRED_MAJOR.to_le_bytes());
    hasher.update([u8::from(neoforge)]);
    let (src, resources) = roots(module, neoforge);
    for dir in &src {
        for path in java_sources(dir)? {
            hash_file(&mut hasher, &module.dir, &path)?;
        }
    }
    for dir in &resources {
        for path in files_with_ext(dir, None)? {
            hash_file(&mut hasher, &module.dir, &path)?;
        }
    }
    for name in [DEPS_FILE, MANIFEST_FILE] {
        let path = module.dir.join(name);
        if path.is_file() {
            hash_file(&mut hasher, &module.dir, &path)?;
        }
    }
    for input in inputs {
        hash_file(&mut hasher, Path::new(""), input)?;
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// The compile class path, the files whose contents pin it (dependency hashes, launch.json), and
/// whether NeoForge's sources are part of this build.
fn class_path(module: &Module, dirs: &Dirs) -> Result<(Vec<PathBuf>, Vec<PathBuf>, bool)> {
    let mut cp = Vec::new();
    let mut inputs = Vec::new();
    let mut neoforge = false;
    for dep in &module.deps {
        match dep {
            Dep::Module(name) => {
                cp.push(dirs.out.join(format!("{name}.jar")));
                inputs.push(dirs.out.join(format!("{name}.hash")));
            }
            Dep::FabricLaunch => {
                let launch = dirs.cache.join(FABRIC_LAUNCH);
                let text = fs::read_to_string(&launch).with_context(|| {
                    format!(
                        "{} missing; run `cargo xtask fabric-fetch` first",
                        launch.display()
                    )
                })?;
                cp.extend(launch_class_path(&text, dirs.cache)?);
                inputs.push(launch);
            }
            Dep::FabricApiModule(name) => {
                let jar = fabric_api_module(name, dirs)?;
                cp.push(jar.clone());
                inputs.push(jar);
            }
            Dep::NeoForgeLaunch { optional } => {
                let launch = dirs.cache.join(NEOFORGE_LAUNCH);
                let text = match fs::read_to_string(&launch) {
                    Ok(text) => text,
                    Err(e) if *optional && e.kind() == std::io::ErrorKind::NotFound => {
                        println!(
                            "[java] {}: building without NeoForge ({} missing; run `cargo xtask neoforge-fetch`)",
                            module.name,
                            launch.display()
                        );
                        continue;
                    }
                    Err(e) => {
                        return Err(e).with_context(|| {
                            format!(
                                "{} missing; run `cargo xtask neoforge-fetch` first",
                                launch.display()
                            )
                        });
                    }
                };
                cp.extend(launch_libraries(&text, dirs.cache)?);
                inputs.push(launch);
                neoforge = true;
            }
            Dep::VanillaClient => {
                let (jars, version_json) = vanilla_class_path(dirs.cache, GAME_VERSION)?;
                cp.extend(jars);
                inputs.push(version_json);
            }
        }
    }
    Ok((cp, inputs, neoforge))
}

/// The client jar and its libraries, found where `fetch` and `fabric-fetch` put them, plus the
/// version.json that pins them all.
pub fn vanilla_class_path(cache: &Path, version: &str) -> Result<(Vec<PathBuf>, PathBuf)> {
    let version_dir = cache.join("versions").join(version);
    let version_json = version_dir.join("version.json");
    let text = fs::read_to_string(&version_json).with_context(|| {
        format!(
            "{} missing; run `cargo xtask fetch` first",
            version_json.display()
        )
    })?;
    let libs = crate::fabric::parse_vanilla_libraries(&text, crate::fabric::Host::current())?;
    let mut cp = Vec::with_capacity(libs.class_path.len() + 1);
    for lib in &libs.class_path {
        let nested = cache.join("fabric/libraries").join(&lib.path);
        let flat = Path::new(&lib.path)
            .file_name()
            .map(|n| cache.join("libraries").join(n));
        match std::iter::once(nested).chain(flat).find(|p| p.is_file()) {
            Some(jar) => cp.push(jar),
            None => bail!(
                "library {} missing; run `cargo xtask fabric-fetch` first",
                lib.name
            ),
        }
    }
    let client = version_dir.join("client.jar");
    if !client.is_file() {
        bail!(
            "{} missing; run `cargo xtask fetch` first",
            client.display()
        );
    }
    cp.push(client);
    Ok((cp, version_json))
}

/// A launch manifest's class path without its game jar.
pub fn launch_libraries(launch_json: &str, cache: &Path) -> Result<Vec<PathBuf>> {
    let root: Value = serde_json::from_str(launch_json).context("launch.json is not JSON")?;
    root["class_path"]
        .as_array()
        .context("launch.json has no class_path")?
        .iter()
        .map(|v| v.as_str().map(|s| cache.join(s)))
        .collect::<Option<_>>()
        .context("launch.json class_path holds a non-string")
}

/// Extracts `META-INF/jars/<name>-<version>.jar` from the Fabric API jar the launch manifest pins.
fn fabric_api_module(name: &str, dirs: &Dirs) -> Result<PathBuf> {
    let launch = dirs.cache.join(FABRIC_LAUNCH);
    let text = fs::read_to_string(&launch).with_context(|| {
        format!(
            "{} missing; run `cargo xtask fabric-fetch` first",
            launch.display()
        )
    })?;
    let root: Value = serde_json::from_str(&text).context("launch.json is not JSON")?;
    let api = root["test_mods"]
        .as_array()
        .context("launch.json has no test_mods")?
        .iter()
        .filter_map(Value::as_str)
        .find(|p| {
            Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("fabric-api-"))
        })
        .map(|p| dirs.cache.join(p))
        .context("launch.json pins no fabric-api jar")?;
    let out = dirs.out.join("deps").join(format!("{name}.jar"));
    if out.is_file() && fs::metadata(&out)?.modified()? >= fs::metadata(&api)?.modified()? {
        return Ok(out);
    }
    let mut zip = zip::ZipArchive::new(fs::File::open(&api)?)
        .with_context(|| format!("reading {}", api.display()))?;
    let prefix = format!("META-INF/jars/{name}-");
    let entry = zip
        .file_names()
        .filter_map(Result::ok)
        .map(std::borrow::Cow::into_owned)
        .find(|n| {
            n.strip_prefix(&prefix).is_some_and(|rest| {
                rest.starts_with(|c: char| c.is_ascii_digit()) && rest.ends_with(".jar")
            })
        })
        .with_context(|| format!("{} has no nested {name} jar", api.display()))?;
    fs::create_dir_all(out.parent().context("deps dir")?)?;
    let mut file = zip.by_name(&entry)?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut bytes)?;
    fs::write(&out, bytes)?;
    Ok(out)
}

pub fn launch_class_path(launch_json: &str, cache: &Path) -> Result<Vec<PathBuf>> {
    let root: Value = serde_json::from_str(launch_json).context("launch.json is not JSON")?;
    let mut out = launch_libraries(launch_json, cache)?;
    out.push(
        cache.join(
            root["game_jar"]
                .as_str()
                .context("launch.json has no game_jar")?,
        ),
    );
    Ok(out)
}

pub fn build_all(dirs: &Dirs, tc: &Toolchain) -> Result<Vec<(Module, Outcome)>> {
    let modules = build_order(discover_modules(dirs.java)?)?;
    if modules.is_empty() {
        bail!("no Java modules under {}", dirs.java.display());
    }
    fs::create_dir_all(dirs.out)?;
    let mut results = Vec::with_capacity(modules.len());
    for module in modules {
        let outcome = build_module(&module, dirs, tc)?;
        results.push((module, outcome));
    }
    Ok(results)
}

/// Builds one module and the modules it depends on, leaving the others alone.
pub fn build_one(dirs: &Dirs, tc: &Toolchain, name: &str) -> Result<PathBuf> {
    let modules = build_order(discover_modules(dirs.java)?)?;
    let mut wanted = vec![name.to_owned()];
    for m in modules.iter().rev() {
        if wanted.contains(&m.name) {
            for d in &m.deps {
                if let Dep::Module(dep) = d {
                    wanted.push(dep.clone());
                }
            }
        }
    }
    if !modules.iter().any(|m| m.name == name) {
        bail!("no Java module {name} under {}", dirs.java.display());
    }
    fs::create_dir_all(dirs.out)?;
    for module in modules.iter().filter(|m| wanted.contains(&m.name)) {
        build_module(module, dirs, tc)?;
    }
    Ok(dirs.out.join(format!("{name}.jar")))
}

fn build_module(module: &Module, dirs: &Dirs, tc: &Toolchain) -> Result<Outcome> {
    let out_dir = dirs.out;
    let jar = out_dir.join(format!("{}.jar", module.name));
    let hash_file = out_dir.join(format!("{}.hash", module.name));
    let (cp, inputs, neoforge) = class_path(module, dirs)?;
    let (src_roots, resource_roots) = roots(module, neoforge);
    let mut sources = Vec::new();
    for dir in &src_roots {
        sources.extend(java_sources(dir)?);
    }
    if sources.is_empty() {
        bail!("module {} has no .java sources", module.name);
    }
    let hash = module_hash(module, &inputs, neoforge)?;
    if jar.is_file() && fs::read_to_string(&hash_file).is_ok_and(|h| h.trim() == hash) {
        // Tests compare mtimes to spot stale jars; a touched-but-identical source must not trip them.
        fs::File::options()
            .write(true)
            .open(&jar)?
            .set_modified(std::time::SystemTime::now())?;
        return Ok(Outcome::UpToDate);
    }

    let classes = out_dir.join(format!("{}.classes", module.name));
    if classes.exists() {
        fs::remove_dir_all(&classes)?;
    }
    fs::create_dir_all(&classes)?;
    let argfile = out_dir.join(format!("{}.sources", module.name));
    fs::write(&argfile, argfile_contents(&sources))?;

    let mut javac = tc.javac.command();
    javac
        .arg("--release")
        .arg(REQUIRED_MAJOR.to_string())
        .arg("-encoding")
        .arg("UTF-8")
        .arg("-proc:none")
        .arg("-Xlint:all,-restricted,-classfile")
        .arg("-Werror")
        .arg("-d")
        .arg(&classes);
    if !cp.is_empty() {
        javac.arg("-cp").arg(std::env::join_paths(&cp)?);
    }
    javac.arg(format!("@{}", argfile.display()));
    run_tool(javac, "javac", &module.name)?;

    let _ = fs::remove_file(&hash_file);
    let mut jar_cmd = tc.jar.command();
    jar_cmd.arg("--create").arg("--file").arg(&jar);
    let manifest = module.dir.join(MANIFEST_FILE);
    if manifest.is_file() {
        jar_cmd.arg("--manifest").arg(&manifest);
    }
    jar_cmd.arg("-C").arg(&classes).arg(".");
    for resources in resource_roots.iter().filter(|d| d.is_dir()) {
        jar_cmd.arg("-C").arg(resources).arg(".");
    }
    run_tool(jar_cmd, "jar", &module.name)?;

    fs::write(&hash_file, &hash)?;
    fs::remove_dir_all(&classes)?;
    fs::remove_file(&argfile)?;
    Ok(Outcome::Built)
}

fn run_tool(mut cmd: Command, tool: &str, module: &str) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("starting {tool} for module {module}"))?;
    if !status.success() {
        bail!("{tool} failed for module {module} ({status})");
    }
    Ok(())
}

fn argfile_contents(sources: &[PathBuf]) -> String {
    let mut out = String::new();
    for path in sources {
        let escaped = path
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        out.push('"');
        out.push_str(&escaped);
        out.push_str("\"\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::tempfile_dir;

    const FAKE_JAVAC: &str = r#"
log="$1"; shift
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-d" ]; then out="$2"; fi
  if [ "$1" = "-cp" ]; then echo "cp $2" >> "$log.cp"; fi
  case "$1" in @*) cat "${1#@}" >> "$log.src";; esac
  shift
done
echo javac >> "$log"
mkdir -p "$out" && echo class > "$out/Fake.class"
"#;

    const FAKE_JAR: &str = r#"
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--file" ]; then out="$2"; fi
  shift
done
echo jar > "$out"
"#;

    fn fake_toolchain(dir: &Path) -> (Toolchain, PathBuf) {
        let javac = dir.join("javac.sh");
        let jar = dir.join("jar.sh");
        let log = dir.join("javac.log");
        fs::write(&javac, FAKE_JAVAC).unwrap();
        fs::write(&jar, FAKE_JAR).unwrap();
        let sh = |script: &Path, extra: Option<&Path>| Tool {
            program: PathBuf::from("sh"),
            prefix: std::iter::once(script.as_os_str().to_owned())
                .chain(extra.map(|p| p.as_os_str().to_owned()))
                .collect(),
        };
        let tc = Toolchain {
            javac: sh(&javac, Some(&log)),
            jar: sh(&jar, None),
        };
        (tc, log)
    }

    fn javac_runs(log: &Path) -> usize {
        fs::read_to_string(log).map_or(0, |s| s.lines().count())
    }

    fn write_module(java_dir: &Path, name: &str, file: &str, body: &str) {
        let pkg = java_dir.join(name).join("src/dev/mcr");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join(file), body).unwrap();
    }

    fn build(dir: &Path, tc: &Toolchain) -> Result<Vec<(Module, Outcome)>> {
        let (java, out, cache) = (dir.join("java"), dir.join("out"), dir.join("cache"));
        build_all(
            &Dirs {
                java: &java,
                out: &out,
                cache: &cache,
            },
            tc,
        )
    }

    #[test]
    fn discovers_only_directories_with_src() {
        let dir = tempfile_dir("java-discover");
        let java = dir.join("java");
        write_module(&java, "b-mod", "B.java", "class B {}");
        write_module(&java, "a-mod", "A.java", "class A {}");
        fs::create_dir_all(java.join("no-src")).unwrap();
        fs::write(java.join("README"), "x").unwrap();
        let names: Vec<_> = discover_modules(&java)
            .unwrap()
            .into_iter()
            .map(|m| m.name)
            .collect();
        assert_eq!(names, ["a-mod", "b-mod"]);
    }

    #[test]
    fn rebuilds_only_changed_modules() {
        let dir = tempfile_dir("java-incremental");
        let java = dir.join("java");
        let out = dir.join("out");
        write_module(&java, "one", "One.java", "class One {}");
        write_module(&java, "two", "Two.java", "class Two {}");
        let (tc, log) = fake_toolchain(&dir);

        let first = build(&dir, &tc).unwrap();
        assert!(first.iter().all(|(_, o)| *o == Outcome::Built));
        assert_eq!(javac_runs(&log), 2);
        assert!(out.join("one.jar").is_file());
        assert!(!out.join("one.classes").exists());

        let second = build(&dir, &tc).unwrap();
        assert!(second.iter().all(|(_, o)| *o == Outcome::UpToDate));
        assert_eq!(javac_runs(&log), 2);

        write_module(&java, "two", "Two.java", "class Two { int x; }");
        let third = build(&dir, &tc).unwrap();
        let outcomes: Vec<_> = third.iter().map(|(m, o)| (m.name.as_str(), *o)).collect();
        assert_eq!(
            outcomes,
            [("one", Outcome::UpToDate), ("two", Outcome::Built)]
        );
        assert_eq!(javac_runs(&log), 3);
    }

    #[test]
    fn missing_jar_forces_rebuild() {
        let dir = tempfile_dir("java-missing-jar");
        let java = dir.join("java");
        write_module(&java, "one", "One.java", "class One {}");
        let (tc, log) = fake_toolchain(&dir);
        build(&dir, &tc).unwrap();
        fs::remove_file(dir.join("out/one.jar")).unwrap();
        let again = build(&dir, &tc).unwrap();
        assert_eq!(again[0].1, Outcome::Built);
        assert_eq!(javac_runs(&log), 2);
    }

    #[test]
    fn dependencies_build_first_and_propagate_rebuilds() {
        let dir = tempfile_dir("java-deps");
        let java = dir.join("java");
        write_module(&java, "a-app", "App.java", "class App {}");
        write_module(&java, "z-lib", "Lib.java", "class Lib {}");
        fs::write(java.join("a-app/deps.txt"), "# lib\nmodule z-lib\n").unwrap();
        let (tc, log) = fake_toolchain(&dir);

        let first = build(&dir, &tc).unwrap();
        let order: Vec<_> = first.iter().map(|(m, _)| m.name.as_str()).collect();
        assert_eq!(order, ["z-lib", "a-app"]);
        let cp = fs::read_to_string(dir.join("javac.log.cp")).unwrap();
        assert!(cp.contains("z-lib.jar"), "{cp}");

        write_module(&java, "z-lib", "Lib.java", "class Lib { int y; }");
        let second = build(&dir, &tc).unwrap();
        assert!(second.iter().all(|(_, o)| *o == Outcome::Built));
        assert_eq!(javac_runs(&log), 4);
    }

    #[test]
    fn resources_change_the_hash() {
        let dir = tempfile_dir("java-resources");
        let java = dir.join("java");
        write_module(&java, "one", "One.java", "class One {}");
        fs::create_dir_all(java.join("one/resources")).unwrap();
        fs::write(java.join("one/resources/fabric.mod.json"), "{}").unwrap();
        let (tc, log) = fake_toolchain(&dir);
        build(&dir, &tc).unwrap();
        fs::write(java.join("one/resources/fabric.mod.json"), "{ }").unwrap();
        assert_eq!(build(&dir, &tc).unwrap()[0].1, Outcome::Built);
        assert_eq!(javac_runs(&log), 2);
    }

    #[test]
    fn deps_parse_and_order_errors() {
        assert_eq!(
            parse_deps("module a\n\n# c\nfabric-launch\nfabric-api-module fabric-api-base\n")
                .unwrap(),
            [
                Dep::Module("a".into()),
                Dep::FabricLaunch,
                Dep::FabricApiModule("fabric-api-base".into())
            ]
        );
        assert_eq!(
            parse_deps("neoforge-launch\nneoforge-launch optional\n").unwrap(),
            [
                Dep::NeoForgeLaunch { optional: false },
                Dep::NeoForgeLaunch { optional: true }
            ]
        );
        assert!(parse_deps("neoforge-launch maybe").is_err());
        assert!(parse_deps("modules a").is_err());
        let m = |name: &str, dep: &str| Module {
            name: name.into(),
            dir: PathBuf::new(),
            src: PathBuf::new(),
            deps: vec![Dep::Module(dep.into())],
        };
        assert!(build_order(vec![m("a", "b"), m("b", "a")]).is_err());
        assert!(build_order(vec![m("a", "missing")]).is_err());
    }

    #[test]
    fn optional_neoforge_sources_follow_the_manifest() {
        let dir = tempfile_dir("java-neoforge");
        let java = dir.join("java");
        write_module(&java, "one", "One.java", "class One {}");
        let neo = java.join("one/src-neoforge/dev/mcr");
        fs::create_dir_all(&neo).unwrap();
        fs::write(neo.join("Neo.java"), "class Neo {}").unwrap();
        fs::write(java.join("one/deps.txt"), "neoforge-launch optional\n").unwrap();
        let (tc, log) = fake_toolchain(&dir);
        let srcs = || fs::read_to_string(dir.join("javac.log.src")).unwrap_or_default();

        assert_eq!(build(&dir, &tc).unwrap()[0].1, Outcome::Built);
        assert_eq!(javac_runs(&log), 1);
        assert!(!srcs().contains("Neo.java"), "{}", srcs());
        assert_eq!(build(&dir, &tc).unwrap()[0].1, Outcome::UpToDate);

        let cache = dir.join("cache/neoforge");
        fs::create_dir_all(&cache).unwrap();
        fs::write(
            cache.join("launch.json"),
            r#"{"class_path": ["neoforge/libraries/loader.jar"], "game_jar": "x.jar"}"#,
        )
        .unwrap();
        assert_eq!(build(&dir, &tc).unwrap()[0].1, Outcome::Built);
        let cp = fs::read_to_string(dir.join("javac.log.cp")).unwrap();
        assert!(cp.contains("loader.jar") && !cp.contains("x.jar"), "{cp}");
        assert!(srcs().contains("Neo.java"), "{}", srcs());

        fs::write(java.join("one/deps.txt"), "neoforge-launch\n").unwrap();
        fs::remove_file(cache.join("launch.json")).unwrap();
        assert!(build(&dir, &tc).is_err(), "a required NeoForge dependency");
    }

    #[test]
    fn vanilla_client_resolves_libraries_from_either_cache_layout() {
        assert_eq!(
            parse_deps("vanilla-client\n").unwrap(),
            [Dep::VanillaClient]
        );
        let cache = tempfile_dir("java-vanilla-client");
        let ver = cache.join("versions/9.9");
        fs::create_dir_all(&ver).unwrap();
        let lib = |name: &str, path: &str| {
            format!(
                r#"{{"name": "{name}", "downloads": {{"artifact": {{"path": "{path}", "url": "u", "sha1": "s"}}}}}}"#
            )
        };
        fs::write(
            ver.join("version.json"),
            format!(
                r#"{{"libraries": [{}, {}]}}"#,
                lib("a:a:1", "a/a/1/a-1.jar"),
                lib("b:b:1", "b/b/1/b-1.jar")
            ),
        )
        .unwrap();
        fs::write(ver.join("client.jar"), "jar").unwrap();
        fs::create_dir_all(cache.join("fabric/libraries/a/a/1")).unwrap();
        fs::write(cache.join("fabric/libraries/a/a/1/a-1.jar"), "a").unwrap();
        assert!(vanilla_class_path(&cache, "9.9").is_err(), "b is missing");
        fs::create_dir_all(cache.join("libraries")).unwrap();
        fs::write(cache.join("libraries/b-1.jar"), "b").unwrap();
        let (cp, pin) = vanilla_class_path(&cache, "9.9").unwrap();
        assert_eq!(
            cp,
            [
                cache.join("fabric/libraries/a/a/1/a-1.jar"),
                cache.join("libraries/b-1.jar"),
                ver.join("client.jar")
            ]
        );
        assert_eq!(pin, ver.join("version.json"));
    }

    #[test]
    fn build_one_skips_unrelated_modules() {
        let dir = tempfile_dir("java-build-one");
        let java = dir.join("java");
        write_module(&java, "app", "App.java", "class App {}");
        write_module(&java, "lib", "Lib.java", "class Lib {}");
        write_module(&java, "other", "Other.java", "class Other {}");
        fs::write(java.join("app/deps.txt"), "module lib\n").unwrap();
        let (tc, log) = fake_toolchain(&dir);
        let (out, cache) = (dir.join("out"), dir.join("cache"));
        let dirs = Dirs {
            java: &java,
            out: &out,
            cache: &cache,
        };
        assert_eq!(build_one(&dirs, &tc, "app").unwrap(), out.join("app.jar"));
        assert_eq!(javac_runs(&log), 2);
        assert!(!out.join("other.jar").exists());
        assert!(build_one(&dirs, &tc, "missing").is_err());
    }

    #[test]
    fn launch_class_path_appends_the_game_jar() {
        let cp = launch_class_path(
            r#"{"class_path": ["fabric/libraries/x.jar"], "game_jar": "versions/26.3/client.jar"}"#,
            Path::new("/c"),
        )
        .unwrap();
        assert_eq!(
            cp,
            [
                PathBuf::from("/c/fabric/libraries/x.jar"),
                PathBuf::from("/c/versions/26.3/client.jar")
            ]
        );
    }

    #[test]
    fn argfile_quotes_paths() {
        let text = argfile_contents(&[PathBuf::from("/a b/C.java")]);
        assert_eq!(text, "\"/a b/C.java\"\n");
    }
}
