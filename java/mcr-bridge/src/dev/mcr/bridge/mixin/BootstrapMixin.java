package dev.mcr.bridge.mixin;

import net.minecraft.server.Bootstrap;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Closing Java's stdout closes file descriptor 1 of the whole mcr process. */
@Mixin(Bootstrap.class)
abstract class BootstrapMixin {
    @Inject(method = "shutdownStdout", at = @At("HEAD"), cancellable = true)
    private static void mcr$keepStdout(CallbackInfo ci) {
        ci.cancel();
    }
}
