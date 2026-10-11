package dev.mcr.bridge.mixin;

import dev.mcr.bridge.net.McrNet;
import dev.mcr.bridge.net.PlayerShadow;
import net.minecraft.client.player.ClientInput;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.entity.player.Input;
import net.minecraft.world.phys.Vec2;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Rust moves the player and sends its movement; Java's player only shadows Rust's. */
@Mixin(LocalPlayer.class)
abstract class LocalPlayerMixin {
    @Shadow
    public ClientInput input;

    @Inject(method = "tick", at = @At("TAIL"))
    private void mcr$shadowRust(CallbackInfo ci) {
        PlayerShadow.apply((LocalPlayer) (Object) this);
    }

    @Inject(
            method = "aiStep",
            at = @At(value = "INVOKE", target = "Lnet/minecraft/client/player/ClientInput;tick()V", shift = At.Shift.AFTER))
    private void mcr$noMovementInput(CallbackInfo ci) {
        if (McrNet.bridged()) {
            input.keyPresses = Input.EMPTY;
            ((ClientInputAccessor) input).mcr$setMoveVector(Vec2.ZERO);
        }
    }

    @Inject(method = "sendChanges", at = @At("HEAD"), cancellable = true)
    private void mcr$rustSendsMovement(CallbackInfo ci) {
        if (McrNet.bridged()) {
            ci.cancel();
        }
    }
}
