package dev.mcr.bridge.mixin;

import net.minecraft.client.FramerateLimiter;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(FramerateLimiter.class)
abstract class FramerateLimiterMixin {
    @Inject(method = "limitDisplayFPS", at = @At("HEAD"), cancellable = true)
    private static void mcr$rustPaces(int framerateLimit, CallbackInfo ci) {
        ci.cancel();
    }
}
