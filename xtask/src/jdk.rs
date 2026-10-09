use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;

use crate::net::download_to;
use crate::paths::{Layout, java_bin};

pub const REQUIRED_MAJOR: u32 = 25;

pub fn ensure_jdk(layout: &Layout, force: bool) -> Result<PathBuf> {
    if !force && system_java_is_suitable() {
        println!("[jdk] using system java");
        return Ok(PathBuf::from("java"));
    }

    let jdk = layout.jdk_dir();
    let bin = java_bin(&jdk);
    if !force && bin.is_file() {
        println!("[jdk] portable JDK {REQUIRED_MAJOR} already installed");
        return Ok(bin);
    }

    let (os, arch) = platform()?;
    let is_zip = os == "windows";
    let tools = layout.tools_dir();
    fs::create_dir_all(&tools)?;
    let archive = tools.join(if is_zip { "jdk25.zip" } else { "jdk25.tar.gz" });
    println!("[jdk] downloading Temurin JDK {REQUIRED_MAJOR} ({os}/{arch})");
    download_to(&adoptium_url(os, arch), &archive)?;

    let staging = tools.join("jdk25.staging");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    if is_zip {
        extract_zip(&archive, &staging)?;
    } else {
        extract_tar_gz(&archive, &staging)?;
    }
    fs::remove_file(&archive)?;

    if jdk.exists() {
        fs::remove_dir_all(&jdk)?;
    }
    fs::rename(&staging, &jdk)?;

    let bin = java_bin(&jdk);
    if !bin.is_file() {
        bail!("JDK extracted but {} is missing", bin.display());
    }
    Ok(bin)
}

pub fn resolve_java(layout: &Layout) -> Result<PathBuf> {
    if system_java_is_suitable() {
        return Ok(PathBuf::from("java"));
    }
    let bin = java_bin(&layout.jdk_dir());
    if bin.is_file() {
        return Ok(bin);
    }
    bail!("no Java {REQUIRED_MAJOR} found; run `cargo xtask jdk` first")
}

fn system_java_is_suitable() -> bool {
    Command::new("java")
        .arg("-version")
        .output()
        .ok()
        .and_then(|out| parse_java_major(&String::from_utf8_lossy(&out.stderr)))
        .is_some_and(|major| major >= REQUIRED_MAJOR)
}

pub fn parse_java_major(version_output: &str) -> Option<u32> {
    let start = version_output.find('"')? + 1;
    let rest = &version_output[start..];
    let raw = &rest[..rest.find('"')?];
    let mut parts = raw.split(['.', '_', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

fn platform() -> Result<(&'static str, &'static str)> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "mac",
        "windows" => "windows",
        other => bail!("no Temurin build mapping for OS {other}"),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        other => bail!("no Temurin build mapping for arch {other}"),
    };
    Ok((os, arch))
}

pub fn adoptium_url(os: &str, arch: &str) -> String {
    format!(
        "https://api.adoptium.net/v3/binary/latest/{REQUIRED_MAJOR}/ga/{os}/{arch}/jdk/hotspot/normal/eclipse"
    )
}

pub fn strip_top_dir(path: &Path) -> Option<PathBuf> {
    let mut comps = path.components();
    match comps.next()? {
        Component::Normal(_) => {}
        _ => return None,
    }
    let rest = comps.as_path();
    if rest.as_os_str().is_empty() {
        None
    } else {
        Some(rest.to_path_buf())
    }
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let mut tar = tar::Archive::new(GzDecoder::new(BufReader::new(file)));
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if let Some(rel) = strip_top_dir(&path) {
            let target = dest.join(rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            entry
                .unpack(&target)
                .with_context(|| format!("extracting {}", path.display()))?;
        }
    }
    Ok(())
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(File::open(archive)?)?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let Some(name) = file.enclosed_name() else {
            continue;
        };
        let Some(rel) = strip_top_dir(&name) else {
            continue;
        };
        let target = dest.join(rel);
        if file.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&target)?;
        io::copy(&mut file, &mut out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_version_string() {
        let out = "openjdk version \"25.0.1\" 2025-10-21 LTS\nOpenJDK Runtime";
        assert_eq!(parse_java_major(out), Some(25));
    }

    #[test]
    fn parses_legacy_1_x_version_string() {
        assert_eq!(parse_java_major("java version \"1.8.0_402\""), Some(8));
    }

    #[test]
    fn rejects_output_without_version() {
        assert_eq!(parse_java_major("command not found"), None);
    }

    #[test]
    fn adoptium_url_targets_jdk25_hotspot_eclipse() {
        assert_eq!(
            adoptium_url("linux", "x64"),
            "https://api.adoptium.net/v3/binary/latest/25/ga/linux/x64/jdk/hotspot/normal/eclipse"
        );
    }

    #[test]
    fn strip_top_dir_removes_first_component() {
        assert_eq!(
            strip_top_dir(Path::new("jdk-25.0.1+8/bin/java")),
            Some(PathBuf::from("bin/java"))
        );
        assert_eq!(strip_top_dir(Path::new("jdk-25.0.1+8/")), None);
    }

    #[test]
    fn tar_gz_extraction_strips_top_dir() {
        let dir = crate::testutil::tempfile_dir("jdk-targz");
        let archive = dir.join("jdk.tar.gz");
        write_tar_gz(&archive);
        let dest = dir.join("out");
        fs::create_dir_all(&dest).unwrap();
        extract_tar_gz(&archive, &dest).unwrap();
        assert_eq!(fs::read(dest.join("bin/java")).unwrap(), b"fake-java");
        assert!(!dest.join("jdk-25.0.1+8").exists());
    }

    #[test]
    fn zip_extraction_strips_top_dir() {
        let dir = crate::testutil::tempfile_dir("jdk-zip");
        let archive = dir.join("jdk.zip");
        write_zip(&archive);
        let dest = dir.join("out");
        fs::create_dir_all(&dest).unwrap();
        extract_zip(&archive, &dest).unwrap();
        assert_eq!(fs::read(dest.join("bin/java.exe")).unwrap(), b"fake-java");
    }

    fn write_tar_gz(path: &Path) {
        let file = File::create(path).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(enc);
        let data = b"fake-java";
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "jdk-25.0.1+8/bin/java", &data[..])
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();
    }

    fn write_zip(path: &Path) {
        use std::io::Write;
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("jdk-25.0.1+8/bin/java.exe", opts).unwrap();
        zip.write_all(b"fake-java").unwrap();
        zip.finish().unwrap();
    }
}
