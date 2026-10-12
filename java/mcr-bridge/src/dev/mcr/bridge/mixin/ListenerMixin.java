package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.audio.Listener;
import com.mojang.blaze3d.audio.ListenerTransform;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** mcr's camera is the listener; Java keeps the transform only for subtitles. */
@Mixin(Listener.class)
abstract class ListenerMixin {
    @Shadow
    private ListenerTransform transform;

    @Inject(method = "setTransform", at = @At("HEAD"), cancellable = true)
    private void mcr$rustOwnsListener(ListenerTransform transform, CallbackInfo ci) {
        this.transform = transform;
        ci.cancel();
    }
}
