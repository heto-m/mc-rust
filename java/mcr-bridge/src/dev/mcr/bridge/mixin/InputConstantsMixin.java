package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.platform.InputConstants;
import com.mojang.blaze3d.platform.Window;
import dev.mcr.bridge.McrInput;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Key state comes from Rust's bitset; grabbing is a wish Rust applies to its own window. */
@Mixin(InputConstants.class)
abstract class InputConstantsMixin {
    @Inject(method = "isKeyDown", at = @At("HEAD"), cancellable = true)
    private static void mcr$keyState(int key, CallbackInfoReturnable<Boolean> cir) {
        if (McrInput.active()) {
            cir.setReturnValue(McrInput.isKeyDown(key));
        }
    }

    @Inject(method = "grabMouse", at = @At("HEAD"), cancellable = true)
    private static void mcr$grab(Window window, double x, double y, CallbackInfo ci) {
        if (McrInput.active()) {
            ci.cancel();
        }
    }

    @Inject(method = "releaseMouse", at = @At("HEAD"), cancellable = true)
    private static void mcr$release(Window window, double x, double y, CallbackInfo ci) {
        if (McrInput.active()) {
            ci.cancel();
        }
    }
}
