package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.platform.ClientShutdownWatchdog;
import net.minecraft.client.Minecraft;
import net.minecraft.client.main.GameConfig;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(ClientShutdownWatchdog.class)
abstract class ClientShutdownWatchdogMixin {
    @Inject(method = "startShutdownWatchdog", at = @At("HEAD"), cancellable = true)
    private static void mcr$noWatchdog(
            String callsite,
            boolean forceShutdown,
            @Nullable Minecraft minecraft,
            GameConfig gameConfig,
            long mainThreadId,
            CallbackInfo ci) {
        ci.cancel();
    }
}
