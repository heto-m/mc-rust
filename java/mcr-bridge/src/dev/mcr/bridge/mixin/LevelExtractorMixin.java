package dev.mcr.bridge.mixin;

import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.chunk.RenderRegionCache;
import net.minecraft.client.renderer.chunk.RenderSectionRegion;
import net.minecraft.client.renderer.extract.LevelExtractor;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Rust draws the block outline and destroy overlay, and sections compile without a block copy. */
@Mixin(LevelExtractor.class)
abstract class LevelExtractorMixin {
    @Redirect(
            method = "extract",
            at = @At(
                    value = "INVOKE",
                    target = "Lnet/minecraft/client/renderer/chunk/RenderRegionCache;createRegion("
                            + "Lnet/minecraft/client/multiplayer/ClientLevel;J)"
                            + "Lnet/minecraft/client/renderer/chunk/RenderSectionRegion;"))
    private RenderSectionRegion mcr$noSectionCopy(RenderRegionCache cache, ClientLevel level, long sectionNode) {
        return null;
    }

    @Inject(method = "extractBlockOutline", at = @At("HEAD"), cancellable = true)
    private void mcr$rustDrawsTheOutline(CallbackInfo ci) {
        ci.cancel();
    }

    @Inject(method = "extractBlockDestroyAnimation", at = @At("HEAD"), cancellable = true)
    private void mcr$rustDrawsDestroyStages(CallbackInfo ci) {
        ci.cancel();
    }
}
