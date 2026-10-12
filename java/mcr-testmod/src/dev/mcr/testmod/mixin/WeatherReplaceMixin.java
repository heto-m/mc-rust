package dev.mcr.testmod.mixin;

import dev.mcr.testmod.McrTestMod;
import net.minecraft.client.renderer.WeatherEffectRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** `-Dmcr.testmod.replaceWeather=true`: cancels vanilla's weather extraction like Particle Rain does. */
@Mixin(WeatherEffectRenderer.class)
abstract class WeatherReplaceMixin {
    @Inject(method = "extractRenderState", at = @At("HEAD"), cancellable = true)
    private void mcr_testmod$ownWeather(CallbackInfo ci) {
        if (McrTestMod.REPLACE_WEATHER) {
            ci.cancel();
        }
    }
}
