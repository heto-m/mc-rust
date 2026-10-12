package dev.mcr.bridge.mixin;

import dev.mcr.bridge.sound.SoundOwnership;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.LivingEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** The server deals fall damage; on the client the call only plays the landing sounds mcr already plays. */
@Mixin(LivingEntity.class)
abstract class LivingEntityFallSoundMixin {
    @Inject(method = "causeFallDamage", at = @At("HEAD"), cancellable = true)
    private void mcr$rustPlaysLocalLanding(
            double fallDistance, float damageModifier, DamageSource damageSource, CallbackInfoReturnable<Boolean> cir) {
        if (SoundOwnership.rustOwns(this)) {
            cir.setReturnValue(false);
        }
    }
}
