package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.commands.GpuQueryPool;
import com.mojang.renderpearl.api.commands.RenderPassDescriptor;
import com.mojang.renderpearl.api.pipeline.IndexType;
import com.mojang.renderpearl.backend.api.BackendRenderPipeline;
import com.mojang.renderpearl.backend.api.RenderPassBackend;
import com.mojang.renderpearl.util.TextureViewAndSampler;
import dev.mcr.bridge.BridgeAbi;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.ByteBuffer;
import java.nio.IntBuffer;
import java.util.Optional;
import java.util.OptionalDouble;
import java.util.function.Supplier;
import org.joml.Vector4fc;
import org.jspecify.annotations.Nullable;
import org.lwjgl.PointerBuffer;

/** Appends pass commands to an off-heap stream that Rust replays in one downcall when the pass ends. */
final class McrRenderPass implements RenderPassBackend {
    private static final ValueLayout.OfInt INT = ValueLayout.JAVA_INT_UNALIGNED;
    private static final int DESC_SLOTS = 1 + 8 * 6 + 7;
    private static final int UNIFORM_ALIGNMENT = 256;
    private static final boolean DEBUG_GROUPS = Boolean.getBoolean("mcr.gpu.debugGroups");

    private final McrCommandEncoder encoder;
    private final MemorySegment desc = Arena.ofAuto().allocate(8L * DESC_SLOTS, 8);
    private MemorySegment stream = Arena.ofAuto().allocate(64 << 10, 8);
    private int at;
    private int descCount;
    private long begun;

    /** Render-thread totals for the bench: time from pass creation to its end, and in the native encode call. */
    static long recordNanos;
    static long nativeNanos;

    McrRenderPass(McrCommandEncoder encoder) {
        this.encoder = encoder;
    }

    private static long bits(float f) {
        return Float.floatToRawIntBits(f) & 0xFFFF_FFFFL;
    }

    void begin(RenderPassDescriptor d) {
        begun = System.nanoTime();
        at = 0;
        int n = 0;
        desc.setAtIndex(ValueLayout.JAVA_LONG, n++, d.colorAttachments().size());
        for (RenderPassDescriptor.Attachment<Optional<Vector4fc>> c : d.colorAttachments()) {
            Vector4fc clear = c == null ? null : c.clearValue().orElse(null);
            desc.setAtIndex(ValueLayout.JAVA_LONG, n++, c == null ? 0 : GpuNative.handleOf(c.textureView()));
            desc.setAtIndex(ValueLayout.JAVA_LONG, n++, clear == null ? 0 : 1);
            desc.setAtIndex(ValueLayout.JAVA_LONG, n++, clear == null ? 0 : bits(clear.x()));
            desc.setAtIndex(ValueLayout.JAVA_LONG, n++, clear == null ? 0 : bits(clear.y()));
            desc.setAtIndex(ValueLayout.JAVA_LONG, n++, clear == null ? 0 : bits(clear.z()));
            desc.setAtIndex(ValueLayout.JAVA_LONG, n++, clear == null ? 0 : bits(clear.w()));
        }
        RenderPassDescriptor.Attachment<OptionalDouble> depth = d.depthAttachment();
        OptionalDouble depthClear = depth == null ? OptionalDouble.empty() : depth.clearValue();
        desc.setAtIndex(ValueLayout.JAVA_LONG, n++, depth == null ? 0 : GpuNative.handleOf(depth.textureView()));
        desc.setAtIndex(ValueLayout.JAVA_LONG, n++, depthClear.isPresent() ? 1 : 0);
        desc.setAtIndex(ValueLayout.JAVA_LONG, n++, depthClear.isPresent() ? bits((float) depthClear.getAsDouble()) : 0);
        desc.setAtIndex(ValueLayout.JAVA_LONG, n++, d.renderArea().x());
        desc.setAtIndex(ValueLayout.JAVA_LONG, n++, d.renderArea().y());
        desc.setAtIndex(ValueLayout.JAVA_LONG, n++, d.renderArea().width());
        desc.setAtIndex(ValueLayout.JAVA_LONG, n++, d.renderArea().height());
        descCount = n;
    }

    void end() {
        long recorded = System.nanoTime();
        GpuNative.check(GpuNative.pass(encoder.handle, desc, descCount, stream, at), "render pass");
        at = 0;
        recordNanos += recorded - begun;
        nativeNanos += System.nanoTime() - recorded;
    }

    private void reserve(int words) {
        long need = at + 4L * words;
        if (need > stream.byteSize()) {
            MemorySegment bigger = Arena.ofAuto().allocate(Math.max(need, stream.byteSize() * 2), 8);
            MemorySegment.copy(stream, 0, bigger, 0, at);
            stream = bigger;
        }
    }

    private void put(int v) {
        stream.set(INT, at, v);
        at += 4;
    }

    private void put64(long v) {
        put((int) v);
        put((int) (v >>> 32));
    }

    @Override
    public void pushDebugGroup(Supplier<String> label) {
        if (!DEBUG_GROUPS) {
            return;
        }
        byte[] bytes = label.get().getBytes(java.nio.charset.StandardCharsets.UTF_8);
        int len = Math.min(bytes.length, 256);
        reserve(2 + (len + 3) / 4);
        put(BridgeAbi.P_DEBUG_PUSH);
        put(len);
        MemorySegment.copy(bytes, 0, stream, ValueLayout.JAVA_BYTE, at, len);
        at += (len + 3) & ~3;
    }

    @Override
    public void popDebugGroup() {
        if (DEBUG_GROUPS) {
            reserve(1);
            put(BridgeAbi.P_DEBUG_POP);
        }
    }

    @Override
    public void setPipeline(BackendRenderPipeline pipeline) {
        reserve(3);
        put(BridgeAbi.P_PIPELINE);
        put64(GpuNative.handleOf(pipeline));
    }

    @Override
    public void setUniform(int index, @Nullable Object value) {
        if (value instanceof GpuBufferSlice slice) {
            reserve(8);
            put(BridgeAbi.P_UNIFORM_BUFFER);
            put(index);
            put64(GpuNative.handleOf(slice.buffer()));
            put64(slice.offset());
            put64(slice.length());
        } else if (value instanceof TextureViewAndSampler tvs) {
            reserve(6);
            put(BridgeAbi.P_UNIFORM_TEXTURE);
            put(index);
            put64(GpuNative.handleOf(tvs.view()));
            put64(GpuNative.handleOf(tvs.sampler()));
        } else {
            reserve(2);
            put(BridgeAbi.P_UNIFORM_NONE);
            put(index);
        }
    }

    @Override
    public void pushConstants(ByteBuffer value) {
        GpuBufferSlice slice = encoder.transientMemory().uploadGpu(value, UNIFORM_ALIGNMENT, GpuBuffer.USAGE_UNIFORM);
        reserve(7);
        put(BridgeAbi.P_PUSH);
        put64(GpuNative.handleOf(slice.buffer()));
        put64(slice.offset());
        put64(slice.length());
    }

    @Override
    public void enableScissor(int x, int y, int width, int height) {
        reserve(5);
        put(BridgeAbi.P_SCISSOR);
        put(x);
        put(y);
        put(width);
        put(height);
    }

    @Override
    public void disableScissor() {
        reserve(1);
        put(BridgeAbi.P_NO_SCISSOR);
    }

    @Override
    public void setVertexBuffer(int slot, @Nullable GpuBufferSlice vertexBuffer) {
        reserve(6);
        put(BridgeAbi.P_VERTEX);
        put(slot);
        put64(vertexBuffer == null ? 0 : GpuNative.handleOf(vertexBuffer.buffer()));
        put64(vertexBuffer == null ? 0 : vertexBuffer.offset());
    }

    @Override
    public void setIndexBuffer(GpuBuffer indexBuffer, IndexType indexType) {
        reserve(4);
        put(BridgeAbi.P_INDEX);
        put64(GpuNative.handleOf(indexBuffer));
        put(indexType == IndexType.INT ? 1 : 0);
    }

    @Override
    public void drawIndexed(int indexCount, int instanceCount, int firstIndex, int vertexOffset, int firstInstance) {
        reserve(6);
        put(BridgeAbi.P_DRAW_INDEXED);
        put(indexCount);
        put(instanceCount);
        put(firstIndex);
        put(vertexOffset);
        put(firstInstance);
    }

    @Override
    public void multiDrawIndexed(IntBuffer drawParameters, int instanceCount, int firstInstance, int drawCount) {
        reserve(4 + 3 * drawCount);
        put(BridgeAbi.P_MULTI_DRAW_INDEXED);
        put(drawCount);
        put(instanceCount);
        put(firstInstance);
        int base = drawParameters.position();
        for (int i = 0; i < 3 * drawCount; i++) {
            put(drawParameters.get(base + i));
        }
    }

    @Override
    public void multiDrawIndexed(PointerBuffer firstIndexOffsets, IntBuffer indexCounts, IntBuffer vertexOffsets, int drawCount) {
        throw new UnsupportedOperationException("the mcr backend does not support multiDrawDirectSeparate");
    }

    @Override
    public void drawIndexedIndirect(GpuBufferSlice commands, int drawCount) {
        reserve(6);
        put(BridgeAbi.P_DRAW_INDEXED_INDIRECT);
        put64(GpuNative.handleOf(commands.buffer()));
        put64(commands.offset());
        put(drawCount);
    }

    @Override
    public void draw(int vertexCount, int instanceCount, int firstVertex, int firstInstance) {
        reserve(5);
        put(BridgeAbi.P_DRAW);
        put(vertexCount);
        put(instanceCount);
        put(firstVertex);
        put(firstInstance);
    }

    @Override
    public void multiDraw(IntBuffer drawParameters, int instanceCount, int firstInstance, int drawCount) {
        reserve(4 + 2 * drawCount);
        put(BridgeAbi.P_MULTI_DRAW);
        put(drawCount);
        put(instanceCount);
        put(firstInstance);
        int base = drawParameters.position();
        for (int i = 0; i < 2 * drawCount; i++) {
            put(drawParameters.get(base + i));
        }
    }

    @Override
    public void multiDraw(IntBuffer firstVertices, IntBuffer vertexCounts, int drawCount) {
        throw new UnsupportedOperationException("the mcr backend does not support multiDrawDirectSeparate");
    }

    @Override
    public void drawIndirect(GpuBufferSlice commands, int drawCount) {
        reserve(6);
        put(BridgeAbi.P_DRAW_INDIRECT);
        put64(GpuNative.handleOf(commands.buffer()));
        put64(commands.offset());
        put(drawCount);
    }

    @Override
    public void writeTimestamp(GpuQueryPool pool, int index) {}
}
