package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrTurn;
import net.minecraft.client.Minecraft;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Past every mod's {@code turn} HEAD hook: Rust turns the player, so vanilla's turn is noted and skipped. */
@Mixin(Entity.class)
abstract class EntityTurnProbeMixin {
    @Inject(
            method = "turn",
            at = @At(value = "INVOKE", target = "Lnet/minecraft/world/entity/Entity;setXRot(F)V", ordinal = 0),
            cancellable = true)
    private void mcr$rustTurnsThePlayer(double xo, double yo, CallbackInfo ci) {
        if ((Object) this == Minecraft.getInstance().player && McrTurn.vanillaReached()) {
            ci.cancel();
        }
    }
}
