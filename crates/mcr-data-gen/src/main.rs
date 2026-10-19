use std::path::PathBuf;

use anyhow::{Context, Result};
use mcr_data_gen::{Inputs, generate, item_tooltips, write_module};

fn default_cache() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .map(|p| p.join(".cache"))
        .find(|p| p.is_dir())
        .unwrap_or_else(|| manifest.join(".cache"))
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let version = args.next().unwrap_or_else(|| "26.3".to_owned());
    let cache = std::env::var_os("MCR_CACHE").map_or_else(default_cache, PathBuf::from);
    let out = args.next().map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../mcr-data/src/generated")
                .join(format!("v{}", version.replace('.', "_")))
        },
        PathBuf::from,
    );
    let inputs = Inputs::in_cache(&cache, &version);
    let blob = generate(&inputs).context("generating tables")?;
    let tooltips = item_tooltips(&inputs).context("generating item tooltips")?;
    write_module(&out, &blob, &tooltips)?;
    println!("wrote {} ({} bytes)", out.display(), blob.len());
    Ok(())
}
