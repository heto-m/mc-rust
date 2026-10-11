package dev.mcr.bridge.mixin;

import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import net.minecraft.client.renderer.PostPass;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(PostPass.class)
public interface PostPassAccessor {
    @Accessor("pipeline")
    RenderPipeline mcr$pipeline();
}
