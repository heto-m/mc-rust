package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrParticles;
import net.minecraft.world.level.Level;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Client block entity tickers (campfire smoke, vaults, conduits) are a path mcr mirrors. */
@Mixin(Level.class)
abstract class LevelMixin {
    @Inject(method = "tickBlockEntities", at = @At("HEAD"))
    private void mcr$blockEntitiesStart(CallbackInfo ci) {
        McrParticles.enter();
    }

    @Inject(method = "tickBlockEntities", at = @At("RETURN"))
    private void mcr$blockEntitiesEnd(CallbackInfo ci) {
        McrParticles.exit();
    }
}
