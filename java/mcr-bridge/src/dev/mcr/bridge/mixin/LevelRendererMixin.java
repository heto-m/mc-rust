package dev.mcr.bridge.mixin;

import net.minecraft.client.renderer.LevelRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Rust owns terrain: Java never meshes or uploads chunk sections. */
@Mixin(LevelRenderer.class)
abstract class LevelRendererMixin {
    @Inject(method = "compileSections", at = @At("HEAD"), cancellable = true)
    private void mcr$rustOwnsTerrain(CallbackInfo ci) {
        ci.cancel();
    }
}
