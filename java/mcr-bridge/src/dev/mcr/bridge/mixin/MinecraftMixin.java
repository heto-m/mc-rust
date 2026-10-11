package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrCrash;
import dev.mcr.bridge.McrFrame;
import java.io.File;
import net.minecraft.CrashReport;
import net.minecraft.client.Minecraft;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(Minecraft.class)
abstract class MinecraftMixin {
    @Inject(method = "runTick", at = @At("HEAD"))
    private void mcr$awaitFrame(boolean advanceGameTime, CallbackInfo ci) {
        McrFrame.awaitFrame((Minecraft) (Object) this);
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
