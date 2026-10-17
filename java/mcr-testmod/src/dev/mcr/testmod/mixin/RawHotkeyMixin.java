package dev.mcr.testmod.mixin;

import com.mojang.blaze3d.platform.SDLEventHandler;
import dev.mcr.testmod.McrTestMod;
import org.lwjgl.sdl.SDL_Event;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** `-Dmcr.testmod.rawHotkey=true`: E is consumed from the raw SDL events as a malilib hotkey would be. */
@Mixin(SDLEventHandler.class)
abstract class RawHotkeyMixin {
    private static final int SCANCODE_E = 8;

    @Inject(method = "handleKeyEvent", at = @At("HEAD"), cancellable = true)
    private void mcr_testmod$hotkey(SDL_Event event, CallbackInfo ci) {
        if (McrTestMod.RAW_HOTKEY && event.key().scancode() == SCANCODE_E) {
            ci.cancel();
        }
    }
}
