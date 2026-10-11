package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.backend.common.BaseGpuBuffer;
import dev.mcr.bridge.BridgeAbi;
import java.lang.foreign.MemorySegment;
import java.nio.ByteOrder;

/** A persistent buffer. Mappable ones expose Rust's host copy; closing a write map stages it in-stream. */
final class McrBuffer extends BaseGpuBuffer implements McrHandle {
    private final McrDeviceBackend device;
    private final long handle;
    private MemorySegment host;
    private boolean closed;

    McrBuffer(McrDeviceBackend device, int usage, long size, long handle) {
        super(usage, size);
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

    @Override
    public GpuBufferSlice.MappedView map(long offset, long length, boolean read, boolean write) {
        if (closed) {
            throw new IllegalStateException("mapping a closed buffer");
        }
        if (offset < 0 || length < 0 || offset + length > size()) {
            throw new IllegalArgumentException("map range " + offset + "+" + length + " of " + size());
        }
        if (host == null) {
            long address = GpuNative.device(BridgeAbi.D_HOST, handle);
            if (address <= 0) {
                throw new IllegalStateException("buffer is not mappable");
            }
            host = MemorySegment.ofAddress(address).reinterpret(size());
        }
        Runnable onClose = write ? () -> device.encoder().flushMapped(handle, offset, length) : () -> {};
        return new GpuBufferSlice.MappedView(
                new GpuBufferSlice(this, offset, length),
                host.asSlice(offset, length).asByteBuffer().order(ByteOrder.nativeOrder()),
                onClose);
    }
}
