package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.audio.Channel;
import com.mojang.blaze3d.audio.SoundBuffer;
import dev.mcr.bridge.sound.BridgeAudio;
import dev.mcr.bridge.sound.BridgeBuffer;
import dev.mcr.bridge.sound.SoundAbi;
import java.util.concurrent.atomic.AtomicBoolean;
import net.minecraft.client.sounds.AudioStream;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Every OpenAL source operation becomes a command for mcr-audio; {@code source} holds a bridge slot. */
@Mixin(Channel.class)
abstract class ChannelMixin {
    @Shadow
    @Final
    private int source;

    @Shadow
    @Final
    private AtomicBoolean initialized;

    @Inject(method = "create", at = @At("HEAD"), cancellable = true)
    private static void mcr$create(CallbackInfoReturnable<Channel> cir) {
        int id = BridgeAudio.create();
        cir.setReturnValue(id == 0 ? null : ChannelInvoker.mcr$new(id));
    }

    @Inject(method = "destroy", at = @At("HEAD"), cancellable = true)
    private void mcr$destroy(CallbackInfo ci) {
        if (initialized.compareAndSet(true, false)) {
            BridgeAudio.destroy(source);
        }
        ci.cancel();
    }

    @Inject(method = "play", at = @At("HEAD"), cancellable = true)
    private void mcr$play(CallbackInfo ci) {
        if (initialized.get()) {
            BridgeAudio.play(source);
        }
        ci.cancel();
    }

    @Inject(method = "pause", at = @At("HEAD"), cancellable = true)
    private void mcr$pause(CallbackInfo ci) {
        BridgeAudio.pause(source);
        ci.cancel();
    }

    @Inject(method = "unpause", at = @At("HEAD"), cancellable = true)
    private void mcr$unpause(CallbackInfo ci) {
        BridgeAudio.unpause(source);
        ci.cancel();
    }

    @Inject(method = "stop", at = @At("HEAD"), cancellable = true)
    private void mcr$stop(CallbackInfo ci) {
        if (initialized.get()) {
            BridgeAudio.stop(source);
        }
        ci.cancel();
    }

    @Inject(method = "playing", at = @At("HEAD"), cancellable = true)
    private void mcr$playing(CallbackInfoReturnable<Boolean> cir) {
        cir.setReturnValue(initialized.get() && BridgeAudio.hasState(source, SoundAbi.STATE_PLAYING));
    }

    @Inject(method = "stopped", at = @At("HEAD"), cancellable = true)
    private void mcr$stopped(CallbackInfoReturnable<Boolean> cir) {
        cir.setReturnValue(!initialized.get() || BridgeAudio.hasState(source, SoundAbi.STATE_STOPPED));
    }

    @Inject(method = "setSelfPosition", at = @At("HEAD"), cancellable = true)
    private void mcr$position(Vec3 position, CallbackInfo ci) {
        BridgeAudio.position(source, position);
        ci.cancel();
    }

    @Inject(method = "setPitch", at = @At("HEAD"), cancellable = true)
    private void mcr$pitch(float pitch, CallbackInfo ci) {
        BridgeAudio.value(source, SoundAbi.SND_PITCH, 0, pitch, 0, 0);
        ci.cancel();
    }

    @Inject(method = "setLooping", at = @At("HEAD"), cancellable = true)
    private void mcr$looping(boolean looping, CallbackInfo ci) {
        BridgeAudio.value(source, SoundAbi.SND_LOOPING, looping ? 1 : 0, 0, 0, 0);
        ci.cancel();
    }

    @Inject(method = "setVolume", at = @At("HEAD"), cancellable = true)
    private void mcr$volume(float volume, CallbackInfo ci) {
        BridgeAudio.value(source, SoundAbi.SND_VOLUME, 0, volume, 0, 0);
        ci.cancel();
    }

    @Inject(method = "disableAttenuation", at = @At("HEAD"), cancellable = true)
    private void mcr$noAttenuation(CallbackInfo ci) {
        BridgeAudio.value(source, SoundAbi.SND_ATTENUATION, 0, 0, 0, 0);
        ci.cancel();
    }

    @Inject(method = "linearAttenuation", at = @At("HEAD"), cancellable = true)
    private void mcr$linearAttenuation(float maxDistance, CallbackInfo ci) {
        BridgeAudio.value(source, SoundAbi.SND_ATTENUATION, 0, maxDistance, 0, 0);
        ci.cancel();
    }

    @Inject(method = "setRelative", at = @At("HEAD"), cancellable = true)
    private void mcr$relative(boolean relative, CallbackInfo ci) {
        BridgeAudio.value(source, SoundAbi.SND_RELATIVE, relative ? 1 : 0, 0, 0, 0);
        ci.cancel();
    }

    @Inject(method = "attachStaticBuffer", at = @At("HEAD"), cancellable = true)
    private void mcr$attachStatic(SoundBuffer buffer, CallbackInfo ci) {
        int id = ((BridgeBuffer) buffer).mcr$bridgeId();
        if (id != 0) {
            BridgeAudio.attachBuffer(source, id);
        }
        ci.cancel();
    }

    @Inject(method = "attachBufferStream", at = @At("HEAD"), cancellable = true)
    private void mcr$attachStream(AudioStream stream, CallbackInfo ci) {
        BridgeAudio.attachStream(source, stream);
        ci.cancel();
    }

    @Inject(method = "updateStream", at = @At("HEAD"), cancellable = true)
    private void mcr$updateStream(CallbackInfo ci) {
        BridgeAudio.updateStream(source);
        ci.cancel();
    }
}
