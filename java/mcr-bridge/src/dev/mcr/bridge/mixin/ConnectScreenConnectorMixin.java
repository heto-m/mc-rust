package dev.mcr.bridge.mixin;

import dev.mcr.bridge.net.McrNet;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** ConnectScreen's connector thread: its bridge channel is activated on the render thread once it is done. */
@Mixin(targets = "net.minecraft.client.gui.screens.ConnectScreen$1")
abstract class ConnectScreenConnectorMixin {
    @Inject(method = "run", at = @At("HEAD"))
    private void mcr$started(CallbackInfo ci) {
        McrNet.connectorStarted();
    }

    @Inject(method = "run", at = @At("RETURN"))
    private void mcr$finished(CallbackInfo ci) {
        McrNet.connectorFinished();
    }
}
