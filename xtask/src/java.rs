use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha1::{Digest, Sha1};

use crate::jdk::REQUIRED_MAJOR;
use crate::paths::Layout;

const HASH_FORMAT: &str = "java-build/1";

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub src: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Built,
    UpToDate,
}

pub fn run(layout: &Layout) -> Result<()> {
    let toolchain = resolve_toolchain(layout)?;
    for (module, outcome) in build_all(&layout.java_src_dir(), &layout.java_out_dir(), &toolchain)?
    {
        let state = match outcome {
            Outcome::Built => "built",
            Outcome::UpToDate => "up to date",
        };
        println!("[java] {}: {state}", module.name);
    }
    Ok(())
}

fn resolve_toolchain(layout: &Layout) -> Result<Toolchain> {
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
        modules.push(Module { name, src });
    }
    modules.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(modules)
}

pub fn java_sources(src: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![src.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "java") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

pub fn source_hash(src: &Path, sources: &[PathBuf]) -> Result<String> {
    let mut hasher = Sha1::new();
    hasher.update(HASH_FORMAT.as_bytes());
    hasher.update(REQUIRED_MAJOR.to_le_bytes());
    for path in sources {
        let rel = path.strip_prefix(src).unwrap_or(path);
        let rel = rel.to_string_lossy();
        let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        hasher.update((rel.len() as u64).to_le_bytes());
        hasher.update(rel.as_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

pub fn build_all(
    java_dir: &Path,
    out_dir: &Path,
    tc: &Toolchain,
) -> Result<Vec<(Module, Outcome)>> {
    let modules = discover_modules(java_dir)?;
    if modules.is_empty() {
        bail!("no Java modules under {}", java_dir.display());
    }
    fs::create_dir_all(out_dir)?;
    let mut results = Vec::with_capacity(modules.len());
    for module in modules {
        let outcome = build_module(&module, out_dir, tc)?;
        results.push((module, outcome));
    }
    Ok(results)
}

fn build_module(module: &Module, out_dir: &Path, tc: &Toolchain) -> Result<Outcome> {
    let jar = out_dir.join(format!("{}.jar", module.name));
    let hash_file = out_dir.join(format!("{}.hash", module.name));
    let sources = java_sources(&module.src)?;
    if sources.is_empty() {
        bail!("module {} has no .java sources", module.name);
    }
    let hash = source_hash(&module.src, &sources)?;
    if jar.is_file() && fs::read_to_string(&hash_file).is_ok_and(|h| h.trim() == hash) {
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
        .arg("-Xlint:all,-restricted")
        .arg("-Werror")
        .arg("-d")
        .arg(&classes)
        .arg(format!("@{}", argfile.display()));
    run_tool(javac, "javac", &module.name)?;

    let _ = fs::remove_file(&hash_file);
    let mut jar_cmd = tc.jar.command();
    jar_cmd
        .arg("--create")
        .arg("--file")
        .arg(&jar)
        .arg("-C")
        .arg(&classes)
        .arg(".");
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

        let first = build_all(&java, &out, &tc).unwrap();
        assert!(first.iter().all(|(_, o)| *o == Outcome::Built));
        assert_eq!(javac_runs(&log), 2);
        assert!(out.join("one.jar").is_file());
        assert!(!out.join("one.classes").exists());

        let second = build_all(&java, &out, &tc).unwrap();
        assert!(second.iter().all(|(_, o)| *o == Outcome::UpToDate));
        assert_eq!(javac_runs(&log), 2);

        write_module(&java, "two", "Two.java", "class Two { int x; }");
        let third = build_all(&java, &out, &tc).unwrap();
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
        let out = dir.join("out");
        write_module(&java, "one", "One.java", "class One {}");
        let (tc, log) = fake_toolchain(&dir);
        build_all(&java, &out, &tc).unwrap();
        fs::remove_file(out.join("one.jar")).unwrap();
        let again = build_all(&java, &out, &tc).unwrap();
        assert_eq!(again[0].1, Outcome::Built);
        assert_eq!(javac_runs(&log), 2);
    }

    #[test]
    fn argfile_quotes_paths() {
        let text = argfile_contents(&[PathBuf::from("/a b/C.java")]);
        assert_eq!(text, "\"/a b/C.java\"\n");
    }
}
