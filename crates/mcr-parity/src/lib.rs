//! Differential harnesses: seeded cases that vanilla's classes (through `java/mcr-oracle`) and our
//! probes both answer, and the diff between the two. Driven by `cargo xtask parity`.

pub mod diff;
pub mod font;
pub mod jsonl;
pub mod rng;

use std::path::PathBuf;

use serde_json::Value;

pub use diff::{Mismatch, Report, first_difference};
pub use jsonl::Case;
pub use rng::Rng;

#[derive(Debug, thiserror::Error)]
pub enum ParityError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{path}:{line}: {source}")]
    Json {
        path: String,
        line: usize,
        source: serde_json::Error,
    },
    #[error(transparent)]
    Ui(#[from] mcr_ui::UiError),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, ParityError>;

/// What a probe needs to load the same resources the oracle loads.
#[derive(Clone, Debug)]
pub struct Env {
    pub client_jar: PathBuf,
    /// The launcher assets dir and asset index id (`unifont.zip` lives only there).
    pub assets: Option<(PathBuf, String)>,
}

pub trait Harness: Sync {
    fn name(&self) -> &'static str;

    /// Repository paths (files or directories) whose contents decide our side's result.
    fn scope(&self) -> &'static [&'static str];

    fn count(&self) -> usize;

    fn case(&self, rng: &mut Rng) -> Value;

    /// One normalized output per case, in case order.
    fn probe(&self, env: &Env, cases: &[Case]) -> Result<Vec<Value>>;
}

pub static HARNESSES: &[&dyn Harness] = &[&font::FontHarness];

pub fn harness(name: &str) -> Option<&'static dyn Harness> {
    HARNESSES.iter().copied().find(|h| h.name() == name)
}

/// The harness's cases, seeded by its name so both sides and every rerun see the same file.
pub fn generate(harness: &dyn Harness) -> Vec<Case> {
    let mut rng = Rng::seeded(harness.name());
    (0..harness.count())
        .map(|id| Case {
            id: id as u32,
            case: harness.case(&mut rng),
        })
        .collect()
}

/// Rounds to a fixed grid so float formatting never shows up as a mismatch.
pub fn round(value: f64, step: f64) -> f64 {
    (value / step).round() * step
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic_and_ids_are_dense() {
        for h in HARNESSES {
            let a = generate(*h);
            let b = generate(*h);
            assert_eq!(a, b, "{}", h.name());
            assert!(a.iter().enumerate().all(|(i, c)| c.id as usize == i));
        }
    }

    #[test]
    fn harness_names_are_unique() {
        let mut names: Vec<_> = HARNESSES.iter().map(|h| h.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), HARNESSES.len());
    }
}
