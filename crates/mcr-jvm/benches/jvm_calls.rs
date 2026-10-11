// Criterion must run on the mcr-java thread inside a downcall (the only place upcalls are allowed), so
// Java calls `host_call(OP_CRITERION)` and the benches run inside that handler.

use std::hint::black_box;

use criterion::Criterion;
use mcr_jvm::{
    DestroyOutcome, HomeSearch, JavaVm, JvmConfig, JvmOptions, StaticVoidMethod, VmOutcome,
    dev_cache_dir, upcall,
};

const OP_CRITERION: i32 = 5;

fn run_benches() -> i64 {
    let mut c = Criterion::default().configure_from_args();
    c.bench_function("upcall_ping", |b| {
        b.iter(|| black_box(upcall::selftest_ping()).unwrap())
    });
    let jni = StaticVoidMethod::lookup("dev.mcr.boot.selftest.RoundTrip", "jniTarget").unwrap();
    c.bench_function("jni_static_void", |b| {
        b.iter(|| black_box(jni.call()).unwrap())
    });
    c.final_summary();
    0
}

fn main() {
    if std::env::var("MCR_JVM_TESTS").as_deref() != Ok("1") {
        eprintln!("jvm_calls skipped: set MCR_JVM_TESTS=1 to boot a JVM");
        return;
    }
    let cache = dev_cache_dir();
    let jar = cache.join("java/mcr-boot.jar");
    assert!(
        jar.is_file(),
        "{} missing: run `cargo xtask java-build`",
        jar.display()
    );
    let home = HomeSearch::from_env(Some(&cache)).locate().expect("JDK 25");
    let mut config = JvmConfig::new(
        home,
        JvmOptions {
            class_path: vec![jar],
            max_heap: "128m".into(),
            extra: Vec::new(),
        },
    );
    config.args = vec!["bench".into()];
    config.host_call = Some(Box::new(|op, _| match op {
        OP_CRITERION => run_benches(),
        _ => -1,
    }));
    let vm = JavaVm::start(config).expect("JVM start");
    match vm.join() {
        VmOutcome::Finished {
            boot: Ok(()),
            destroy: DestroyOutcome::Destroyed,
        } => {}
        other => panic!("unexpected VM outcome {other:?}"),
    }
}
