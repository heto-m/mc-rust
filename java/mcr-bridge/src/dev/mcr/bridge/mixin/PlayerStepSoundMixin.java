package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrSteps;
import dev.mcr.bridge.sound.SoundOwnership;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(Player.class)
abstract class PlayerStepSoundMixin {
    @Inject(method = "playStepSound", at = @At("HEAD"), cancellable = true)
    private void mcr$rustPlaysLocalSteps(BlockPos onPos, BlockState onState, CallbackInfo ci) {
        if (McrSteps.probeReached() || SoundOwnership.rustOwns(this)) {
            ci.cancel();
        }
    }
}
