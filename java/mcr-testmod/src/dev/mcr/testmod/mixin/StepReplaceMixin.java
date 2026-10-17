package dev.mcr.testmod.mixin;

import dev.mcr.testmod.McrTestMod;
import net.minecraft.client.Minecraft;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** `-Dmcr.testmod.replaceSteps=true`: the player's vanilla footsteps are cancelled, like Presence Footsteps does. */
@Mixin(Entity.class)
abstract class StepReplaceMixin {
    @Inject(method = "walkingStepSound", at = @At("HEAD"), cancellable = true)
    private void mcr_testmod$ownSteps(CallbackInfo ci) {
        if (McrTestMod.REPLACE_STEPS && (Object) this == Minecraft.getInstance().player) {
            ci.cancel();
        }
    }
}
