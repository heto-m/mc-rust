package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.commands.GpuQueryPool;
import com.mojang.renderpearl.api.device.DeviceFeatures;
import com.mojang.renderpearl.api.device.DeviceInfo;
import com.mojang.renderpearl.api.device.DeviceLimits;
import com.mojang.renderpearl.api.device.DeviceType;
import com.mojang.renderpearl.api.device.HintsAndWorkarounds;
import com.mojang.renderpearl.api.pipeline.BlendFactor;
import com.mojang.renderpearl.api.pipeline.BlendOp;
import com.mojang.renderpearl.api.pipeline.CompareOp;
import com.mojang.renderpearl.api.pipeline.PolygonMode;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import com.mojang.renderpearl.api.pipeline.UniformType;
import com.mojang.renderpearl.api.textures.AddressMode;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuSampler;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import com.mojang.renderpearl.backend.api.BackendRenderPipeline;
import com.mojang.renderpearl.backend.api.CommandEncoderBackend;
import com.mojang.renderpearl.backend.api.GpuDeviceBackend;
import com.mojang.renderpearl.backend.api.GpuSurfaceBackend;
import dev.mcr.bridge.BridgeAbi;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.List;
import java.util.OptionalDouble;
import java.util.Set;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.function.BooleanSupplier;
import java.util.function.Supplier;
import org.jspecify.annotations.Nullable;

/** Renderpearl's device backend over mcr's wgpu device: every object is a Rust handle. */
public final class McrDeviceBackend implements GpuDeviceBackend {
    private static final int MAX_ANISOTROPY = 16;
    private static final int MAX_MULTI_DRAW = 1 << 16;

    static volatile @Nullable McrDeviceBackend instance;

    private final DeviceInfo info;
    private final ConcurrentLinkedQueue<Long> pendingDestroys = new ConcurrentLinkedQueue<>();
    private @Nullable McrCommandEncoder encoder;

    public McrDeviceBackend() {
        checkEnums();
        this.info = readInfo();
        GpuNative.LOGGER.info("mcr GPU backend on {}", info.name());
        instance = this;
    }

    private static void checkEnums() {
        check("GpuFormat", BridgeAbi.GPU_FORMATS, GpuFormat.values());
        check("BlendFactor", BridgeAbi.BLEND_FACTORS, BlendFactor.values());
        check("BlendOp", BridgeAbi.BLEND_OPS, BlendOp.values());
        check("CompareOp", BridgeAbi.COMPARE_OPS, CompareOp.values());
        check("PolygonMode", BridgeAbi.POLYGON_MODES, PolygonMode.values());
        check("PrimitiveTopology", BridgeAbi.TOPOLOGIES, PrimitiveTopology.values());
        check("UniformType", BridgeAbi.UNIFORM_TYPES, UniformType.values());
        check("AddressMode", BridgeAbi.ADDRESS_MODES, AddressMode.values());
        check("FilterMode", BridgeAbi.FILTER_MODES, FilterMode.values());
    }

    private static void check(String what, String[] expected, Enum<?>[] live) {
        String[] names = Arrays.stream(live).map(Enum::name).toArray(String[]::new);
        if (!Arrays.equals(expected, names)) {
            throw new IllegalStateException(what + " changed: " + Arrays.toString(names));
        }
    }

    private static DeviceInfo readInfo() {
        try (Arena arena = Arena.ofConfined()) {
            int cap = 8 * BridgeAbi.GPU_INFO_SLOTS + 4096;
            MemorySegment out = arena.allocate(cap, 8);
            GpuNative.require(GpuNative.device(BridgeAbi.D_INFO, out.address(), cap), "device info");
            long[] v = out.asSlice(0, 8L * BridgeAbi.GPU_INFO_SLOTS).toArray(ValueLayout.JAVA_LONG);
            int textLen = (int) Math.min(v[5], 4096);
            String[] text = new String(
                            out.asSlice(8L * BridgeAbi.GPU_INFO_SLOTS, textLen).toArray(ValueLayout.JAVA_BYTE),
                            StandardCharsets.UTF_8)
                    .split("\n", -1);
            String part = text.length > 2 ? text[2] : "";
            boolean line = (v[4] & BridgeAbi.GPU_FEATURE_POLYGON_LINE) != 0;
            return new DeviceInfo(
                    text[0],
                    text.length > 1 ? text[1] : "",
                    part,
                    true,
                    "mcr-wgpu",
                    1.0F,
                    new DeviceLimits(
                            MAX_ANISOTROPY, (int) v[0], (int) v[1], v[2], MAX_MULTI_DRAW, (int) v[3], MAX_MULTI_DRAW),
                    new DeviceFeatures(line, false, true, false, true, true, true, false),
                    Set.of(),
                    new HintsAndWorkarounds(false, false, false, false),
                    DeviceType.OTHER);
        }
    }

    McrCommandEncoder encoder() {
        if (encoder == null) {
            throw new IllegalStateException("no command encoder yet");
        }
        return encoder;
    }

    /** Destroys run after the next submit, so passes recorded before a close still see the object. */
    void destroyLater(long handle) {
        pendingDestroys.add(handle);
    }

    void runDestroys() {
        Long h;
        while ((h = pendingDestroys.poll()) != null) {
            GpuNative.check(GpuNative.device(BridgeAbi.D_DESTROY, h), "destroy");
        }
    }

    @Override
    public GpuSurfaceBackend createSurface(long windowHandle, BooleanSupplier isIconified) {
        return new McrSurface(this, GpuNative.require(GpuNative.device(BridgeAbi.D_SURFACE), "surface"));
    }

    @Override
    public CommandEncoderBackend createCommandEncoder() {
        McrCommandEncoder e = new McrCommandEncoder(this, GpuNative.require(GpuNative.device(BridgeAbi.D_ENCODER), "encoder"));
        if (encoder == null) {
            encoder = e;
        }
        return e;
    }

    @Override
    public GpuSampler createSampler(
            AddressMode u, AddressMode v, FilterMode min, FilterMode mag, int maxAnisotropy, OptionalDouble maxLod) {
        long lod = maxLod.isPresent() ? Double.doubleToRawLongBits(maxLod.getAsDouble()) : -1L;
        long h = GpuNative.require(
                GpuNative.device(BridgeAbi.D_SAMPLER, u.ordinal(), v.ordinal(), min.ordinal(), mag.ordinal(), maxAnisotropy, lod),
                "sampler");
        return new McrSampler(this, h, u, v, min, mag, maxAnisotropy, maxLod);
    }

    @Override
    public GpuTexture createTexture(
            @Nullable String label, int usage, GpuFormat format, int width, int height, int depthOrLayers, int mipLevels) {
        long h = GpuNative.require(
                GpuNative.device(BridgeAbi.D_TEXTURE, FrameSlots.textureUsage(usage), format.ordinal(), width, height, depthOrLayers, mipLevels),
                "texture " + label);
        return new McrTexture(this, h, usage, label == null ? "" : label, format, width, height, depthOrLayers, mipLevels);
    }

    @Override
    public GpuTextureView createTextureView(GpuTexture texture, int baseMipLevel, int mipLevels) {
        long h = GpuNative.require(
                GpuNative.device(BridgeAbi.D_VIEW, GpuNative.handleOf(texture), baseMipLevel, mipLevels), "texture view");
        return new McrTextureView(this, h, texture, baseMipLevel, mipLevels);
    }

    @Override
    public GpuBuffer createBuffer(@Nullable Supplier<String> label, int usage, long size) {
        long h = GpuNative.require(GpuNative.device(BridgeAbi.D_BUFFER, usage, size, 0, 0), "buffer");
        return new McrBuffer(this, usage, size, h);
    }

    @Override
    public GpuBuffer createBuffer(@Nullable Supplier<String> label, int usage, ByteBuffer data) {
        long size = data.remaining();
        long h = GpuNative.require(GpuNative.device(BridgeAbi.D_BUFFER, usage, size, GpuNative.address(data), size), "buffer");
        return new McrBuffer(this, usage, size, h);
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
    public BackendRenderPipeline.Pending compilePipeline(BackendRenderPipeline.CreateInfo createInfo) {
        ByteBuffer encoded = PipelineInfo.encode(createInfo);
        if (System.getProperty(PipelineInfo.DUMP_PROPERTY) != null) {
            PipelineInfo.dump(createInfo, encoded);
        }
        if (createInfo.uniforms().stream().anyMatch(u -> u.type() == UniformType.TEXEL_BUFFER)) {
            GpuNative.LOGGER.debug("{} uses texel buffers, which the wgpu backend does not support", createInfo.name());
            return BackendRenderPipeline.Pending.NULL;
        }
        long h;
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment bytes = arena.allocate(Math.max(encoded.remaining(), 1), 8);
            MemorySegment.copy(MemorySegment.ofBuffer(encoded), 0, bytes, 0, encoded.remaining());
            h = GpuNative.device(BridgeAbi.D_PIPELINE, bytes.address(), encoded.remaining());
        }
        if (h <= 0) {
            GpuNative.LOGGER.error("pipeline {} failed to compile: {}", createInfo.name(), GpuNative.lastError());
            return BackendRenderPipeline.Pending.NULL;
        }
        McrPipeline pipeline = new McrPipeline(this, h);
        return () -> pipeline;
    }

    @Override
    public void close() {
        runDestroys();
    }

    @Override
    public GpuQueryPool createTimestampQueryPool(int size) {
        return new McrQueryPool(size);
    }

    @Override
    public long getTimestampCalibrationOffset() {
        return 0;
    }

    @Override
    public DeviceInfo getDeviceInfo() {
        return info;
    }

    /** Backend counters (`passes ops draws skipped bindHits bindMisses encodeNs submits uploaded errors`). */
    public static long[] stats() {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment out = arena.allocate(8 * 16, 8);
            long n = GpuNative.device(BridgeAbi.D_STATS, out.address(), 8 * 16);
            return n <= 0 ? new long[0] : out.asSlice(0, 8 * n).toArray(ValueLayout.JAVA_LONG);
        }
    }
}
