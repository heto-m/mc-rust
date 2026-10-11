package dev.mcr.bridge.gpu;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import dev.mcr.bridge.Bridge;
import dev.mcr.bridge.mixin.PostChainAccessor;
import dev.mcr.bridge.mixin.PostPassAccessor;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.renderer.LevelTargetBundle;
import net.minecraft.client.renderer.PostChain;
import net.minecraft.client.renderer.PostPass;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.resources.Identifier;

/**
 * Opt-in test hooks (`-Dmcr.gpu.selftest=…`), run on the render thread once the title screen is idle. `bench` reports
 * `gpu_bench` per-frame averages for each screen it sees, after a warm-up.
 */
public final class GpuSelfTest {
    public static final String PROPERTY = "mcr.gpu.selftest";
    private static final String MODE = System.getProperty(PROPERTY, "");

    private static final int BENCH_WARMUP = 30;
    private static final int BENCH_FRAMES = 120;

    private static boolean ran;
    private static final java.util.Set<String> benched = new java.util.HashSet<>();
    private static String benchScreen = "";
    private static int benchFrame;
    private static long[] benchStats;
    private static long benchRecord;
    private static long benchNative;
    private static long benchTime;

    public static void onFrame(Minecraft minecraft) {
        if (MODE.contains("bench")) {
            bench(minecraft);
        }
        if (ran || MODE.isEmpty() || !(minecraft.gui.screen() instanceof TitleScreen) || minecraft.gui.overlay() != null) {
            return;
        }
        ran = true;
        if (MODE.contains("compile-all")) {
            compileAll(minecraft);
        }
        if (MODE.contains("stats")) {
            Bridge.info("gpu_stats", java.util.Arrays.toString(McrDeviceBackend.stats()));
        }
    }

    private static void bench(Minecraft minecraft) {
        if (minecraft.gui.overlay() != null) {
            return;
        }
        Object screen = minecraft.gui.screen();
        String name = screen == null ? "none" : screen.getClass().getSimpleName();
        if (benched.contains(name)) {
            return;
        }
        if (!name.equals(benchScreen)) {
            benchScreen = name;
            benchFrame = 0;
        }
        if (++benchFrame == BENCH_WARMUP) {
            benchStats = McrDeviceBackend.stats();
            benchRecord = McrRenderPass.recordNanos;
            benchNative = McrRenderPass.nativeNanos;
            benchTime = System.nanoTime();
        } else if (benchFrame == BENCH_WARMUP + BENCH_FRAMES && benchStats.length >= 10) {
            benched.add(name);
            long[] now = McrDeviceBackend.stats();
            double n = BENCH_FRAMES;
            long hits = now[4] - benchStats[4];
            long misses = now[5] - benchStats[5];
            Bridge.info(
                    "gpu_bench",
                    String.format(
                            java.util.Locale.ROOT,
                            "screen=%s frames=%d frame_ms=%.2f passes=%.1f ops=%.0f draws=%.0f record_us=%.0f native_us=%.0f"
                                    + " encode_us=%.0f bind_hit=%.1f%% submits=%.1f upload_kb=%.1f errors=%d",
                            name,
                            BENCH_FRAMES,
                            (System.nanoTime() - benchTime) / n / 1e6,
                            (now[0] - benchStats[0]) / n,
                            (now[1] - benchStats[1]) / n,
                            (now[2] - benchStats[2]) / n,
                            (McrRenderPass.recordNanos - benchRecord) / n / 1e3,
                            (McrRenderPass.nativeNanos - benchNative) / n / 1e3,
                            (now[6] - benchStats[6]) / n / 1e3,
                            hits + misses == 0 ? 0.0 : 100.0 * hits / (hits + misses),
                            (now[7] - benchStats[7]) / n,
                            (now[8] - benchStats[8]) / n / 1024,
                            now[9] - benchStats[9]));
        }
    }

    /** Compiles every core and post pipeline and reports `gpu_pipelines` (`core=ok/total post=ok/total texel=n failed=…`). */
    public static void compileAll(Minecraft minecraft) {
        List<String> failed = new ArrayList<>();
        int coreOk = 0;
        int coreTotal = 0;
        int texel = 0;
        List<RenderPipeline> core = new ArrayList<>(RenderPipelines.requiredPipelines());
        core.addAll(RenderPipelines.optionalPipelines());
        for (RenderPipeline pipeline : core) {
            if (TexelBuffers.uses(pipeline)) {
                texel++;
                if (RenderSystem.getCompiledPipelineNullable(pipeline) != null) {
                    failed.add("texel-compiled:" + pipeline.getLocation());
                }
                continue;
            }
            coreTotal++;
            if (RenderSystem.getCompiledPipelineNullable(pipeline) != null) {
                coreOk++;
            } else {
                failed.add(pipeline.getLocation().toString());
            }
        }
        int postOk = 0;
        int postTotal = 0;
        List<Identifier> effects = minecraft.getShaderManager().getAvailablePostEffects().sorted().toList();
        for (Identifier id : effects) {
            PostChain chain = minecraft.getShaderManager().getPostChain(id, LevelTargetBundle.OUTLINE_TARGETS);
            if (chain == null) {
                postTotal++;
                failed.add("post:" + id);
                continue;
            }
            for (PostPass pass : ((PostChainAccessor) chain).mcr$passes()) {
                RenderPipeline pipeline = ((PostPassAccessor) pass).mcr$pipeline();
                postTotal++;
                if (RenderSystem.getCompiledPipelineNullable(pipeline) != null) {
                    postOk++;
                } else {
                    failed.add("post:" + pipeline.getLocation());
                }
            }
        }
        Bridge.info(
                "gpu_pipelines",
                "core=" + coreOk + "/" + coreTotal + " post=" + postOk + "/" + postTotal + " texel=" + texel + " failed="
                        + String.join(",", failed));
    }

    private GpuSelfTest() {}
}
