package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.platform.SDLEventHandler;
import dev.mcr.bridge.McrRawInput;
import org.lwjgl.sdl.SDL_Event;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Applied last so that its HEAD callbacks run after every mod's at the same point. */
@Mixin(value = SDLEventHandler.class, priority = 100000)
abstract class SDLEventProbeMixin {
    @Inject(method = "<init>", at = @At("TAIL"))
    private void mcr$capture(CallbackInfo ci) {
        McrRawInput.capture((SDLEventHandler) (Object) this);
    }

    @Inject(method = "handleKeyEvent", at = @At("HEAD"), cancellable = true)
    private void mcr$key(SDL_Event event, CallbackInfo ci) {
        if (McrRawInput.reachedVanilla()) {
            ci.cancel();
        }
    }

    @Inject(method = "handleMouseButtonEvent", at = @At("HEAD"), cancellable = true)
    private void mcr$button(SDL_Event event, CallbackInfo ci) {
        if (McrRawInput.reachedVanilla()) {
            ci.cancel();
        }
    }
}
