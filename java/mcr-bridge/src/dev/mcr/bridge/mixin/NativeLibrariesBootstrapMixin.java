package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.platform.NativeLibrariesBootstrap;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** mcr-audio plays Java's sounds, so libopenal is never loaded. */
@Mixin(NativeLibrariesBootstrap.class)
abstract class NativeLibrariesBootstrapMixin {
    @Inject(method = "loadOpenAL", at = @At("HEAD"), cancellable = true)
    private static void mcr$noOpenAl(CallbackInfo ci) {
        ci.cancel();
    }
}
