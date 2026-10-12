package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrSync;
import net.minecraft.client.KeyMapping;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Vanilla's Controls screen and mods like Controlling call {@code resetMapping} after every rebind. */
@Mixin(KeyMapping.class)
abstract class KeyMappingMixin {
    @Inject(method = "resetMapping", at = @At("TAIL"))
    private static void mcr$rebound(CallbackInfo ci) {
        McrSync.keysMayHaveChanged();
    }
}
