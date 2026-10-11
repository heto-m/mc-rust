// HotSpot allows one VM per process, so this single test runs every scenario in sequence and re-runs
// itself as a child process for the System.exit path.

use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mcr_jvm::{
    DestroyOutcome, Doorbell, HomeSearch, JavaVm, JvmConfig, JvmError, JvmOptions, PushError, Ring,
    StaticVoidMethod, VmOutcome, dev_cache_dir, upcall,
};

const OP_ADDRESS: i32 = 1;
const OP_UPCALLS: i32 = 2;
const OP_BENCH_UPCALL: i32 = 3;
const OP_BENCH_JNI: i32 = 4;
const KIND_END: u16 = 0xFFF0;
const KIND_RESULT: u16 = 0xFFF1;
const ECHO_BYTES: usize = 1 << 20;
const RING_CAPACITY: usize = 64 << 10;
const TEST_NAME: &str = "jvm_round_trip";
const CHILD_ENV: &str = "MCR_JVM_CHILD";
const DEADLINE: Duration = Duration::from_secs(120);

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn messages() -> Vec<(u16, Vec<u8>)> {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut out = Vec::new();
    let mut total = 0;
    while total < ECHO_BYTES {
        let len = ((rng.next() % 4096) as usize).min(ECHO_BYTES - total);
        let payload: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        out.push(((rng.next() % 1000) as u16, payload));
        total += len;
    }
    out
}

fn config(args: &[&str]) -> JvmConfig {
    let cache = dev_cache_dir();
    let jar = cache.join("java/mcr-boot.jar");
    assert!(
        jar.is_file(),
        "{} missing: run `cargo xtask java-build`",
        jar.display()
    );
    let home = HomeSearch::from_env(Some(&cache)).locate().expect("JDK 25");
    let mut cfg = JvmConfig::new(
        home,
        JvmOptions {
            class_path: vec![jar],
            max_heap: "128m".into(),
            extra: Vec::new(),
        },
    );
    cfg.args = args.iter().map(|s| (*s).to_owned()).collect();
    cfg
}

#[derive(Debug, Default)]
struct UpcallReport {
    counter: u64,
    throw_status: Option<upcall::UpcallError>,
    ping_after_throw: bool,
}

fn run_upcall_checks(report: &Mutex<UpcallReport>) -> i64 {
    let mut counter = Box::new(0u64);
    let addr = &raw mut *counter as i64;
    for _ in 0..100 {
        if upcall::selftest_add(addr, 5).is_err() {
            return -1;
        }
    }
    let throw = upcall::selftest_throw();
    let ping = upcall::selftest_ping().is_ok();
    let mut r = report.lock().unwrap();
    r.counter = *counter;
    r.throw_status = throw.err();
    r.ping_after_throw = ping;
    100
}

fn time_loop(n: i64, mut f: impl FnMut()) -> i64 {
    let start = Instant::now();
    for _ in 0..n {
        f();
    }
    start.elapsed().as_nanos() as i64
}

#[test]
fn jvm_round_trip() {
    if std::env::var("MCR_JVM_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set MCR_JVM_TESTS=1 to boot a JVM");
        return;
    }
    if std::env::var(CHILD_ENV).as_deref() == Ok("exit") {
        exit_child();
    }

    let to_java = Ring::new(RING_CAPACITY).unwrap();
    let from_java = Ring::new(RING_CAPACITY).unwrap();
    let to_bell = Arc::new(Doorbell::new());
    let from_bell = Arc::new(Doorbell::new());
    let addresses = [
        to_java.addr(),
        to_bell.addr(),
        from_java.addr(),
        from_bell.addr(),
    ];
    let report = Arc::new(Mutex::new(UpcallReport::default()));

    let mut cfg = config(&["selftest"]);
    let handler_report = report.clone();
    cfg.host_call = Some(Box::new(move |op, arg| match op {
        OP_ADDRESS => addresses.get(arg as usize).copied().unwrap_or(0),
        OP_UPCALLS => run_upcall_checks(&handler_report),
        OP_BENCH_UPCALL => time_loop(arg, || upcall::selftest_ping().unwrap()),
        OP_BENCH_JNI => {
            let m =
                StaticVoidMethod::lookup("dev/mcr/boot/selftest/RoundTrip", "jniTarget").unwrap();
            time_loop(arg, || m.call().unwrap())
        }
        _ => -1,
    }));
    let vm = JavaVm::start(cfg).expect("JVM start");

    let expected = messages();
    let producer = {
        let (ring, bell, expected) = (to_java.clone(), to_bell.clone(), expected.clone());
        std::thread::spawn(move || {
            let mut w = ring.writer().unwrap();
            let start = Instant::now();
            let items = expected.iter().map(|(k, p)| (*k, p.as_slice()));
            for (kind, payload) in items.chain([(KIND_END, &[][..])]) {
                loop {
                    let seen = bell.seq();
                    match w.try_push(kind, payload) {
                        Ok(()) => break,
                        Err(PushError::Full) => {
                            assert!(start.elapsed() < DEADLINE, "producer stalled");
                            bell.wait(seen, Some(Duration::from_millis(100)));
                        }
                        Err(e) => panic!("{e}"),
                    }
                }
                bell.notify();
            }
        })
    };

    let mut reader = from_java.reader().unwrap();
    let start = Instant::now();
    let mut echoed = 0;
    let mut result_line = None;
    while result_line.is_none() {
        assert!(
            start.elapsed() < DEADLINE,
            "consumer stalled after {echoed} messages"
        );
        let seen = from_bell.seq();
        let Some(msg) = reader.try_pop().unwrap() else {
            from_bell.wait(seen, Some(Duration::from_millis(100)));
            continue;
        };
        match msg.kind() {
            KIND_RESULT => result_line = Some(String::from_utf8(msg.payload().to_vec()).unwrap()),
            KIND_END => assert_eq!(echoed, expected.len(), "END before all messages"),
            kind => {
                let (k, p) = &expected[echoed];
                assert_eq!(
                    (kind, msg.payload()),
                    (*k, p.as_slice()),
                    "message {echoed}"
                );
                echoed += 1;
            }
        }
        drop(msg);
        from_bell.notify();
    }
    producer.join().unwrap();

    match vm.join() {
        VmOutcome::Finished {
            boot: Ok(()),
            destroy: DestroyOutcome::Destroyed,
        } => {}
        other => panic!("unexpected VM outcome {other:?}"),
    }

    let line = result_line.unwrap();
    eprintln!("{line}");
    let field = |name: &str| -> String {
        line.split_whitespace()
            .find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='))
            .unwrap_or_else(|| panic!("{name} missing in {line}"))
            .to_owned()
    };
    let sum: u64 = expected
        .iter()
        .flat_map(|(_, p)| p.iter())
        .map(|&b| u64::from(b))
        .sum();
    assert_eq!(field("sum"), sum.to_string());
    assert_eq!(field("messages"), expected.len().to_string());
    assert_eq!(field("bytes"), ECHO_BYTES.to_string());
    assert_eq!(field("upcalls"), "100");
    assert_eq!(field("mismatch"), "ok");
    for cost in ["critical_ns", "normal_ns", "upcall_ns", "jni_ns"] {
        let ns: f64 = field(cost).parse().unwrap();
        assert!(ns > 0.0, "{cost}={ns}");
    }

    let report = report.lock().unwrap();
    assert_eq!(report.counter, 500);
    assert_eq!(
        report.throw_status,
        Some(upcall::UpcallError::Failed {
            name: "selftest_throw",
            status: -1
        })
    );
    assert!(
        report.ping_after_throw,
        "VM unusable after an upcall failure"
    );

    assert!(matches!(
        JavaVm::start(config(&["selftest"])),
        Err(JvmError::AlreadyStarted)
    ));

    let child = Command::new(std::env::current_exe().unwrap())
        .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "exit")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&child.stdout);
    assert_eq!(
        child.status.code(),
        Some(3),
        "child: {stdout}\n{}",
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(stdout.contains("exit hook recorded code 3"), "{stdout}");
}

fn exit_child() -> ! {
    let vm = JavaVm::start(config(&["selftest-exit"])).expect("JVM start");
    match vm.join() {
        VmOutcome::Exited { code } => {
            assert_eq!(mcr_jvm::exit_code(), Some(code));
            println!("exit hook recorded code {code}");
            std::process::exit(code);
        }
        other => panic!("expected an exit request, got {other:?}"),
    }
}
