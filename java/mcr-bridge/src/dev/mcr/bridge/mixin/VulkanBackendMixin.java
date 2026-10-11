package dev.mcr.bridge.mixin;

import com.mojang.renderpearl.api.device.BackendCreationException;
import com.mojang.renderpearl.backend.vulkan.VulkanBackend;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(VulkanBackend.class)
abstract class VulkanBackendMixin {
    @Inject(method = "checkBackendAvailable", at = @At("HEAD"), cancellable = true)
    private static void mcr$noVulkanProbe(CallbackInfoReturnable<BackendCreationException> cir) {
        cir.setReturnValue(null);
    }
}
