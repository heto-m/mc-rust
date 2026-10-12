package dev.mcr.bridge.mixin;

import dev.mcr.bridge.sound.BridgeAudio;
import net.minecraft.client.Options;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.client.sounds.SoundEngine;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(SoundEngine.class)
abstract class SoundEngineMixin {
    @Shadow
    @Final
    private Options options;

    @Inject(method = "tick", at = @At("HEAD"))
    private void mcr$shareVolumes(boolean paused, CallbackInfo ci) {
        BridgeAudio.shareVolumes(options);
    }

    @Inject(method = "play", at = @At("HEAD"))
    private void mcr$trace(SoundInstance instance, CallbackInfoReturnable<?> cir) {
        BridgeAudio.trace(instance);
    }
}
