package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrFrame;
import net.minecraft.client.Camera;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.renderer.extract.LevelExtractor;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(LevelExtractor.class)
abstract class LevelExtractorMixin {
    @Inject(method = "extract", at = @At("HEAD"), cancellable = true)
    private void mcr$noWorldExtract(DeltaTracker deltaTracker, Camera camera, float worldPartialTicks, CallbackInfo ci) {
        if (McrFrame.COMPOSE) {
            ci.cancel();
        }
    }
}
