package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.platform.TextInputManager;
import dev.mcr.bridge.McrInput;
import org.lwjgl.sdl.SDLKeyboard;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Text input and its IME area drive mcr's window instead of SDL's. */
@Mixin(TextInputManager.class)
abstract class TextInputManagerMixin {
    @Shadow
    private int areaX;

    @Shadow
    private int areaY;

    @Shadow
    private int areaWidth;

    @Shadow
    private int areaHeight;

    @Redirect(
            method = "startTextInput",
            at = @At(value = "INVOKE", target = "Lorg/lwjgl/sdl/SDLKeyboard;SDL_StartTextInput(J)Z", remap = false))
    private boolean mcr$start(long window) {
        if (!McrInput.active()) {
            return SDLKeyboard.SDL_StartTextInput(window);
        }
        McrInput.setTextInput(true);
        return true;
    }

    @Redirect(
            method = "startTextInput",
            at = @At(value = "INVOKE", target = "Lorg/lwjgl/sdl/SDLKeyboard;SDL_ClearComposition(J)Z", remap = false))
    private boolean mcr$clear(long window) {
        return McrInput.active() || SDLKeyboard.SDL_ClearComposition(window);
    }

    @Redirect(
            method = "stopTextInput()V",
            at = @At(value = "INVOKE", target = "Lorg/lwjgl/sdl/SDLKeyboard;SDL_StopTextInput(J)Z", remap = false))
    private boolean mcr$stop(long window) {
        if (!McrInput.active()) {
            return SDLKeyboard.SDL_StopTextInput(window);
        }
        McrInput.setTextInput(false);
        return true;
    }

    @Inject(method = "applyTextInputArea", at = @At("HEAD"), cancellable = true)
    private void mcr$area(CallbackInfo ci) {
        if (McrInput.active()) {
            McrInput.setTextArea(areaX, areaY, areaWidth, areaHeight);
            ci.cancel();
        }
    }
}
