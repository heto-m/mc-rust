//! `mcr-parity list | cases | probe | diff`; `cargo xtask parity` drives it.
//! Exit codes: 0 done (diff: every case matched), 1 diff found mismatches, 2 usage or I/O error.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use mcr_parity::{Env, HARNESSES, diff, harness, jsonl};
use serde_json::json;

const USAGE: &str = "usage:
  mcr-parity list
  mcr-parity cases <name> <out.jsonl>
  mcr-parity probe <name> --cases <file> --out <file> --client-jar <jar> [--assets <dir> --asset-index <id>]
  mcr-parity diff <name> --cases <file> --vanilla <file> --mcr <file> [--max <n>] [--summary <file>]";

struct Args {
    positional: Vec<String>,
    flags: Vec<(String, String)>,
}

impl Args {
    fn parse(raw: impl Iterator<Item = String>) -> Result<Self> {
        let mut args = Self {
            positional: Vec::new(),
            flags: Vec::new(),
        };
        let mut raw = raw.peekable();
        while let Some(a) = raw.next() {
            if let Some(name) = a.strip_prefix("--") {
                let value = raw
                    .next()
                    .with_context(|| format!("--{name} needs a value"))?;
                args.flags.push((name.to_owned(), value));
            } else {
                args.positional.push(a);
            }
        }
        Ok(args)
    }

    fn opt(&self, name: &str) -> Option<&str> {
        self.flags
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn path(&self, name: &str) -> Result<PathBuf> {
        self.opt(name)
            .map(PathBuf::from)
            .with_context(|| format!("missing --{name}\n{USAGE}"))
    }

    fn harness(&self) -> Result<&'static dyn mcr_parity::Harness> {
        let name = self
            .positional
            .get(1)
            .with_context(|| format!("missing harness name\n{USAGE}"))?;
        harness(name).with_context(|| format!("unknown harness {name}"))
    }
}

fn run() -> Result<ExitCode> {
    let args = Args::parse(std::env::args().skip(1))?;
    match args.positional.first().map(String::as_str) {
        Some("list") => {
            for h in HARNESSES {
                println!(
                    "{}",
                    json!({"name": h.name(), "scope": h.scope(), "cases": h.count()})
                );
            }
        }
        Some("cases") => {
            let h = args.harness()?;
            let out = args
                .positional
                .get(2)
                .with_context(|| format!("missing output path\n{USAGE}"))?;
            jsonl::write_cases(out.as_ref(), &mcr_parity::generate(h))?;
        }
        Some("probe") => {
            let h = args.harness()?;
            // Panics inside `guarded` become outputs; the default hook would print each one.
            std::panic::set_hook(Box::new(|_| {}));
            let cases = jsonl::read_cases(&args.path("cases")?)?;
            let env = Env {
                client_jar: args.path("client-jar")?,
                assets: match (args.opt("assets"), args.opt("asset-index")) {
                    (Some(d), Some(i)) => Some((PathBuf::from(d), i.to_owned())),
                    (None, None) => None,
                    _ => bail!("--assets and --asset-index go together"),
                },
            };
            let outs = h.probe(&env, &cases)?;
            jsonl::write_outputs(&args.path("out")?, &cases, outs)?;
        }
        Some("diff") => {
            let h = args.harness()?;
            let cases = jsonl::read_cases(&args.path("cases")?)?;
            let vanilla = jsonl::read_outputs(&args.path("vanilla")?)?;
            let mcr = jsonl::read_outputs(&args.path("mcr")?)?;
            let max = args.opt("max").map_or(Ok(5), str::parse)?;
            let report = diff::diff_with(&cases, &vanilla, &mcr, &|field| h.tolerance(field));
            print!("{}", report.render(h.name(), &cases, &vanilla, &mcr, max));
            if let Some(path) = args.opt("summary") {
                let summary = json!({
                    "cases": report.cases,
                    "matched": report.matched,
                    "classes": report.classes(),
                    "failing": report.mismatches.iter().map(|m| m.id).collect::<Vec<_>>(),
                });
                std::fs::write(path, summary.to_string()).with_context(|| path.to_owned())?;
            }
            if !report.passed() {
                return Ok(ExitCode::from(1));
            }
        }
        _ => bail!("{USAGE}"),
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("mcr-parity: {e:#}");
            ExitCode::from(2)
        }
    }
}
