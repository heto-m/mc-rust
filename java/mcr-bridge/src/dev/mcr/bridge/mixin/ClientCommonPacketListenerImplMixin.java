package dev.mcr.bridge.mixin;

import dev.mcr.bridge.BridgeAbi;
import dev.mcr.bridge.net.McrNet;
import net.minecraft.client.multiplayer.ClientCommonPacketListenerImpl;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ServerboundChatCommandPacket;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Unattended commands reach here only after vanilla's parse/signature/permission confirmation, so route them to Rust. */
@Mixin(ClientCommonPacketListenerImpl.class)
abstract class ClientCommonPacketListenerImplMixin {
    @Inject(method = "send", at = @At("HEAD"), cancellable = true)
    private void mcr$command(Packet<?> packet, CallbackInfo ci) {
        if (packet instanceof ServerboundChatCommandPacket command && McrNet.bridged()) {
            McrNet.sendChat(BridgeAbi.CHAT_COMMAND, command.command());
            ci.cancel();
        }
    }
}
