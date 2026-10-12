//! Buffers, textures, views and samplers behind Java's handles.

use std::ptr::NonNull;

use super::enums;

pub const USAGE_MAP_READ: u32 = 1;
pub const USAGE_MAP_WRITE: u32 = 2;
pub const USAGE_INDIRECT: u32 = 512;

pub const TEXTURE_CUBEMAP: u32 = 16;
/// Bridge-only usage bit: a texture of Java's `MainTarget`, which the compositor reads.
pub const TEXTURE_VIRTUAL: u32 = 1 << 30;

/// Host memory Java reads and writes through a `MemorySegment` (mapped buffers, transient blocks).
#[derive(Debug)]
pub struct Shadow {
    ptr: NonNull<u8>,
    len: usize,
}

// SAFETY: the allocation is plain bytes owned by this value; Java touches it only on the thread that
// records with it, between the downcalls that create and destroy the owning buffer.
unsafe impl Send for Shadow {}
// SAFETY: as above; Rust only reads or writes it inside downcalls made by that same thread.
unsafe impl Sync for Shadow {}

impl Shadow {
    pub fn new(len: usize) -> Self {
        let boxed = vec![0u8; len.max(1)].into_boxed_slice();
        let ptr =
            NonNull::new(Box::into_raw(boxed).cast::<u8>()).expect("box pointers are non-null");
        Self { ptr, len }
    }

    pub fn addr(&self) -> i64 {
        self.ptr.as_ptr() as i64
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn bytes(&self) -> &[u8] {
        // SAFETY: `ptr` came from a live boxed slice of `len.max(1)` bytes; see the Send note on
        // why no other writer runs concurrently.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    pub fn ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    pub fn write(&self, offset: usize, data: &[u8]) {
        let end = offset.saturating_add(data.len()).min(self.len);
        if offset >= end {
            return;
        }
        // SAFETY: the range was clamped to the allocation; same confinement as `bytes`.
        unsafe {
            std::ptr::copy_nonoverlapping(
                data.as_ptr(),
                self.ptr.as_ptr().add(offset),
                end - offset,
            );
        }
    }
}

impl Drop for Shadow {
    fn drop(&mut self) {
        // SAFETY: reconstructs exactly the boxed slice leaked in `new`.
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                self.ptr.as_ptr(),
                self.len.max(1),
            )));
        }
    }
}

#[derive(Debug)]
pub struct Buffer {
    pub buffer: wgpu::Buffer,
    pub size: u64,
    pub usage: u32,
    pub shadow: Option<Shadow>,
    /// Transient blocks: the host range Java filled since the last submit, uploaded at submit.
    pub dirty: Option<(u64, u64)>,
    pub transient: bool,
}

pub fn wgpu_buffer_usage(java: u32) -> wgpu::BufferUsages {
    use wgpu::BufferUsages as U;
    let mut usage = U::COPY_DST | U::COPY_SRC;
    for (bit, flag) in [
        (32, U::VERTEX),
        (64, U::INDEX),
        (128, U::UNIFORM),
        (USAGE_INDIRECT, U::INDIRECT),
    ] {
        if java & bit != 0 {
            usage |= flag;
        }
    }
    usage
}

pub fn aligned4(n: u64) -> u64 {
    n.div_ceil(4) * 4
}

impl Buffer {
    pub fn new(device: &wgpu::Device, usage: u32, size: u64, init: Option<&[u8]>) -> Self {
        // Vanilla sizes uniform buffers without std140 tail padding; bindings round up to the block.
        let wgpu_size = if usage & 128 != 0 {
            size.max(16).next_multiple_of(16)
        } else {
            aligned4(size.max(4))
        };
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("java buffer"),
            size: wgpu_size,
            usage: wgpu_buffer_usage(usage),
            mapped_at_creation: init.is_some(),
        });
        if let Some(data) = init {
            if let Ok(mut view) = buffer.slice(..).get_mapped_range_mut() {
                let n = data.len().min(view.len());
                view.slice(..n).copy_from_slice(&data[..n]);
            }
            buffer.unmap();
        }
        let shadow = (usage & (USAGE_MAP_READ | USAGE_MAP_WRITE) != 0).then(|| {
            let s = Shadow::new(size as usize);
            if let Some(data) = init {
                s.write(0, data);
            }
            s
        });
        Self {
            buffer,
            size,
            usage,
            shadow,
            dirty: None,
            transient: false,
        }
    }

    /// A transient arena block: GPU memory plus the host copy Java fills, uploaded at submit.
    pub fn transient(device: &wgpu::Device, size: u64) -> Self {
        let size = size.max(256).next_multiple_of(256);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("java transient block"),
            size,
            usage: wgpu_buffer_usage(32 | 64 | 128 | USAGE_INDIRECT),
            mapped_at_creation: false,
        });
        Self {
            buffer,
            size,
            usage: 32 | 64 | 128 | USAGE_INDIRECT,
            shadow: Some(Shadow::new(size as usize)),
            dirty: None,
            transient: true,
        }
    }
}

#[derive(Debug)]
pub struct Texture {
    pub texture: wgpu::Texture,
    pub format: wgpu::TextureFormat,
    pub java_format: u32,
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub mips: u32,
    pub cube: bool,
    /// Part of Java's main target: color clears are dropped and the compositor reads it.
    pub virtual_main: bool,
}

impl Texture {
    pub fn block_size(&self) -> u32 {
        enums::block_size(self.java_format).unwrap_or(4)
    }

    pub fn mip_size(&self, mip: u32) -> (u32, u32) {
        ((self.width >> mip).max(1), (self.height >> mip).max(1))
    }

    /// Aspect for copies and sampling: depth-stencil formats expose their depth plane.
    pub fn aspect(&self) -> wgpu::TextureAspect {
        if self.format.has_depth_aspect() {
            wgpu::TextureAspect::DepthOnly
        } else {
            wgpu::TextureAspect::All
        }
    }

    /// A single-mip, single-layer 2D view, as render attachments need.
    pub fn attachment_view(&self, mip: u32) -> wgpu::TextureView {
        self.layer_view(mip, 0)
    }

    pub fn layer_view(&self, mip: u32, layer: u32) -> wgpu::TextureView {
        self.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("java attachment"),
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: mip,
            mip_level_count: Some(1),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..Default::default()
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("texture format code {0} has no wgpu equivalent")]
    Format(u32),
    #[error("texture size {0}x{1}x{2} with {3} mips is out of range")]
    Size(u32, u32, u32, u32),
}

#[allow(clippy::too_many_arguments)]
pub fn create_texture(
    device: &wgpu::Device,
    features: wgpu::Features,
    usage: u32,
    java_format: u32,
    width: u32,
    height: u32,
    layers: u32,
    mips: u32,
) -> Result<Texture, ResourceError> {
    let format = enums::texture_format(java_format).ok_or(ResourceError::Format(java_format))?;
    let limits = device.limits();
    let max = limits.max_texture_dimension_2d;
    let cube = usage & TEXTURE_CUBEMAP != 0;
    let max_mips = 32 - width.max(height).max(1).leading_zeros();
    if width == 0
        || height == 0
        || width > max
        || height > max
        || layers == 0
        || layers > 6
        || (cube && layers != 6)
        || mips == 0
        || mips > max_mips
    {
        return Err(ResourceError::Size(width, height, layers, mips));
    }
    use wgpu::TextureUsages as U;
    let mut wgpu_usage = U::COPY_DST | U::COPY_SRC | U::TEXTURE_BINDING;
    // Clears are render passes, so every renderable format gets the attachment usage.
    if format
        .guaranteed_format_features(features)
        .allowed_usages
        .contains(U::RENDER_ATTACHMENT)
    {
        wgpu_usage |= U::RENDER_ATTACHMENT;
    } else if usage & 8 != 0 {
        return Err(ResourceError::Format(java_format));
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("java texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: layers,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu_usage,
        view_formats: &[],
    });
    Ok(Texture {
        texture,
        format,
        java_format,
        width,
        height,
        layers,
        mips,
        cube,
        virtual_main: usage & TEXTURE_VIRTUAL != 0,
    })
}

#[derive(Debug)]
pub struct View {
    pub view: wgpu::TextureView,
    pub texture: u64,
    pub format: wgpu::TextureFormat,
    pub base_mip: u32,
    /// Single-mip 2D view for use as a render attachment.
    pub attachment: Option<wgpu::TextureView>,
    pub width: u32,
    pub height: u32,
    pub sample: SampleClass,
    pub dimension: wgpu::TextureViewDimension,
}

/// What kind of texture binding a view can satisfy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleClass {
    Filterable,
    /// Depth and 32-bit float formats: only unfilterable-float bindings with non-filtering samplers.
    Unfilterable,
    Uint,
    Sint,
}

pub fn sample_class(format: wgpu::TextureFormat, features: wgpu::Features) -> SampleClass {
    match format.sample_type(Some(wgpu::TextureAspect::DepthOnly), Some(features)) {
        Some(wgpu::TextureSampleType::Float { filterable: true }) => SampleClass::Filterable,
        Some(wgpu::TextureSampleType::Uint) => SampleClass::Uint,
        Some(wgpu::TextureSampleType::Sint) => SampleClass::Sint,
        _ => SampleClass::Unfilterable,
    }
}

pub fn create_view(
    tex: &Texture,
    texture: u64,
    base_mip: u32,
    mips: u32,
    features: wgpu::Features,
) -> Option<View> {
    if mips == 0 || base_mip.checked_add(mips)? > tex.mips {
        return None;
    }
    let dimension = if tex.cube {
        wgpu::TextureViewDimension::Cube
    } else {
        wgpu::TextureViewDimension::D2
    };
    let view = tex.texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("java view"),
        dimension: Some(dimension),
        aspect: tex.aspect(),
        base_mip_level: base_mip,
        mip_level_count: Some(mips),
        ..Default::default()
    });
    let renderable = tex
        .texture
        .usage()
        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT);
    let (width, height) = tex.mip_size(base_mip);
    Some(View {
        view,
        texture,
        format: tex.format,
        base_mip,
        attachment: (renderable && !tex.cube).then(|| tex.attachment_view(base_mip)),
        width,
        height,
        sample: sample_class(tex.format, features),
        dimension,
    })
}

#[derive(Debug)]
pub struct Sampler {
    pub sampler: wgpu::Sampler,
    /// Nearest-filter twin used where the binding cannot filter (depth textures).
    pub nearest: wgpu::Sampler,
    pub filtering: bool,
}

pub fn create_sampler(
    device: &wgpu::Device,
    address_u: u32,
    address_v: u32,
    min: u32,
    mag: u32,
    anisotropy: u32,
    max_lod: Option<f64>,
) -> Sampler {
    let min_filter = enums::filter_mode(min);
    let mag_filter = enums::filter_mode(mag);
    // Vanilla's Vulkan rule: mipmaps interpolate unless the LOD range collapses to the base level.
    let lod = max_lod.unwrap_or(1000.0);
    let mipmap_filter = if lod > 0.25 {
        wgpu::MipmapFilterMode::Linear
    } else {
        wgpu::MipmapFilterMode::Nearest
    };
    let all_linear = min_filter == wgpu::FilterMode::Linear
        && mag_filter == wgpu::FilterMode::Linear
        && mipmap_filter == wgpu::MipmapFilterMode::Linear;
    let desc = wgpu::SamplerDescriptor {
        label: Some("java sampler"),
        address_mode_u: enums::address_mode(address_u),
        address_mode_v: enums::address_mode(address_v),
        address_mode_w: enums::address_mode(address_v),
        mag_filter,
        min_filter,
        mipmap_filter,
        lod_min_clamp: 0.0,
        lod_max_clamp: (lod.max(0.25) as f32).min(32.0),
        compare: None,
        anisotropy_clamp: if all_linear {
            anisotropy.clamp(1, 16) as u16
        } else {
            1
        },
        border_color: None,
    };
    let sampler = device.create_sampler(&desc);
    let nearest = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        anisotropy_clamp: 1,
        ..desc
    });
    Sampler {
        sampler,
        nearest,
        filtering: min_filter == wgpu::FilterMode::Linear
            || mag_filter == wgpu::FilterMode::Linear
            || mipmap_filter == wgpu::MipmapFilterMode::Linear,
    }
}
