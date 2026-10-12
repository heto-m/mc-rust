package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.platform.Window;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(Window.class)
public interface WindowAccessor {
    @Accessor("width")
    void mcr$setScreenWidth(int width);

    @Accessor("height")
    void mcr$setScreenHeight(int height);
}
