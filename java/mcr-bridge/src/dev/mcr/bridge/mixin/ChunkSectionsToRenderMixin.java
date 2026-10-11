package dev.mcr.bridge.mixin;

import net.minecraft.client.renderer.chunk.ChunkSectionsToRender;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(ChunkSectionsToRender.class)
abstract class ChunkSectionsToRenderMixin {
    @Inject(method = "renderGroup", at = @At("HEAD"), cancellable = true)
    private void mcr$rustDrawsTerrain(CallbackInfo ci) {
        ci.cancel();
    }

    @Inject(method = "renderOit", at = @At("HEAD"), cancellable = true)
    private void mcr$rustDrawsOitTerrain(CallbackInfo ci) {
        ci.cancel();
    }
}
