package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrInput;
import net.minecraft.client.input.InputQuirks;
import org.lwjgl.sdl.SDLKeyboard;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

@Mixin(InputQuirks.class)
abstract class InputQuirksMixin {
    @Redirect(
            method = "isQuitShortcutDown",
            at = @At(value = "INVOKE", target = "Lorg/lwjgl/sdl/SDLKeyboard;SDL_GetModState()S", remap = false))
    private static short mcr$mods() {
        return McrInput.active() ? (short) McrInput.mods() : SDLKeyboard.SDL_GetModState();
    }
}
