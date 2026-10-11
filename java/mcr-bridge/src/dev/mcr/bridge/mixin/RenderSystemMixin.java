package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.systems.RenderSystem;
import dev.mcr.bridge.Bridge;
import org.lwjgl.sdl.SDLHints;
import org.lwjgl.sdl.SDLVideo;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(RenderSystem.class)
abstract class RenderSystemMixin {
    @Inject(method = "initBackendSystem", at = @At("HEAD"))
    private static void mcr$offscreenVideo(CallbackInfoReturnable<?> cir) {
        SDLHints.SDL_SetHint("SDL_VIDEO_DRIVER", "offscreen");
    }

    @Inject(method = "initBackendSystem", at = @At("RETURN"))
    private static void mcr$reportVideo(CallbackInfoReturnable<?> cir) {
        Bridge.info("sdl_driver", SDLVideo.SDL_GetCurrentVideoDriver());
    }
}
