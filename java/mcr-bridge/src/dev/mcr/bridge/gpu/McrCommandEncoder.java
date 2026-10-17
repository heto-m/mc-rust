package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.buffers.TransientMemory;
import com.mojang.renderpearl.api.commands.GpuFence;
import com.mojang.renderpearl.api.commands.GpuQueryPool;
import com.mojang.renderpearl.api.commands.RenderPassDescriptor;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.backend.api.CommandEncoderBackend;
import com.mojang.renderpearl.backend.api.RenderPassBackend;
import dev.mcr.bridge.BridgeAbi;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.ByteBuffer;
import java.util.HashMap;
import java.util.Map;
import org.joml.Vector4fc;
import org.jspecify.annotations.Nullable;

/** Records vanilla's commands in order on a Rust-side wgpu encoder; passes go through {@link McrRenderPass}. */
final class McrCommandEncoder implements CommandEncoderBackend {
    private static final int MAX_COMPLETIONS = 256;

    final McrDeviceBackend device;
    final long handle;
    private final McrTransientMemory transientMemory;
    private final McrRenderPass pass;
    private final Map<Long, Runnable> callbacks = new HashMap<>();
    private final MemorySegment completions = Arena.ofAuto().allocate(8L * MAX_COMPLETIONS, 8);
    private long nextCallback = 1;
    /** The submit being recorded: Rust's submit count plus one. */
    long submitIndex = 1;
    private boolean inPass;

    McrCommandEncoder(McrDeviceBackend device, long handle) {
        this.device = device;
        this.handle = handle;
        this.transientMemory = new McrTransientMemory(this);
        this.pass = new McrRenderPass(this);
    }

    private long op(int op, String what, long... args) {
        return GpuNative.check(GpuNative.encode(handle, op, args), what);
    }

    private static int bits(float f) {
        return Float.floatToRawIntBits(f);
    }

    @Override
    public void submit() {
        long n = op(BridgeAbi.E_SUBMIT, "submit");
        if (n > 0) {
            submitIndex = n + 1;
        }
        transientMemory.rotate();
        device.runDestroys();
        pollCallbacks();
    }

    /** Cuts the recording at a slot change; an open pass is cut too and continues after it. */
    void split() {
        if (inPass) {
            pass.cut();
        }
        op(BridgeAbi.E_SPLIT, "slot split");
    }

    void pollCallbacks() {
        long n = op(BridgeAbi.E_POLL, "poll", completions.address(), MAX_COMPLETIONS);
        for (int i = 0; i < n; i++) {
            Runnable r = callbacks.remove(completions.getAtIndex(ValueLayout.JAVA_LONG, i));
            if (r != null) {
                r.run();
            }
        }
    }

    boolean await(long submit, long timeoutNs) {
        if (timeoutNs == 0 && submit >= submitIndex) {
            // A poll of the open recording: not done, and nothing may rotate the transient memory.
            return false;
        }
        long r = op(BridgeAbi.E_WAIT, "fence wait", submit, timeoutNs < 0 ? -1L : timeoutNs);
        if (submit >= submitIndex) {
            submitIndex = Math.max(submitIndex, submit + 1);
            transientMemory.rotate();
        }
        pollCallbacks();
        return r != 0;
    }

    @Override
    public TransientMemory transientMemory() {
        return transientMemory;
    }

    @Override
    public RenderPassBackend createRenderPass(RenderPassDescriptor descriptor) {
        inPass = true;
        pass.begin(descriptor);
        return pass;
    }

    @Override
    public void submitRenderPass() {
        pass.end();
        inPass = false;
    }

    private void clear(long color, @Nullable Vector4fc c, long depth, double d) {
        op(BridgeAbi.E_CLEAR, "clear", color, c == null ? 0 : bits(c.x()), c == null ? 0 : bits(c.y()),
                c == null ? 0 : bits(c.z()), c == null ? 0 : bits(c.w()), depth, bits((float) d));
    }

    @Override
    public void clearColorTexture(GpuTexture colorTexture, Vector4fc clearColor) {
        clear(GpuNative.handleOf(colorTexture), clearColor, 0, 0);
    }

    @Override
    public void clearColorAndDepthTextures(GpuTexture colorTexture, Vector4fc clearColor, GpuTexture depthTexture, double clearDepth) {
        clear(GpuNative.handleOf(colorTexture), clearColor, GpuNative.handleOf(depthTexture), clearDepth);
    }

    @Override
    public void clearColorAndDepthTextures(
            GpuTexture colorTexture,
            Vector4fc c,
            GpuTexture depthTexture,
            double clearDepth,
            int regionX,
            int regionY,
            int regionWidth,
            int regionHeight,
            int mipLevel) {
        op(BridgeAbi.E_CLEAR_REGION, "region clear", GpuNative.handleOf(colorTexture), bits(c.x()), bits(c.y()),
                bits(c.z()), bits(c.w()), GpuNative.handleOf(depthTexture), bits((float) clearDepth), regionX, regionY,
                regionWidth, regionHeight, mipLevel);
    }

    @Override
    public void clearDepthTexture(GpuTexture depthTexture, double clearDepth) {
        clear(0, null, GpuNative.handleOf(depthTexture), clearDepth);
    }

    @Override
    public void writeToBuffer(GpuBufferSlice destination, ByteBuffer data) {
        int len = (int) Math.min(data.remaining(), destination.length());
        op(BridgeAbi.E_WRITE_BUFFER, "buffer write", GpuNative.handleOf(destination.buffer()), destination.offset(),
                GpuNative.address(data), len);
    }

    @Override
    public void copyToBuffer(GpuBufferSlice source, GpuBufferSlice target) {
        op(BridgeAbi.E_COPY_BUFFER, "buffer copy", GpuNative.handleOf(source.buffer()), source.offset(),
                GpuNative.handleOf(target.buffer()), target.offset(), source.length());
    }

    @Override
    public void writeToTexture(GpuTexture destination, ByteBuffer source, int mipLevel, int depthOrLayer, int destX, int destY,
            int width, int height) {
        op(BridgeAbi.E_WRITE_TEXTURE, "texture write", GpuNative.handleOf(destination), GpuNative.address(source),
                source.remaining(), mipLevel, depthOrLayer, destX, destY, width, height);
    }

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
            int arrayLayer) {
        op(BridgeAbi.E_BUFFER_TO_TEXTURE, "buffer to texture", GpuNative.handleOf(source.buffer()), source.offset(),
                sourceX, sourceY, sourceWidth, sourceHeight, GpuNative.handleOf(destination), destinationX, destinationY,
                copyWidth, copyHeight, mipLevel, arrayLayer);
    }

    @Override
    public void copyTextureToBuffer(GpuTexture source, GpuBuffer destination, long offset, Runnable callback, int mipLevel) {
        copyTextureToBuffer(source, destination, offset, callback, mipLevel, 0, 0, source.getWidth(mipLevel),
                source.getHeight(mipLevel));
    }

    @Override
    public void copyTextureToBuffer(GpuTexture source, GpuBuffer destination, long offset, Runnable callback, int mipLevel,
            int x, int y, int width, int height) {
        long id = nextCallback++;
        long r = op(BridgeAbi.E_TEXTURE_TO_BUFFER, "texture readback", GpuNative.handleOf(source),
                GpuNative.handleOf(destination), offset, id, mipLevel, x, y, width, height);
        if (r < 0) {
            callback.run();
        } else {
            callbacks.put(id, callback);
        }
    }

    @Override
    public void copyTextureToTexture(GpuTexture source, GpuTexture destination, int mipLevel, int destX, int destY,
            int sourceX, int sourceY, int width, int height) {
        op(BridgeAbi.E_TEXTURE_TO_TEXTURE, "texture copy", GpuNative.handleOf(source), GpuNative.handleOf(destination),
                mipLevel, destX, destY, sourceX, sourceY, width, height);
    }

    @Override
    public GpuFence createFence() {
        return new McrFence(this, submitIndex);
    }

    @Override
    public void writeTimestamp(GpuQueryPool pool, int index) {}

    void flushMapped(long buffer, long offset, long length) {
        op(BridgeAbi.E_FLUSH_MAPPED, "mapped flush", buffer, offset, length);
    }

    void dirty(long block, long offset, long length) {
        op(BridgeAbi.E_DIRTY, "transient upload", block, offset, length);
    }

    void blit(long surface, long view) {
        op(BridgeAbi.E_BLIT, "surface blit", surface, view);
    }

    boolean inPass() {
        return inPass;
    }
}
