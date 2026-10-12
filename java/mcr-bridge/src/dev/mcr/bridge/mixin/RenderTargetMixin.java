package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.pipeline.MainTarget;
import com.mojang.blaze3d.pipeline.RenderTarget;
import dev.mcr.bridge.gpu.FrameSlots;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** `MainTarget.resize` reallocates through the base class. */
@Mixin(RenderTarget.class)
abstract class RenderTargetMixin {
    @Inject(method = "createBuffers", at = @At("HEAD"))
    private void mcr$virtualStart(int width, int height, CallbackInfo ci) {
        if ((Object) this instanceof MainTarget) {
            FrameSlots.enterMainTarget();
        }
    }

    @Inject(method = "createBuffers", at = @At("RETURN"))
    private void mcr$virtualEnd(int width, int height, CallbackInfo ci) {
        if ((Object) this instanceof MainTarget) {
            FrameSlots.exitMainTarget();
        }
    }
}
