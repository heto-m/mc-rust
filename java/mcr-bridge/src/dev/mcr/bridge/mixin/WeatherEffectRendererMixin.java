package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrWeather;
import net.minecraft.client.renderer.WeatherEffectRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Rust draws weather; vanilla's extraction only proves that no mod replaced it. */
@Mixin(WeatherEffectRenderer.class)
abstract class WeatherEffectRendererMixin {
    @Inject(
            method = "extractRenderState",
            at = @At(
                    value = "FIELD",
                    target = "Lnet/minecraft/client/renderer/state/level/WeatherRenderState;intensity:F",
                    opcode = org.objectweb.asm.Opcodes.PUTFIELD,
                    shift = At.Shift.AFTER),
            cancellable = true)
    private void mcr$noExtract(CallbackInfo ci) {
        McrWeather.vanillaRan();
        ci.cancel();
    }

    @Inject(method = "prepare", at = @At("HEAD"), cancellable = true)
    private void mcr$noPrepare(CallbackInfo ci) {
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
