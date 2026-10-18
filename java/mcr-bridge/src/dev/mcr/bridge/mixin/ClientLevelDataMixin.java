package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrWorlds;
import net.minecraft.client.multiplayer.ClientLevel;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** mcr's lock outlives the server's own difficulty packets, which report it unlocked until the next start. */
@Mixin(ClientLevel.ClientLevelData.class)
abstract class ClientLevelDataMixin {
    @Inject(method = "isDifficultyLocked", at = @At("HEAD"), cancellable = true)
    private void mcr$locked(CallbackInfoReturnable<Boolean> cir) {
        if (McrWorlds.difficultyLocked()) {
            cir.setReturnValue(true);
        }
    }
}
