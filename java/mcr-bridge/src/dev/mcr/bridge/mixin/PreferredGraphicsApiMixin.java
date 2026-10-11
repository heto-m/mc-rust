package dev.mcr.bridge.mixin;

import com.mojang.renderpearl.api.device.GpuBackend;
import dev.mcr.bridge.gpu.McrGpuBackend;
import net.minecraft.client.PreferredGraphicsApi;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(PreferredGraphicsApi.class)
abstract class PreferredGraphicsApiMixin {
    @Inject(method = "getBackendsToTry", at = @At("HEAD"), cancellable = true)
    private void mcr$onlyMcrBackend(CallbackInfoReturnable<GpuBackend[]> cir) {
        cir.setReturnValue(new GpuBackend[] {new McrGpuBackend()});
    }
}
