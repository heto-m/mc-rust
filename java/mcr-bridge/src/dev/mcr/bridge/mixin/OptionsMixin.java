package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrSync;
import net.minecraft.client.Options;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Fog, entity view scale and culling follow mcr's render distance; saved key changes reach mcr. */
@Mixin(Options.class)
abstract class OptionsMixin {
    @Inject(method = "getEffectiveRenderDistance", at = @At("HEAD"), cancellable = true)
    private void mcr$rustRenderDistance(CallbackInfoReturnable<Integer> cir) {
        int distance = McrSync.renderDistance();
        if (distance > 0) {
            cir.setReturnValue(distance);
        }
    }

    @Inject(method = "save", at = @At("TAIL"))
    private void mcr$saved(CallbackInfo ci) {
        McrSync.keysMayHaveChanged();
    }
}
