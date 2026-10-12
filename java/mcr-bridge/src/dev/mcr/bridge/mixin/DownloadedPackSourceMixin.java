package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrSync;
import net.minecraft.client.resources.server.DownloadedPackSource;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(DownloadedPackSource.class)
abstract class DownloadedPackSourceMixin {
    @Inject(method = "cleanupAfterDisconnect", at = @At("TAIL"))
    private void mcr$forgetPacks(CallbackInfo ci) {
        McrSync.packsCleared();
    }
}
