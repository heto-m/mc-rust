package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrFrame;
import net.minecraft.client.renderer.GameRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** While mcr composes, Rust draws the world; Java only draws its GUI over it. */
@Mixin(GameRenderer.class)
abstract class GameRendererMixin {
    @Inject(method = "renderLevel", at = @At("HEAD"), cancellable = true)
    private void mcr$rustDrawsTheWorld(CallbackInfo ci) {
        if (McrFrame.COMPOSE) {
            ci.cancel();
        }
    }

    @Inject(method = "applyPostEffects", at = @At("HEAD"), cancellable = true)
    private void mcr$noWorldPostEffects(CallbackInfo ci) {
        if (McrFrame.COMPOSE) {
            ci.cancel();
        }
    }
}
