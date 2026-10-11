package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.buffers.TransientMemory;
import com.mojang.renderpearl.api.commands.GpuFence;
import com.mojang.renderpearl.api.commands.GpuQueryPool;
import com.mojang.renderpearl.api.commands.RenderPassDescriptor;
import com.mojang.renderpearl.api.device.DeviceFeatures;
import com.mojang.renderpearl.api.device.DeviceInfo;
import com.mojang.renderpearl.api.device.DeviceLimits;
import com.mojang.renderpearl.api.device.DeviceType;
import com.mojang.renderpearl.api.device.GpuSurface;
import com.mojang.renderpearl.api.device.HintsAndWorkarounds;
import com.mojang.renderpearl.api.pipeline.IndexType;
import com.mojang.renderpearl.api.textures.AddressMode;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuSampler;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import com.mojang.renderpearl.backend.api.BackendRenderPipeline;
import com.mojang.renderpearl.backend.api.CommandEncoderBackend;
import com.mojang.renderpearl.backend.api.GpuDeviceBackend;
import com.mojang.renderpearl.backend.api.GpuSurfaceBackend;
import com.mojang.renderpearl.backend.api.RenderPassBackend;
import com.mojang.renderpearl.backend.common.BaseGpuBuffer;
import com.mojang.renderpearl.backend.common.BaseGpuTexture;
import com.mojang.renderpearl.backend.common.BaseGpuTextureView;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.IntBuffer;
import java.util.ArrayList;
import java.util.Collection;
import java.util.EnumSet;
import java.util.List;
import java.util.OptionalDouble;
import java.util.OptionalLong;
import java.util.Set;
import java.util.function.BooleanSupplier;
import java.util.function.Supplier;
import org.joml.Vector4fc;
import org.jspecify.annotations.Nullable;
import org.lwjgl.PointerBuffer;

/** A device that accepts every call and draws nothing; mod-render-backend replaces it with wgpu. */
public final class NullDeviceBackend implements GpuDeviceBackend {
    private static final DeviceInfo INFO = new DeviceInfo(
            "mcr null device",
            "mcr",
            "none",
            true,
            "mcr-null",
            1.0F,
            new DeviceLimits(16, 256, 16384, 1L << 30, 0, 8, 0),
            new DeviceFeatures(true, false, false, false, false, true, true, false),
            Set.of(),
            new HintsAndWorkarounds(false, false, false, false),
            DeviceType.OTHER);

    private final Scratch scratch = new Scratch();
    private final NullBuffer transientBuffer = new NullBuffer(scratch, 0xFFFF, Long.MAX_VALUE / 2);
    private final NullTransientMemory transientMemory = new NullTransientMemory(scratch, transientBuffer);

    @Override
    public GpuSurfaceBackend createSurface(long windowHandle, BooleanSupplier isIconified) {
        return new NullSurface();
    }

    @Override
    public CommandEncoderBackend createCommandEncoder() {
        return new NullEncoder(transientMemory);
    }

    @Override
    public GpuSampler createSampler(
            AddressMode u, AddressMode v, FilterMode min, FilterMode mag, int maxAnisotropy, OptionalDouble maxLod) {
        return new NullSampler(u, v, min, mag, maxAnisotropy, maxLod);
    }

    @Override
    public GpuTexture createTexture(
            @Nullable String label, int usage, GpuFormat format, int width, int height, int depthOrLayers, int mipLevels) {
        return new NullTexture(usage, label == null ? "" : label, format, width, height, depthOrLayers, mipLevels);
    }

    @Override
    public GpuTextureView createTextureView(GpuTexture texture, int baseMipLevel, int mipLevels) {
        return new NullTextureView(texture, baseMipLevel, mipLevels);
    }

    @Override
    public GpuBuffer createBuffer(@Nullable Supplier<String> label, int usage, long size) {
        return new NullBuffer(scratch, usage, size);
    }

    @Override
    public GpuBuffer createBuffer(@Nullable Supplier<String> label, int usage, ByteBuffer data) {
        return new NullBuffer(scratch, usage, data.remaining());
    }

    @Override
    public List<String> getLastDebugMessages() {
        return List.of();
    }

    @Override
    public boolean isDebuggingEnabled() {
        return false;
    }

    @Override
    public BackendRenderPipeline.Pending compilePipeline(BackendRenderPipeline.CreateInfo info) {
        if (System.getProperty(PipelineInfo.DUMP_PROPERTY) != null) {
            PipelineInfo.dump(info, PipelineInfo.encode(info));
        }
        return NullPipeline::new;
    }

    @Override
    public void close() {}

    @Override
    public GpuQueryPool createTimestampQueryPool(int size) {
        return new NullQueryPool(size);
    }

    @Override
    public long getTimestampCalibrationOffset() {
        return 0;
    }

    @Override
    public DeviceInfo getDeviceInfo() {
        return INFO;
    }

    /** One growing direct buffer that every mapping aliases; nothing reads what was written. */
    static final class Scratch {
        private ByteBuffer buffer = ByteBuffer.allocateDirect(1 << 16);

        ByteBuffer view(long size) {
            if (size < 0 || size > Integer.MAX_VALUE - 8) {
                throw new IllegalArgumentException("mapping of " + size + " bytes");
            }
            int needed = (int) size;
            if (buffer.capacity() < needed) {
                buffer = ByteBuffer.allocateDirect(Math.max(needed, Math.min(buffer.capacity() * 2, Integer.MAX_VALUE - 8)));
            }
            return buffer.slice(0, needed).order(ByteOrder.nativeOrder());
        }
    }

    static final class NullBuffer extends BaseGpuBuffer {
        private final Scratch scratch;
        private boolean closed;

        NullBuffer(Scratch scratch, int usage, long size) {
            super(usage, size);
            this.scratch = scratch;
        }

        @Override
        public boolean isClosed() {
            return closed;
        }

        @Override
        public void close() {
            closed = true;
        }

        @Override
        public GpuBufferSlice.MappedView map(long offset, long length, boolean read, boolean write) {
            return new GpuBufferSlice.MappedView(new GpuBufferSlice(this, offset, length), scratch.view(length), () -> {});
        }
    }

    static final class NullTexture extends BaseGpuTexture {
        private boolean closed;

        NullTexture(int usage, String label, GpuFormat format, int width, int height, int depthOrLayers, int mipLevels) {
            super(usage, label, format, width, height, depthOrLayers, mipLevels);
        }

        @Override
        public boolean isClosed() {
            return closed;
        }

        @Override
        public void close() {
            closed = true;
        }
    }

    static final class NullTextureView extends BaseGpuTextureView {
        private boolean closed;

        NullTextureView(GpuTexture texture, int baseMipLevel, int mipLevels) {
            super(texture, baseMipLevel, mipLevels);
        }

        @Override
        public boolean isClosed() {
            return closed;
        }

        @Override
        public void close() {
            closed = true;
        }
    }

    record NullSampler(AddressMode u, AddressMode v, FilterMode min, FilterMode mag, int anisotropy, OptionalDouble maxLod)
            implements GpuSampler {
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
            return false;
        }

        @Override
        public void close() {}
    }

    static final class NullPipeline implements BackendRenderPipeline {
        private boolean closed;

        @Override
        public boolean isClosed() {
            return closed;
        }

        @Override
        public void close() {
            closed = true;
        }
    }

    record NullQueryPool(int size) implements GpuQueryPool {
        @Override
        public OptionalLong getValue(int index) {
            return OptionalLong.empty();
        }

        @Override
        public OptionalLong[] getValues(int index, int count) {
            OptionalLong[] out = new OptionalLong[count];
            java.util.Arrays.fill(out, OptionalLong.empty());
            return out;
        }

        @Override
        public void close() {}
    }

    static final class NullFence implements GpuFence {
        @Override
        public void close() {}

        @Override
        public boolean awaitCompletion(long timeoutNs) {
            return true;
        }
    }

    static final class NullSurface implements GpuSurfaceBackend {
        @Override
        public void configure(GpuSurface.Configuration config) {}

        @Override
        public boolean isSuboptimal() {
            return false;
        }

        @Override
        public void acquireNextTexture() {}

        @Override
        public void blitFromTexture(CommandEncoderBackend commandEncoder, GpuTextureView textureView) {}

        @Override
        public void present() {}

        @Override
        public Collection<GpuSurface.PresentMode> supportedPresentModes() {
            return EnumSet.allOf(GpuSurface.PresentMode.class);
        }

        @Override
        public void close() {}
    }

    record NullTransientMemory(Scratch scratch, NullBuffer buffer) implements TransientMemory {
        private GpuBufferSlice slice(long size) {
            return new GpuBufferSlice(buffer, 0, size);
        }

        private GpuBufferSlice.MappedView mapped(long size) {
            return new GpuBufferSlice.MappedView(slice(size), scratch.view(size), () -> {});
        }

        private static long total(List<ByteBuffer> data) {
            long total = 0;
            for (ByteBuffer b : data) {
                total += b.remaining();
            }
            return total;
        }

        @Override
        public ByteBuffer allocateCpu(long size, long alignment, long minimumAllocation, long elementSize) {
            return scratch.view(size);
        }

        @Override
        public GpuBufferSlice.MappedView allocateStaging(
                long size, long alignment, int usage, long minimumAllocation, long elementSize) {
            return mapped(size);
        }

        @Override
        public GpuBufferSlice allocateGpu(long size, long alignment, int usage, long minimumAllocation, long elementSize) {
            return slice(size);
        }

        @Override
        public GpuBufferSlice.MappedView allocateGpuMapped(
                long size, long alignment, int usage, long minimumAllocation, long elementSize) {
            return mapped(size);
        }

        @Override
        public GpuBufferSlice uploadStaging(
                List<ByteBuffer> data, long alignment, int usage, long minimumAllocation, long elementSize) {
            return slice(total(data));
        }

        @Override
        public GpuBufferSlice uploadGpu(
                List<ByteBuffer> data, long alignment, int usage, long minimumAllocation, long elementSize) {
            return slice(total(data));
        }

        @Override
        public List<GpuBufferSlice> multiUploadStaging(List<ByteBuffer> data, long alignment, int usage) {
            List<GpuBufferSlice> out = new ArrayList<>(data.size());
            for (ByteBuffer b : data) {
                out.add(slice(b.remaining()));
            }
            return out;
        }

        @Override
        public List<GpuBufferSlice> multiUploadGpu(List<ByteBuffer> data, long alignment, int usage) {
            return multiUploadStaging(data, alignment, usage);
        }
    }

    record NullEncoder(TransientMemory transientMemory) implements CommandEncoderBackend {
        @Override
        public void submit() {}

        @Override
        public RenderPassBackend createRenderPass(RenderPassDescriptor descriptor) {
            return new NullRenderPass();
        }

        @Override
        public void submitRenderPass() {}

        @Override
        public void clearColorTexture(GpuTexture colorTexture, Vector4fc clearColor) {}

        @Override
        public void clearColorAndDepthTextures(
                GpuTexture colorTexture, Vector4fc clearColor, GpuTexture depthTexture, double clearDepth) {}

        @Override
        public void clearColorAndDepthTextures(
                GpuTexture colorTexture,
                Vector4fc clearColor,
                GpuTexture depthTexture,
                double clearDepth,
                int regionX,
                int regionY,
                int regionWidth,
                int regionHeight,
                int mipLevel) {}

        @Override
        public void clearDepthTexture(GpuTexture depthTexture, double clearDepth) {}

        @Override
        public void writeToBuffer(GpuBufferSlice destination, ByteBuffer data) {}

        @Override
        public void copyToBuffer(GpuBufferSlice source, GpuBufferSlice target) {}

        @Override
        public void writeToTexture(
                GpuTexture destination, ByteBuffer source, int mipLevel, int depthOrLayer, int destX, int destY, int width, int height) {}

        @Override
        public void copyBufferToTexture(
                GpuBufferSlice source,
                int sourceX,
                int sourceY,
                int sourceWidth,
                int sourceHeight,
                GpuTexture destination,
                int destinationX,
                int destinationY,
                int copyWidth,
                int copyHeight,
                int mipLevel,
                int arrayLayer) {}

        @Override
        public void copyTextureToBuffer(GpuTexture source, GpuBuffer destination, long offset, Runnable callback, int mipLevel) {
            callback.run();
        }

        @Override
        public void copyTextureToBuffer(
                GpuTexture source,
                GpuBuffer destination,
                long offset,
                Runnable callback,
                int mipLevel,
                int x,
                int y,
                int width,
                int height) {
            callback.run();
        }

        @Override
        public void copyTextureToTexture(
                GpuTexture source, GpuTexture destination, int mipLevel, int destX, int destY, int sourceX, int sourceY, int width, int height) {}

        @Override
        public GpuFence createFence() {
            return new NullFence();
        }

        @Override
        public void writeTimestamp(GpuQueryPool pool, int index) {}
    }

    static final class NullRenderPass implements RenderPassBackend {
        @Override
        public void pushDebugGroup(Supplier<String> label) {}

        @Override
        public void popDebugGroup() {}

        @Override
        public void setPipeline(BackendRenderPipeline pipeline) {}

        @Override
        public void setUniform(int index, @Nullable Object value) {}

        @Override
        public void pushConstants(ByteBuffer value) {}

        @Override
        public void enableScissor(int x, int y, int width, int height) {}

        @Override
        public void disableScissor() {}

        @Override
        public void setVertexBuffer(int slot, @Nullable GpuBufferSlice vertexBuffer) {}

        @Override
        public void setIndexBuffer(GpuBuffer indexBuffer, IndexType indexType) {}

        @Override
        public void drawIndexed(int indexCount, int instanceCount, int firstIndex, int vertexOffset, int firstInstance) {}

        @Override
        public void multiDrawIndexed(IntBuffer drawParameters, int instanceCount, int firstInstance, int drawCount) {}

        @Override
        public void multiDrawIndexed(PointerBuffer firstIndexOffsets, IntBuffer indexCounts, IntBuffer vertexOffsets, int drawCount) {}

        @Override
        public void drawIndexedIndirect(GpuBufferSlice commands, int drawCount) {}

        @Override
        public void draw(int vertexCount, int instanceCount, int firstVertex, int firstInstance) {}

        @Override
        public void multiDraw(IntBuffer drawParameters, int instanceCount, int firstInstance, int drawCount) {}

        @Override
        public void multiDraw(IntBuffer firstVertices, IntBuffer vertexCounts, int drawCount) {}

        @Override
        public void drawIndirect(GpuBufferSlice commands, int drawCount) {}

        @Override
        public void writeTimestamp(GpuQueryPool pool, int index) {}
    }
}
