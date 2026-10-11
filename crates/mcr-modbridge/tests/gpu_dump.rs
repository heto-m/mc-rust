mod gpu_common;

use gpu_common::*;
use mcr_modbridge::{Bridge, ShutdownOutcome};

/// Dumps every pipeline CreateInfo the frontend produces into `.cache/spv/26.3` for the shader tests.
#[test]
fn dump_pipelines() {
    if std::env::var("MCR_JVM_TESTS").as_deref() != Ok("1")
        || std::env::var("MCR_GPU_DUMP").as_deref() != Ok("1")
    {
        eprintln!("skipped: set MCR_JVM_TESTS=1 MCR_GPU_DUMP=1 to dump pipelines");
        return;
    }
    let dir = mcr_jvm::dev_cache_dir().join("spv/26.3");
    let _ = std::fs::remove_dir_all(&dir);
    let extra = vec![
        format!("-Dmcr.gpu.dump={}", dir.display()),
        "-Dmcr.gpu.selftest=compile-all".into(),
        "-Dmcr.gpu.device=null".into(),
    ];
    let mut bridge = Bridge::start(config("dump", &extra)).expect("bridge start");
    let report = info_value(&mut bridge, "gpu_pipelines");
    eprintln!("gpu_pipelines: {report}");
    let outcome = bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "{outcome:?}"
    );
    let count = std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
    eprintln!("dumped {count} pipelines into {}", dir.display());
    assert!(count > 100, "only {count} pipelines dumped");
}
