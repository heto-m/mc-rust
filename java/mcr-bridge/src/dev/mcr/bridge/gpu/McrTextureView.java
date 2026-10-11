package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.backend.common.BaseGpuTextureView;

final class McrTextureView extends BaseGpuTextureView implements McrHandle {
    private final McrDeviceBackend device;
    private final long handle;
    private boolean closed;

    McrTextureView(McrDeviceBackend device, long handle, GpuTexture texture, int baseMipLevel, int mipLevels) {
        super(texture, baseMipLevel, mipLevels);
        this.device = device;
        this.handle = handle;
    }

    @Override
    public long handle() {
        return handle;
    }

    @Override
    public boolean isClosed() {
        return closed || texture().isClosed();
    }

    @Override
    public void close() {
        if (!closed) {
            closed = true;
            device.destroyLater(handle);
        }
    }
}
