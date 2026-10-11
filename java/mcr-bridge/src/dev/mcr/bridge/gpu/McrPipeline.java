package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.backend.api.BackendRenderPipeline;

final class McrPipeline implements BackendRenderPipeline, McrHandle {
    private final McrDeviceBackend device;
    private final long handle;
    private boolean closed;

    McrPipeline(McrDeviceBackend device, long handle) {
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
