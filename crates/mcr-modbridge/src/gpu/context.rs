//! The wgpu device the backend records on: mcr-render's when installed, else a headless one.

use std::sync::{Arc, Mutex, OnceLock, PoisonError};

#[derive(Debug)]
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
    pub features: wgpu::Features,
    pub limits: wgpu::Limits,
}

static GPU: OnceLock<Gpu> = OnceLock::new();
static INIT: Mutex<()> = Mutex::new(());

/// Features the backend uses when the adapter has them; none is required.
pub const OPTIONAL_FEATURES: wgpu::Features = wgpu::Features::POLYGON_MODE_LINE
    .union(wgpu::Features::DEPTH32FLOAT_STENCIL8)
    .union(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM)
    .union(wgpu::Features::INDIRECT_FIRST_INSTANCE)
    .union(wgpu::Features::FLOAT32_FILTERABLE)
    .union(wgpu::Features::FLOAT32_BLENDABLE);

#[derive(Debug, Clone, thiserror::Error)]
pub enum GpuError {
    #[error("no GPU adapter: {0}")]
    NoAdapter(String),
    #[error("device request failed: {0}")]
    Device(String),
    #[error("a device was already chosen")]
    AlreadySet,
    #[error("composition needs mcr-render's device installed first")]
    NotInstalled,
}

static INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The backend records on mcr-render's device rather than a headless one.
pub fn installed() -> bool {
    INSTALLED.load(std::sync::atomic::Ordering::Acquire)
}

/// Makes the backend share mcr-render's device; must run before Java creates its device.
pub fn install(
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: wgpu::AdapterInfo,
) -> Result<(), GpuError> {
    let _guard = INIT.lock().unwrap_or_else(PoisonError::into_inner);
    let features = device.features();
    let limits = device.limits();
    // wgpu's default handler panics; Java's errors on the shared device must only be logged.
    device.on_uncaptured_error(Arc::new(|e: wgpu::Error| {
        tracing::error!("uncaptured wgpu error on the shared device: {e}");
    }));
    GPU.set(Gpu {
        device,
        queue,
        info,
        features,
        limits,
    })
    .map_err(|_| GpuError::AlreadySet)?;
    INSTALLED.store(true, std::sync::atomic::Ordering::Release);
    Ok(())
}

pub fn gpu() -> Result<&'static Gpu, GpuError> {
    if let Some(g) = GPU.get() {
        return Ok(g);
    }
    let _guard = INIT.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(g) = GPU.get() {
        return Ok(g);
    }
    let g = pollster::block_on(headless())?;
    Ok(GPU.get_or_init(|| g))
}

async fn headless() -> Result<Gpu, GpuError> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        })
        .await
        .map_err(|e| GpuError::NoAdapter(e.to_string()))?;
    let al = adapter.limits();
    let limits = wgpu::Limits {
        max_uniform_buffer_binding_size: al.max_uniform_buffer_binding_size,
        max_buffer_size: al.max_buffer_size,
        max_texture_dimension_2d: al.max_texture_dimension_2d,
        max_color_attachments: al.max_color_attachments.min(8),
        ..wgpu::Limits::default()
    };
    let features = adapter.features() & OPTIONAL_FEATURES;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("mcr-modbridge"),
            required_features: features,
            required_limits: limits,
            ..Default::default()
        })
        .await
        .map_err(|e| GpuError::Device(e.to_string()))?;
    device.on_uncaptured_error(Arc::new(|e: wgpu::Error| {
        tracing::error!("uncaptured wgpu error in the mod backend: {e}");
    }));
    Ok(Gpu {
        info: adapter.get_info(),
        features: device.features(),
        limits: device.limits(),
        device,
        queue,
    })
}
