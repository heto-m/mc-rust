package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrCameraHandoff;
import dev.mcr.bridge.McrInput;
import dev.mcr.bridge.McrTurn;
import net.minecraft.client.MouseHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Rust turns the player unless a mod camera took its input or a mod consumes the turn (see {@link McrTurn});
 * the cursor only moves by Rust's events.
 */
@Mixin(MouseHandler.class)
abstract class MouseHandlerMixin {
    @Inject(
            method = "turnPlayer",
            at = @At(value = "INVOKE", target = "Lnet/minecraft/client/player/LocalPlayer;turn(DD)V"))
    private void mcr$turnStart(double mousea, CallbackInfo ci) {
        if (McrInput.active() && !McrCameraHandoff.inputCaptured()) {
            McrTurn.before();
        }
    }

    @Inject(
            method = "turnPlayer",
            at = @At(value = "INVOKE", target = "Lnet/minecraft/client/player/LocalPlayer;turn(DD)V", shift = At.Shift.AFTER))
    private void mcr$turnEnd(double mousea, CallbackInfo ci) {
        McrTurn.after();
    }

    @Inject(method = "resyncMousePosition", at = @At("HEAD"), cancellable = true)
    private void mcr$noResync(CallbackInfo ci) {
        if (McrInput.active()) {
            ci.cancel();
        }
    }
}
