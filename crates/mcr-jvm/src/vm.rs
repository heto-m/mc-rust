use std::ffi::{CString, c_void};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Condvar, Mutex, OnceLock, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use jni_sys::{
    JNI_FALSE, JNI_OK, JNI_VERSION_21, JNIEnv, JavaVM, JavaVMInitArgs, JavaVMOption, jint, jsize,
    jvalue,
};
use libloading::Library;

use crate::host::{self, HostCallFn};
use crate::{JavaHome, JvmError, JvmOptions, table, upcall};

pub const JAVA_THREAD_NAME: &str = "mcr-java";
pub const JAVA_THREAD_STACK: usize = 16 << 20;
pub const DESTROY_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_BOOT_CLASS: &str = "dev/mcr/boot/McrBoot";
const BOOT_SIGNATURE: &std::ffi::CStr = c"(J[Ljava/lang/String;)V";

type CreateJavaVm =
    unsafe extern "system" fn(*mut *mut JavaVM, *mut *mut c_void, *mut c_void) -> jint;
type GetCreatedJavaVms = unsafe extern "system" fn(*mut *mut JavaVM, jsize, *mut jsize) -> jint;

static STARTED: AtomicBool = AtomicBool::new(false);
static LIBJVM: OnceLock<Library> = OnceLock::new();
pub(crate) static VM: AtomicPtr<JavaVM> = AtomicPtr::new(std::ptr::null_mut());
static EXIT: Mutex<ExitState> = Mutex::new(ExitState {
    code: None,
    released: false,
    events: None,
});
static EXIT_CHANGED: Condvar = Condvar::new();

struct ExitState {
    code: Option<i32>,
    released: bool,
    events: Option<Sender<Event>>,
}

enum Event {
    BootReturned(Result<(), JvmError>),
    Destroyed(jint),
    Exit(i32),
}

pub struct JvmConfig {
    pub home: JavaHome,
    pub options: JvmOptions,
    /// Binary name with slashes, e.g. `dev/mcr/boot/McrBoot`.
    pub boot_class: String,
    pub args: Vec<String>,
    pub host_call: Option<HostCallFn>,
    /// How long [`JavaVm::join`] waits for `DestroyJavaVM`, which also writes an AOT recording.
    pub destroy_timeout: Duration,
}

impl JvmConfig {
    pub fn new(home: JavaHome, options: JvmOptions) -> Self {
        Self {
            home,
            options,
            boot_class: DEFAULT_BOOT_CLASS.into(),
            args: Vec::new(),
            host_call: None,
            destroy_timeout: DESTROY_TIMEOUT,
        }
    }
}

impl fmt::Debug for JvmConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JvmConfig")
            .field("home", &self.home)
            .field("options", &self.options)
            .field("boot_class", &self.boot_class)
            .field("args", &self.args)
            .field("host_call", &self.host_call.is_some())
            .field("destroy_timeout", &self.destroy_timeout)
            .finish()
    }
}

#[derive(Debug)]
pub enum DestroyOutcome {
    Destroyed,
    Failed(i32),
    /// `DestroyJavaVM` did not return within [`JvmConfig::destroy_timeout`]; the thread is left behind.
    Leaked,
}

#[derive(Debug)]
pub enum VmOutcome {
    /// Java called `System.exit`/`Runtime.halt`. The VM is frozen in the exit hook until
    /// [`release_exit`] lets it terminate the process, or the process exits on its own.
    Exited { code: i32 },
    Finished {
        boot: Result<(), JvmError>,
        destroy: DestroyOutcome,
    },
}

/// The embedded JVM. Only one can ever be started per process.
#[derive(Debug)]
pub struct JavaVm {
    events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
    destroy_timeout: Duration,
}

impl JavaVm {
    pub fn start(mut config: JvmConfig) -> Result<Self, JvmError> {
        if STARTED.swap(true, Ordering::AcqRel) {
            return Err(JvmError::AlreadyStarted);
        }
        let prepared = prepare(&config);
        let (create, options) = match prepared {
            Ok(p) => p,
            Err(e) => {
                STARTED.store(false, Ordering::Release);
                return Err(e);
            }
        };
        host::install(config.host_call.take());
        let (events_tx, events) = mpsc::channel();
        EXIT.lock().unwrap_or_else(PoisonError::into_inner).events = Some(events_tx.clone());
        let (created_tx, created_rx) = mpsc::sync_channel(1);
        let destroy_timeout = config.destroy_timeout;
        let boot_class = config.boot_class;
        let args = config.args;
        let thread = std::thread::Builder::new()
            .name(JAVA_THREAD_NAME.into())
            .stack_size(JAVA_THREAD_STACK)
            .spawn(move || java_thread(create, options, boot_class, args, created_tx, events_tx))
            .map_err(JvmError::Spawn)?;
        match created_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                events,
                thread: Some(thread),
                destroy_timeout,
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(JvmError::ThreadLost),
        }
    }

    pub fn exit_code(&self) -> Option<i32> {
        exit_code()
    }

    /// Blocks until Java exits, or until the boot call returns and the VM is destroyed (bounded by
    /// [`JvmConfig::destroy_timeout`]).
    pub fn join(mut self) -> VmOutcome {
        let mut boot = None;
        loop {
            let event = match &boot {
                None => self
                    .events
                    .recv()
                    .map_err(|_| RecvTimeoutError::Disconnected),
                Some(_) => self.events.recv_timeout(self.destroy_timeout),
            };
            match event {
                Ok(Event::Exit(code)) => return VmOutcome::Exited { code },
                Ok(Event::BootReturned(result)) => boot = Some(result),
                Ok(Event::Destroyed(rc)) => {
                    if let Some(t) = self.thread.take() {
                        let _ = t.join();
                    }
                    let destroy = if rc == JNI_OK {
                        DestroyOutcome::Destroyed
                    } else {
                        DestroyOutcome::Failed(rc)
                    };
                    return VmOutcome::Finished {
                        boot: boot.unwrap_or(Err(JvmError::ThreadLost)),
                        destroy,
                    };
                }
                Err(RecvTimeoutError::Timeout) => {
                    eprintln!(
                        "mcr-jvm: DestroyJavaVM did not finish in {:?}; leaking the VM",
                        self.destroy_timeout
                    );
                    return VmOutcome::Finished {
                        boot: boot.unwrap_or(Err(JvmError::ThreadLost)),
                        destroy: DestroyOutcome::Leaked,
                    };
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return VmOutcome::Finished {
                        boot: boot.unwrap_or(Err(JvmError::ThreadLost)),
                        destroy: DestroyOutcome::Failed(-1),
                    };
                }
            }
        }
    }
}

pub fn exit_code() -> Option<i32> {
    EXIT.lock().unwrap_or_else(PoisonError::into_inner).code
}

/// Lets a blocked exit hook return, after which HotSpot terminates the process with its code.
pub fn release_exit() {
    EXIT.lock().unwrap_or_else(PoisonError::into_inner).released = true;
    EXIT_CHANGED.notify_all();
}

extern "system" fn exit_hook(code: jint) {
    let mut state = EXIT.lock().unwrap_or_else(PoisonError::into_inner);
    state.code = Some(code);
    if let Some(events) = &state.events {
        let _ = events.send(Event::Exit(code));
    }
    EXIT_CHANGED.notify_all();
    while !state.released {
        state = EXIT_CHANGED
            .wait(state)
            .unwrap_or_else(PoisonError::into_inner);
    }
}

extern "system" fn abort_hook() {
    eprintln!("mcr-jvm: the JVM aborted");
    std::process::abort();
}

fn prepare(config: &JvmConfig) -> Result<(CreateJavaVm, Vec<CString>), JvmError> {
    let options = config
        .options
        .to_strings()?
        .into_iter()
        .map(|o| CString::new(o).map_err(|e| JvmError::InvalidOption(e.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    if config.boot_class.contains('\0') {
        return Err(JvmError::InvalidOption(config.boot_class.clone()));
    }
    let path = config.home.libjvm();
    let lib = match LIBJVM.get() {
        Some(lib) => lib,
        None => {
            // SAFETY: loading libjvm runs its initialisers, which have no preconditions on our side.
            let lib = unsafe { Library::new(&path) }.map_err(|source| JvmError::Load {
                path: path.clone(),
                source,
            })?;
            LIBJVM.get_or_init(|| lib)
        }
    };
    let load = |source| JvmError::Load {
        path: path.clone(),
        source,
    };
    // SAFETY: the symbol types match the JNI invocation API declarations in jni.h.
    let (create, created) = unsafe {
        let create = *lib
            .get::<CreateJavaVm>(b"JNI_CreateJavaVM\0")
            .map_err(load)?;
        let created = *lib
            .get::<GetCreatedJavaVms>(b"JNI_GetCreatedJavaVMs\0")
            .map_err(load)?;
        (create, created)
    };
    let mut existing: *mut JavaVM = std::ptr::null_mut();
    let mut count: jsize = 0;
    // SAFETY: valid out-pointers for a buffer of length 1.
    let rc = unsafe { created(&mut existing, 1, &mut count) };
    if rc == JNI_OK && count > 0 {
        return Err(JvmError::AlreadyStarted);
    }
    Ok((create, options))
}

fn java_thread(
    create: CreateJavaVm,
    options: Vec<CString>,
    boot_class: String,
    args: Vec<String>,
    created: mpsc::SyncSender<Result<(), JvmError>>,
    events: Sender<Event>,
) {
    upcall::mark_java_thread();
    let mut vm_options: Vec<JavaVMOption> = options
        .iter()
        .map(|o| JavaVMOption {
            optionString: o.as_ptr().cast_mut(),
            extraInfo: std::ptr::null_mut(),
        })
        .collect();
    vm_options.push(JavaVMOption {
        optionString: c"exit".as_ptr().cast_mut(),
        extraInfo: exit_hook as *mut c_void,
    });
    vm_options.push(JavaVMOption {
        optionString: c"abort".as_ptr().cast_mut(),
        extraInfo: abort_hook as *mut c_void,
    });
    let mut init = JavaVMInitArgs {
        version: JNI_VERSION_21,
        nOptions: vm_options.len() as jint,
        options: vm_options.as_mut_ptr(),
        ignoreUnrecognized: JNI_FALSE,
    };
    let mut vm: *mut JavaVM = std::ptr::null_mut();
    let mut env: *mut JNIEnv = std::ptr::null_mut();
    // SAFETY: init args, option strings and out-pointers stay alive for the call; HotSpot copies them.
    let rc = unsafe { create(&mut vm, (&raw mut env).cast(), (&raw mut init).cast()) };
    drop(vm_options);
    drop(options);
    if rc != JNI_OK || vm.is_null() || env.is_null() {
        let _ = created.send(Err(JvmError::Create(rc)));
        return;
    }
    VM.store(vm, Ordering::Release);
    let _ = created.send(Ok(()));

    // SAFETY: env belongs to this thread, which JNI_CreateJavaVM attached.
    let boot = unsafe { boot_call(env, &boot_class, &args) };
    let _ = events.send(Event::BootReturned(boot));
    // SAFETY: vm is the live VM created above; DestroyJavaVM is called once, from its creating thread.
    let rc = unsafe { ((**vm).v1_4.DestroyJavaVM)(vm) };
    VM.store(std::ptr::null_mut(), Ordering::Release);
    let _ = events.send(Event::Destroyed(rc));
}

/// # Safety
/// `env` must be the `JNIEnv` of the current, attached thread.
unsafe fn boot_call(env: *mut JNIEnv, class: &str, args: &[String]) -> Result<(), JvmError> {
    // SAFETY: env is valid per the caller; every JNI result is checked before use.
    unsafe {
        let jni = &(**env).v1_6;
        let class_name = CString::new(class).map_err(|_| JvmError::InvalidOption(class.into()))?;
        let cls = (jni.FindClass)(env, class_name.as_ptr());
        if cls.is_null() {
            describe_and_clear(env);
            return Err(JvmError::BootClassNotFound(class.into()));
        }
        let main = (jni.GetStaticMethodID)(env, cls, c"main".as_ptr(), BOOT_SIGNATURE.as_ptr());
        if main.is_null() {
            describe_and_clear(env);
            return Err(JvmError::BootMethodNotFound(class.into()));
        }
        let string_cls = (jni.FindClass)(env, c"java/lang/String".as_ptr());
        let len =
            jsize::try_from(args.len()).map_err(|_| JvmError::Jni("too many boot arguments"))?;
        let array = (jni.NewObjectArray)(env, len, string_cls, std::ptr::null_mut());
        if string_cls.is_null() || array.is_null() {
            describe_and_clear(env);
            return Err(JvmError::Jni("cannot allocate the boot argument array"));
        }
        for (i, arg) in args.iter().enumerate() {
            let utf16: Vec<u16> = arg.encode_utf16().collect();
            let chars = jsize::try_from(utf16.len())
                .map_err(|_| JvmError::Jni("boot argument too long"))?;
            let s = (jni.NewString)(env, utf16.as_ptr(), chars);
            if s.is_null() {
                describe_and_clear(env);
                return Err(JvmError::Jni("cannot allocate a boot argument"));
            }
            (jni.SetObjectArrayElement)(env, array, i as jsize, s);
            (jni.DeleteLocalRef)(env, s);
        }
        let call_args = [
            jvalue {
                j: table::table_address(),
            },
            jvalue { l: array },
        ];
        (jni.CallStaticVoidMethodA)(env, cls, main, call_args.as_ptr());
        if (jni.ExceptionCheck)(env) {
            describe_and_clear(env);
            return Err(JvmError::BootException);
        }
        (jni.DeleteLocalRef)(env, array);
        (jni.DeleteLocalRef)(env, string_cls);
        (jni.DeleteLocalRef)(env, cls);
        Ok(())
    }
}

/// # Safety
/// `env` must be the `JNIEnv` of the current, attached thread.
pub(crate) unsafe fn describe_and_clear(env: *mut JNIEnv) {
    // SAFETY: guaranteed by the caller.
    unsafe {
        let jni = &(**env).v1_6;
        if (jni.ExceptionCheck)(env) {
            (jni.ExceptionDescribe)(env);
            (jni.ExceptionClear)(env);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_preparation_does_not_latch_the_single_start() {
        let dir = std::env::temp_dir().join(format!("mcr-jvm-vm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("release"), "JAVA_VERSION=\"25\"\n").unwrap();
        let home = JavaHome::open(&dir).unwrap();
        for _ in 0..2 {
            let config = JvmConfig::new(home.clone(), JvmOptions::default());
            assert!(matches!(JavaVm::start(config), Err(JvmError::Load { .. })));
        }
        assert!(!STARTED.load(Ordering::Acquire));
    }
}
