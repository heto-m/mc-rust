package dev.mcr.bridge.mixin;

import net.minecraft.client.renderer.CloudRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Rust draws clouds; their pipelines need texel buffers, which the wgpu backend does not compile. */
@Mixin(CloudRenderer.class)
abstract class CloudRendererMixin {
    @Inject(
            method = "render(Lnet/minecraft/client/CloudStatus;Lcom/mojang/renderpearl/api/commands/RenderPass;)V",
            at = @At("HEAD"),
            cancellable = true)
    private void mcr$rustDrawsClouds(CallbackInfo ci) {
        ci.cancel();
    }

    @Inject(method = "renderOit", at = @At("HEAD"), cancellable = true)
    private void mcr$rustDrawsOitClouds(CallbackInfo ci) {
        ci.cancel();
    }
}
