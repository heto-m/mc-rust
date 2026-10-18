use std::fs::{self, File};
use std::io::Read;

use anyhow::{Context, Result, bail};
use mcr_data_gen::version_tables::{Reports, version_tables};

use crate::paths::Layout;

/// `(name, protocol)` from the client jar's `version.json`.
fn version_info(layout: &Layout, ver: &str) -> Result<(String, u32)> {
    let jar = layout.client_jar(ver);
    let mut zip = zip::ZipArchive::new(
        File::open(&jar).with_context(|| format!("opening {}", jar.display()))?,
    )?;
    let mut text = String::new();
    zip.by_name("version.json")?
        .take(64 * 1024)
        .read_to_string(&mut text)?;
    let json: serde_json::Value = serde_json::from_str(&text)?;
    let name = json["id"].as_str().context("version.json id")?.to_owned();
    let protocol = json["protocol_version"]
        .as_u64()
        .and_then(|p| u32::try_from(p).ok())
        .context("version.json protocol_version")?;
    Ok((name, protocol))
}

pub fn run(layout: &Layout, old: &str, canonical: &str) -> Result<()> {
    for ver in [old, canonical] {
        let reports = layout.generated_dir(ver).join("reports");
        if !reports.join("packets.json").is_file() {
            bail!(
                "{} is missing; run `cargo xtask fetch --version {ver}` and `cargo xtask datagen --version {ver}` first",
                reports.display()
            );
        }
    }
    let (old_dir, new_dir) = (
        layout.generated_dir(old).join("reports"),
        layout.generated_dir(canonical).join("reports"),
    );
    let (old_name, old_protocol) = version_info(layout, old)?;
    let (new_name, new_protocol) = version_info(layout, canonical)?;
    let blob = version_tables(
        &Reports {
            dir: &old_dir,
            name: &old_name,
            protocol: old_protocol,
        },
        &Reports {
            dir: &new_dir,
            name: &new_name,
            protocol: new_protocol,
        },
    )?;
    let out = layout.version_tables(old);
    fs::create_dir_all(out.parent().context("tables path")?)?;
    fs::write(&out, &blob)?;
    println!(
        "[version-tables] {old_name} ({old_protocol}) -> {new_name} ({new_protocol}): {} ({} bytes)",
        out.display(),
        blob.len()
    );
    Ok(())
}
