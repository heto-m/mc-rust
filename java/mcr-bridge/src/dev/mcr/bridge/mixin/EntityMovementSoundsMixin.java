package dev.mcr.bridge.mixin;

import dev.mcr.bridge.sound.SoundOwnership;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(Entity.class)
abstract class EntityMovementSoundsMixin {
    @Inject(
            method = {"waterSwimSound", "doWaterSplashEffect", "playAmethystStepSound"},
            at = @At("HEAD"),
            cancellable = true)
    private void mcr$rustPlaysLocalMovement(CallbackInfo ci) {
        if (SoundOwnership.rustOwns(this)) {
            ci.cancel();
        }
    }
}
