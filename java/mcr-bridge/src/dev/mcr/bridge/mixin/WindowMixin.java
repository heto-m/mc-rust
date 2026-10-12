package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.platform.Window;
import com.mojang.blaze3d.platform.cursor.CursorType;
import dev.mcr.bridge.McrInput;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Focus and the cursor shape belong to mcr's window, not the offscreen SDL one. */
@Mixin(Window.class)
abstract class WindowMixin {
    @Inject(method = "isFocused", at = @At("HEAD"), cancellable = true)
    private void mcr$focused(CallbackInfoReturnable<Boolean> cir) {
        if (McrInput.active()) {
            cir.setReturnValue(McrInput.focused());
        }
    }

    @Redirect(
            method = "selectCursor",
            at = @At(value = "INVOKE", target = "Lcom/mojang/blaze3d/platform/cursor/CursorType;select()V"))
    private void mcr$cursor(CursorType cursor) {
        if (McrInput.active()) {
            McrInput.setCursor(cursor.toString());
        } else {
            cursor.select();
        }
    }
}
