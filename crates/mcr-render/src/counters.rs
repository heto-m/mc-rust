/// Live GPU objects and bytes on one device, read from wgpu's internal counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GpuCounters {
    pub buffers: i64,
    pub buffer_bytes: i64,
    pub textures: i64,
    pub texture_bytes: i64,
    pub texture_views: i64,
    pub bind_groups: i64,
    pub allocations: i64,
}

impl GpuCounters {
    pub fn read(device: &wgpu::Device) -> Self {
        let hal = device.get_internal_counters().hal;
        Self {
            buffers: hal.buffers.read() as i64,
            buffer_bytes: hal.buffer_memory.read() as i64,
            textures: hal.textures.read() as i64,
            texture_bytes: hal.texture_memory.read() as i64,
            texture_views: hal.texture_views.read() as i64,
            bind_groups: hal.bind_groups.read() as i64,
            allocations: hal.memory_allocations.read() as i64,
        }
    }
}
