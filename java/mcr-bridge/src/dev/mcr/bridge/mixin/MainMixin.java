package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrPreload;
import net.minecraft.client.main.Main;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Every preLaunch entrypoint has run and added its mixin configs by the time the game's main starts. */
@Mixin(Main.class)
abstract class MainMixin {
    @Inject(method = "main", at = @At("HEAD"))
    private static void mcr$preload(String[] args, CallbackInfo ci) {
        McrPreload.milestone(McrPreload.MAIN);
    }
}
