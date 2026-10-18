package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrWorlds;
import java.nio.file.Path;
import java.util.Optional;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.worldselection.CreateWorldScreen;
import net.minecraft.core.LayeredRegistryAccess;
import net.minecraft.network.chat.Component;
import net.minecraft.server.RegistryLayer;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.level.storage.LevelDataAndDimensions;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Create New World hands its options to mcr, which creates the folder through its own server. */
@Mixin(CreateWorldScreen.class)
abstract class CreateWorldScreenMixin extends Screen {
    @Shadow
    private @Nullable Path tempDataPackDir;

    protected CreateWorldScreenMixin(Component title) {
        super(title);
    }

    @Inject(method = "createWorldAndCleanup", at = @At("HEAD"), cancellable = true)
    private void mcr$createInMcr(
            LayeredRegistryAccess<RegistryLayer> finalLayers,
            LevelDataAndDimensions.WorldDataAndGenSettings worldDataAndGenSettings,
            Optional<GameRules> gameRules,
            CallbackInfo ci) {
        if (!McrWorlds.enabled()) {
            return;
        }
        ci.cancel();
        CreateWorldScreen self = (CreateWorldScreen) (Object) this;
        String request = McrWorlds.refuseWhileBusy(minecraft)
                ? null
                : McrWorlds.createRequest(minecraft, self.getUiState(), tempDataPackDir);
        if (request == null) {
            if (minecraft.gui.screen() != self) {
                minecraft.gui.setScreen(self);
            }
            return;
        }
        Path packDir = tempDataPackDir;
        tempDataPackDir = null;
        McrWorlds.create(minecraft, request, packDir, McrWorlds.dropsPacks(self.getUiState()));
    }
}
