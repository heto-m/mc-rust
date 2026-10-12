package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.audio.SoundBuffer;
import dev.mcr.bridge.sound.BridgeAudio;
import dev.mcr.bridge.sound.BridgeBuffer;
import java.lang.ref.Cleaner;
import java.nio.ByteBuffer;
import java.util.OptionalInt;
import javax.sound.sampled.AudioFormat;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** The "AL buffer" is a Rust PCM id: uploaded once, freed on discard or when the buffer is collected. */
@Mixin(SoundBuffer.class)
abstract class SoundBufferMixin implements BridgeBuffer {
    @Shadow
    private @Nullable ByteBuffer data;

    @Shadow
    @Final
    private AudioFormat format;

    @Shadow
    private boolean hasAlBuffer;

    @Shadow
    private int alBuffer;

    @Unique
    private Cleaner.@Nullable Cleanable mcr$cleanable;

    @Shadow
    abstract OptionalInt getAlBuffer();

    @Inject(method = "getAlBuffer", at = @At("HEAD"), cancellable = true)
    private void mcr$upload(CallbackInfoReturnable<OptionalInt> cir) {
        if (!hasAlBuffer) {
            int id = data == null ? 0 : BridgeAudio.upload(data, format);
            if (id == 0) {
                cir.setReturnValue(OptionalInt.empty());
                return;
            }
            mcr$cleanable = BridgeAudio.track(this, id);
            alBuffer = id;
            hasAlBuffer = true;
            data = null;
        }
        cir.setReturnValue(OptionalInt.of(alBuffer));
    }

    @Inject(method = "discardAlBuffer", at = @At("HEAD"), cancellable = true)
    private void mcr$free(CallbackInfo ci) {
        if (hasAlBuffer && mcr$cleanable != null) {
            mcr$cleanable.clean();
            mcr$cleanable = null;
        }
        hasAlBuffer = false;
        ci.cancel();
    }

    @Override
    public int mcr$bridgeId() {
        return getAlBuffer().orElse(0);
    }
}
