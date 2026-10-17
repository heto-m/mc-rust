package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.platform.SDLEventHandler;
import org.lwjgl.sdl.SDL_Event;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

@Mixin(SDLEventHandler.class)
public interface SDLEventHandlerInvoker {
    @Invoker("handleKeyEvent")
    void mcr$handleKeyEvent(SDL_Event event);

    @Invoker("handleMouseButtonEvent")
    void mcr$handleMouseButtonEvent(SDL_Event event);
}
