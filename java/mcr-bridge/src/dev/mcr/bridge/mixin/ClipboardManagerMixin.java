package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.platform.ClipboardManager;
import dev.mcr.bridge.McrInput;
import net.minecraft.util.StringDecomposer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(ClipboardManager.class)
abstract class ClipboardManagerMixin {
    @Inject(method = "getClipboard", at = @At("HEAD"), cancellable = true)
    private void mcr$get(CallbackInfoReturnable<String> cir) {
        if (McrInput.active()) {
            cir.setReturnValue(StringDecomposer.filterBrokenSurrogates(McrInput.clipboard()));
        }
    }

    @Inject(method = "setClipboard", at = @At("HEAD"), cancellable = true)
    private void mcr$set(String text, CallbackInfo ci) {
        if (McrInput.active()) {
            McrInput.setClipboard(text);
            ci.cancel();
        }
    }
}
