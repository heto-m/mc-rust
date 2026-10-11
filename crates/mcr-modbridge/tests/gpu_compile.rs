mod gpu_common;

use gpu_common::*;
use mcr_modbridge::{Bridge, ShutdownOutcome};

/// Every core and post pipeline without texel buffers compiles on wgpu; texel ones fail cleanly.
#[test]
fn vanilla_pipelines_compile_on_wgpu() {
    if !jvm_tests() {
        return;
    }
    let extra = vec!["-Dmcr.gpu.selftest=compile-all".into()];
    let mut bridge = Bridge::start(config("compile", &extra)).expect("bridge start");
    let report = info_value(&mut bridge, "gpu_pipelines");
    eprintln!("gpu_pipelines: {report}");
    let stats = mcr_modbridge::gpu::backend().expect("backend").stats();
    eprintln!("backend stats after the title screen: {stats:?}");
    let outcome = bridge.shutdown(mcr_modbridge::SHUTDOWN_TIMEOUT);
    assert!(
        matches!(outcome, ShutdownOutcome::Clean { .. }),
        "{outcome:?}"
    );
    let failed = report.split(" failed=").nth(1).unwrap_or("?");
    assert!(failed.is_empty(), "failed pipelines: {failed}");
    assert!(report.contains(" texel="), "{report}");
    assert!(
        stats.passes > 0 && stats.draws > 0,
        "the title screen drew nothing"
    );
}
