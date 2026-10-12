//! The four downcalls Java's backend uses. Arguments arrive as a block of `u64` slots.
//!
//! Every entry returns `-1` on failure and keeps the message for [`gpu_error`]; nothing panics
//! across the boundary.

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::backend::{self, Backend, MAX_TRANSFER, OpError, OpResult};
use super::pass::{self, PassDesc};

pub const D_INFO: i32 = 1;
pub const D_BUFFER: i32 = 2;
pub const D_TRANSIENT: i32 = 3;
pub const D_HOST: i32 = 4;
pub const D_TEXTURE: i32 = 5;
pub const D_VIEW: i32 = 6;
pub const D_SAMPLER: i32 = 7;
pub const D_PIPELINE: i32 = 8;
pub const D_DESTROY: i32 = 9;
pub const D_ENCODER: i32 = 10;
pub const D_SURFACE: i32 = 11;
pub const D_SURFACE_CONFIGURE: i32 = 12;
pub const D_SURFACE_PRESENT: i32 = 13;
pub const D_STATS: i32 = 14;
/// `[slot, frame]`: tags the following submits for the compositor.
pub const D_SLOT: i32 = 15;

pub const E_CLEAR: i32 = 1;
pub const E_CLEAR_REGION: i32 = 2;
pub const E_WRITE_BUFFER: i32 = 3;
pub const E_COPY_BUFFER: i32 = 4;
pub const E_WRITE_TEXTURE: i32 = 5;
pub const E_BUFFER_TO_TEXTURE: i32 = 6;
pub const E_TEXTURE_TO_BUFFER: i32 = 7;
pub const E_TEXTURE_TO_TEXTURE: i32 = 8;
pub const E_FLUSH_MAPPED: i32 = 9;
pub const E_BLIT: i32 = 10;
pub const E_DIRTY: i32 = 11;
pub const E_SUBMIT: i32 = 12;
pub const E_POLL: i32 = 13;
pub const E_WAIT: i32 = 14;
/// Cuts the recording at a slot change without ending Java's submit.
pub const E_SPLIT: i32 = 15;

/// Slots in Java's argument block.
pub const MAX_ARGS: usize = 32;
/// `D_INFO`: `[min_ubo_align, max_texture, max_buffer, max_colors, features, text_len]` then text.
pub const INFO_SLOTS: usize = 6;
pub const FEATURE_POLYGON_LINE: u64 = 1;
pub const FEATURE_FIRST_INSTANCE: u64 = 2;
const MAX_PASS_DESC: usize = 1 + pass::MAX_COLOR_ATTACHMENTS * 6 + 8;
const MAX_STREAM_BYTES: usize = 64 << 20;
const LOGGED_ERRORS: u64 = 20;

thread_local! {
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

static REPORTED: AtomicU64 = AtomicU64::new(0);

fn record(e: &OpError) {
    let n = REPORTED.fetch_add(1, Ordering::Relaxed);
    if n < LOGGED_ERRORS || n.is_multiple_of(1000) {
        tracing::warn!(count = n + 1, "mod gpu backend: {e}");
    }
    if let Ok(b) = backend::backend() {
        b.stats.errors.fetch_add(1, Ordering::Relaxed);
    }
    LAST_ERROR.with(|l| *l.borrow_mut() = e.to_string());
}

fn guarded(f: impl FnOnce() -> OpResult<i64>) -> i64 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            record(&e);
            -1
        }
        Err(_) => {
            record(&OpError::Invalid("panic in the gpu backend".into()));
            -1
        }
    }
}

/// # Safety
/// `ptr` must be null or point at `count` readable `u64`s for the duration of the call.
unsafe fn slots<'a>(ptr: i64, count: i32) -> OpResult<&'a [u64]> {
    let count =
        usize::try_from(count).map_err(|_| OpError::Invalid("negative arg count".into()))?;
    if count == 0 {
        return Ok(&[]);
    }
    if ptr == 0 || count > MAX_ARGS.max(MAX_PASS_DESC) || ptr % 8 != 0 {
        return Err(OpError::Invalid("bad argument block".into()));
    }
    // SAFETY: Java passes an 8-aligned off-heap segment of `count` longs that outlives the call.
    Ok(unsafe { std::slice::from_raw_parts(ptr as usize as *const u64, count) })
}

/// # Safety
/// `ptr` must point at `len` readable bytes for the duration of the call.
unsafe fn bytes<'a>(ptr: u64, len: u64) -> OpResult<&'a [u8]> {
    if len == 0 {
        return Ok(&[]);
    }
    if ptr == 0 || len > MAX_TRANSFER {
        return Err(OpError::Invalid(format!(
            "bad data pointer or length {len}"
        )));
    }
    // SAFETY: Java passes the address of a live direct buffer of at least `len` bytes; the
    // bridge trusts in-process Java with raw memory, so only null and size are checked.
    Ok(unsafe { std::slice::from_raw_parts(ptr as usize as *const u8, len as usize) })
}

fn arg(a: &[u64], i: usize) -> OpResult<u64> {
    a.get(i)
        .copied()
        .ok_or_else(|| OpError::Invalid(format!("missing argument {i}")))
}

fn u32_arg(a: &[u64], i: usize) -> OpResult<u32> {
    u32::try_from(arg(a, i)?).map_err(|_| OpError::Invalid(format!("argument {i} out of range")))
}

fn f32_arg(a: &[u64], i: usize) -> OpResult<f32> {
    Ok(f32::from_bits(arg(a, i)? as u32))
}

fn handle(v: u64) -> i64 {
    v as i64
}

/// Runs `f` inside a validation scope so wgpu errors become reported failures, not panics.
fn scoped<T>(b: &Backend, f: impl FnOnce() -> OpResult<T>) -> OpResult<T> {
    let scope = b.gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let result = f();
    match pollster::block_on(scope.pop()) {
        None => result,
        Some(e) => Err(OpError::Wgpu(e.to_string())),
    }
}

fn info(b: &Backend, out: u64, cap: u64) -> OpResult<i64> {
    let g = b.gpu;
    let mut features = 0;
    if g.features.contains(wgpu::Features::POLYGON_MODE_LINE) {
        features |= FEATURE_POLYGON_LINE;
    }
    if g.features.contains(wgpu::Features::INDIRECT_FIRST_INSTANCE) {
        features |= FEATURE_FIRST_INSTANCE;
    }
    let text = format!(
        "{}\n{:?}\n{} {}\n{:?}",
        g.info.name, g.info.vendor, g.info.driver, g.info.driver_info, g.info.backend
    );
    let head = [
        u64::from(g.limits.min_uniform_buffer_offset_alignment),
        u64::from(g.limits.max_texture_dimension_2d),
        g.limits.max_buffer_size.min(backend::MAX_BUFFER),
        u64::from(g.limits.max_color_attachments),
        features,
        text.len() as u64,
    ];
    let need = INFO_SLOTS * 8 + text.len();
    if out == 0 || (cap as usize) < need {
        return Err(OpError::Invalid(format!("info needs {need} bytes")));
    }
    let mut data = Vec::with_capacity(need);
    for v in head {
        data.extend_from_slice(&v.to_ne_bytes());
    }
    data.extend_from_slice(text.as_bytes());
    // SAFETY: Java passes a writable off-heap segment of `cap` bytes, checked to fit `need`.
    unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), out as usize as *mut u8, need) };
    Ok(need as i64)
}

fn stats(b: &Backend, out: u64, cap: u64) -> OpResult<i64> {
    let s = b.stats();
    let values = [
        s.passes,
        s.ops,
        s.draws,
        s.skipped_draws,
        s.bind_hits,
        s.bind_misses,
        s.encode_ns,
        s.submits,
        s.uploaded,
        s.errors,
    ];
    if out == 0 || (cap as usize) < values.len() * 8 {
        return Err(OpError::Invalid("stats buffer too small".into()));
    }
    // SAFETY: Java passes a writable off-heap segment of `cap` bytes, checked to fit.
    unsafe {
        std::ptr::copy_nonoverlapping(values.as_ptr(), out as usize as *mut u64, values.len());
    }
    Ok(values.len() as i64)
}

pub extern "C" fn gpu_device(op: i32, args: i64, count: i32) -> i64 {
    guarded(|| {
        // SAFETY: see `slots`.
        let a = unsafe { slots(args, count) }?;
        let b = backend::backend()?;
        let lod = |v: u64| (v != u64::MAX).then(|| f64::from_bits(v));
        match op {
            D_INFO => info(b, arg(a, 0)?, arg(a, 1)?),
            D_BUFFER => {
                // SAFETY: see `bytes`.
                let init = unsafe { bytes(arg(a, 2)?, arg(a, 3)?) }?;
                let init = (arg(a, 2)? != 0).then_some(init);
                b.create_buffer(u32_arg(a, 0)?, arg(a, 1)?, init)
                    .map(handle)
            }
            D_TRANSIENT => b.create_transient_block(arg(a, 0)?).map(handle),
            D_HOST => b.buffer_host(arg(a, 0)?).map(|(addr, _)| addr),
            D_TEXTURE => b
                .create_texture(
                    u32_arg(a, 0)?,
                    u32_arg(a, 1)?,
                    u32_arg(a, 2)?,
                    u32_arg(a, 3)?,
                    u32_arg(a, 4)?,
                    u32_arg(a, 5)?,
                )
                .map(handle),
            D_VIEW => b
                .create_view(arg(a, 0)?, u32_arg(a, 1)?, u32_arg(a, 2)?)
                .map(handle),
            D_SAMPLER => b
                .create_sampler(
                    u32_arg(a, 0)?,
                    u32_arg(a, 1)?,
                    u32_arg(a, 2)?,
                    u32_arg(a, 3)?,
                    u32_arg(a, 4)?,
                    lod(arg(a, 5)?),
                )
                .map(handle),
            D_PIPELINE => {
                // SAFETY: see `bytes`.
                let data = unsafe { bytes(arg(a, 0)?, arg(a, 1)?) }?;
                scoped(b, || b.compile_pipeline(data)).map(handle)
            }
            D_DESTROY => b.destroy(arg(a, 0)?).map(|()| 0),
            D_ENCODER => b.create_encoder().map(handle),
            D_SURFACE => b.create_surface().map(handle),
            D_SURFACE_CONFIGURE => b
                .configure_surface(arg(a, 0)?, u32_arg(a, 1)?, u32_arg(a, 2)?)
                .map(|()| 0),
            D_SURFACE_PRESENT => b.present(arg(a, 0)?).map(|()| 0),
            D_STATS => stats(b, arg(a, 0)?, arg(a, 1)?),
            D_SLOT => {
                b.slots.set_slot(u32_arg(a, 0)?, arg(a, 1)?);
                Ok(0)
            }
            other => Err(OpError::Invalid(format!("unknown device op {other}"))),
        }
    })
}

pub extern "C" fn gpu_encode(encoder: i64, op: i32, args: i64, count: i32) -> i64 {
    guarded(|| {
        // SAFETY: see `slots`.
        let a = unsafe { slots(args, count) }?;
        let b = backend::backend()?;
        let g = b.gpu;
        let e = encoder as u64;
        let mut s = b.lock();
        let color = |a: &[u64], i: usize| -> OpResult<[f32; 4]> {
            Ok([
                f32_arg(a, i)?,
                f32_arg(a, i + 1)?,
                f32_arg(a, i + 2)?,
                f32_arg(a, i + 3)?,
            ])
        };
        match op {
            E_CLEAR => {
                let c = (arg(a, 0)? != 0).then(|| color(a, 1).map(|c| (arg(a, 0).unwrap_or(0), c)));
                let d =
                    (arg(a, 5)? != 0).then(|| f32_arg(a, 6).map(|d| (arg(a, 5).unwrap_or(0), d)));
                let (c, d) = (c.transpose()?, d.transpose()?);
                let world = b.slots.world_targets(&g.device, b.slots.current().0);
                scoped(b, || s.clear(g, e, c, d, world.as_ref())).map(|()| 0)
            }
            E_CLEAR_REGION => scoped(b, || {
                s.clear_region(
                    g,
                    e,
                    (arg(a, 0)?, color(a, 1)?),
                    (arg(a, 5)?, f32_arg(a, 6)?),
                    u32_arg(a, 7)?,
                    u32_arg(a, 8)?,
                    u32_arg(a, 9)?,
                    u32_arg(a, 10)?,
                    u32_arg(a, 11)?,
                    b.slots
                        .world_targets(&g.device, b.slots.current().0)
                        .is_some(),
                )
            })
            .map(|()| 0),
            E_WRITE_BUFFER => {
                // SAFETY: see `bytes`.
                let data = unsafe { bytes(arg(a, 2)?, arg(a, 3)?) }?;
                scoped(b, || s.write_buffer(g, e, arg(a, 0)?, arg(a, 1)?, data)).map(|()| 0)
            }
            E_COPY_BUFFER => scoped(b, || {
                s.copy_buffer(
                    g,
                    e,
                    arg(a, 0)?,
                    arg(a, 1)?,
                    arg(a, 2)?,
                    arg(a, 3)?,
                    arg(a, 4)?,
                )
            })
            .map(|()| 0),
            E_WRITE_TEXTURE => {
                // SAFETY: see `bytes`.
                let data = unsafe { bytes(arg(a, 1)?, arg(a, 2)?) }?;
                scoped(b, || {
                    s.write_texture(
                        g,
                        e,
                        arg(a, 0)?,
                        data,
                        u32_arg(a, 3)?,
                        u32_arg(a, 4)?,
                        u32_arg(a, 5)?,
                        u32_arg(a, 6)?,
                        u32_arg(a, 7)?,
                        u32_arg(a, 8)?,
                    )
                })
                .map(|()| 0)
            }
            E_BUFFER_TO_TEXTURE => scoped(b, || {
                s.copy_buffer_to_texture(
                    g,
                    e,
                    arg(a, 0)?,
                    arg(a, 1)?,
                    u32_arg(a, 2)?,
                    u32_arg(a, 3)?,
                    u32_arg(a, 4)?,
                    u32_arg(a, 5)?,
                    arg(a, 6)?,
                    u32_arg(a, 7)?,
                    u32_arg(a, 8)?,
                    u32_arg(a, 9)?,
                    u32_arg(a, 10)?,
                    u32_arg(a, 11)?,
                    u32_arg(a, 12)?,
                )
            })
            .map(|()| 0),
            E_TEXTURE_TO_BUFFER => scoped(b, || {
                s.refresh_main(g, &b.slots, e, arg(a, 0)?)?;
                s.copy_texture_to_buffer(
                    g,
                    e,
                    arg(a, 0)?,
                    arg(a, 1)?,
                    arg(a, 2)?,
                    arg(a, 3)?,
                    u32_arg(a, 4)?,
                    u32_arg(a, 5)?,
                    u32_arg(a, 6)?,
                    u32_arg(a, 7)?,
                    u32_arg(a, 8)?,
                )
            })
            .map(|()| 0),
            E_TEXTURE_TO_TEXTURE => scoped(b, || {
                s.refresh_main(g, &b.slots, e, arg(a, 0)?)?;
                s.copy_texture_to_texture(
                    g,
                    e,
                    arg(a, 0)?,
                    arg(a, 1)?,
                    u32_arg(a, 2)?,
                    u32_arg(a, 3)?,
                    u32_arg(a, 4)?,
                    u32_arg(a, 5)?,
                    u32_arg(a, 6)?,
                    u32_arg(a, 7)?,
                    u32_arg(a, 8)?,
                )
            })
            .map(|()| 0),
            E_FLUSH_MAPPED => scoped(b, || {
                s.flush_mapped(g, e, arg(a, 0)?, arg(a, 1)?, arg(a, 2)?)
            })
            .map(|()| 0),
            E_BLIT => scoped(b, || s.blit(g, e, arg(a, 0)?, arg(a, 1)?)).map(|()| 0),
            E_DIRTY => s
                .transient_dirty(arg(a, 0)?, arg(a, 1)?, arg(a, 2)?)
                .map(|()| 0),
            E_SUBMIT => scoped(b, || s.submit(g, &b.stats, &b.slots, e)).map(|n| n as i64),
            E_SPLIT => scoped(b, || s.split(g, &b.stats, &b.slots, e)).map(|()| 0),
            E_POLL => {
                let (out, cap) = (arg(a, 0)?, arg(a, 1)? as usize);
                let ids = s.poll(g, e)?;
                if out == 0 || ids.len() > cap {
                    return Err(OpError::Invalid(format!(
                        "{} completions for {cap} slots",
                        ids.len()
                    )));
                }
                // SAFETY: Java passes a writable segment of `cap` longs, checked to fit.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        ids.as_ptr(),
                        out as usize as *mut u64,
                        ids.len(),
                    );
                }
                Ok(ids.len() as i64)
            }
            E_WAIT => {
                let (n, timeout) = (arg(a, 0)?, arg(a, 1)?);
                if n > s.fence_pending(e)? - 1 {
                    scoped(b, || s.submit(g, &b.stats, &b.slots, e))?;
                }
                let timeout = (timeout != u64::MAX).then(|| Duration::from_nanos(timeout));
                s.wait(g, e, n, timeout).map(i64::from)
            }
            other => Err(OpError::Invalid(format!("unknown encoder op {other}"))),
        }
    })
}

pub extern "C" fn gpu_pass(
    encoder: i64,
    desc: i64,
    desc_count: i32,
    stream: i64,
    stream_bytes: i32,
) -> i64 {
    guarded(|| {
        // SAFETY: see `slots`.
        let d = unsafe { slots(desc, desc_count) }?;
        let desc = PassDesc::parse(d)?;
        let len =
            usize::try_from(stream_bytes).map_err(|_| OpError::Invalid("stream length".into()))?;
        if len > MAX_STREAM_BYTES
            || !len.is_multiple_of(4)
            || (len > 0 && (stream == 0 || stream % 4 != 0))
        {
            return Err(OpError::Invalid(format!("bad pass stream of {len} bytes")));
        }
        let words: &[u32] = if len == 0 {
            &[]
        } else {
            // SAFETY: Java passes a 4-aligned off-heap stream of `len` bytes valid for this call.
            unsafe { std::slice::from_raw_parts(stream as usize as *const u32, len / 4) }
        };
        let b = backend::backend()?;
        let mut s = b.lock();
        let report = scoped(b, || {
            pass::encode_pass(b, &mut s, encoder as u64, &desc, words)
        })?;
        if let Some(e) = &report.first_error {
            record(&OpError::Invalid(format!(
                "{} draws skipped: {e}",
                report.skipped
            )));
        }
        Ok(report.draws as i64)
    })
}

/// Copies this thread's last error message (UTF-8) into `out`; returns its full length.
pub extern "C" fn gpu_error(out: i64, cap: i32) -> i32 {
    catch_unwind(|| {
        LAST_ERROR.with(|l| {
            let msg = l.borrow();
            let n = msg.len().min(usize::try_from(cap).unwrap_or(0));
            if out != 0 && n > 0 {
                // SAFETY: Java passes a writable segment of `cap` bytes; `n` is clamped to it.
                unsafe { std::ptr::copy_nonoverlapping(msg.as_ptr(), out as usize as *mut u8, n) };
            }
            msg.len() as i32
        })
    })
    .unwrap_or(0)
}
