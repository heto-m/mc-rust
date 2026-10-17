package dev.mcr.testmod.mixin;

import dev.mcr.testmod.McrTestMod;
import net.minecraft.client.Camera;
import org.joml.Quaternionf;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Tilts the camera's rotation quaternion about its view axis. */
@Mixin(Camera.class)
abstract class CameraRollMixin {
    @Shadow
    @Final
    private Quaternionf rotation;

    @Inject(method = "alignWithEntity", at = @At("RETURN"))
    private void mcr_testmod$roll(float partialTicks, CallbackInfo ci) {
        if (McrTestMod.CAMERA_ROLL) {
            rotation.rotateZ((float) Math.toRadians(15.0));
        }
    }
}
