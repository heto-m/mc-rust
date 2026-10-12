package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrParticles;
import net.minecraft.network.protocol.common.ClientboundCustomPayloadPacket;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Mod payload handlers are mod code: mcr does not know their particles. */
@Mixin(ClientboundCustomPayloadPacket.class)
abstract class CustomPayloadPacketMixin {
    @Inject(method = "handle(Lnet/minecraft/network/protocol/common/ClientCommonPacketListener;)V", at = @At("HEAD"))
    private void mcr$modStart(CallbackInfo ci) {
        McrParticles.enterMod();
    }

    @Inject(method = "handle(Lnet/minecraft/network/protocol/common/ClientCommonPacketListener;)V", at = @At("RETURN"))
    private void mcr$modEnd(CallbackInfo ci) {
        McrParticles.exitMod();
    }
}
