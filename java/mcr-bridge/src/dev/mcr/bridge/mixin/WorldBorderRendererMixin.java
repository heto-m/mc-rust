package dev.mcr.bridge.mixin;

import net.minecraft.client.renderer.WorldBorderRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Rust draws the world border. */
@Mixin(WorldBorderRenderer.class)
abstract class WorldBorderRendererMixin {
    @Inject(method = "extract", at = @At("HEAD"), cancellable = true)
    private void mcr$noExtract(CallbackInfo ci) {
        ci.cancel();
    }

    @Inject(method = "render", at = @At("HEAD"), cancellable = true)
    private void mcr$noRender(CallbackInfo ci) {
        ci.cancel();
    }

    @Inject(method = "renderOit", at = @At("HEAD"), cancellable = true)
    private void mcr$noRenderOit(CallbackInfo ci) {
        ci.cancel();
    }
}
