package dev.mcr.bridge.mixin;

import dev.mcr.bridge.BridgeAbi;
import dev.mcr.bridge.net.McrNet;
import dev.mcr.bridge.net.NetProbe;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.minecraft.network.protocol.game.ClientboundLevelChunkWithLightPacket;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Chat and commands go to Rust as text; Rust's chat session signs and orders them. */
@Mixin(ClientPacketListener.class)
abstract class ClientPacketListenerMixin {
    @Inject(method = "sendChat", at = @At("HEAD"), cancellable = true)
    private void mcr$chat(String content, CallbackInfo ci) {
        if (McrNet.bridged()) {
            McrNet.sendChat(BridgeAbi.CHAT_MESSAGE, content);
            ci.cancel();
        }
    }

    @Inject(method = "sendCommand", at = @At("HEAD"), cancellable = true)
    private void mcr$command(String command, CallbackInfo ci) {
        if (McrNet.bridged()) {
            McrNet.sendChat(BridgeAbi.CHAT_COMMAND, command);
            ci.cancel();
        }
    }

    @Inject(method = "sendUnattendedCommand", at = @At("HEAD"), cancellable = true)
    private void mcr$unattendedCommand(String command, @Nullable Screen screenAfterCommand, CallbackInfo ci) {
        if (McrNet.bridged()) {
            McrNet.sendChat(BridgeAbi.CHAT_COMMAND, command);
            Minecraft.getInstance().gui.setScreen(screenAfterCommand);
            ci.cancel();
        }
    }

    @Inject(method = "handleLevelChunkWithLight", at = @At("HEAD"))
    private void mcr$chunkStart(ClientboundLevelChunkWithLightPacket packet, CallbackInfo ci) {
        NetProbe.chunkPacketStarted();
    }

    @Inject(method = "handleLevelChunkWithLight", at = @At("RETURN"))
    private void mcr$chunkDone(ClientboundLevelChunkWithLightPacket packet, CallbackInfo ci) {
        NetProbe.chunkPacketDone();
    }
}
