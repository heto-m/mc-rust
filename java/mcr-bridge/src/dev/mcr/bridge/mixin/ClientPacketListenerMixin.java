package dev.mcr.bridge.mixin;

import dev.mcr.bridge.BridgeAbi;
import dev.mcr.bridge.Loaders;
import dev.mcr.bridge.McrCameraHandoff;
import dev.mcr.bridge.McrPreload;
import dev.mcr.bridge.net.McrNet;
import dev.mcr.bridge.net.NetProbe;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.minecraft.client.Minecraft;
import net.minecraft.network.protocol.game.ClientboundLevelChunkWithLightPacket;
import net.minecraft.network.protocol.game.ClientboundLoginPacket;
import net.minecraft.network.protocol.game.ClientboundSetCameraPacket;
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
            // This runs ahead of NeoForge's patch in sendCommand, so its client commands go first here.
            if (!Loaders.runClientCommand(command)) {
                McrNet.sendChat(BridgeAbi.CHAT_COMMAND, command);
            }
            ci.cancel();
        }
    }

    @Inject(method = "handleLogin", at = @At("TAIL"))
    private void mcr$preloadWorld(ClientboundLoginPacket packet, CallbackInfo ci) {
        McrPreload.milestone(McrPreload.LOGIN);
    }

    @Inject(method = "handleSetCamera", at = @At("TAIL"))
    private void mcr$serverCamera(ClientboundSetCameraPacket packet, CallbackInfo ci) {
        McrCameraHandoff.serverCamera(Minecraft.getInstance().getCameraEntity());
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
