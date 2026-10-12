package dev.mcr.testmod.mixin;

import dev.mcr.testmod.MarkerParticle;
import net.minecraft.client.Camera;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Zooms like a zoom mod while the test key's burst runs: the field of view halves. */
@Mixin(Camera.class)
abstract class CameraFovMixin {
    @Inject(method = "calculateFov", at = @At("RETURN"), cancellable = true)
    private void mcr_testmod$zoom(float partialTicks, CallbackInfoReturnable<Float> cir) {
        if (MarkerParticle.active()) {
            cir.setReturnValue(cir.getReturnValueF() * 0.5F);
        }
    }
}
