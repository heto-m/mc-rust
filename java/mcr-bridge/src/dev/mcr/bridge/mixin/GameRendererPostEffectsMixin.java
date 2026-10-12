package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.blaze3d.resource.GraphicsResourceAllocator;
import dev.mcr.bridge.McrFrame;
import java.util.Set;
import net.minecraft.client.renderer.GameRenderer;
import net.minecraft.client.renderer.PostChain;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/** mcr runs vanilla's own post chains between Java's world and GUI; Java keeps the chains mods add. */
@Mixin(GameRenderer.class)
abstract class GameRendererPostEffectsMixin {
    private static final Set<Identifier> MCR_CHAINS = Set.of(
            Identifier.withDefaultNamespace("blur"),
            Identifier.withDefaultNamespace("creeper"),
            Identifier.withDefaultNamespace("invert"),
            Identifier.withDefaultNamespace("spider"));

    @Redirect(
            method = "applyPostEffects",
            at = @At(
                    value = "INVOKE",
                    target = "Lnet/minecraft/client/renderer/PostChain;process("
                            + "Lcom/mojang/blaze3d/pipeline/RenderTarget;"
                            + "Lcom/mojang/blaze3d/resource/GraphicsResourceAllocator;)V"))
    @SuppressWarnings("deprecation")
    private void mcr$skipChainsMcrRuns(PostChain chain, RenderTarget main, GraphicsResourceAllocator allocator) {
        if (McrFrame.COMPOSE && MCR_CHAINS.contains(chain.id())) {
            return;
        }
        chain.process(main, allocator);
    }
}
