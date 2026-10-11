package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.buffers.TransientMemory;
import com.mojang.renderpearl.backend.common.BaseGpuBuffer;
import com.mojang.renderpearl.backend.util.TransientBlockAllocator;
import dev.mcr.bridge.BridgeAbi;
import java.lang.foreign.MemorySegment;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.ArrayList;
import java.util.List;
import org.lwjgl.system.MemoryUtil;

/**
 * Per-submit memory. GPU blocks are Rust buffers with a host copy that Java fills; every allocation marks its
 * range for upload at submit. Rust's upload is ordered after earlier submits, so blocks recycle at once.
 */
final class McrTransientMemory implements TransientMemory {
    private static final long BLOCK_SIZE = 512 << 10;
    private static final long MAX_ALIGNMENT = 1 << 16;

    private final McrCommandEncoder encoder;
    private final TransientBlockAllocator<Block> gpu;
    private final TransientBlockAllocator<TransientBlockAllocator.Allocator.CpuBlock> cpu =
            new TransientBlockAllocator<>(BLOCK_SIZE, 16, TransientBlockAllocator.Allocator.CpuBlock.memalloc());

    record Block(long handle, MemorySegment host, long size) implements TransientBlockAllocator.Allocator.Block {
        @Override
        public boolean suboptimal() {
            return false;
        }
    }

    McrTransientMemory(McrCommandEncoder encoder) {
        this.encoder = encoder;
        this.gpu = new TransientBlockAllocator<>(BLOCK_SIZE, MAX_ALIGNMENT, TransientBlockAllocator.Allocator.create(
                size -> {
                    long h = GpuNative.require(GpuNative.device(BridgeAbi.D_TRANSIENT, size), "transient block");
                    long address = GpuNative.require(GpuNative.device(BridgeAbi.D_HOST, h), "transient host memory");
                    return new Block(h, MemorySegment.ofAddress(address).reinterpret(size), size);
                },
                block -> GpuNative.check(GpuNative.device(BridgeAbi.D_DESTROY, block.handle()), "transient free")));
    }

    void rotate() {
        gpu.rotate().run();
        cpu.rotate().run();
    }

    private TransientBlockAllocator.Allocation<Block> allocate(long size, long alignment) {
        return gpu.allocate(Math.max(size, 1), Math.max(alignment, 1), size, 1);
    }

    private GpuBufferSlice slice(TransientBlockAllocator.Allocation<Block> a, int usage) {
        return new GpuBufferSlice(new TransientBuffer(encoder, a.block(), usage), a.offset(), a.size());
    }

    private GpuBufferSlice.MappedView mapped(long size, long alignment, int usage) {
        TransientBlockAllocator.Allocation<Block> a = allocate(size, alignment);
        encoder.dirty(a.block().handle(), a.offset(), a.size());
        ByteBuffer host = a.block().host().asSlice(a.offset(), a.size()).asByteBuffer().order(ByteOrder.nativeOrder());
        return new GpuBufferSlice.MappedView(slice(a, usage), host, () -> {});
    }

    @Override
    public ByteBuffer allocateCpu(long size, long alignment, long minimumAllocation, long elementSize) {
        TransientBlockAllocator.Allocation<TransientBlockAllocator.Allocator.CpuBlock> a =
                cpu.allocate(size, alignment, size, 1);
        return MemoryUtil.memByteBuffer(a.block().address() + a.offset(), (int) a.size());
    }

    @Override
    public GpuBufferSlice.MappedView allocateStaging(long size, long alignment, int usage, long minimumAllocation, long elementSize) {
        return mapped(size, alignment, usage);
    }

    @Override
    public GpuBufferSlice allocateGpu(long size, long alignment, int usage, long minimumAllocation, long elementSize) {
        return slice(allocate(size, alignment), usage);
    }

    @Override
    public GpuBufferSlice.MappedView allocateGpuMapped(long size, long alignment, int usage, long minimumAllocation, long elementSize) {
        return mapped(size, alignment, usage);
    }

    @Override
    public GpuBufferSlice uploadStaging(List<ByteBuffer> data, long alignment, int usage, long minimumAllocation, long elementSize) {
        return upload(data, alignment, usage);
    }

    @Override
    public GpuBufferSlice uploadGpu(List<ByteBuffer> data, long alignment, int usage, long minimumAllocation, long elementSize) {
        return upload(data, alignment, usage);
    }

    private static long roundUp(long n, long alignment) {
        return alignment <= 1 ? n : (n + alignment - 1) / alignment * alignment;
    }

    private GpuBufferSlice upload(List<ByteBuffer> data, long alignment, int usage) {
        long total = 0;
        for (ByteBuffer b : data) {
            total = roundUp(total + b.remaining(), alignment);
        }
        GpuBufferSlice.MappedView view = mapped(total, alignment, usage);
        MemorySegment host = MemorySegment.ofBuffer(view.data());
        long offset = 0;
        for (ByteBuffer b : data) {
            MemorySegment.copy(MemorySegment.ofBuffer(b), 0, host, offset, b.remaining());
            offset = roundUp(offset + b.remaining(), alignment);
        }
        return view.slice();
    }

    @Override
    public List<GpuBufferSlice> multiUploadStaging(List<ByteBuffer> data, long alignment, int usage) {
        return multiUpload(data, alignment, usage);
    }

    @Override
    public List<GpuBufferSlice> multiUploadGpu(List<ByteBuffer> data, long alignment, int usage) {
        return multiUpload(data, alignment, usage);
    }

    private List<GpuBufferSlice> multiUpload(List<ByteBuffer> data, long alignment, int usage) {
        List<GpuBufferSlice> out = new ArrayList<>(data.size());
        for (ByteBuffer b : data) {
            out.add(upload(List.of(b), alignment, usage));
        }
        return out;
    }

    /** A view of one block for one submit; it reports closed once that submit went out. */
    static final class TransientBuffer extends BaseGpuBuffer implements McrHandle {
        private final McrCommandEncoder encoder;
        private final long handle;
        private final long submit;

        TransientBuffer(McrCommandEncoder encoder, Block block, int usage) {
            super(usage, block.size());
            this.encoder = encoder;
            this.handle = block.handle();
            this.submit = encoder.submitIndex;
        }

        @Override
        public long handle() {
            return handle;
        }

        @Override
        public boolean isClosed() {
            return submit < encoder.submitIndex;
        }

        @Override
        public void close() {}

        @Override
        public GpuBufferSlice.MappedView map(long offset, long length, boolean read, boolean write) {
            throw new IllegalStateException("Cannot map a transient buffer");
        }
    }
}
