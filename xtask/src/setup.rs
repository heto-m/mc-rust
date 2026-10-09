use anyhow::{Context, Result};

use crate::datagen;
use crate::decompile;
use crate::fetch;
use crate::jdk;
use crate::paths::Layout;

pub const STEPS: [&str; 4] = ["fetch", "jdk", "datagen", "decompile"];

pub fn run(layout: &Layout, ver: &str, force: bool) -> Result<()> {
    run_steps(&STEPS, |step| match step {
        "fetch" => fetch::fetch(layout, Some(ver), force).map(|_| ()),
        "jdk" => jdk::ensure_jdk(layout, force).map(|_| ()),
        "datagen" => datagen::run(layout, ver, force),
        "decompile" => decompile::run(layout, ver, force),
        other => unreachable!("unknown setup step {other}"),
    })
}

pub fn run_steps<F>(steps: &[&str], mut run_step: F) -> Result<()>
where
    F: FnMut(&str) -> Result<()>,
{
    for step in steps {
        run_step(step).with_context(|| format!("setup step `{step}` failed"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;

    #[test]
    fn steps_run_in_order() {
        let mut seen = Vec::new();
        run_steps(&STEPS, |s| {
            seen.push(s.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, ["fetch", "jdk", "datagen", "decompile"]);
    }

    #[test]
    fn pipeline_stops_at_first_failure() {
        let mut seen = Vec::new();
        let result = run_steps(&STEPS, |s| {
            seen.push(s.to_string());
            if s == "datagen" {
                bail!("boom");
            }
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(seen, ["fetch", "jdk", "datagen"]);
    }
}
