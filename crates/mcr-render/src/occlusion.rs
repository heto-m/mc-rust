//! Hierarchical depth pyramid for two-phase occlusion culling.

const HIZ_SRC: &str = include_str!("shaders/hiz.wgsl");
const HIZ_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

/// Level 0 is half the depth size rounded up to powers of two, so every level halves exactly and a
/// level-m texel always covers `2^(m+1)` depth pixels; texels past the screen edge repeat edge depths.
pub(crate) fn pyramid_size(width: u32, height: u32) -> (u32, u32, u32) {
    let w = width.div_ceil(2).max(1).next_power_of_two();
    let h = height.div_ceil(2).max(1).next_power_of_two();
    (w, h, w.max(h).ilog2() + 1)
}

pub(crate) struct HiZ {
    from_depth: wgpu::ComputePipeline,
    downsample: wgpu::ComputePipeline,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    levels: Vec<(wgpu::BindGroup, u32, u32)>,
}

impl HiZ {
    pub(crate) fn new(device: &wgpu::Device, depth: &wgpu::TextureView, w: u32, h: u32) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hiz"),
            source: wgpu::ShaderSource::Wgsl(HIZ_SRC.into()),
        });
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let from_depth = pipeline("from_depth");
        let downsample = pipeline("downsample");
        let (texture, view, levels) = Self::targets(device, &from_depth, &downsample, depth, w, h);
        Self {
            from_depth,
            downsample,
            texture,
            view,
            levels,
        }
    }

    fn targets(
        device: &wgpu::Device,
        from_depth: &wgpu::ComputePipeline,
        downsample: &wgpu::ComputePipeline,
        depth: &wgpu::TextureView,
        w: u32,
        h: u32,
    ) -> (
        wgpu::Texture,
        wgpu::TextureView,
        Vec<(wgpu::BindGroup, u32, u32)>,
    ) {
        let (pw, ph, mips) = pyramid_size(w, h);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hiz"),
            size: wgpu::Extent3d {
                width: pw,
                height: ph,
                depth_or_array_layers: 1,
            },
            mip_level_count: mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HIZ_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let mip_view = |m: u32| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: m,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let levels = (0..mips)
            .map(|m| {
                let dst = mip_view(m);
                let bg = if m == 0 {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("hiz level"),
                        layout: &from_depth.get_bind_group_layout(0),
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(depth),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&dst),
                            },
                        ],
                    })
                } else {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("hiz level"),
                        layout: &downsample.get_bind_group_layout(0),
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&dst),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&mip_view(m - 1)),
                            },
                        ],
                    })
                };
                (bg, (pw >> m).max(1), (ph >> m).max(1))
            })
            .collect();
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view, levels)
    }

    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        depth: &wgpu::TextureView,
        w: u32,
        h: u32,
    ) {
        (self.texture, self.view, self.levels) =
            Self::targets(device, &self.from_depth, &self.downsample, depth, w, h);
    }

    pub(crate) fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub(crate) fn mip_count(&self) -> u32 {
        self.texture.mip_level_count()
    }

    pub(crate) fn bytes(&self) -> u64 {
        self.levels
            .iter()
            .map(|&(_, w, h)| u64::from(w) * u64::from(h) * 4)
            .sum()
    }

    pub(crate) fn record(&self, pass: &mut wgpu::ComputePass<'_>) {
        for (m, (bg, w, h)) in self.levels.iter().enumerate() {
            pass.set_pipeline(if m == 0 {
                &self.from_depth
            } else {
                &self.downsample
            });
            pass.set_bind_group(0, bg, &[]);
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pyramid_levels_halve_exactly_down_to_one_texel() {
        assert_eq!(pyramid_size(1920, 1080), (1024, 1024, 11));
        assert_eq!(pyramid_size(1, 1), (1, 1, 1));
        let (w, h, mips) = pyramid_size(1366, 7);
        assert_eq!((w, h), (1024, 4));
        assert_eq!(w >> (mips - 1), 1);
        for (width, height) in [(1920, 1080), (1366, 7), (3, 5000)] {
            let (w, h, mips) = pyramid_size(width, height);
            assert!(1u64 << mips >= u64::from(width.max(height)));
            assert!(2 * w >= width && 2 * h >= height);
        }
    }
}
