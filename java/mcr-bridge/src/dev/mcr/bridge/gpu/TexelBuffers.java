package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.pipeline.BindGroupLayout;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import com.mojang.renderpearl.api.pipeline.UniformType;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.renderer.RenderPipelines;

/** Pipelines with texel-buffer uniforms; the wgpu backend refuses them by design. */
public final class TexelBuffers {
    public static boolean uses(RenderPipeline pipeline) {
        for (BindGroupLayout layout : pipeline.getBindGroupLayouts()) {
            for (BindGroupLayout.UniformDescription uniform : layout.uniforms()) {
                if (uniform.type() == UniformType.TEXEL_BUFFER) {
                    return true;
                }
            }
        }
        return false;
    }

    public static List<RenderPipeline> withoutTexelPipelines(List<RenderPipeline> pipelines) {
        return pipelines.stream().filter(p -> !uses(p)).toList();
    }

    /** Called for the optional list; the unfiltered required list is read through a guard to avoid recursion. */
    public static List<RenderPipeline> withTexelPipelines(List<RenderPipeline> optional) {
        List<RenderPipeline> out = new ArrayList<>(optional);
        for (RenderPipeline p : requiredUnfiltered()) {
            if (uses(p)) {
                out.add(p);
            }
        }
        return List.copyOf(out);
    }

    private static final ThreadLocal<Boolean> READING = ThreadLocal.withInitial(() -> false);

    private static List<RenderPipeline> requiredUnfiltered() {
        READING.set(true);
        try {
            return RenderPipelines.requiredPipelines();
        } finally {
            READING.set(false);
        }
    }

    public static boolean readingUnfiltered() {
        return READING.get();
    }

    private TexelBuffers() {}
}
