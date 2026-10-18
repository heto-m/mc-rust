package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrWorlds;
import java.util.Optional;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.network.chat.Component;
import net.minecraft.server.WorldStem;
import net.minecraft.server.packs.repository.PackRepository;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.level.storage.LevelStorageSource;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyArg;
import org.objectweb.asm.Opcodes;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A world on mcr's local server behaves like vanilla singleplayer: pausing, Save and Quit, no integrated server. */
@Mixin(Minecraft.class)
abstract class MinecraftWorldsMixin {
    @Shadow
    @Final
    public Gui gui;

    @Shadow
    public @Nullable ClientLevel level;

    @Shadow
    private volatile boolean pause;

    @Unique
    private boolean mcr$leavingLocal;

    @Inject(method = "doWorldLoad", at = @At("HEAD"), cancellable = true)
    private void mcr$noIntegratedServer(
            LevelStorageSource.LevelStorageAccess access,
            PackRepository packRepository,
            WorldStem worldStem,
            Optional<GameRules> gameRules,
            boolean newWorld,
            CallbackInfo ci) {
        if (!McrWorlds.enabled()) {
            return;
        }
        ci.cancel();
        access.safeClose();
        worldStem.close();
        McrWorlds.toast((Minecraft) (Object) this, "Worlds open through mcr's world list");
        gui.setScreen(new TitleScreen());
    }

    @Redirect(method = "runTick", at = @At(value = "FIELD", target = "Lnet/minecraft/client/Minecraft;pause:Z", opcode = Opcodes.PUTFIELD))
    private void mcr$localPause(Minecraft self, boolean paused) {
        pause = paused || (McrWorlds.local() && !McrWorlds.published() && level != null && gui.isPausing());
    }

    @ModifyArg(method = "pauseGame", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/Gui;setPauseScreen(ZZ)V"), index = 1)
    private boolean mcr$canPause(boolean canPause) {
        return canPause || (McrWorlds.local() && !McrWorlds.published());
    }

    @Inject(method = "disconnectFromWorld", at = @At("HEAD"))
    private void mcr$leaving(Component message, CallbackInfo ci) {
        mcr$leavingLocal = McrWorlds.local();
    }

    @Redirect(method = "disconnectFromWorld", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Minecraft;isLocalServer()Z"))
    private boolean mcr$savingPath(Minecraft self) {
        return self.isLocalServer() || mcr$leavingLocal;
    }

    @Inject(method = "disconnectFromWorld", at = @At("TAIL"))
    private void mcr$waitForSave(Component message, CallbackInfo ci) {
        if (mcr$leavingLocal) {
            mcr$leavingLocal = false;
            McrWorlds.leftWorld((Minecraft) (Object) this);
        }
    }
}
