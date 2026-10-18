package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrWorlds;
import java.util.function.Function;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.worldselection.WorldOpenFlows;
import net.minecraft.core.HolderLookup;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.levelgen.WorldDimensions;
import net.minecraft.world.level.levelgen.WorldOptions;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Playing a world starts mcr's managed server instead of Java's integrated one. */
@Mixin(WorldOpenFlows.class)
abstract class WorldOpenFlowsMixin {
    @Shadow
    @Final
    private Minecraft minecraft;

    @Inject(method = "openWorld", at = @At("HEAD"), cancellable = true)
    private void mcr$playInMcr(String levelId, Runnable onCancel, CallbackInfo ci) {
        if (McrWorlds.enabled()) {
            ci.cancel();
            McrWorlds.play(minecraft, levelId);
        }
    }

    @Inject(method = "createFreshLevel", at = @At("HEAD"), cancellable = true)
    private void mcr$noFreshLevel(
            String levelId,
            LevelSettings levelSettings,
            WorldOptions options,
            Function<HolderLookup.Provider, WorldDimensions> dimensionsProvider,
            Screen parentScreen,
            CallbackInfo ci) {
        if (McrWorlds.enabled()) {
            ci.cancel();
            McrWorlds.refuseFreshLevel(minecraft, parentScreen);
        }
    }
}
