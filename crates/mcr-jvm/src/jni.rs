use std::ffi::{CString, c_void};
use std::sync::atomic::Ordering;

use jni_sys::{JNI_OK, JNI_VERSION_21, JNIEnv, jclass, jmethodID, jsize, jvalue};

use crate::JvmError;
use crate::vm::{VM, describe_and_clear};

/// A cached `static void m()` handle for measuring JNI calls from Rust. Diagnostics only: the bridge
/// itself never calls Java through JNI after boot.
#[derive(Debug)]
pub struct StaticVoidMethod {
    env: *mut JNIEnv,
    class: jclass,
    method: jmethodID,
}

impl StaticVoidMethod {
    /// Looks up `class.name()V` through the system class loader on the current thread, which must be
    /// attached to the VM. `class` is a binary name with dots or slashes.
    pub fn lookup(class: &str, name: &str) -> Result<Self, JvmError> {
        let env = current_env().ok_or(JvmError::Jni("thread not attached to the VM"))?;
        let name_c = CString::new(name).map_err(|_| JvmError::InvalidOption(name.into()))?;
        // SAFETY: env is this thread's JNIEnv; results are null-checked and exceptions cleared.
        unsafe {
            let jni = &(**env).v1_6;
            let local = load_system_class(env, &class.replace('/', "."))?;
            let method = (jni.GetStaticMethodID)(env, local, name_c.as_ptr(), c"()V".as_ptr());
            if method.is_null() {
                describe_and_clear(env);
                (jni.DeleteLocalRef)(env, local);
                return Err(JvmError::Jni("method not found"));
            }
            let class = (jni.NewGlobalRef)(env, local);
            (jni.DeleteLocalRef)(env, local);
            if class.is_null() {
                return Err(JvmError::Jni("NewGlobalRef failed"));
            }
            Ok(Self { env, class, method })
        }
    }

    pub fn call(&self) -> Result<(), JvmError> {
        // SAFETY: the handle is !Send, so env is still this thread's JNIEnv, and class is a global ref.
        unsafe {
            let jni = &(**self.env).v1_6;
            (jni.CallStaticVoidMethodA)(self.env, self.class, self.method, std::ptr::null());
            if (jni.ExceptionCheck)(self.env) {
                describe_and_clear(self.env);
                return Err(JvmError::Jni("Java method threw"));
            }
        }
        Ok(())
    }
}

impl Drop for StaticVoidMethod {
    fn drop(&mut self) {
        // SAFETY: same thread and live global ref, as in `call`.
        unsafe { ((**self.env).v1_6.DeleteGlobalRef)(self.env, self.class) };
    }
}

/// `FindClass` inside an FFM downcall resolves against the boot loader (the nearest Java frame is a
/// java.base stub), so application classes go through `ClassLoader.getSystemClassLoader()`.
///
/// # Safety
/// `env` must be the current thread's `JNIEnv`.
unsafe fn load_system_class(env: *mut JNIEnv, dotted: &str) -> Result<jclass, JvmError> {
    // SAFETY: guaranteed by the caller; every result is null-checked and exceptions cleared.
    unsafe {
        let jni = &(**env).v1_6;
        let fail = |what| {
            describe_and_clear(env);
            JvmError::Jni(what)
        };
        let loader_cls = (jni.FindClass)(env, c"java/lang/ClassLoader".as_ptr());
        if loader_cls.is_null() {
            return Err(fail("java.lang.ClassLoader not found"));
        }
        let get_system = (jni.GetStaticMethodID)(
            env,
            loader_cls,
            c"getSystemClassLoader".as_ptr(),
            c"()Ljava/lang/ClassLoader;".as_ptr(),
        );
        let load = (jni.GetMethodID)(
            env,
            loader_cls,
            c"loadClass".as_ptr(),
            c"(Ljava/lang/String;)Ljava/lang/Class;".as_ptr(),
        );
        if get_system.is_null() || load.is_null() {
            (jni.DeleteLocalRef)(env, loader_cls);
            return Err(fail("ClassLoader methods not found"));
        }
        let loader = (jni.CallStaticObjectMethodA)(env, loader_cls, get_system, std::ptr::null());
        (jni.DeleteLocalRef)(env, loader_cls);
        if loader.is_null() {
            return Err(fail("no system class loader"));
        }
        let utf16: Vec<u16> = dotted.encode_utf16().collect();
        let len =
            jsize::try_from(utf16.len()).map_err(|_| JvmError::InvalidOption(dotted.into()))?;
        let name = (jni.NewString)(env, utf16.as_ptr(), len);
        if name.is_null() {
            (jni.DeleteLocalRef)(env, loader);
            return Err(fail("cannot allocate the class name"));
        }
        let args = [jvalue { l: name }];
        let class = (jni.CallObjectMethodA)(env, loader, load, args.as_ptr());
        (jni.DeleteLocalRef)(env, name);
        (jni.DeleteLocalRef)(env, loader);
        if class.is_null() || (jni.ExceptionCheck)(env) {
            return Err(fail("class not found"));
        }
        Ok(class)
    }
}

fn current_env() -> Option<*mut JNIEnv> {
    let vm = VM.load(Ordering::Acquire);
    if vm.is_null() {
        return None;
    }
    let mut env: *mut c_void = std::ptr::null_mut();
    // SAFETY: vm is live while non-null; GetEnv only reports the current thread's attachment.
    let rc = unsafe { ((**vm).v1_4.GetEnv)(vm, &mut env, JNI_VERSION_21) };
    (rc == JNI_OK && !env.is_null()).then_some(env.cast())
}
