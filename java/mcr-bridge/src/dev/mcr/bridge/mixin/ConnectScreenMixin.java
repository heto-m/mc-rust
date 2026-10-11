package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrNet;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.ConnectScreen;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.TransferState;
import net.minecraft.client.multiplayer.resolver.ServerAddress;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(ConnectScreen.class)
abstract class ConnectScreenMixin {
    @Inject(method = "connect", at = @At("HEAD"), cancellable = true)
    private void mcr$rustConnects(
            Minecraft minecraft, ServerAddress address, ServerData server, @Nullable TransferState transfer, CallbackInfo ci) {
        McrNet.requestConnect(address, server);
        ci.cancel();
    }
}
