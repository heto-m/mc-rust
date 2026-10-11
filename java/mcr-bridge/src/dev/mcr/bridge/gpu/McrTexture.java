package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.backend.common.BaseGpuTexture;

final class McrTexture extends BaseGpuTexture implements McrHandle {
    private final McrDeviceBackend device;
    private final long handle;
    private boolean closed;

    McrTexture(McrDeviceBackend device, long handle, int usage, String label, GpuFormat format, int width, int height,
            int depthOrLayers, int mipLevels) {
        super(usage, label, format, width, height, depthOrLayers, mipLevels);
        this.device = device;
        this.handle = handle;
    }

    @Override
    public long handle() {
        return handle;
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
