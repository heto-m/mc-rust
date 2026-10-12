package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrFrame;
import net.minecraft.client.multiplayer.ClientLevel;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Block break particles are vanilla's; mcr spawns its own from the same level event. */
@Mixin(ClientLevel.class)
abstract class ClientLevelMixin {
    @Inject(method = "addDestroyBlockEffect", at = @At("HEAD"), cancellable = true)
    private void mcr$rustDrawsBreakParticles(CallbackInfo ci) {
        if (McrFrame.COMPOSE) {
            ci.cancel();
        }
    }
}
