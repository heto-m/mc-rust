package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.platform.MessageBox;
import dev.mcr.bridge.Bridge;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** SDL message boxes can fall back to zenity or kdialog and block the render thread on a dialog. */
@Mixin(MessageBox.class)
abstract class MessageBoxMixin {
    @Inject(method = "error", at = @At("HEAD"), cancellable = true)
    private static void mcr$error(String message, CallbackInfo ci) {
        Bridge.info("message_box", message);
        ci.cancel();
    }

    @Inject(method = "errorWithContinue", at = @At("HEAD"), cancellable = true)
    private static void mcr$errorWithContinue(String message, CallbackInfoReturnable<Boolean> cir) {
        Bridge.info("message_box", message);
        cir.setReturnValue(true);
    }
}
