package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrFrame;
import dev.mcr.bridge.McrParticles;
import net.minecraft.client.multiplayer.ClientLevel;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Block break particles are vanilla's; mcr spawns its own from the same level event. Entity, level and ambient
 * block ticks are paths mcr mirrors; Fabric's level tick events fire around {@code tickEntities}, outside them.
 */
@Mixin(ClientLevel.class)
abstract class ClientLevelMixin {
    @Inject(method = "addDestroyBlockEffect", at = @At("HEAD"), cancellable = true)
    private void mcr$rustDrawsBreakParticles(CallbackInfo ci) {
        if (McrFrame.COMPOSE) {
            ci.cancel();
        }
    }

    @Inject(method = {"tickNonPassenger", "tick", "animateTick"}, at = @At("HEAD"))
    private void mcr$mirroredStart(CallbackInfo ci) {
        McrParticles.enter();
    }

    @Inject(method = {"tickNonPassenger", "tick", "animateTick"}, at = @At("RETURN"))
    private void mcr$mirroredEnd(CallbackInfo ci) {
        McrParticles.exit();
    }
}
