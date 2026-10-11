package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.textures.AddressMode;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuSampler;
import java.util.OptionalDouble;

final class McrSampler implements GpuSampler, McrHandle {
    private final McrDeviceBackend device;
    private final long handle;
    private final AddressMode u;
    private final AddressMode v;
    private final FilterMode min;
    private final FilterMode mag;
    private final int anisotropy;
    private final OptionalDouble maxLod;
    private boolean closed;

    McrSampler(McrDeviceBackend device, long handle, AddressMode u, AddressMode v, FilterMode min, FilterMode mag,
            int anisotropy, OptionalDouble maxLod) {
        this.device = device;
        this.handle = handle;
        this.u = u;
        this.v = v;
        this.min = min;
        this.mag = mag;
        this.anisotropy = anisotropy;
        this.maxLod = maxLod;
    }

    @Override
    public long handle() {
        return handle;
    }

    @Override
    public AddressMode getAddressModeU() {
        return u;
    }

    @Override
    public AddressMode getAddressModeV() {
        return v;
    }

    @Override
    public FilterMode getMinFilter() {
        return min;
    }

    @Override
    public FilterMode getMagFilter() {
        return mag;
    }

    @Override
    public int getMaxAnisotropy() {
        return anisotropy;
    }

    @Override
    public OptionalDouble getMaxLod() {
        return maxLod;
    }

    @Override
    public boolean isClosed() {
        return closed;
    }

    @Override
    public void close() {
        if (!closed) {
            closed = true;
            device.destroyLater(handle);
        }
    }
}
