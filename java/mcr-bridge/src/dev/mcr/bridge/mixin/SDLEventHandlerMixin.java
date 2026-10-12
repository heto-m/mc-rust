package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.platform.SDLEventHandler;
import dev.mcr.bridge.McrInput;
import org.lwjgl.sdl.SDLKeyboard;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/** The offscreen driver produces no input, but anything that still reads modifiers sees Rust's. */
@Mixin(SDLEventHandler.class)
abstract class SDLEventHandlerMixin {
    @Redirect(
            method = "handleMouseButtonEvent",
            at = @At(value = "INVOKE", target = "Lorg/lwjgl/sdl/SDLKeyboard;SDL_GetModState()S", remap = false))
    private short mcr$buttonMods() {
        return mods();
    }

    @Redirect(
            method = "isShiftInvertedScroll",
            at = @At(value = "INVOKE", target = "Lorg/lwjgl/sdl/SDLKeyboard;SDL_GetModState()S", remap = false))
    private static short mcr$scrollMods() {
        return mods();
    }

    private static short mods() {
        return McrInput.active() ? (short) McrInput.mods() : SDLKeyboard.SDL_GetModState();
    }
}
