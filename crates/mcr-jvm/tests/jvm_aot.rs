// HotSpot allows one VM per process, so each session runs as a child of this test binary.

use std::path::{Path, PathBuf};
use std::process::Command;

use mcr_jvm::aot::{AotCache, AotPlan, CreateOutcome, KeyInputs};
use mcr_jvm::{HomeSearch, JavaHome, JavaVm, JvmConfig, JvmOptions, VmOutcome, dev_cache_dir};

const TEST_NAME: &str = "jvm_aot";
const CHILD_ENV: &str = "MCR_AOT_CHILD";
const DIR_ENV: &str = "MCR_AOT_DIR";

fn home() -> JavaHome {
    HomeSearch::from_env(Some(&dev_cache_dir()))
        .locate()
        .expect("JDK 25")
}

fn options() -> JvmOptions {
    JvmOptions {
        class_path: vec![dev_cache_dir().join("java/mcr-boot.jar")],
        max_heap: "128m".into(),
        extra: Vec::new(),
    }
}

fn cache(dir: &Path) -> AotCache {
    let options = options();
    let stable: Vec<String> = options
        .to_strings()
        .unwrap()
        .into_iter()
        .filter(|o| !o.starts_with("-D"))
        .collect();
    AotCache::new(
        dir,
        &KeyInputs {
            java_home: home().path(),
            options: &stable,
            class_path: &options.class_path,
            jars: &[],
            extra: TEST_NAME,
        },
    )
    .unwrap()
}

/// One JVM session with `plan`; prints whether the cache got mapped.
fn child(dir: &Path, mode: &str) {
    let cache = cache(dir);
    let plan = cache.plan();
    match mode {
        "record" => assert!(plan.records(), "{plan:?}"),
        _ => assert!(matches!(plan, AotPlan::Use(_)), "{plan:?}"),
    }
    cache.begin(&plan).unwrap();
    let mut options = options();
    options.extra.extend(plan.options());
    let mut cfg = JvmConfig::new(home(), options);
    cfg.destroy_timeout = std::time::Duration::from_secs(60);
    let vm = JavaVm::start(cfg).expect("JVM start with any cache state");
    if let AotPlan::Use(path) = &plan {
        println!("mapped={:?}", mcr_jvm::aot::mapped(path));
    }
    let clean = matches!(vm.join(), VmOutcome::Finished { boot: Ok(()), .. });
    if plan.records() {
        println!("kept={}", cache.end_recording(clean).unwrap());
    }
}

fn run_child(dir: &Path, mode: &str) -> String {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, mode)
        .env(DIR_ENV, dir)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{mode} child failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
}

#[test]
fn jvm_aot() {
    if std::env::var("MCR_JVM_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set MCR_JVM_TESTS=1 to boot a JVM");
        return;
    }
    if let (Ok(mode), Ok(dir)) = (std::env::var(CHILD_ENV), std::env::var(DIR_ENV)) {
        child(Path::new(&dir), &mode);
        return;
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("jvm-aot");
    let _ = std::fs::remove_dir_all(&dir);
    let c = cache(&dir);

    assert!(run_child(&dir, "record").contains("kept=true"));
    let options = options();
    let stable: Vec<String> = options
        .to_strings()
        .unwrap()
        .into_iter()
        .filter(|o| !o.starts_with("-D"))
        .collect();
    c.write_job(&home().java(), &stable, &options.class_path)
        .unwrap();
    assert!(c.needs_create());
    let outcome = c.create().unwrap();
    assert!(
        matches!(outcome, CreateOutcome::Created { bytes } if bytes > 1 << 20),
        "{outcome:?}"
    );

    assert!(run_child(&dir, "use").contains("mapped=Some(true)"));

    let cache_path = c.cache_path();
    let bytes = std::fs::read(&cache_path).unwrap();
    std::fs::write(&cache_path, &bytes[..bytes.len() / 2]).unwrap();
    assert!(
        run_child(&dir, "use").contains("mapped=Some(false)"),
        "a truncated cache is refused and the JVM still starts"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
