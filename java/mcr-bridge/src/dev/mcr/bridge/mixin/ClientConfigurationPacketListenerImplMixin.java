package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrRegistries;
import net.minecraft.client.multiplayer.ClientConfigurationPacketListenerImpl;
import net.minecraft.network.protocol.configuration.ClientboundFinishConfigurationPacket;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Rust holds its end-of-configuration answer until it has Java's synced registries and bound item components. */
@Mixin(ClientConfigurationPacketListenerImpl.class)
abstract class ClientConfigurationPacketListenerImplMixin {
    @Inject(method = "handleConfigurationFinished", at = @At("TAIL"))
    private void mcr$reportRegistries(ClientboundFinishConfigurationPacket packet, CallbackInfo ci) {
        McrRegistries.report("configuration");
    }
}
