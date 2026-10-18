package dev.mcr.bridge.mixin;

import dev.mcr.bridge.BridgeAbi;
import dev.mcr.bridge.BridgeInit;
import dev.mcr.bridge.McrCrash;
import dev.mcr.bridge.McrFrame;
import dev.mcr.bridge.McrFrameTimes;
import dev.mcr.bridge.McrInput;
import dev.mcr.bridge.McrPreload;
import dev.mcr.bridge.gpu.GpuSelfTest;
import dev.mcr.bridge.net.GameplayKeys;
import dev.mcr.bridge.net.McrNet;
import java.io.File;
import net.minecraft.CrashReport;
import net.minecraft.client.Minecraft;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyArg;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(Minecraft.class)
abstract class MinecraftMixin {
    /** Rust signs chat with the keys; Java only needs Mojang's key set to verify other players' chat. */
    @ModifyArg(
        method = "<init>",
        at = @At(
            value = "INVOKE",
            target = "Lcom/mojang/authlib/services/MinecraftServicesDiscoveryService;create(Ljava/net/Proxy;Z)Lcom/mojang/authlib/services/MinecraftServicesDiscoveryService;"
        ),
        index = 1
    )
    private static boolean mcr$servicesKeys(boolean enabled) {
        return enabled || Boolean.getBoolean("mcr.bridge.servicesKeys");
    }

    @Inject(method = "runTick", at = @At("HEAD"))
    private void mcr$awaitFrame(boolean advanceGameTime, CallbackInfo ci) {
        McrPreload.milestone(McrPreload.GAME_LOOP);
        McrFrame.awaitFrame((Minecraft) (Object) this);
    }

    @Inject(method = "runTick", at = @At("TAIL"))
    private void mcr$flushNet(boolean advanceGameTime, CallbackInfo ci) {
        McrNet.endOfTick();
        GpuSelfTest.onFrame((Minecraft) (Object) this);
        McrFrame.frameDone((Minecraft) (Object) this);
    }

    @Inject(method = "tick", at = @At("HEAD"))
    private void mcr$countTick(CallbackInfo ci) {
        McrFrame.tickRan();
        McrFrameTimes.begin(BridgeAbi.PROF_TICK);
        BridgeInit.onTick((Minecraft) (Object) this);
    }

    @Inject(method = "tick", at = @At("RETURN"))
    private void mcr$tickTime(CallbackInfo ci) {
        McrFrameTimes.end(BridgeAbi.PROF_TICK);
    }

    @Inject(method = "renderFrame", at = @At("HEAD"))
    private void mcr$frameStart(boolean advanceGameTime, CallbackInfo ci) {
        McrFrameTimes.begin(BridgeAbi.PROF_FRAME);
    }

    @Inject(method = "renderFrame", at = @At("RETURN"))
    private void mcr$frameEnd(boolean advanceGameTime, CallbackInfo ci) {
        McrFrameTimes.end(BridgeAbi.PROF_FRAME);
    }

    @Inject(method = "handleKeybinds", at = @At("HEAD"))
    private void mcr$rustOwnsGameplayKeys(CallbackInfo ci) {
        if (McrNet.bridged()) {
            GameplayKeys.suppress(((Minecraft) (Object) this).options);
        }
    }

    @Inject(method = "stop", at = @At("TAIL"))
    private void mcr$quit(CallbackInfo ci) {
        McrPreload.cancel();
        if (McrInput.active() && !McrFrame.stopping()) {
            McrInput.quit();
        }
    }

    @Inject(method = "toggleFullscreen", at = @At("HEAD"), cancellable = true)
    private void mcr$rustOwnsFullscreen(CallbackInfo ci) {
        if (McrInput.active()) {
            ci.cancel();
        }
    }

    @Inject(method = "crash", at = @At("HEAD"))
    private static void mcr$crash(
            @Nullable Minecraft minecraft, File gameDirectory, CrashReport crash, int exitCode, CallbackInfo ci) {
        McrCrash.saveReportAndPark(minecraft, gameDirectory, crash, exitCode);
    }

    @Inject(method = "emergencySaveAndCrash", at = @At("HEAD"))
    private void mcr$emergencySaveAndCrash(CrashReport partialReport, CallbackInfo ci) {
        Minecraft self = (Minecraft) (Object) this;
        McrCrash.saveReportAndPark(self, self.gameDirectory, self.fillReport(partialReport), -1);
    }
}
