package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrInput;
import net.minecraft.client.MouseHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Rust turns the player; the cursor position only ever comes from Rust's motion events. */
@Mixin(MouseHandler.class)
abstract class MouseHandlerMixin {
    @Inject(method = "turnPlayer", at = @At("HEAD"), cancellable = true)
    private void mcr$rustTurns(double mousea, CallbackInfo ci) {
        if (McrInput.active()) {
            ci.cancel();
        }
    }

    @Inject(method = "resyncMousePosition", at = @At("HEAD"), cancellable = true)
    private void mcr$noResync(CallbackInfo ci) {
        if (McrInput.active()) {
            ci.cancel();
        }
    }
}
