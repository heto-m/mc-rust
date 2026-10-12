package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.pipeline.MainTarget;
import dev.mcr.bridge.gpu.FrameSlots;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(MainTarget.class)
abstract class MainTargetMixin {
    @Inject(method = "allocateAttachments", at = @At("HEAD"))
    private void mcr$virtualStart(int width, int height, CallbackInfoReturnable<?> cir) {
        FrameSlots.enterMainTarget();
    }

    @Inject(method = "allocateAttachments", at = @At("RETURN"))
    private void mcr$virtualEnd(int width, int height, CallbackInfoReturnable<?> cir) {
        FrameSlots.exitMainTarget();
    }
}
