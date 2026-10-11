package dev.mcr.bridge.mixin;

import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import dev.mcr.bridge.gpu.TexelBuffers;
import java.util.List;
import net.minecraft.client.renderer.RenderPipelines;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Texel-buffer pipelines (clouds, which Rust draws) cannot compile on wgpu, so their failure must not abort a reload. */
@Mixin(RenderPipelines.class)
abstract class RenderPipelinesMixin {
    @Inject(method = "requiredPipelines", at = @At("RETURN"), cancellable = true)
    private static void mcr$dropTexelBufferPipelines(CallbackInfoReturnable<List<RenderPipeline>> cir) {
        if (!TexelBuffers.readingUnfiltered()) {
            cir.setReturnValue(TexelBuffers.withoutTexelPipelines(cir.getReturnValue()));
        }
    }

    @Inject(method = "optionalPipelines", at = @At("RETURN"), cancellable = true)
    private static void mcr$addTexelBufferPipelines(CallbackInfoReturnable<List<RenderPipeline>> cir) {
        cir.setReturnValue(TexelBuffers.withTexelPipelines(cir.getReturnValue()));
    }
}
